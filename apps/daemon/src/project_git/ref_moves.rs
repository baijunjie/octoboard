//! Re-reading a repository's projects when a session's own git says it moved their refs, so the
//! badge follows a commit, merge, switch or fetch of an agent session at once rather than when the
//! platform gets round to reporting the files ([`super::watch`]) or the client next polls.
//!
//! A project session's `reference-transaction` hook dispatcher reports every committed
//! transaction that touches `HEAD`, a local branch or a remote-tracking branch
//! (`crate::session_git_hooks`), naming the repository it ran in. Every project whose repository
//! that is — the linked worktrees of one repository share its branches — is re-read with
//! `header::publish_reread`, the same local-only read the watch makes; nothing here goes to the
//! remote. Reports are coalesced per repository
//! ([`super::debounced`]): one arriving while a read is waiting pushes it back by [`DEBOUNCE`], up
//! to [`MAX_DELAY`] after the first, so a rebase or a fetch of many refs costs a read or two per
//! project.
//!
//! Only git run under a session's hooks reports anything; the watch and the client's poll still
//! cover git run anywhere else, and a switch to an existing branch made by a git older than 2.46,
//! which runs no `reference-transaction` hook for an update of `HEAD` alone.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::git::layout::is_git_repo;
use crate::project_git::debounced::{Debounced, Timing};
use crate::project_git::header;
use crate::state::AppState;

/// How long a repository's reports stay quiet before it is read: the transactions of one command
/// (a commit's, a rebase's picks) arrive in a burst, and one read answers them all.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// The longest a read is pushed back after the first report it answers, so a stream of
/// transactions (a long rebase, a script committing in a loop) still shows on the badge as it goes.
const MAX_DELAY: Duration = Duration::from_secs(2);

/// The repositories with a read due or running, by canonical common directory.
pub struct RefMoves {
    scheduler: Arc<Debounced<PathBuf>>,
}

impl Default for RefMoves {
    fn default() -> Self {
        Self {
            scheduler: Arc::new(Debounced::new(Timing {
                debounce: DEBOUNCE,
                max_delay: Some(MAX_DELAY),
                owed: DEBOUNCE,
            })),
        }
    }
}

/// Takes a report that a session's git moved refs of `repository`, its canonical common directory,
/// and has its projects re-read once the reports settle. Must be called inside the runtime.
pub fn moved(state: &Arc<AppState>, repository: PathBuf) {
    state.ref_moves.scheduler.report(repository.clone(), || {
        let state = state.clone();
        move || reread(&state, &repository)
    });
}

/// Re-reads every project of `repository`. A project whose `.git` is gone is left alone, as the
/// watch leaves it: `git status` would walk up to an enclosing repository.
fn reread(state: &AppState, repository: &Path) {
    let projects = match super::projects_in_repository(state, repository) {
        Ok(projects) => projects,
        Err(err) => {
            tracing::warn!(%err, "listing projects to re-read their git status failed");
            return;
        }
    };
    for project in projects {
        if is_git_repo(Path::new(&project.path)) {
            header::publish_reread(state, &project);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo_files::repository_of;
    use crate::protocol::Event;
    use crate::test_support::{app_state, insert_project, repo_with, PATIENCE};

    #[tokio::test(start_paused = true)]
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
        // On the paused clock the reads hold the clock, so every read the burst set off has run
        // when this times out.
        let second = tokio::time::timeout(DEBOUNCE * 3, async {
            loop {
                if reread(events.recv().await.unwrap()) {
                    break;
                }
            }
        })
        .await;
        assert!(second.is_err(), "the burst costs one read");
        assert!(state.ref_moves.scheduler.is_idle());
    }
}
