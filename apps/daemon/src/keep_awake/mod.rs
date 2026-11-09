//! Prevent sleep: the daemon holds the machine it runs on awake (`platform::sleep`) while the
//! setting is on (`Settings::keep_awake`) and its condition is met (`Settings::keep_awake_when`):
//! `always`, or while any session is working. The hold is the daemon's, not a window's, so it
//! stands from the daemon's start with no client open, and the system releases it with the
//! process. It is released at once when the answer turns false, and what is actually held is told
//! to clients (`AppState::set_keep_awake_held`): a host that cannot hold (`sleep::available`) is
//! never asked, and one that refuses the hold leaves it false, with the reason in the log, and the
//! hold is not asked for again until the answer has gone false and true again. A refusal is logged
//! once, until a hold succeeds.
//!
//! "Working" is the UI's own reading of a session (`sessionIsWorking`): starting, working or
//! dormant, or idle with work left running in the background, in any console or project. A
//! session waiting for the user is not at work. The set of working sessions is kept from the
//! `session_upserted` and `session_deleted` events, which carry the status a client is shown, and
//! read afresh from the store when the daemon falls behind them or a console or project is
//! deleted, which removes its sessions without an event for each.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::broadcast::error::RecvError;

use crate::platform::sleep;
use crate::protocol::{Event, KeepAwakeWhen, Session, SessionStatus};
use crate::state::AppState;

/// Starts the task that keeps the hold in step with the setting and the sessions. Call it once
/// the state exists, before clients can be told anything.
pub fn spawn(state: &Arc<AppState>) {
    spawn_with(state, sleep::available, sleep::acquire);
}

/// [`spawn`] with the host supplied by the caller: whether it can hold, and how to take a hold,
/// which is dropped to release it.
fn spawn_with<H, A, F>(state: &Arc<AppState>, available: A, acquire: F)
where
    H: Send + 'static,
    A: FnOnce() -> bool + Send + 'static,
    F: Fn() -> Result<H> + Send + Sync + 'static,
{
    let state = state.clone();
    let acquire = Arc::new(acquire);
    // Subscribed before the sessions are read, so none that changes in between is missed.
    let mut events = state.subscribe();
    tokio::spawn(async move {
        // `available`, supplied by the caller, may block (a probe of the host's bus, say), so it
        // runs on a blocking thread instead of the runtime.
        let available = tokio::task::spawn_blocking(available)
            .await
            .unwrap_or(false);
        let mut working = working_sessions(&state);
        let mut hold: Option<H> = None;
        let mut refused = false;
        let mut warned = false;
        loop {
            if available && wanted(&state, !working.is_empty()) {
                if hold.is_none() && !refused {
                    let acquire = acquire.clone();
                    let taken = tokio::task::spawn_blocking(move || acquire())
                        .await
                        .map_err(anyhow::Error::from)
                        .and_then(|taken| taken);
                    match taken {
                        Ok(taken) => {
                            hold = Some(taken);
                            warned = false;
                        }
                        Err(err) => {
                            if !warned {
                                tracing::warn!(%err, "keeping the computer awake failed");
                                warned = true;
                            }
                            refused = true;
                        }
                    }
                }
            } else {
                refused = false;
                if let Some(taken) = hold.take() {
                    // Dropping the hold may call out to the host, which must not stall the runtime.
                    let _ = tokio::task::spawn_blocking(move || drop(taken)).await;
                }
            }
            state.set_keep_awake_held(hold.is_some());

            loop {
                match events.recv().await {
                    Ok(Event::SettingsUpdated { .. }) => break,
                    Ok(Event::SessionUpserted { session }) => {
                        let changed = if session_is_working(&session) {
                            working.insert(session.id)
                        } else {
                            working.remove(&session.id)
                        };
                        if changed {
                            break;
                        }
                    }
                    Ok(Event::SessionDeleted { session }) => {
                        if working.remove(&session) {
                            break;
                        }
                    }
                    // Deleting a console or a project deletes its sessions without an event each.
                    Ok(Event::ProjectDeleted { .. } | Event::ConsoleDeleted { .. }) => {
                        let now = working_sessions(&state);
                        let changed = now != working;
                        working = now;
                        if changed {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(RecvError::Lagged(_)) => {
                        working = working_sessions(&state);
                        break;
                    }
                    Err(RecvError::Closed) => return,
                }
            }
        }
    });
}

/// Whether the settings ask for a hold now, given whether any session is working. A setting that
/// cannot be read asks for none.
fn wanted(state: &AppState, any_working: bool) -> bool {
    let read = || -> Result<(bool, KeepAwakeWhen)> {
        Ok((state.store.keep_awake()?, state.store.keep_awake_when()?))
    };
    match read() {
        Ok((false, _)) => false,
        Ok((true, KeepAwakeWhen::Always)) => true,
        Ok((true, KeepAwakeWhen::Working)) => any_working,
        Err(err) => {
            tracing::warn!(%err, "reading Prevent sleep failed; the computer is not held awake");
            false
        }
    }
}

/// The ids of the sessions working now, as a client would be shown them.
fn working_sessions(state: &AppState) -> HashSet<String> {
    match state.store.list_sessions() {
        Ok(sessions) => sessions
            .into_iter()
            .map(|session| state.wire_session(session))
            .filter(session_is_working)
            .map(|session| session.id)
            .collect(),
        Err(err) => {
            tracing::warn!(%err, "listing the sessions for Prevent sleep failed");
            HashSet::new()
        }
    }
}

/// Whether `session`, as a client is shown it, is at work: starting, working or dormant, or idle
/// with work left running in the background. Mirrors the UI's `sessionIsWorking`; the two must
/// stay in step.
fn session_is_working(session: &Session) -> bool {
    match session.status {
        SessionStatus::Starting | SessionStatus::Working | SessionStatus::Dormant => true,
        SessionStatus::Idle => !session.background_tasks.is_empty(),
        SessionStatus::WaitingUser | SessionStatus::Interrupted | SessionStatus::Archived => false,
    }
}

#[cfg(test)]
mod tests;
