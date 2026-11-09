//! A session's status taken from what the program in its terminal reports about itself through
//! OSC 7501 (`crate::terminal::program_status`), for an agent whose hooks and own signals give no
//! status (`AgentProvider::has_status_integration`). Where they do, they win: every session's
//! terminal still keeps the reports, takes them out of what the clients receive and answers their
//! support query, since supporting the protocol is the terminal's, but they move nothing.
//!
//! The root record's state is the session's status: `working` is `Working`, `blocked` is
//! `WaitingUser`, `idle` and `done` are `Idle` with the session not paused, `error` is `Idle` with
//! it paused, and a root record cleared or forgotten is `Idle` too. A turn starts when the root
//! goes to working or blocked from neither, and ends when it leaves both, closed and reported as a
//! hook's turn end is. Dormant and working in background never come from here.
//!
//! The moves are taken on the PTY reader thread and applied in order by one blocking task at a
//! time, since applying one writes to the store and may wait on the owner's terminal. The end of
//! the process waits for them ([`LiveSession::settle_program_status`]) before it is recorded.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use super::LiveSession;
use crate::hooks::TurnEnd;
use crate::messaging::reporting;
use crate::protocol::SessionStatus;
use crate::state::AppState;
use crate::terminal::program_status::ReportedState;

/// The moves of a session's root status record on their way to its status, shared with the task
/// applying them.
#[derive(Default)]
pub(super) struct StatusReports {
    moves: Mutex<Moves>,
    /// Woken whenever the task applying the moves stops with none left: all applied, or the rest
    /// dropped (the launch is no longer the session's, or applying one failed).
    settled: Notify,
    /// Held by a test to keep the next move from being applied.
    #[cfg(test)]
    pub(super) gate: Mutex<()>,
}

/// The moves themselves, behind [`StatusReports`]' lock.
#[derive(Default)]
struct Moves {
    /// Taken from the output and not yet applied, oldest first.
    pending: VecDeque<Option<ReportedState>>,
    /// The root's state as last applied.
    applied: Option<ReportedState>,
    /// A task is applying `pending`.
    draining: bool,
}

impl StatusReports {
    fn moves(&self) -> std::sync::MutexGuard<'_, Moves> {
        self.moves.lock().expect("status reports lock poisoned")
    }
}

/// The wire status the root record's state gives.
pub(super) fn wire_status(root: Option<ReportedState>) -> SessionStatus {
    match root {
        Some(ReportedState::Working) => SessionStatus::Working,
        Some(ReportedState::Blocked) => SessionStatus::WaitingUser,
        Some(ReportedState::Idle | ReportedState::Done | ReportedState::Error) | None => {
            SessionStatus::Idle
        }
    }
}

impl LiveSession {
    /// Takes the moves of the root status record the latest chunk of output made, and has them
    /// applied where the reports give this session its status. Called on the PTY reader thread
    /// after each chunk.
    pub(super) fn note_program_status(&self) {
        let moves = self.pty.take_program_status();
        if moves.is_empty() || !self.status_from_reports {
            return;
        }
        self.status_reports.moves().pending.extend(moves);
        self.apply_program_status();
    }

    /// Starts a task applying the pending moves, unless one runs or the session is not yet bound
    /// to the daemon's state (`Self::observe_titles` calls this again once it is).
    pub(super) fn apply_program_status(&self) {
        let (state, runtime) = {
            let observer = self.title.lock().expect("title observer lock poisoned");
            (observer.state.upgrade(), observer.runtime.clone())
        };
        let (Some(state), Some(runtime)) = (state, runtime) else {
            return;
        };
        {
            let mut moves = self.status_reports.moves();
            if moves.draining || moves.pending.is_empty() {
                return;
            }
            moves.draining = true;
        }
        let mut applying = Applying {
            reports: self.status_reports.clone(),
            stopped: false,
        };
        let (id, launch_id) = (self.id.clone(), self.launch_id.clone());
        runtime.spawn_blocking(move || loop {
            let reports = applying.reports.clone();
            #[cfg(test)]
            let _held = reports.gate.lock().expect("status reports gate poisoned");
            // A session relaunched meanwhile, or gone, has nothing more of this launch applied.
            let current = state
                .live_session(&id)
                .is_some_and(|live| live.launch_id == launch_id);
            let next = {
                let mut moves = reports.moves();
                match moves.pending.pop_front() {
                    Some(next) if current => {
                        Some((std::mem::replace(&mut moves.applied, next), next))
                    }
                    _ => {
                        moves.pending.clear();
                        moves.draining = false;
                        reports.settled.notify_waiters();
                        applying.stopped = true;
                        None
                    }
                }
            };
            let Some((previous, next)) = next else {
                return;
            };
            apply(&state, &id, previous, next);
        });
    }

    /// Waits until every move the program's reports made so far has been applied: for the end of
    /// the process, where a report read before the exit, the program's last `done` above all,
    /// decides how its turn ended before the exit is recorded, and for whatever else reads the
    /// turn those moves change. Bounded as a hook queue's settling is (`HookQueues::settled`);
    /// where the bound runs out, the exit finds the turn cut short, and a move applied after it
    /// is refused ([`apply`]). Only what the PTY reader has taken in counts, so the end of the
    /// process calls this once the reader has drained, or its own bounded wait for that ran out.
    pub(crate) async fn settle_program_status(&self) {
        let reports = self.status_reports.clone();
        let wait = async {
            loop {
                let woken = reports.settled.notified();
                tokio::pin!(woken);
                // Registered before the check, so a task settling between the two is not missed.
                woken.as_mut().enable();
                {
                    let moves = reports.moves();
                    if !moves.draining && moves.pending.is_empty() {
                        return;
                    }
                }
                woken.await;
            }
        };
        let _ = tokio::time::timeout(crate::hooks::queue::settle_bound(), wait).await;
    }
}

/// The task applying a launch's moves. However it stops without saying so, a panic in applying a
/// move or a runtime refusing to start it, the moves left are dropped and `draining` cleared, so
/// no settling waits on a task that is gone and the next move starts another.
struct Applying {
    reports: Arc<StatusReports>,
    /// The task stopped as it should, having cleared `draining` itself.
    stopped: bool,
}

impl Drop for Applying {
    fn drop(&mut self) {
        if self.stopped {
            return;
        }
        let mut moves = self
            .reports
            .moves
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        moves.pending.clear();
        moves.draining = false;
        self.reports.settled.notify_waiters();
    }
}

/// Applies one move of the root record, from `previous` to `next`: the turn it starts or ends,
/// the pause and the status, and what the status sets going, in the order a hook's are.
pub(super) fn apply(
    state: &Arc<AppState>,
    id: &str,
    previous: Option<ReportedState>,
    next: Option<ReportedState>,
) {
    // A move the program made just before its process ended reaches a session without one: the
    // end has settled its turn, and nothing of the move may open another.
    if state
        .session_record(id)
        .is_ok_and(|session| session.status.has_no_process())
    {
        return;
    }
    let was_in_turn = previous.is_some_and(ReportedState::in_turn);
    let in_turn = next.is_some_and(ReportedState::in_turn);
    if in_turn && !was_in_turn {
        if let Err(err) = state.mark_conversation_started(id) {
            tracing::debug!(session = %id, %err, "recording the session's first turn failed");
        }
        state.turn_started(id, None);
    } else if was_in_turn && !in_turn {
        reporting::close_turn_and_report(
            state,
            id,
            TurnEnd {
                last_assistant_message: None,
                failed: next == Some(ReportedState::Error),
                backstop: false,
            },
        );
    } else if in_turn {
        state.touch_turn(id);
    }
    let status = wire_status(next);
    if status == SessionStatus::Idle {
        state.set_paused(id, next == Some(ReportedState::Error));
    }
    match state.apply_hook_status(id, status) {
        // Telling the owner writes into its PTY, which blocks.
        Ok(true) if status == SessionStatus::WaitingUser => {
            let (state, id) = (state.clone(), id.to_string());
            tokio::task::spawn_blocking(move || {
                crate::messaging::owner_notices::tell_owner_it_waits(&state, &id)
            });
        }
        Ok(_) => {}
        Err(err) => {
            tracing::debug!(session = %id, %err, "applying the program's reported status failed");
        }
    }
    state.spawn_flush_outbox(id, status);
}

#[cfg(test)]
mod tests;
