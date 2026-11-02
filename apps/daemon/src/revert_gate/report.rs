//! The words of a rejection.

use super::content::Finding;
use super::git::{lossy, short};
use super::Route;

/// When blocked at the local layer, each command has already completed a different amount of
/// work: merge / pull write the work tree and the index first and update the ref last; reset
/// changes the index first (and the work tree too with `--hard`); rebase stops at its last step.
pub(super) const RESTORE_HINT: &str = "\
If a merge / pull was blocked, the working copy has been written to the branch content; restore it
with `git reset --merge`: it only reverts the files written this time and keeps other uncommitted
changes. Do not use it when a commit was blocked, it would discard the staged changes.
If a reset was blocked, the index (and the work tree too with `--hard`) has been changed to the
content of the target position: restore `--hard` with `git reset --merge`, `--mixed` with `git reset`.
If a rebase was blocked, leave it with `git rebase --abort`.
";

pub(super) fn describe(findings: &[Finding]) -> String {
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
pub(super) fn report(
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
             (e.g. committing after `git reset --soft {branch}`). On the source branch, go back to\n\
             before the squash and identify its target branch. Use\n\
             `git rebase -i $(git merge-base HEAD <target branch>)` to squash without moving\n\
             the branch to the base first, preserving deliberate-revert lines. Verify the final\n\
             tree matches the pre-squash tip, then `git rebase <target branch>` and resolve\n\
             the conflicts faithfully."
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
