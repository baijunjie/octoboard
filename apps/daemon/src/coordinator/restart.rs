//! What the restarts of a session's process share: a switch of its account, a change of its model
//! or effort and a change of its account's sharing end the process and relaunch it, carrying over
//! what it started, and judge the relaunch by whether it stays up; and whether finding the
//! session's conversation record needs the shell snapshot.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::processes::ledger::HeldForLaunch;
use crate::protocol::{Session, SessionStatus};
use crate::state::AppState;

use super::launch::{came_up, stop_process};

/// How long a relaunched session has to stay up before a switch counts it as having come up. An
/// agent that cannot find the conversation it was asked to resume says so and exits within a
/// moment, each in its own words, which the switch does not read: its process ending is the signal,
/// as it is everywhere else the daemon decides a session is interrupted.
///
/// One failing relaunch could outlive the window: Grok Build restores a session it cannot find
/// locally from a remote registry, a network round trip. That is harmless here, because the
/// switch's own copy means Grok finds the session locally; and if a relaunch did fail after the
/// window, the session would be interrupted on the new account, where the copy is, and so
/// resumable.
///
/// An account that is not signed in is not caught here, for any of the three agents: each comes up
/// on its own sign-in instead of exiting — Grok Build included, which waits on a browser
/// device-code approval — so a switch to such an account counts as having come up.
const SWITCH_SETTLE: Duration = Duration::from_secs(4);

/// Ends the process of `id`, a session about to be restarted by a switch, a change of its model or
/// a change of its account's sharing, the way archiving ends it, with what the process started
/// carried over to the relaunch rather than stopped with it: held while the session has no process,
/// and, where the caller watches the relaunch coming up (a switch and a model change), while it is,
/// and given up by [`give_up_what_a_restart_held`].
/// Returns what is held, and whether this stopped a running process: one that had exited by itself
/// already, and is only waiting to be deregistered, is not the restart's doing.
pub(crate) async fn stop_carrying_processes(
    state: &Arc<AppState>,
    id: &str,
) -> Result<(HeldForLaunch, bool)> {
    let held = state.process_ledger.hold_for_launch(id);
    let ended_a_process = match state.live_session(id) {
        Some(live) => {
            state.process_ledger.hand_over(&live.launch_id, id);
            stop_process(state, live).await?
        }
        None => false,
    };
    Ok((held, ended_a_process))
}

/// Gives up what a restart still holds for the relaunch, once the relaunch is over: taken in by
/// it, or, when it `failed` after the restart had ended the session's process, kept (see
/// [`keep_what_a_failed_restart_carried`]). On a blocking task: keeping what is held writes the
/// ledger file.
pub(crate) async fn give_up_what_a_restart_held(
    state: &Arc<AppState>,
    id: &str,
    held: HeldForLaunch,
    failed: bool,
) {
    let (state_for_hold, session_id) = (state.clone(), id.to_string());
    let given_up = tokio::task::spawn_blocking(move || {
        if failed {
            keep_what_a_failed_restart_carried(&state_for_hold, &session_id, held);
        } else {
            drop(held);
        }
    })
    .await;
    if let Err(err) = given_up {
        tracing::warn!(session = %id, %err, "giving up what a restart held for the relaunch did not finish");
    }
}

/// Gives up what a restart (a switch, a model change or a change of an account's sharing) that
/// failed after it had ended the session's process still holds for the relaunch: kept for the
/// user, among the processes of the session's project, as a tree the user keeps is, when the
/// session is left interrupted in a project; stopped, when it is a console session, which keeps
/// nothing, or was archived meanwhile, which stops what it started, or when the daemon is shutting
/// down. A process that had exited by itself is not the restart's doing, so what it left is
/// stopped as for any agent that exits on its own: the caller does not come here. **Blocks.**
pub(super) fn keep_what_a_failed_restart_carried(
    state: &Arc<AppState>,
    id: &str,
    held: HeldForLaunch,
) {
    match state.session_record(id) {
        Ok(session) if session.status != SessionStatus::Archived => {
            if let Some(project) = session.project_id {
                held.keep_what_is_held(project);
            }
        }
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(session = %id, %err, "reading a session whose restart failed; what it carried is stopped");
        }
    }
}

/// Whether finding `session`'s conversation record needs the shell snapshot: the default account
/// has no directory of its own, which is resolved from the environment.
pub(crate) fn record_needs_shell_env(session: &Session) -> bool {
    session.config_dir.is_none()
}

/// Whether the process an account switch (or a restore after one) relaunched is still running after
/// [`SWITCH_SETTLE`]. One that is has been seen to come up, so its end from then on is the agent's
/// own and is told to the owner as any is.
pub(super) async fn came_up_under_claim(state: &Arc<AppState>, id: &str) -> bool {
    let Some(live) = state.live_session(id) else {
        return false;
    };
    let up = came_up(&live, SWITCH_SETTLE).await;
    if up {
        live.release_watch();
    }
    up
}
