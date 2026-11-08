//! Resuming the interrupted sessions at the daemon's start, when the user asked for that
//! (`Settings.resume_sessions_on_start`). The daemon owns the agent processes, so a run that ended
//! — a quit, a signal, a crash — left every running session interrupted; with the setting on, the
//! start after it brings back every session that is interrupted by then, one resume each, as the
//! user's click on an interrupted session would. That includes one interrupted by an earlier run
//! or by a resume that failed, since nothing records why a session is interrupted.
//!
//! Which sessions those are is the store's to say
//! ([`crate::store::Store::list_interrupted_sessions`]), read once the start has marked what the
//! previous run left running as interrupted. This module orders them, paces the resumes and leaves
//! each failure where the resume left the session: interrupted, with the reason in the log and no
//! alert, since nobody asked for this resume.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::protocol::{error_code, CodedError, Session, SessionStatus};
use crate::state::AppState;

use super::resume::resume_session;

/// How many resumes run at once. A resume reads the shell snapshot, checks the conversation and
/// forks an agent, which can take seconds on a busy machine; a few at a time brings a dozen
/// sessions back without starting a dozen agents in the same instant.
const WIDTH: usize = 3;

/// How long a resume waits to start after the previous one started.
const START_GAP: Duration = Duration::from_millis(400);

/// Starts the task that resumes the interrupted sessions, unless the setting is off (or cannot be
/// read, which counts as off). Call it once the start has marked the sessions the previous run left
/// running as interrupted. The setting is read now, at the daemon's start; a change afterwards
/// applies to the next one.
pub fn spawn(state: &Arc<AppState>) {
    spawn_with(state, |state, id| async move {
        resume_if_still_interrupted(&state, &id).await
    });
}

/// [`spawn`] with the resume of one session supplied by the caller.
fn spawn_with<F, Fut>(state: &Arc<AppState>, resume: F)
where
    F: Fn(Arc<AppState>, String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    match state.store.resume_sessions_on_start() {
        Ok(true) => {}
        Ok(false) => return,
        Err(err) => {
            tracing::warn!(%err, "reading Resume sessions on start failed; no session is resumed");
            return;
        }
    }
    let interrupted = match state.store.list_interrupted_sessions() {
        Ok(sessions) => sessions,
        Err(err) => {
            tracing::warn!(%err, "listing the interrupted sessions failed; no session is resumed");
            return;
        }
    };
    let ids = in_resume_order(interrupted);
    if ids.is_empty() {
        return;
    }
    tracing::info!(sessions = ids.len(), "resuming the interrupted sessions");
    let state = state.clone();
    tokio::spawn(async move {
        run(&state, ids, START_GAP, resume).await;
    });
}

/// Resumes `id` as a click on an interrupted session would, unless it is no longer interrupted:
/// the user may have resumed or archived it since the start, and resuming would then unarchive it.
async fn resume_if_still_interrupted(state: &Arc<AppState>, id: &str) -> Result<()> {
    if !state
        .session_record(id)
        .is_ok_and(|session| session.status == SessionStatus::Interrupted)
    {
        return Ok(());
    }
    // The terminal appearance comes from a client, and the daemon has none to give.
    resume_session(state, id, None, None).await
}

/// The ids of `sessions` with every session after the session it reports to when that one is among
/// them, so what a session reports reaches a running owner. Otherwise the order they came in.
fn in_resume_order(sessions: Vec<Session>) -> Vec<String> {
    let depth = |session: &Session| {
        let mut depth = 0;
        let mut owner = session.bound_to.as_deref();
        // A chain longer than the set is a cycle, which the store does not write but a damaged
        // database could hold.
        while let Some(id) = owner.filter(|_| depth < sessions.len()) {
            let Some(above) = sessions.iter().find(|candidate| candidate.id == id) else {
                break;
            };
            depth += 1;
            owner = above.bound_to.as_deref();
        }
        depth
    };
    let mut ordered: Vec<(usize, &Session)> = sessions
        .iter()
        .map(|session| (depth(session), session))
        .collect();
    ordered.sort_by_key(|(depth, _)| *depth);
    ordered
        .into_iter()
        .map(|(_, session)| session.id.clone())
        .collect()
}

/// Resumes `ids` through `resume`, at most [`WIDTH`] at once and each starting `gap` after the
/// previous one started. Stops starting new ones once the daemon is shutting down, and still waits
/// for the ones running, which end at their registration or fail as the shutdown refuses them; a
/// failure is logged and the rest go on. A session whose resume found it being started already,
/// as a sibling's reopening of their archived owner does, is tried once more after the others
/// have settled.
async fn run<F, Fut>(state: &Arc<AppState>, ids: Vec<String>, gap: Duration, resume: F)
where
    F: Fn(Arc<AppState>, String) -> Fut,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    let busy = run_pass(state, ids, gap, &resume, false).await;
    if !busy.is_empty() && !state.is_shutting_down() {
        run_pass(state, busy, gap, &resume, true).await;
    }
}

/// One pass of [`run`]; returns the ids whose resume failed with `session_already_starting`. Such a
/// failure is logged only on the `last` pass, as the earlier one is retried.
async fn run_pass<F, Fut>(
    state: &Arc<AppState>,
    ids: Vec<String>,
    gap: Duration,
    resume: &F,
    last: bool,
) -> Vec<String>
where
    F: Fn(Arc<AppState>, String) -> Fut,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    let mut running = tokio::task::JoinSet::new();
    let mut busy = Vec::new();
    for (index, id) in ids.into_iter().enumerate() {
        if index > 0 {
            tokio::time::sleep(gap).await;
        }
        if running.len() >= WIDTH {
            busy.extend(running.join_next().await.and_then(Result::ok).flatten());
        }
        if state.is_shutting_down() {
            break;
        }
        let resumed = resume(state.clone(), id.clone());
        running.spawn(async move {
            let err = resumed.await.err()?;
            let busy = is_already_starting(&err);
            if last || !busy {
                report(&id, &err);
            }
            busy.then_some(id)
        });
    }
    while let Some(joined) = running.join_next().await {
        busy.extend(joined.ok().flatten());
    }
    busy
}

/// Whether the resume failed because the session is being started already, which a later pass
/// retries.
fn is_already_starting(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::SESSION_ALREADY_STARTING)
}

/// Logs why a session was not resumed. A conversation its agent no longer has is expected over
/// time; anything else is worth a warning.
fn report(id: &str, err: &anyhow::Error) {
    let unresumable = err
        .downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::CONVERSATION_UNRESUMABLE);
    if unresumable {
        tracing::info!(session = %id, "not resumed on start: its conversation is gone");
    } else {
        tracing::warn!(session = %id, %err, "resuming the session on start failed");
    }
}

#[cfg(test)]
mod tests;
