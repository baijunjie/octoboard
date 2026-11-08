//! Removing what is bound to sessions once their processes have ended.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::report::lines;
use super::survey::{exists, failed, survey, BranchSurvey, WorktreeRemovability, WorktreeSurvey};
use crate::branch_origin::bound::{bound_in, Bound};
use crate::git::run::Runner;
use crate::git::worktrees::worktrees;
use crate::host_tools::Pace;
use crate::platform::names;
use crate::protocol::{
    Session, SessionBranchItem, SessionBranchKind, SessionBranchState, SessionBranchTarget,
    SessionStatus,
};
use crate::state::AppState;

/// How long removal waits for the sessions' processes to end before going ahead anyway. Far above
/// the graceful period an ending process gets; past it, a process that would not die is left to
/// itself, and an unforced removal still refuses a worktree with changes.
pub(super) const STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// Removes, once none of `sessions` has a process any more, what can go of what is bound to those
/// of them still archived or deleted by then, together with what `forced` names, and returns what
/// is left, each item with the reason. What cannot be done at all — no `git` to run, the removal
/// not finishing — leaves every bound item as `failed`; only a store that cannot be read leaves
/// nothing to name, and is logged.
pub async fn remove_once_stopped(
    state: Arc<AppState>,
    sessions: Vec<Session>,
    forced: Vec<SessionBranchTarget>,
) -> Vec<SessionBranchItem> {
    let ids: Vec<String> = sessions.iter().map(|session| session.id.clone()).collect();
    wait_until_stopped(&state, &ids).await;
    let reading = Arc::clone(&state);
    let found = tokio::task::spawn_blocking(move || {
        let ended: Vec<Session> = sessions
            .into_iter()
            .filter(|session| has_ended(&reading, &session.id))
            .collect();
        bound_in(&reading, &ended)
    })
    .await;
    let bound = match found {
        Ok(Ok(bound)) => bound,
        Ok(Err(err)) => {
            tracing::warn!(%err, "reading what archived sessions are bound to failed");
            return Vec::new();
        }
        Err(err) => {
            tracing::warn!(%err, "reading what archived sessions are bound to did not finish");
            return Vec::new();
        }
    };
    if bound.is_empty() {
        return Vec::new();
    }
    let unfinished = bound.clone();
    let removed = tokio::task::spawn_blocking(move || {
        let git = match super::cleanup_git(&state.host_tools.get(), Pace::Scheduled) {
            Ok(git) => git,
            Err(err) => return all_failed(&bound, &format!("{err:#}")),
        };
        let mut judge = match WorktreeRemovability::of_projects(&state, None) {
            Ok(judge) => judge,
            Err(err) => return all_failed(&bound, &format!("{err:#}")),
        };
        let mut left = Vec::new();
        for repo in &bound {
            left.extend(state.branch_cleanups.serialized(&repo.repo, || {
                remove_in(&state, &git, repo, &mut judge, &forced)
            }));
        }
        left
    })
    .await;
    removed.unwrap_or_else(|err| {
        all_failed(&unfinished, &format!("the removal did not finish: {err}"))
    })
}

/// Whether the session `id` is archived, or gone from the store: one resumed or reopened since is
/// neither, and what is bound to it is its own again.
fn has_ended(state: &AppState, id: &str) -> bool {
    match state.store.get_session(id) {
        Ok(Some(session)) => session.status == SessionStatus::Archived,
        Ok(None) => true,
        Err(_) => false,
    }
}

/// Waits until none of the sessions `ids` that are still archived or deleted has a process, or
/// [`STOP_TIMEOUT`] has passed.
pub(super) async fn wait_until_stopped(state: &Arc<AppState>, ids: &[String]) {
    let deadline = Instant::now() + STOP_TIMEOUT;
    loop {
        let running: Vec<String> = ids
            .iter()
            .filter(|id| state.has_process(id))
            .cloned()
            .collect();
        if running.is_empty() {
            return;
        }
        // Whether they still count is a store read, so it goes to the blocking pool.
        let reading = Arc::clone(state);
        let waited_on =
            tokio::task::spawn_blocking(move || running.iter().any(|id| has_ended(&reading, id)))
                .await
                .unwrap_or(false);
        if !waited_on {
            return;
        }
        if Instant::now() >= deadline {
            tracing::warn!("removing the branches of sessions whose process is still up");
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// [`remove_once_stopped`] with nothing forced, logging what is left rather than telling any
/// client: for an archive or a deletion no one waits on the removal of.
pub async fn remove_logging_left(state: Arc<AppState>, sessions: Vec<Session>) {
    let left = remove_once_stopped(state, sessions, Vec::new()).await;
    if !left.is_empty() {
        tracing::info!(left = ?lines(&left), "branches of an archived or deleted session were left");
    }
}

/// [`remove_logging_left`] off the caller's path.
pub fn remove_in_background(state: &Arc<AppState>, sessions: Vec<Session>) {
    if sessions.is_empty() {
        return;
    }
    tokio::spawn(remove_logging_left(Arc::clone(state), sessions));
}

pub(super) fn all_failed(bound: &[Bound], detail: &str) -> Vec<SessionBranchItem> {
    bound
        .iter()
        .flat_map(|repo| repo.all_failed(detail))
        .collect()
}

/// Whether `target` names `item`.
pub(super) fn names(target: &SessionBranchTarget, item: &SessionBranchItem) -> bool {
    target.kind == item.kind
        && target.repository == item.repository
        && match item.kind {
            SessionBranchKind::Branch => target.branch == item.branch,
            SessionBranchKind::Worktree => target.path == item.path,
        }
}

/// Whether `target` names `item` as it stands now: a worktree in the same state, without changes
/// the user was not shown; a branch in the same state at the same commit, so that commits made
/// after the user confirmed are not deleted with it, or merged by now, which takes no force.
pub(super) fn consents(target: &SessionBranchTarget, item: &SessionBranchItem) -> bool {
    names(target, item)
        && match item.kind {
            SessionBranchKind::Branch => {
                (target.state == item.state && target.tip == item.tip)
                    || item.state == SessionBranchState::Removable
            }
            SessionBranchKind::Worktree => {
                target.state == item.state && (!item.uncommitted || target.uncommitted)
            }
        }
}

/// The branches some worktree of the repository has checked out now.
fn held_branches(git: &Runner, repo: &Path) -> Result<HashSet<Vec<u8>>, String> {
    Ok(worktrees(git, repo)
        .map_err(|err| err.to_string())?
        .into_iter()
        .filter_map(|worktree| worktree.branch)
        .collect())
}

pub(super) fn remove_in(
    state: &AppState,
    git: &Runner,
    bound: &Bound,
    judge: &mut WorktreeRemovability,
    forced: &[SessionBranchTarget],
) -> Vec<SessionBranchItem> {
    let survey = match survey(state, git, bound, judge) {
        Ok(survey) => survey,
        Err(detail) => return bound.all_failed(&detail),
    };
    let consented = |item: &SessionBranchItem| forced.iter().any(|target| consents(target, item));
    let mut left = Vec::new();
    for WorktreeSurvey { item, root } in survey.worktrees {
        // A locked worktree takes the force twice, as git asks for it.
        let force: &[&OsStr] = match item.state {
            SessionBranchState::Removable => &[],
            SessionBranchState::UncommittedChanges | SessionBranchState::Submodules
                if consented(&item) =>
            {
                &[OsStr::new("--force")]
            }
            SessionBranchState::Locked if consented(&item) => {
                &[OsStr::new("--force"), OsStr::new("--force")]
            }
            _ => {
                left.push(item);
                continue;
            }
        };
        let mut args = vec![OsStr::new("worktree"), OsStr::new("remove")];
        args.extend(force);
        args.extend([OsStr::new("--"), root.as_os_str()]);
        if let Err(detail) = git.write(&bound.repo, &args) {
            let still_there = worktrees(git, &bound.repo).map_or(true, |listed| {
                listed.iter().any(|worktree| worktree.root == root)
            });
            if still_there {
                left.push(failed(item, detail));
            }
        }
    }

    // Which branches are still checked out somewhere, now that the worktrees that could go have.
    let held = match held_branches(git, &bound.repo) {
        Ok(held) => held,
        Err(detail) => {
            left.extend(
                survey
                    .branches
                    .into_iter()
                    .map(|branch| failed(branch.item, detail.clone())),
            );
            return left;
        }
    };
    for BranchSurvey {
        mut item,
        name,
        merged,
    } in survey.branches
    {
        // A branch a worktree that stays has checked out is kept for that, merged or not.
        let flag = if held.contains(&name) {
            item.state = SessionBranchState::CheckedOut;
            left.push(item);
            continue;
        } else if !merged && consented(&item) {
            "-D"
        } else if merged {
            "-d"
        } else {
            item.state = SessionBranchState::Unmerged;
            left.push(item);
            continue;
        };
        let Some(branch) = names::name_from_bytes(&name) else {
            left.push(failed(
                item,
                "the branch name is not one this host can pass to git",
            ));
            continue;
        };
        let args = [
            OsStr::new("branch"),
            OsStr::new(flag),
            OsStr::new("--"),
            &branch,
        ];
        if let Err(detail) = git.write(&bound.repo, &args) {
            if exists(git, &bound.repo, &name) {
                left.push(failed(item, detail));
            }
        }
    }
    left
}
