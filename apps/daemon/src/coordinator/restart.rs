//! What the restarts of a session's process share: a switch of its account, a change of its model
//! or effort and a change of its account's sharing end the process and relaunch it, carrying over
//! what it started, and judge the relaunch by whether it stays up; and whether finding the
//! session's conversation record needs the shell snapshot. [`run`] drives the stop, the wait for
//! the process to be down and the give-up; what happens between is the caller's.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::accounts::choice::account_dir;
use crate::accounts::relocate;
use crate::messaging::delivery::{write_message, WhenBlocked};
use crate::messaging::texts;
use crate::processes::ledger::HeldForLaunch;
use crate::protocol::{AgentId, ProcessTree, Session, SessionStatus};
use crate::records::switch_continue;
use crate::state::AppState;

use super::launch::{
    came_up, stop_process_listing_trees, wait_until_down_within, SWITCH_STOP_TIMEOUT,
};
use super::resume::resumable_agent_session_id;

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
/// An account that is not signed in is not caught here, for any of the registered providers: each
/// comes up on its own sign-in instead of exiting — Grok Build included, which waits on a browser
/// device-code approval — so a switch to such an account counts as having come up.
const SWITCH_SETTLE: Duration = Duration::from_secs(4);

/// How long a restart waits for the process it ended to be recorded down: [`SWITCH_STOP_TIMEOUT`],
/// shortened by a test.
#[cfg_attr(not(test), allow(unused_variables))]
fn stop_timeout(state: &AppState) -> Duration {
    #[cfg(test)]
    if let Some(shorter) = *state.restart_stop_timeout.lock().unwrap() {
        return shorter;
    }
    SWITCH_STOP_TIMEOUT
}

/// Waits for `id`, a session a restart ended, to be recorded down, for [`stop_timeout`].
pub(super) async fn wait_until_stopped(state: &Arc<AppState>, id: &str) -> Result<()> {
    wait_until_down_within(state, id, stop_timeout(state)).await
}

/// Why [`run`] failed: the error, and whether the restart had ended the session's process and seen
/// it down by then, which leaves the session interrupted and not as it was. A process that did not
/// stop in time, or one that had exited by itself before the stop, is not "ended" by the restart.
#[derive(Debug)]
pub(crate) struct Failed {
    pub error: anyhow::Error,
    pub ended_process: bool,
}

impl<E: Into<anyhow::Error>> From<E> for Failed {
    fn from(error: E) -> Self {
        Self {
            error: error.into(),
            ended_process: false,
        }
    }
}

/// Restarts `id`, a session whose claim the caller holds: ends its process with what it started
/// carried over ([`stop_carrying_processes`]), waits for it to be recorded down, runs `after_stop`
/// (told whether this stopped a running process, and the trees it had running that the restart
/// ended: see [`trees_ended`]) and gives up what was held for the relaunch
/// ([`give_up_what_a_restart_held`]), kept when `after_stop` failed after the process had been
/// ended. `did_not_stop` words the failure of the wait for the process to be down.
pub(crate) async fn run<T, Fut>(
    state: &Arc<AppState>,
    id: &str,
    did_not_stop: impl FnOnce(anyhow::Error) -> anyhow::Error,
    after_stop: impl FnOnce(bool, Vec<ProcessTree>) -> Fut,
) -> Result<T, Failed>
where
    Fut: Future<Output = Result<T>>,
{
    let not_ended = |error| Failed {
        error,
        ended_process: false,
    };
    let (held, ended_a_process, running) = stop_carrying_processes(state, id)
        .await
        .map_err(not_ended)?;
    if let Err(err) = wait_until_stopped(state, id).await {
        return Err(not_ended(did_not_stop(err)));
    }
    let ended = if ended_a_process {
        trees_ended(state, id, running).await
    } else {
        Vec::new()
    };
    let restarted = after_stop(ended_a_process, ended).await;
    give_up_what_a_restart_held(state, id, held, restarted.is_err() && ended_a_process).await;
    restarted.map_err(|error| Failed {
        error,
        ended_process: ended_a_process,
    })
}

/// Whether a restart that `ended` a process carries on the turn it cut off, or that a usage limit
/// had stopped: the conversation record at `record`, relative to the account directory `dir`,
/// says so (`switch_continue::should_continue`). A process that had ended before was left there by
/// something else.
pub(crate) fn carry_on(agent: AgentId, dir: &Path, record: &Path, ended: bool) -> bool {
    ended && switch_continue::should_continue(agent, &dir.join(record))
}

/// [`carry_on`] for `stopped`, a session restarted on its own account: its record is found there,
/// where `shell_env` resolves the default account. A record that cannot be found carries nothing
/// on. Blocking.
pub(crate) fn carry_on_in_place(
    stopped: &Session,
    shell_env: Option<&HashMap<String, String>>,
    ended: bool,
) -> bool {
    ended
        && resumable_agent_session_id(stopped).is_some_and(|agent_session_id| {
            let dir = account_dir(stopped.agent, stopped.config_dir.as_deref(), shell_env);
            relocate::locate(stopped.agent, &dir, &agent_session_id)
                .is_ok_and(|record| carry_on(stopped.agent, &dir, &record, ended))
        })
}

/// Records the model and effort `before` had, for a restart that recorded others for its relaunch
/// and failed. A revert that fails is logged, never allowed to hide the reason the restart failed.
pub(crate) fn put_back_model_and_effort(state: &Arc<AppState>, before: &Session) {
    if let Ok(now) = state.session_record(&before.id) {
        if (&now.model, &now.effort) != (&before.model, &before.effort) {
            if let Err(revert) =
                state.set_session_launch_overrides(&before.id, &before.model, &before.effort)
            {
                tracing::warn!(
                    session = %before.id,
                    %revert,
                    "putting the session's model and effort back failed"
                );
            }
        }
    }
}

/// Ends the process of `id`, a session about to be restarted by a switch, a change of its model or
/// a change of its account's sharing, the way archiving ends it, with what the process started
/// carried over to the relaunch rather than stopped with it: held while the session has no process,
/// and, where the caller watches the relaunch coming up (a switch and a model change), while it is,
/// and given up by [`give_up_what_a_restart_held`].
/// Returns what is held, whether this stopped a running process (one that had exited by itself
/// already, and is only waiting to be deregistered, is not the restart's doing), and the trees the
/// session had running just before it was stopped, for [`trees_ended`] to compare.
pub(crate) async fn stop_carrying_processes(
    state: &Arc<AppState>,
    id: &str,
) -> Result<(HeldForLaunch, bool, Vec<ProcessTree>)> {
    let held = state.process_ledger.hold_for_launch(id);
    let (ended_a_process, running) = match state.live_session(id) {
        Some(live) => {
            state.process_ledger.mark_carry_over(&live.launch_id, id);
            stop_process_listing_trees(state, live).await?
        }
        None => (false, Vec::new()),
    };
    Ok((held, ended_a_process, running))
}

/// Which of `running`, the trees a session had running before its restart stopped its process,
/// no longer run: ended during the restart, by whatever did it (an agent shutting down its own
/// background terminals as it exits among them). For the caller to ask once the process is
/// recorded down ([`wait_until_stopped`]), when the exit watcher has ended the launch and carried
/// what it left over to the hold, so a tree that is to survive is held by then; one still dying at
/// that point is not told. A table that cannot be read tells nothing ended.
pub(crate) async fn trees_ended(
    state: &Arc<AppState>,
    id: &str,
    running: Vec<ProcessTree>,
) -> Vec<ProcessTree> {
    if running.is_empty() {
        return running;
    }
    let (ledger, session) = (state.process_ledger.clone(), id.to_string());
    let named: Vec<String> = running.iter().map(|tree| tree.id.clone()).collect();
    let still = tokio::task::spawn_blocking(move || ledger.trees_still_running(&session, &named))
        .await
        .ok()
        .flatten();
    let Some(still) = still else {
        tracing::warn!(session = %id, "reading what a restart ended failed; none is told");
        return Vec::new();
    };
    running
        .into_iter()
        .filter(|tree| !still.contains(&tree.id))
        .collect()
}

/// Tells `id`, a session its restart relaunched without a prompt to carry on, what the restart
/// ended (`ended`, from [`texts::processes_ended_by_restart`]), as a message written into it:
/// at once when it is idle, which starts a turn, else queued until it can take one. The turn it
/// opens is credited as reported, so a session bound to an owner is not made to report on it. A
/// notice that cannot be written is logged: the session is not owed it later. Not to be called
/// under the session's claim: the write can block for seconds (an idle Claude Code session's send
/// wait), which would keep the claim held and delay the request's answer.
pub(crate) async fn tell_what_a_restart_ended(state: &Arc<AppState>, id: &str, ended: String) {
    let opening = crate::session::term::stripped(texts::RESTART_ENDED_OPENING);
    state.credit_turn_opened_by(id, opening.clone());
    let (state_for_write, session) = (state.clone(), id.to_string());
    let told = tokio::task::spawn_blocking(move || {
        write_message(
            &state_for_write,
            &session,
            &texts::restart_ended_notice(&ended),
            WhenBlocked::Queue,
        )
    })
    .await;
    if let Err(err) = told.map_err(anyhow::Error::from).and_then(|told| told) {
        state.clear_turn_credit(id, &opening);
        tracing::warn!(session = %id, %err, "telling a session what its restart ended failed");
    }
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
