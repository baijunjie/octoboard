//! Pulling a project's current branch when the user asks (`pull_project_branch`), and telling them
//! what it did.
//!
//! A manual pull is never silent: it answers with a [`PullOutcome`] or refuses with a code and
//! `git`'s own message, so the person is told why nothing happened. Without `merge` the pull is
//! the project's ordinary check ([`Asked::Pull`]), whose fetch must have worked, followed by a
//! fast-forward where the branch has no commits of its own; a branch that has diverged is
//! reported (`needs_merge`) for the person to confirm. With `merge`, the confirmed request,
//! nothing is fetched and the diverged branch has its upstream merged in, the merge aborted if it
//! meets conflicts.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Error;
use tokio::time::Instant;

use crate::git::env::SshConfiguration;
use crate::git::layout::{git_dir, is_git_repo};
use crate::git::run::Runner;
use crate::host_tools::Pace;
use crate::hostfs;
use crate::project_git::header::{self, read_branch_header, BranchHeader};
use crate::project_git::status::{
    merge_upstream, run_checks, Asked, FAST_FORWARD, MANUAL_ANSWER_WAIT,
};
use crate::project_git::{no_git_or, not_a_repository, project_git};
use crate::protocol::{error_code, CodedError, GitActivity, GitStatus, Project, PullOutcome};
use crate::state::{blank_git_status, AppState, PullAnswer};

/// The most a `MERGE_HEAD` may hold: an object id and its newline.
const MERGE_HEAD_LIMIT: usize = 128;

/// How often a confirmed merge looks again for the claim of a project whose check holds it.
const CLAIM_POLL: Duration = Duration::from_millis(100);

/// One project's pull, started right now at the user's request: ignores the one-minute floor
/// between checks and the **Automatically pull repositories** setting — asking for it is the
/// consent the setting would otherwise give — and answers with what it did, or the refusal.
/// Without `merge` it is the project's ordinary check ([`Asked::Pull`]): a project already running
/// its check gets one more check once that one ends, and one still waiting for a place in a sweep,
/// or in the settings-triggered `pull_behind_projects` pass, runs it at once in its stead, so the
/// answer always comes from a fetch that started after the request. It waits at most
/// `MANUAL_ANSWER_WAIT`. With `merge` it is [`merge_project_now`].
pub async fn pull_project_now(state: &Arc<AppState>, project: Project, merge: bool) -> PullAnswer {
    if merge {
        return merge_project_now(state, project).await;
    }
    // One project at the user's request, so it does not wait for a place behind a sweep.
    let (claim, answered) = state.claim_git_pull(&project.id);
    if let Some(claim) = claim {
        let state = state.clone();
        tokio::task::spawn_blocking(move || run_checks(&state, project, claim, Asked::Pull));
    }
    match tokio::time::timeout(MANUAL_ANSWER_WAIT, answered).await {
        Ok(Ok(answer)) => answer,
        Ok(Err(_)) => Err(refused(pull_failed(
            "the check was abandoned before it ended, its project may have been removed.",
        ))),
        Err(_) => Err(refused(pull_failed(
            "the check had not ended in time, it goes on running.",
        ))),
    }
}

/// The pull of a `pull_project_branch` the person confirmed with `merge`: no fetch, since the
/// confirmation was asked on the counts of one just made. It takes the project's claim, so it never
/// runs alongside a check of the same project (two `git` writers on one working tree), and waits
/// for it when a check holds it: the claim is tried again every [`CLAIM_POLL`] up to
/// `MANUAL_ANSWER_WAIT`, which is simpler than a second queue beside the claim's and costs nothing
/// the fetch it waits behind does not. A project whose check is waiting for a sweep's place holds
/// the claim as well, so the merge waits for that check's turn and run. The claim is one that
/// starts at once (`AppState::claim_git_check_now`): nobody waits for a place behind it, a request
/// that arrives meanwhile is owed a check, and the release hands that check back to run. The
/// deadline is on the runtime's clock, so a test can run it on a paused one.
async fn merge_project_now(state: &Arc<AppState>, project: Project) -> PullAnswer {
    let deadline = Instant::now() + MANUAL_ANSWER_WAIT;
    let claim = loop {
        if let Some(claim) = state.claim_git_check_now(&project.id) {
            break claim;
        }
        if Instant::now() >= deadline {
            return Err(refused(pull_failed(
                "a check of the project had not ended in time, it goes on running.",
            )));
        }
        tokio::time::sleep(CLAIM_POLL).await;
    };
    let worker = state.clone();
    let id = project.id.clone();
    let merged = tokio::task::spawn_blocking(move || {
        let answer = merge_pull(&worker, &project);
        (answer, claim)
    })
    .await;
    let Ok((answer, mut claim)) = merged else {
        return Err(refused(pull_failed("the pull stopped unexpectedly.")));
    };
    // What arrived meanwhile is run after the answer is sent, not before it.
    if let Some(owed) = claim.release_unless_rerun_owed() {
        if let Ok(Some(fresh)) = state.store.get_project(&id) {
            let state = state.clone();
            tokio::task::spawn_blocking(move || {
                run_checks(&state, fresh, claim, Asked::owed(owed))
            });
        }
    }
    answer
}

/// What a pull does about a branch, from its header and whether the request allows a merge.
#[derive(Debug, PartialEq, Eq)]
enum PullPlan {
    UpToDate,
    FastForward,
    NeedsMerge { ahead: u32, behind: u32 },
    Merge,
}

/// The pull `header` allows, or why there is none. Pure over the two, so the choice is unit-tested
/// without a repository. A branch that is not behind its upstream is up to date, whether or not it
/// is ahead; one with commits of its own and commits to take needs a merge. A branch with no commit
/// yet has nothing to be ahead or behind with, so the fast-forward is left to `git`, which takes
/// the upstream's commit onto it or says why it cannot.
fn plan_pull(header: &BranchHeader, merge: bool) -> Result<PullPlan, Error> {
    if header.detached {
        return Err(CodedError::raised(
            error_code::NOT_ON_A_BRANCH,
            "This project is not on a branch, so there is nothing to pull.",
            &[],
        ));
    }
    if header.upstream.is_none() {
        return Err(CodedError::raised(
            error_code::NO_UPSTREAM_TO_PULL,
            "This branch has no upstream to pull from.",
            &[],
        ));
    }
    if header.upstream_gone {
        return Err(CodedError::raised(
            error_code::NO_UPSTREAM_TO_PULL,
            "This branch's upstream branch no longer exists, so there is nothing to pull.",
            &[],
        ));
    }
    Ok(if header.branch.is_none() {
        PullPlan::FastForward
    } else if header.behind == 0 {
        PullPlan::UpToDate
    } else if header.ahead == 0 {
        PullPlan::FastForward
    } else if merge {
        PullPlan::Merge
    } else {
        PullPlan::NeedsMerge {
            ahead: header.ahead,
            behind: header.behind,
        }
    })
}

/// What a check run for a pull read, for the pull: the local branch header, or why it could not be
/// read, and why the fetch failed when it did. Kept apart, since the badge's one `error` holds
/// whichever step failed last.
pub(crate) struct CheckRead {
    pub(crate) header: Result<BranchHeader, String>,
    pub(crate) fetch_error: Option<String>,
}

/// The manual pull of a check, `status` being the check's own. A header that could not be read is
/// a failed pull, and a branch that is detached or has no upstream is refused as such, before a
/// failed fetch is: that is what the person has to fix first. A fetch that failed pulls nothing, so
/// a branch is never merged onto refs that are stale for a reason the person is told.
pub(crate) fn pull_checked(
    state: &AppState,
    git: &Runner,
    path: &Path,
    status: &mut GitStatus,
    ssh: &SshConfiguration,
    read: CheckRead,
) -> PullAnswer {
    let header = read
        .header
        .map_err(|detail| refused(pull_failed(&detail)))?;
    let plan = plan_pull(&header, false).map_err(refused)?;
    if let Some(detail) = read.fetch_error {
        return Err(refused(pull_failed(&detail)));
    }
    carry_out(state, git, path, status, ssh, plan).map_err(refused)
}

/// The pull of a `merge` request, with the claim held: reads the branch header again (no fetch),
/// pulls what it allows, and publishes the status that results from a local read. Nothing is
/// recorded on the badge's `error`: the refused request is the report, and the badge keeps showing
/// what the last check found.
fn merge_pull(state: &AppState, project: &Project) -> PullAnswer {
    let project = state
        .store
        .require_project(&project.id)
        .map_err(|err| refused(err.into()))?;
    let path = PathBuf::from(&project.path);
    if !is_git_repo(&path) {
        return Err(refused(not_a_repository(&project)));
    }
    let git =
        project_git(state, Pace::Person).map_err(|err| refused(no_git_or(err, pull_failed)))?;
    let header =
        header::read_header(&git, &path).map_err(|detail| refused(pull_failed(&detail)))?;
    let plan = plan_pull(&header, true).map_err(refused)?;
    let mut status = state
        .git_status(&project.id)
        .filter(|status| status.repository)
        .unwrap_or_else(|| blank_git_status(&project.id));
    header::apply_header(&mut status, header);
    let ssh = git.git().ssh_configuration(&path);
    let outcome = carry_out(state, &git, &path, &mut status, &ssh, plan);
    status.activity = GitActivity::Idle;
    // The check's completion time is not recorded: nothing here went to the remote, so the next
    // sweep's own check must not be held off on this project's account.
    state.publish_git_status(status);
    outcome.map_err(refused)
}

/// Does `plan` in the repository at `path`, with `status` showing the pull in flight while `git`
/// changes the branch and the branch header read again afterwards, whether the change worked or
/// not. A change `git` refuses fails with its message as `git_pull_failed`, and a merge that meets
/// conflicts is aborted ([`merge_failed`]).
fn carry_out(
    state: &AppState,
    git: &Runner,
    path: &Path,
    status: &mut GitStatus,
    ssh: &SshConfiguration,
    plan: PullPlan,
) -> Result<PullOutcome, Error> {
    let outcome = match plan {
        PullPlan::UpToDate => return Ok(PullOutcome::UpToDate),
        PullPlan::NeedsMerge { ahead, behind } => {
            return Ok(PullOutcome::NeedsMerge { ahead, behind })
        }
        PullPlan::FastForward => merge_upstream(state, git, path, status, ssh, &FAST_FORWARD)
            .map(|()| PullOutcome::FastForwarded)
            .map_err(|detail| pull_failed(&detail)),
        PullPlan::Merge => merge_diverged(state, git, path, status, ssh),
    };
    read_branch_header(git, path, status);
    outcome
}

/// Merges the upstream into the branch with a merge commit. `git merge` is told what it is to do
/// (`--no-ff --no-squash --commit --no-log`) so that `merge.ff` and `branch.<name>.mergeOptions`
/// cannot turn the merge the person confirmed into a refusal, a squash, a merge left uncommitted
/// or a message with a second shortlog. It keeps `git`'s own cleanup of the message, so
/// `commit.cleanup` and what a `prepare-commit-msg` hook writes are as for any merge. It is given
/// the upstream's commit, read first, rather than `@{upstream}`, so that the merge aborted after a
/// failure is known to be this one ([`merge_failed`]). A commit names no branch, so `git` is given
/// the message it would have written for the upstream branch ([`merge_message`]). A merge already
/// under way is refused and left alone, as is one whose state cannot be read.
fn merge_diverged(
    state: &AppState,
    git: &Runner,
    path: &Path,
    status: &mut GitStatus,
    ssh: &SshConfiguration,
) -> Result<PullOutcome, Error> {
    match merge_head(path) {
        Ok(None) => {}
        Ok(Some(_)) => {
            return Err(pull_failed(
                "a merge is already in progress in this project.",
            ))
        }
        Err(detail) => {
            return Err(pull_failed(&format!(
                "whether a merge is in progress could not be read: {detail}"
            )))
        }
    }
    // `@{upstream}` rather than the upstream's short name, which a local branch of the same name
    // (`origin/main`) would shadow.
    let upstream = git
        .read(
            path,
            ["rev-parse", "--verify", "-q", "@{upstream}^{commit}"],
        )
        .map(|output| String::from_utf8_lossy(&output).trim().to_owned())
        .map_err(|err| pull_failed(&err.to_string()))?;
    let message = merge_message(git, path, &upstream).map_err(|detail| pull_failed(&detail))?;
    let args = [
        "merge",
        "--no-edit",
        "--no-ff",
        "--no-squash",
        "--commit",
        "--no-log",
        "-m",
        &message,
        &upstream,
    ];
    merge_upstream(state, git, path, status, ssh, &args)
        .map(|()| PullOutcome::Merged)
        .map_err(|detail| merge_failed(git, path, &detail, &upstream))
}

/// The message of the merge of the upstream, `upstream` being its commit: the one `git` itself
/// words for merging that branch into the current one (`git fmt-merge-msg`), with the destination
/// left out where `git` leaves it out and a local upstream worded as a branch, not a
/// remote-tracking one. `git` is given the line `git merge` gives it for the branch, in a
/// temporary file, since the runner has no standard input. When `merge.log` asks for a shortlog,
/// this message already holds it, so the merge is told not to add its own. `git fmt-merge-msg` also
/// prints comment lines crediting the authors of the merged commits, which a merge writes only
/// when it opens an editor; they are left out. They are found by running it with `#` as the comment
/// character, whatever the repository sets, since no other line of its output starts with one.
fn merge_message(git: &Runner, path: &Path, upstream: &str) -> Result<String, String> {
    let named = git
        .read(path, ["rev-parse", "--symbolic-full-name", "@{upstream}"])
        .map_err(|err| err.to_string())?;
    let named = String::from_utf8_lossy(&named);
    let named = named.trim();
    let described = if let Some(branch) = named.strip_prefix("refs/heads/") {
        format!("branch '{branch}' of .")
    } else if let Some(branch) = named.strip_prefix("refs/remotes/") {
        format!("remote-tracking branch '{branch}' of .")
    } else {
        return Err(format!("the upstream {named} is not a branch"));
    };
    let input = std::env::temp_dir().join(format!("octoboard-merge-{}", uuid::Uuid::new_v4()));
    std::fs::write(&input, format!("{upstream}\t\t{described}\n"))
        .map_err(|err| format!("{}: {err}", input.display()))?;
    let message = git.read(
        path,
        [
            OsStr::new("-c"),
            OsStr::new("core.commentChar=#"),
            OsStr::new("fmt-merge-msg"),
            OsStr::new("-F"),
            input.as_os_str(),
        ],
    );
    // A file that cannot be removed is left in the temporary directory.
    let _ = std::fs::remove_file(&input);
    message
        .map(|output| {
            let output = String::from_utf8_lossy(&output);
            let kept: Vec<&str> = output
                .lines()
                .filter(|line| !line.starts_with('#'))
                .collect();
            kept.join("\n").trim_end().to_owned()
        })
        .map_err(|err| err.to_string())
}

/// The commit a merge under way in the repository at `path` is merging, `None` when there is no
/// merge under way: what the worktree's `MERGE_HEAD` holds, found through its git directory, which
/// is not under `path` in a linked worktree. A failure to find or read it is the error, never a
/// guess that no merge runs.
fn merge_head(path: &Path) -> Result<Option<String>, String> {
    let git_dir = git_dir(path).ok_or_else(|| "the git directory could not be found".to_owned())?;
    let file = git_dir.join("MERGE_HEAD");
    match hostfs::read_regular(&file, MERGE_HEAD_LIMIT) {
        Ok(oid) => Ok(Some(String::from_utf8_lossy(&oid).trim().to_owned())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("{}: {err}", file.display())),
    }
}

/// The refusal of the merge of `upstream` that `git` failed with `detail`. A merge left under way
/// that is this one (its `MERGE_HEAD` is `upstream`), with unmerged paths, is conflicts: the paths
/// are read, the merge is aborted so the working tree is as it was before the pull, and the refusal
/// names the paths (`pull_merge_conflict`). Any other failure (no committer identity, a hook) is
/// `git_pull_failed`, with a merge of this one it left under way aborted all the same. A merge that
/// is not this one is never aborted, whoever began it. An abort that fails is `git_pull_failed`
/// carrying both messages, the tree being then in the state `git` left it, as is a merge state that
/// cannot be read.
fn merge_failed(git: &Runner, path: &Path, detail: &str, upstream: &str) -> Error {
    let ours = match merge_head(path) {
        Ok(head) => head.as_deref() == Some(upstream),
        Err(unread) => {
            return pull_failed(&format!(
                "{detail}\nWhether the merge was left under way could not be read: {unread}"
            ))
        }
    };
    if !ours {
        return pull_failed(detail);
    }
    let conflicted: Vec<String> = git
        .read(
            path,
            [
                "diff",
                "--ignore-submodules=dirty",
                "--name-only",
                "-z",
                "--diff-filter=U",
            ],
        )
        .map(|output| {
            String::from_utf8_lossy(&output)
                .split('\0')
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if let Err(abort) = git.write(path, ["merge", "--abort"]) {
        return pull_failed(&format!(
            "{detail}\nThe merge could not be aborted: {abort}"
        ));
    }
    if conflicted.is_empty() {
        return pull_failed(detail);
    }
    let paths = conflicted.join("\n");
    CodedError::raised(
        error_code::PULL_MERGE_CONFLICT,
        format!(
            "Merging the upstream into this branch met conflicts in {}, so the merge was aborted.",
            conflicted.join(", ")
        ),
        &[("detail", &paths)],
    )
}

fn pull_failed(detail: &str) -> Error {
    CodedError::raised(
        error_code::GIT_PULL_FAILED,
        format!("Git could not pull this branch: {detail}"),
        &[("detail", detail)],
    )
}

/// The refusal of a pull whose check found no `git`: `binary_not_found` kept as it is, any other
/// failure to have one the pull's own.
pub(crate) fn no_git(err: Error) -> CodedError {
    refused(no_git_or(err, pull_failed))
}

/// `err`, a refusal built by this module's helpers, as the answer's error.
fn refused(err: Error) -> CodedError {
    CodedError::of(&err)
}

#[cfg(test)]
mod tests;
