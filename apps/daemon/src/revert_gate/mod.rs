//! `octoboardd revert-gate` — blocks updates that quietly undo changes already merged into a
//! guarded branch. The session hook dispatchers do not start it: `octoboardd gate-hook` asks the
//! daemon for the inputs and calls [`check`] in-process. The command runs the gate directly, with
//! the inputs given as arguments and the hook's stdin, and its exit status is the hook's verdict.
//! Its first step, before any `git` runs, is to see whether a guarded branch is involved
//! at all, and it allows at once when none is.
//!
//! The typical cause is a squash that changed the base without merging the content (e.g.
//! committing after `git reset --soft <target branch>`), so old code gets squashed in as if it
//! were new. `git merge --ff-only` only checks the ancestry relation and fast-forwards all the
//! same, so guarded branches are checked by content whenever they move. The guarded ones are the
//! resident guarded branches (`--guard`, usually the main branch) and the target branches the
//! development branches have recorded (`--target`). Only those are consulted, not which branch
//! the main working copy currently has checked out, so that rebasing or amending on your own
//! feature branch is not blocked either; a target branch with no record is therefore not
//! guarded. Rules:
//!
//! 1. A non-fast-forward can only be allowed when none of the dropped commits were ever
//!    published (`pull --rebase`, `git rebase <branch>@{u}` replaying local unpushed commits on
//!    top of the latest remote commit, rewriting local commits that have not been pushed), and it
//!    still has to pass the content check of rule 2 — an unpushed commit in the same clone may
//!    also be one someone else just merged. A move that aligns with a remote-tracking branch and
//!    drops no local unpushed commit is allowed outright (published history cannot be held back
//!    locally); when the branch being moved is not checked out in any worktree and is not being
//!    rebased, a dropped unpushed commit that is still on another local branch does not count as
//!    dropped (first `git switch -c` to a new branch to carry the local commits, then
//!    `git branch -f` to move it back to the remote — git refuses to let `branch -f` touch a
//!    checked-out branch). This does not apply while the branch is checked out or being rebased:
//!    when you reset or rebase on it, or update-ref it from another worktree, another branch
//!    pointing at those commits is most likely only temporary (a scratch branch, a feature branch
//!    already fast-forward merged and about to be deleted), and deleting it loses the commits.
//!    Known gap: `switch -c` to a scratch branch, `branch -f` back, then switching back and
//!    deleting the scratch branch, or `switch --detach` first and then `branch -f` back (the
//!    feature branch still holds those commits) and deleting that branch afterwards, both have
//!    the same shape as the legitimate flows above; they are a deliberate bypass, the gate cannot
//!    tell them apart, and they are allowed.
//! 2. Reject when any of the most recent non-merge commits on the branch before the move is
//!    undone by this move; the criteria are in `content::find_reverts`. How far back: at least
//!    the most recent `content::WINDOW` commits; when the move can be attributed to a feature
//!    branch that recorded its fork point (`--fork-point`), extend to every commit after that fork
//!    point.
//!    The fork point cannot be found from git history alone — a squash that changed the base is
//!    exactly one that rewrote the parent to the latest target branch, so it has to be recorded
//!    when the branch is created. When the target branch was rebased while aligning with the
//!    remote, the fork point is no longer its ancestor, so counting starts from the merge base of
//!    the fork point and the target branch instead.
//!    When a local move lands above some remote-tracking branch tip and that tip is not below the
//!    position before the move (`git rebase <branch>@{u}`), commits on the tip (the published
//!    ones) are checked against the tip instead, and the position before the move is used only
//!    for commits not on the tip (the local unpushed ones):
//!    - Changes already undone on the tip were undone by someone else on the remote (a manual
//!      revert does not necessarily leave a `Reverts:`), cannot be held back locally, and do not
//!      count as undone by this move; undoing a change that is still live on the tip is rejected
//!      all the same, including within the same commit and the same file.
//!    - Commits someone else just pushed that are absent from the position before the move are
//!      included too: they are exactly what a conflict resolution keeping only the local side
//!      undoes, they cannot be found by checking against the position before the move alone, and
//!      would only be caught at push time, when redoing the work is already inconvenient.
//!
//!    The content criteria look at "does this resemble being restored", so a change that only
//!    adds lines and never deletes any is invisible when the whole thing is wiped out in one go
//!    (this covers undoing a purely additive commit, and also deleting a file that was only ever
//!    appended to). This shape is instead caught by the third criterion — the structural check
//!    for a changed base: if this move changed some file that the branch itself never touched
//!    since its fork point, that is exactly what a changed base drags in, whether the change was
//!    an add, a delete, or a content edit. It requires a recorded fork point; branches with no
//!    recorded fork point (`--fork-point`) are not protected against this kind of accidental
//!    deletion.
//! 3. When a commit message added by this move carries `Reverts: <sha>`, or git revert's default
//!    `This reverts commit <sha>.`, the corresponding commit is allowed — a deliberate revert
//!    must leave a record.
//!
//! A pull request is merged on the remote and its target branch never moves locally, so none of
//! the above fires for it; the `pre-push` hook covers it instead:
//!
//! - A pushed branch that has a recorded target branch (`--target`) is checked by rules 2 and 3
//!   against the content of that target branch's remote-tracking tip, extended back to its own
//!   recorded fork point (`--fork-point`), which has to be recorded before squashing since it
//!   cannot be computed once the rebase has happened. It only runs when the pushed tip is a
//!   descendant of that tip, i.e. rebased onto the latest target branch: a branch not yet rebased
//!   is pushed every day, and checking it regardless would be nothing but false positives. A
//!   target branch whose remote state cannot be pinned down is not checked, with a line on stderr
//!   saying so; one that is not on a remote at all is left alone silently. Rule 1 does not
//!   apply: force-pushing a branch you alone own, after a squash, is expected.
//! - A pushed guarded branch is checked by rules 2 and 3 against its own remote value, as given by
//!   the remote when pushing; any non-fast-forward is rejected (it would overwrite what the remote
//!   has, and a forced push is not refused by the remote). A remote value missing
//!   locally is rejected outright: the content cannot be judged, and letting the push through
//!   could overwrite commits someone else just pushed.
//! - A branch with no recorded target branch, and a push not yet rebased onto the latest target
//!   branch, are left unchecked, and the target branch's remote state is as of the last fetch.
//!
//! The exit status is 0 to allow and 3 to reject, with the reason on stderr; the caller treats
//! only 3 as a rejection. When the gate itself fails (git failing, a deadline, bad input) the
//! update is allowed, with a line on stderr saying so: better to miss one check than to let it
//! wedge every ref update or push. `REVERT_GATE_SKIP=1` in the environment skips the check, only
//! for cases a human confirmed. The `reference-transaction` hook needs git 2.28+.

mod content;
mod git;
mod push;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};

use anyhow::Result;

use content::{find_reverts, Finding};
use git::{
    branches_at, busy_branches, is_ancestor, is_null, is_oid, is_space, lossy, remote_tips,
    rev_count, short, upstream_of, Git,
};

/// The exit status of a rejection.
const REJECTED: i32 = 3;

/// When blocked at the local layer, each command has already completed a different amount of
/// work: merge / pull write the work tree and the index first and update the ref last; reset
/// changes the index first (and the work tree too with `--hard`); rebase stops at its last step.
const RESTORE_HINT: &str = "\
If a merge / pull was blocked, the working copy has been written to the branch content; restore it
with `git reset --merge`: it only reverts the files written this time and keeps other uncommitted
changes. Do not use it when a commit was blocked, it would discard the staged changes.
If a reset was blocked, the index (and the work tree too with `--hard`) has been changed to the
content of the target position: restore `--hard` with `git reset --merge`, `--mixed` with `git reset`.
If a rebase was blocked, leave it with `git rebase --abort`.
";

/// The arguments of `octoboardd revert-gate`: the `reference-transaction` hook's state, or the
/// `pre-push` subcommand.
#[derive(clap::Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct Args {
    /// The `reference-transaction` hook's argument: `prepared`, `committed` or `aborted`. Only
    /// `prepared` is checked. The hook's ref update lines are read from stdin.
    #[arg(required = true)]
    state: Option<String>,
    #[command(flatten)]
    options: Options,
    #[command(subcommand)]
    pre_push: Option<PrePush>,
}

#[derive(clap::Subcommand)]
enum PrePush {
    /// Check the refs of a push; the lines `<local ref> <local oid> <remote ref> <remote oid>` are
    /// read from stdin.
    PrePush {
        /// The `pre-push` hook's first argument: the remote's name.
        remote: String,
        /// The `pre-push` hook's second argument: the remote's URL.
        url: String,
        #[command(flatten)]
        options: Options,
    },
}

// What the gate is told about the repository's branches. Branch names are short names, without
// `refs/heads/`.
#[derive(clap::Args)]
struct Options {
    /// A resident guarded branch.
    #[arg(long, value_name = "BRANCH")]
    guard: Vec<String>,
    /// A branch and its recorded target branch; every target branch named is guarded.
    #[arg(long, num_args = 2, value_names = ["BRANCH", "TARGET_BRANCH"])]
    target: Vec<String>,
    /// A branch and its recorded fork point: a commit id, or anything `git rev-parse` resolves.
    #[arg(long, num_args = 2, value_names = ["BRANCH", "COMMIT"])]
    fork_point: Vec<String>,
}

impl Options {
    fn inputs(&self) -> GateInputs {
        let pairs = |flat: &[String]| {
            flat.chunks(2)
                .map(|pair| (pair[0].clone(), pair[1].clone()))
                .collect()
        };
        GateInputs {
            guard: self.guard.clone(),
            targets: pairs(&self.target),
            fork_points: pairs(&self.fork_point),
        }
    }
}

/// The hook the gate is run for.
#[derive(clap::Subcommand)]
pub enum Hook {
    /// The `reference-transaction` hook at `state` (`prepared`, `committed` or `aborted`; only
    /// `prepared` is checked); the ref updates come on stdin.
    ReferenceTransaction { state: String },
    /// The `pre-push` hook; the refs being pushed come on stdin. The gate judges the refs, not
    /// where they go, so neither the remote nor its URL is taken.
    PrePush,
}

/// What the gate knows about the repository's branches besides git's own state. It is also what the
/// daemon answers the `gate-hook` mode with, so it is the wire form too.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GateInputs {
    /// Resident guarded branches.
    pub guard: Vec<String>,
    /// `(branch, target branch)` records; every target branch named is guarded.
    pub targets: Vec<(String, String)>,
    /// `(branch, fork point)` records; the fork point is anything `git rev-parse` resolves.
    pub fork_points: Vec<(String, String)>,
}

/// [`GateInputs`] looked up by branch.
struct Inputs {
    guarded: HashSet<String>,
    targets: HashMap<String, String>,
    fork_points: HashMap<String, String>,
}

impl Inputs {
    fn new(inputs: &GateInputs) -> Self {
        let guarded = inputs
            .guard
            .iter()
            .chain(inputs.targets.iter().map(|(_, target)| target))
            .filter(|branch| !branch.is_empty())
            .cloned()
            .collect();
        // The last record given for a branch wins.
        Self {
            guarded,
            targets: inputs.targets.iter().cloned().collect(),
            fork_points: inputs.fork_points.iter().cloned().collect(),
        }
    }
}

/// Runs the gate for the process's own arguments and stdin, and returns the exit status for the
/// hook.
pub fn run(args: &Args) -> i32 {
    let (hook, inputs) = match &args.pre_push {
        Some(PrePush::PrePush { options, .. }) => (Hook::PrePush, options.inputs()),
        None => (
            Hook::ReferenceTransaction {
                state: args.state.clone().unwrap_or_default(),
            },
            args.options.inputs(),
        ),
    };
    check(&hook, &inputs, std::io::stdin())
}

/// Runs the gate for `hook`, with the hook's stdin, and returns the exit status for the hook. The
/// first step, before any `git` runs, is to see whether a guarded branch is involved at all.
pub fn check(hook: &Hook, inputs: &GateInputs, mut stdin: impl Read) -> i32 {
    // Always drained: git writes the update lines to the hook's stdin.
    let mut input = Vec::new();
    let _ = stdin.read_to_end(&mut input);
    let (status, message) = run_in(&Git::hook(), skip_requested(), hook, inputs, &input);
    let _ = std::io::stderr().write_all(message.as_bytes());
    status
}

/// Whether the environment asks to skip the check (`REVERT_GATE_SKIP=1`).
pub fn skip_requested() -> bool {
    std::env::var_os("REVERT_GATE_SKIP").is_some_and(|value| value == "1")
}

/// The exit status and the stderr text for `hook` run with `git`; `skipped` is the request to
/// skip the check.
fn run_in(
    git: &Git,
    skipped: bool,
    hook: &Hook,
    inputs: &GateInputs,
    input: &[u8],
) -> (i32, String) {
    if skipped {
        return (0, String::new());
    }
    let inputs = Inputs::new(inputs);
    outcome(match hook {
        Hook::ReferenceTransaction { state } => {
            decide(git, &inputs, state, input).map(Verdict::rejection)
        }
        Hook::PrePush => push::decide_push(git, &inputs, input),
    })
}

/// What the gate says about an update: whether it is rejected, and the stderr text, which can be
/// there for an allowed update too (a check that could not be made).
#[derive(Default)]
struct Verdict {
    rejected: bool,
    text: String,
}

impl Verdict {
    /// The verdict for `Some(message)` rejecting, `None` allowing.
    fn rejection(message: Option<String>) -> Self {
        Self {
            rejected: message.is_some(),
            text: message.unwrap_or_default(),
        }
    }

    fn reject(&mut self, message: String) {
        self.rejected = true;
        self.text.push_str(&message);
    }

    fn note(&mut self, message: String) {
        self.text.push_str(&message);
    }
}

/// The exit status and the stderr text for the result of [`decide`] or [`push::decide_push`]. A
/// failure of the gate itself allows the update.
fn outcome(result: Result<Verdict>) -> (i32, String) {
    match result {
        Ok(verdict) if verdict.rejected => (REJECTED, verdict.text),
        Ok(verdict) => (0, verdict.text),
        Err(err) => (
            0,
            format!("[revert-gate] Check failed, allowed anyway: {err:#}\n"),
        ),
    }
}

/// `None` to allow the transaction, or the message that rejects it.
fn decide(git: &Git, inputs: &Inputs, state: &str, stdin: &[u8]) -> Result<Option<String>> {
    if state != "prepared" {
        return Ok(None);
    }
    for line in lossy(stdin).split('\n') {
        let parts: Vec<&str> = line
            .split(is_space)
            .filter(|part| !part.is_empty())
            .collect();
        let [old, new, refname] = parts[..] else {
            continue;
        };
        let Some(branch) = refname.strip_prefix("refs/heads/") else {
            continue;
        };
        if !inputs.guarded.contains(branch) {
            continue;
        }
        // An update with no expected old value arrives with all zeros as the old value; at this
        // point the ref is already locked but still holds the old value.
        let old = if is_null(old) {
            git.line_lenient(["rev-parse", "--verify", "-q", refname])?
        } else {
            old.to_string()
        };
        if !is_oid(&old) || !is_oid(new) {
            continue; // a symref value (`ref:...`) is not a commit
        }
        let forks = forks_of_others(git, inputs, new, branch)?;
        if let Some(message) = check_move(git, Route::Local, branch, &old, new, &forks)? {
            return Ok(Some(message));
        }
    }
    Ok(None)
}

/// The commit ids of the fork points recorded by the branches at `commit` other than `moved`.
fn forks_of_others(git: &Git, inputs: &Inputs, commit: &str, moved: &str) -> Result<Vec<String>> {
    let branches: Vec<String> = branches_at(git, commit)?
        .into_iter()
        .filter(|name| name != moved)
        .collect();
    recorded_forks(git, inputs, &branches)
}

/// The commit ids that the fork points recorded by `branches` resolve to; branches with no
/// record, or a record that will not resolve, are skipped.
fn recorded_forks(git: &Git, inputs: &Inputs, branches: &[String]) -> Result<Vec<String>> {
    let mut forks = Vec::new();
    for name in branches {
        let Some(fork) = inputs.fork_points.get(name).filter(|fork| !fork.is_empty()) else {
            continue;
        };
        let oid =
            git.line_lenient(["rev-parse", "--verify", "-q", &format!("{fork}^{{commit}}")])?;
        if !oid.is_empty() {
            forks.push(oid);
        }
    }
    Ok(forks)
}

/// Remote-tracking branch tips below `new` but not below `old`; see rule 2. A tip below `old`
/// (the target branch is merely ahead of the remote, or a same-named branch on another remote is
/// behind) means this move is not an alignment with the remote.
fn tips_below(git: &Git, branch: &str, old: &str, new: &str) -> Result<Vec<String>> {
    let mut tips = Vec::new();
    for tip in remote_tips(git, branch)? {
        if tips.contains(&tip) || tip == old {
            continue;
        }
        if !is_ancestor(git, &tip, old)? && is_ancestor(git, &tip, new)? {
            tips.push(tip);
        }
    }
    Ok(tips)
}

/// Commits on each tip that `new` undid, checked against that tip (all of them published).
/// `forks` decides how far back the checked range extends; `bases` is the branch's actual fork
/// point — `forks` here is mixed with `old` and parent commits, so it cannot be used as one.
fn reverted_on_tips(
    git: &Git,
    new: &str,
    tips: &[String],
    forks: &[String],
    bases: &[String],
) -> Result<Vec<Finding>> {
    let mut found: Vec<Finding> = Vec::new();
    for tip in tips {
        for finding in find_reverts(git, tip, new, forks, bases)? {
            if !found.iter().any(|known| known.sha == finding.sha) {
                found.push(finding);
            }
        }
    }
    Ok(found)
}

/// What moves a guarded branch.
#[derive(Clone, Copy, PartialEq)]
enum Route {
    /// A ref update in this clone.
    Local,
    /// A push, which moves the branch on the remote.
    Push,
}

/// Checks one move of the guarded branch `branch` from `old` to `new`: `None` when it is allowed,
/// else the message that rejects it.
fn check_move(
    git: &Git,
    route: Route,
    branch: &str,
    old: &str,
    new: &str,
    forks: &[String],
) -> Result<Option<String>> {
    if is_null(old) || is_null(new) || old == new {
        return Ok(None);
    }
    // Number of commits dropped and on no remote-tracking branch: an unpushed commit in the same
    // clone may also be one someone else just merged.
    let not_new = format!("^{new}");
    let unpushed = rev_count(git, &[old, &not_new, "--not", "--remotes"])?;
    // Of those, only the ones not on another local branch either are truly lost, and that only
    // counts for a branch that is neither checked out nor being rebased; see rule 1. The branch
    // itself has to be excluded — in the prepared phase it still points at `old`. A branch name
    // cannot contain glob characters, so it works directly as an `--exclude` pattern.
    let mut stranded = unpushed;
    if route == Route::Local && unpushed > 0 && !busy_branches(git)?.contains(branch) {
        let exclude = format!("--exclude={branch}");
        stranded = rev_count(
            git,
            &[old, &not_new, "--not", "--remotes", &exclude, "--branches"],
        )?;
    }
    if route == Route::Local
        && stranded == 0
        && remote_tips(git, branch)?.iter().any(|tip| tip == new)
    {
        return Ok(None);
    }
    if !is_ancestor(git, old, new)? {
        if route == Route::Push {
            return Ok(Some(format!(
                "\n[revert-gate] Refusing to rewrite remote {branch} from {} to {}: not a \
                 fast-forward, it would drop commits the remote already has. Do not force-push; \
                 `git fetch` first, then `git rebase {}` in the working copy that holds {branch} \
                 to replay the local unpushed commits on top of the remote, verify, then push.\n\n",
                short(old),
                short(new),
                upstream_of(git, branch)?.1
            )));
        }
        // If even one dropped commit is already on a remote-tracking branch, this rewrites
        // published history.
        if rev_count(git, &[old, &not_new])? > unpushed {
            let shown = upstream_of(git, branch)?.1;
            return Ok(Some(format!(
                "\n[revert-gate] Refusing to move {branch} from {} to {}: not a fast-forward, it would \
                 drop published commits, and {branch} must not rewrite published history.\n\
                 To align with the remote, `git fetch` first, then `git rebase {shown}` in the working \
                 copy that holds {branch} to replay the local unpushed commits on top of the remote.\n\n\
                 {RESTORE_HINT}\n",
                short(old),
                short(new)
            )));
        }
    }
    // `find_reverts` does not require `old` to be an ancestor of `new`: a non-fast-forward is
    // checked against `old` just the same, and the dropped local commits are checked as well.
    // `bases` only ever gets the actual fork points: the forks used by `reverted_on_tips` are
    // mixed with other reference points and cannot be used as fork points.
    let mut findings = find_reverts(git, old, new, forks, forks)?;
    let mut remote_findings = Vec::new();
    let tips = if route == Route::Local {
        tips_below(git, branch, old, new)?
    } else {
        Vec::new()
    };
    if !tips.is_empty() {
        // Commits on the tip are checked against the tip instead; see rule 2. The parents of
        // published commits already hit against `old` are passed in as fork points too: when the
        // tip is many commits ahead of `old` they may fall outside the window counted from the
        // tip.
        let mut published = Vec::new();
        for finding in &findings {
            for tip in &tips {
                if is_ancestor(git, &finding.sha, tip)? {
                    published.push(finding.sha.clone());
                    break;
                }
            }
        }
        findings.retain(|finding| !published.contains(&finding.sha));
        let tip_forks: Vec<String> = [old.to_string()]
            .into_iter()
            .chain(forks.iter().cloned())
            .chain(published.iter().map(|sha| format!("{sha}^")))
            .collect();
        remote_findings = reverted_on_tips(git, new, &tips, &tip_forks, forks)?;
    }
    if findings.is_empty() && remote_findings.is_empty() {
        return Ok(None);
    }
    Ok(Some(report(
        branch,
        old,
        new,
        &findings,
        &remote_findings,
        route,
        !tips.is_empty(),
    )))
}

fn describe(findings: &[Finding]) -> String {
    let mut text = String::new();
    for finding in findings {
        text.push_str(&format!("  {} {}\n", short(&finding.sha), finding.subject));
        for (file, why) in &finding.hits {
            text.push_str(&format!("      {}: {why}\n", lossy(file)));
        }
    }
    text
}

/// The message that rejects a move. `remote_findings` are the published commits found by
/// checking against the remote-tracking branch tip; `aligning` means this local move replayed the
/// branch on top of the remote-tracking branch (aligning with the remote), in which case
/// `findings` only holds local unpushed commits.
fn report(
    branch: &str,
    old: &str,
    new: &str,
    findings: &[Finding],
    remote_findings: &[Finding],
    route: Route,
    aligning: bool,
) -> String {
    let mut text = format!(
        "\n[revert-gate] Refusing to move {branch} to {}: this update undoes changes made by \
         existing commits.\n\n",
        short(new)
    );
    if !findings.is_empty() {
        text.push_str(&format!(
            "Against {} ({branch} before the move){}:\n{}",
            short(old),
            if aligning {
                ", local unpushed commits only"
            } else {
                ""
            },
            describe(findings)
        ));
    }
    if !remote_findings.is_empty() {
        text.push_str(&format!(
            "Against {branch} on the remote-tracking branch (published commits):\n{}",
            describe(remote_findings)
        ));
    }
    let cause = if route == Route::Push {
        format!(
            "The revert comes from a squash that changed the base, or from a conflict resolution \
             that kept only one side: add a commit in the working copy that holds {branch} to \
             bring the undone changes back, verify, then push; do not reset {branch}."
        )
    } else if aligning {
        "Most likely the wrong side was kept while resolving conflicts when aligning with the \
         remote: if a rebase was blocked, leave it with `git rebase --abort` and rebase again, \
         resolving conflicts so that the changes from both sides are combined."
            .to_string()
    } else {
        format!(
            "The most common cause is a squash that changed the base without merging the content\n\
             (e.g. committing after `git reset --soft {branch}`). On the branch, go back to before\n\
             the squash, squash with `git reset --soft $(git merge-base HEAD {branch})` instead,\n\
             then `git rebase {branch}` and resolve the conflicts faithfully."
        )
    };
    text.push_str(&format!(
        "\n{cause}\nIf the revert really is deliberate, confirm it and use `git revert` instead, or add one\n\
         `Reverts: <sha>` line per undone commit to the message of the commit that undoes it.\n\n"
    ));
    if route == Route::Local {
        text.push_str(RESTORE_HINT);
        text.push('\n');
    }
    text
}
