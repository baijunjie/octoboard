//! `octoboardd revert-gate` — blocks updates that quietly undo changes already merged into a
//! guarded branch. Meant to be started by a session's `reference-transaction` hook dispatcher, in
//! the foreground, for each transaction; its exit status is the hook's verdict.
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
//! The exit status is 0 to allow and 3 to reject, with the reason on stderr; the caller treats
//! only 3 as a rejection. When the gate itself fails (git failing, a deadline, bad input) the
//! update is allowed, with a line on stderr saying so: better to miss one check than to let it
//! wedge every ref update. `REVERT_GATE_SKIP=1` in the environment skips the check, only for
//! cases a human confirmed. The hook needs git 2.28+.

mod content;
mod git;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};

use anyhow::Result;

use content::{find_reverts, Finding};
use git::{is_null, is_oid, is_space, lossy, short, Git};

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

/// The arguments of `octoboardd revert-gate`. Branch names are short names, without
/// `refs/heads/`.
#[derive(clap::Args)]
pub struct Args {
    /// The `reference-transaction` hook's argument: `prepared`, `committed` or `aborted`. Only
    /// `prepared` is checked. The hook's ref update lines are read from stdin.
    state: String,
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

/// What the gate knows about the repository's branches besides git's own state.
struct Inputs {
    guarded: HashSet<String>,
    fork_points: HashMap<String, String>,
}

impl Inputs {
    fn from_args(args: &Args) -> Self {
        let guarded = args
            .guard
            .iter()
            .chain(args.target.chunks(2).map(|pair| &pair[1]))
            .filter(|branch| !branch.is_empty())
            .cloned()
            .collect();
        // The last `--fork-point` given for a branch wins.
        let fork_points = args
            .fork_point
            .chunks(2)
            .map(|pair| (pair[0].clone(), pair[1].clone()))
            .collect();
        Self {
            guarded,
            fork_points,
        }
    }
}

/// Runs the gate and returns the exit status for the hook.
pub fn run(args: &Args) -> i32 {
    // Always drained: git writes the update lines to the hook's stdin.
    let mut stdin = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut stdin);
    if std::env::var_os("REVERT_GATE_SKIP").is_some_and(|value| value == "1") {
        return 0;
    }
    let (status, message) = outcome(decide(
        &Git::hook(),
        &Inputs::from_args(args),
        &args.state,
        &stdin,
    ));
    let _ = std::io::stderr().write_all(message.as_bytes());
    status
}

/// The exit status and the stderr text for the result of [`decide`]. A failure of the gate
/// itself allows the update.
fn outcome(result: Result<Option<String>>) -> (i32, String) {
    match result {
        Ok(None) => (0, String::new()),
        Ok(Some(message)) => (REJECTED, message),
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
        let forks = recorded_forks(git, inputs, &branches_at(git, new)?, branch)?;
        if let Some(message) = check_move(git, branch, &old, new, &forks)? {
            return Ok(Some(message));
        }
    }
    Ok(None)
}

/// The commit ids that the fork points recorded by `branches` (other than `moved`) resolve to;
/// branches with no record, or a record that will not resolve, are skipped.
fn recorded_forks(
    git: &Git,
    inputs: &Inputs,
    branches: &[String],
    moved: &str,
) -> Result<Vec<String>> {
    let mut forks = Vec::new();
    for name in branches.iter().filter(|name| *name != moved) {
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

/// Local branch names pointing at `commit`. On a fast-forward merge `new` is the feature branch
/// tip, which is how its recorded fork point is found.
fn branches_at(git: &Git, commit: &str) -> Result<Vec<String>> {
    let out = git.read([
        "for-each-ref",
        "--format=%(refname)",
        &format!("--points-at={commit}"),
        "refs/heads/",
    ])?;
    Ok(lossy(&out)
        .split_whitespace()
        .filter_map(|name| name.strip_prefix("refs/heads/"))
        .map(str::to_string)
        .collect())
}

fn is_ancestor(git: &Git, a: &str, b: &str) -> Result<bool> {
    git.succeeds(["merge-base", "--is-ancestor", a, b])
}

fn rev_count(git: &Git, args: &[&str]) -> Result<usize> {
    let out = git.read(
        ["rev-list", "--count"]
            .into_iter()
            .chain(args.iter().copied()),
    )?;
    Ok(lossy(&out).trim().parse().unwrap_or(0))
}

fn remote_refs(git: &Git, branch: &str) -> Result<Vec<String>> {
    let out = git.read([
        "for-each-ref",
        "--format=%(refname)",
        &format!("refs/remotes/*/{branch}"),
    ])?;
    Ok(lossy(&out).split_whitespace().map(str::to_string).collect())
}

fn remote_tips(git: &Git, branch: &str) -> Result<Vec<String>> {
    let out = git.read([
        "for-each-ref",
        "--format=%(objectname)",
        &format!("refs/remotes/*/{branch}"),
    ])?;
    Ok(lossy(&out).split_whitespace().map(str::to_string).collect())
}

/// How the branch's upstream is written in a hint. With no upstream (or an upstream ref that does
/// not exist), the unique same-named branch on a remote is used instead; with no or several
/// same-named branches the hint is written `<remote>/<branch>`.
fn upstream_hint(git: &Git, branch: &str) -> Result<String> {
    let upstream = git.line_lenient([
        "for-each-ref",
        "--format=%(upstream)",
        &format!("refs/heads/{branch}"),
    ])?;
    if !upstream.is_empty() && git.succeeds(["rev-parse", "--verify", "-q", upstream.as_str()])? {
        return Ok(format!("{branch}@{{u}}"));
    }
    let refs = remote_refs(git, branch)?;
    Ok(match refs.as_slice() {
        [only] => only
            .strip_prefix("refs/remotes/")
            .unwrap_or(only)
            .to_string(),
        _ => format!("<remote>/{branch}"),
    })
}

/// Names of branches checked out in any worktree, or currently being rebased. During a rebase
/// HEAD is detached, so the worktree listing does not show which branch it is on; read that from
/// each worktree's rebase state directory instead.
fn busy_branches(git: &Git) -> Result<HashSet<String>> {
    let mut names = HashSet::new();
    let listing = git.read(["worktree", "list", "--porcelain"])?;
    for line in lossy(&listing).split('\n') {
        if let Some(name) = line.strip_prefix("branch refs/heads/") {
            names.insert(name.to_string());
        }
    }
    let common = git
        .dir()?
        .join(lossy(&git.read(["rev-parse", "--git-common-dir"])?).trim());
    let mut gitdirs = vec![common.clone()];
    if let Ok(linked) = std::fs::read_dir(common.join("worktrees")) {
        gitdirs.extend(linked.filter_map(Result::ok).map(|entry| entry.path()));
    }
    for gitdir in gitdirs {
        for state in ["rebase-merge", "rebase-apply"] {
            let Ok(head) = std::fs::read(gitdir.join(state).join("head-name")) else {
                continue;
            };
            if let Some(name) = lossy(&head).trim().strip_prefix("refs/heads/") {
                names.insert(name.to_string());
            }
        }
    }
    Ok(names)
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

/// Checks one move of the guarded branch `branch` from `old` to `new`: `None` when it is allowed,
/// else the message that rejects it.
fn check_move(
    git: &Git,
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
    if unpushed > 0 && !busy_branches(git)?.contains(branch) {
        let exclude = format!("--exclude={branch}");
        stranded = rev_count(
            git,
            &[old, &not_new, "--not", "--remotes", &exclude, "--branches"],
        )?;
    }
    if stranded == 0 && remote_tips(git, branch)?.iter().any(|tip| tip == new) {
        return Ok(None);
    }
    if !is_ancestor(git, old, new)? {
        // If even one dropped commit is already on a remote-tracking branch, this rewrites
        // published history.
        if rev_count(git, &[old, &not_new])? > unpushed {
            let shown = upstream_hint(git, branch)?;
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
    let tips = tips_below(git, branch, old, new)?;
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
/// checking against the remote-tracking branch tip; `aligning` means this local move replayed
/// the branch on top of the remote-tracking branch (aligning with the remote), in which case
/// `findings` only holds local unpushed commits.
fn report(
    branch: &str,
    old: &str,
    new: &str,
    findings: &[Finding],
    remote_findings: &[Finding],
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
    let cause = if aligning {
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
         `Reverts: <sha>` line per undone commit to the message of the commit that undoes it.\n\n{RESTORE_HINT}\n"
    ));
    text
}
