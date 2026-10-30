//! The `pre-push` path of the gate: a pull request is merged on the remote and its target branch
//! never moves locally, so what a push is about to publish is checked here (see the module header
//! of [`super`] for the rules). A pushed branch with no recorded target branch is checked against
//! the default target branch, the first resident guarded branch, unless it is guarded itself.

use anyhow::Result;

use super::content::find_reverts;
use super::git::{
    has_commit, is_ancestor, is_null, is_space, lossy, remote_refs, short, upstream_of, Git,
};
use super::{check_move, describe, fork_points_of, forks_of_others, Inputs, Route, Verdict};

/// One ref of a push that the gate has something to say about: a branch being pushed (not
/// deleted) that is guarded or has a target branch, recorded or by default.
struct Pushed<'a> {
    /// The local branch, where the target branch and the fork point are recorded; `push
    /// <local>:<remote>` can give the two sides different names.
    source: &'a str,
    /// The branch on the remote.
    branch: &'a str,
    local: &'a str,
    remote: &'a str,
    /// The target branch `source` records, or the default target branch for one that records none
    /// and is not guarded.
    target: Option<&'a str>,
    /// Whether `branch` is guarded.
    guarded: bool,
}

/// The refs of a push that the gate has something to say about; none for a push that involves no
/// guarded branch and no target branch, which is decided here, before any `git` runs.
fn pushed_branches<'a>(inputs: &'a Inputs, stdin: &'a str) -> Vec<Pushed<'a>> {
    let mut pushed = Vec::new();
    for line in stdin.split('\n') {
        let parts: Vec<&str> = line
            .split(is_space)
            .filter(|part| !part.is_empty())
            .collect();
        let [source, local, refname, remote] = parts[..] else {
            continue;
        };
        let Some(branch) = refname.strip_prefix("refs/heads/") else {
            continue;
        };
        if is_null(local) {
            continue;
        }
        let source = source.strip_prefix("refs/heads/").unwrap_or(branch);
        let guarded = inputs.guarded.contains(branch);
        // A recorded target branch wins. With none recorded, merging into the default branch is
        // what a pull request does; a branch that is itself the default target has no one above
        // it to check against.
        let target = inputs
            .targets
            .get(source)
            .map(String::as_str)
            .filter(|target| !target.is_empty())
            .or_else(|| {
                inputs
                    .default_target
                    .as_deref()
                    .filter(|target| !guarded && *target != source && *target != branch)
            });
        if guarded || target.is_some() {
            pushed.push(Pushed {
                source,
                branch,
                local,
                remote,
                target,
                guarded,
            });
        }
    }
    pushed
}

/// The verdict on a push: every ref is checked, and the messages of all the rejections are
/// given.
pub(super) fn decide_push(git: &Git, inputs: &Inputs, stdin: &[u8]) -> Result<Verdict> {
    let stdin = lossy(stdin);
    let mut verdict = Verdict::default();
    for pushed in pushed_branches(inputs, &stdin) {
        // Both checks run, not either-or: a branch can both record its own target branch and be
        // guarded because another branch records it. The first is checked against the target
        // branch's remote tip, the second against the branch's own remote tip; neither can stand
        // in for the other.
        if let Some(target) = pushed.target {
            check_target(
                git,
                inputs,
                &mut verdict,
                pushed.source,
                target,
                pushed.local,
            )?;
        }
        let branch = pushed.branch;
        if !pushed.guarded || is_null(pushed.remote) {
            continue;
        }
        if !has_commit(git, pushed.remote)? {
            // No network: the remote value is the live one git got when pushing, so not having it
            // locally means the remote has commits that have not been fetched yet.
            verdict.reject(format!(
                "\n[revert-gate] Refusing to push {branch}: remote {branch} has commits the local \
                 one does not. `git fetch` first, then `git rebase {}` in the working copy that \
                 holds {branch}, verify, then push.\n\n",
                upstream_of(git, branch)?.1
            ));
            continue;
        }
        // As for a transaction: after a fast-forward merge the local value still points at that
        // feature branch, which is how its fork point is found.
        let forks = forks_of_others(git, inputs, pushed.local, branch, &|| {
            Ok(vec![pushed.remote.to_string()])
        })?;
        if let Some(message) = check_move(
            git,
            Route::Push,
            branch,
            pushed.remote,
            pushed.local,
            &forks,
            &[],
        )? {
            verdict.reject(message);
        }
    }
    Ok(verdict)
}

/// Checks the pushed branch `source`, whose tip is `new`, against the remote state of the target
/// branch `target` it records. A target branch whose remote state cannot be obtained lets the push
/// through with a note, so that exit status 0 is not taken for a check that ran.
fn check_target(
    git: &Git,
    inputs: &Inputs,
    verdict: &mut Verdict,
    source: &str,
    target: &str,
    new: &str,
) -> Result<()> {
    // The base is the remote tip; an upstream that points at a local branch
    // (`branch.<target>.remote = .`) counts as unobtainable.
    let upstream = upstream_of(git, target)?
        .0
        .filter(|r| r.starts_with("refs/remotes/"));
    let tip = match &upstream {
        Some(upstream) => git.line_lenient([
            "rev-parse",
            "--verify",
            "-q",
            &format!("{upstream}^{{commit}}"),
        ])?,
        None => String::new(),
    };
    let Some(upstream) = upstream.filter(|_| !tip.is_empty()) else {
        // A target branch with no remote-tracking branch at all is simply not published to a
        // remote, not a case of "cannot be checked"; noting it on every push would only be noise.
        if !remote_refs(git, target)?.is_empty() {
            verdict.note(format!(
                "\n[revert-gate] {source} was not checked: {target}'s remote state could not be \
                 pinned down — it has several same-named remote-tracking branches with no upstream \
                 set, or its upstream is not a remote branch. Set an upstream with `git branch -u \
                 <remote>/{target} {target}` and push again.\n\n"
            ));
        }
        return Ok(());
    };
    // Hints name the target branch as `<remote>/<target branch>`, the ref the branch is rebased
    // onto, not as `<branch>@{u}`.
    let shown = upstream.strip_prefix("refs/remotes/").unwrap_or(&upstream);
    check_pr(git, inputs, verdict, source, shown, &tip, new)
}

/// Content check for a pushed branch: whether `new`, its tip, undoes changes made by existing
/// commits, relative to `tip`, the target branch's remote state (`shown` in hints). Skipped when
/// `tip` is not an ancestor of `new`: a branch not yet rebased onto the latest target branch is
/// pushed every day, and checking it regardless would be nothing but false positives.
fn check_pr(
    git: &Git,
    inputs: &Inputs,
    verdict: &mut Verdict,
    branch: &str,
    shown: &str,
    tip: &str,
    new: &str,
) -> Result<()> {
    if is_null(tip) || is_null(new) || tip == new || !is_ancestor(git, tip, new)? {
        return Ok(());
    }
    let forks = fork_points_of(git, inputs, &[branch.to_string()], &|| {
        Ok(vec![tip.to_string()])
    })?;
    let findings = find_reverts(git, tip, new, &forks, &forks)?;
    if findings.is_empty() {
        return Ok(());
    }
    verdict.reject(format!(
        "\n[revert-gate] Refusing to push {branch}: it undoes changes made by existing commits, \
         relative to {shown}.\n\n\
         Against {shown} ({}):\n{}\n\
         The cause is resolving every rebase conflict to only your own side, or a squash that \
         changed the base\n(e.g. committing after `git reset --soft {shown}`). Go back to before \
         the squash, squash with\n`git reset --soft $(git merge-base HEAD {shown})` instead, then \
         `git rebase {shown}` and resolve the\nconflicts faithfully, combining the changes from \
         both sides; do not bypass this with `--no-verify`.\n\
         If the revert really is deliberate, confirm it and use `git revert` instead, or add one\n\
         `Reverts: <sha>` line per undone commit to the message of the commit that undoes it.\n\n",
        short(tip),
        describe(&findings)
    ));
    Ok(())
}
