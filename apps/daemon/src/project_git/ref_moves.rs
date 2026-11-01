//! Re-reading a repository's projects when a session's own git says it moved their refs, so the
//! badge follows a commit, merge, switch or fetch of an agent session at once rather than when the
//! platform gets round to reporting the files ([`super::watch`]) or the client next polls.
//!
//! A project session's `reference-transaction` hook dispatcher reports every committed
//! transaction that touches `HEAD`, a local branch or a remote-tracking branch
//! (`crate::session_git_hooks`), naming the repository it ran in. Every project whose repository
//! that is — the linked worktrees of one repository share its branches — is re-read with
//! `header::publish_reread`, the same local-only read the watch makes; nothing here goes to the
//! remote. Reports are coalesced per repository: one arriving while a read is waiting pushes it
//! back by [`DEBOUNCE`], up to [`MAX_DELAY`] after the first, and one arriving while a read runs
//! owes the repository one more read afterwards, so a rebase or a fetch of many refs costs a read
//! or two per project and reads of one repository never overlap. At most [`BLOCKING_WIDTH`]
//! repositories are read at once.
//!
//! Only git run under a session's hooks reports anything; the watch and the client's poll still
//! cover git run anywhere else, and a switch to an existing branch made by a git older than 2.46,
//! which runs no `reference-transaction` hook for an update of `HEAD` alone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use crate::blocking_pass::BLOCKING_WIDTH;
use crate::git::layout::is_git_repo;
use crate::git::repo_files::repository_of;
use crate::project_git::header;
use crate::state::AppState;

/// How long a repository's reports stay quiet before it is read: the transactions of one command
/// (a commit's, a rebase's picks) arrive in a burst, and one read answers them all.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// The longest a read is pushed back after the first report it answers, so a stream of
/// transactions (a long rebase, a script committing in a loop) still shows on the badge as it goes.
const MAX_DELAY: Duration = Duration::from_secs(2);

enum Phase {
    /// A read is due at `at`, pushed back by later reports but never past `latest`.
    Waiting { at: Instant, latest: Instant },
    /// A read is running; `again` once a report has arrived since it started.
    Reading { again: bool },
}

/// The repositories with a read due or running, by canonical common directory, and the places for
/// running reads.
pub struct RefMoves {
    phases: Mutex<HashMap<PathBuf, Phase>>,
    reads: tokio::sync::Semaphore,
}

impl Default for RefMoves {
    fn default() -> Self {
        Self {
            phases: Mutex::default(),
            reads: tokio::sync::Semaphore::new(BLOCKING_WIDTH),
        }
    }
}

impl RefMoves {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Phase>> {
        self.phases.lock().expect("ref moves lock poisoned")
    }
}

/// Takes a report that a session's git moved refs of `repository`, its canonical common directory,
/// and has its projects re-read once the reports settle. Must be called inside the runtime.
pub fn moved(state: &Arc<AppState>, repository: PathBuf) {
    let now = Instant::now();
    let mut phases = state.ref_moves.lock();
    match phases.get_mut(&repository) {
        Some(Phase::Waiting { at, latest }) => *at = (now + DEBOUNCE).min(*latest),
        Some(Phase::Reading { again }) => *again = true,
        None => {
            phases.insert(
                repository.clone(),
                Phase::Waiting {
                    at: now + DEBOUNCE,
                    latest: now + MAX_DELAY,
                },
            );
            tokio::spawn(settle(state.clone(), repository));
        }
    }
}

/// Waits out the repository's reports, reads its projects, and goes round again while reports
/// arrived during the read.
async fn settle(state: Arc<AppState>, repository: PathBuf) {
    loop {
        loop {
            let at = match state.ref_moves.lock().get(&repository) {
                Some(Phase::Waiting { at, .. }) => *at,
                _ => return,
            };
            if Instant::now() >= at {
                break;
            }
            tokio::time::sleep_until(at).await;
        }
        // A report that arrives while the read waits for a place only moves `at`: the read that
        // follows sees it.
        let place = state
            .ref_moves
            .reads
            .acquire()
            .await
            .expect("the read places are never closed");
        state
            .ref_moves
            .lock()
            .insert(repository.clone(), Phase::Reading { again: false });
        let read = {
            let state = state.clone();
            let repository = repository.clone();
            tokio::task::spawn_blocking(move || reread(&state, &repository))
        };
        if let Err(err) = read.await {
            tracing::warn!(%err, "re-reading a repository's projects failed");
        }
        drop(place);
        let mut phases = state.ref_moves.lock();
        if let Some(Phase::Reading { again: true }) = phases.get(&repository) {
            let now = Instant::now();
            phases.insert(
                repository.clone(),
                Phase::Waiting {
                    at: now + DEBOUNCE,
                    latest: now + MAX_DELAY,
                },
            );
        } else {
            phases.remove(&repository);
            return;
        }
    }
}

/// Re-reads every project of `repository`. A project whose `.git` is gone is left alone, as the
/// watch leaves it: `git status` would walk up to an enclosing repository.
fn reread(state: &AppState, repository: &Path) {
    let projects = match state.store.list_projects() {
        Ok(projects) => projects,
        Err(err) => {
            tracing::warn!(%err, "listing projects to re-read their git status failed");
            return;
        }
    };
    for project in projects {
        let dir = Path::new(&project.path);
        if is_git_repo(dir) && repository_of(dir).as_deref() == Some(repository) {
            header::publish_reread(state, &project);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Event;
    use crate::test_support::{app_state, insert_project, repo_with, PATIENCE};

    #[tokio::test]
    async fn a_burst_of_reports_reads_the_project_once() {
        let (state, dir) = app_state("ref-moves-burst");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        let repo = repo_with("ref-moves-burst-repo", &[(b"a", b"a")]);
        insert_project(&state, &repo);
        let repository = repository_of(&repo).unwrap();
        let mut events = state.subscribe();

        for _ in 0..20 {
            moved(&state, repository.clone());
        }

        let reread = |event: Event| match event {
            Event::ProjectGitStatus { status } => status.project == "p1",
            _ => false,
        };
        let first = tokio::time::timeout(PATIENCE, async {
            loop {
                if reread(events.recv().await.unwrap()) {
                    break;
                }
            }
        })
        .await;
        assert!(first.is_ok(), "the project is re-read");
        let second = tokio::time::timeout(DEBOUNCE * 3, async {
            loop {
                if reread(events.recv().await.unwrap()) {
                    break;
                }
            }
        })
        .await;
        assert!(second.is_err(), "the burst costs one read");
        assert!(state.ref_moves.lock().is_empty());
    }
}
