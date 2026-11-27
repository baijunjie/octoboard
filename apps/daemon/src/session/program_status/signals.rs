//! What a provider whose hooks give its status declares its program status reports say beside
//! them (`AgentProvider::reported_signals`): when it waits on the user, and when a turn was
//! cancelled. The hooks stay the status's source for everything else.
//!
//! **The wait.** A `blocked` root record raises the session's hand and its end lowers it, as a
//! title that says only the user can act does (`LiveSession::needs_user_action`): the hand is the
//! session's own, held over whatever the hooks say meanwhile, so a hook that reports the turn
//! working while the program still waits does not lower it. A `blocked` of kind `auth` counts only
//! once it has stood for the settle a session whose status the reports give waits too
//! (`super::AUTH_SETTLE`). From the program's first report on, its title says nothing of the
//! wait; before it, the title stands.
//!
//! **The cancel.** A root record that goes from `working` straight to `idle` is a cancelled turn:
//! a program reports `done` where its turn ends and `idle` where it is cancelled. The turn is
//! closed at once, without the settle a title back at the prompt has to stand for
//! (`crate::session::title_interrupt`), since the report cannot be a turn's ordinary end. Hook
//! events posted before it are applied first, and the session is put at its prompt only if it
//! still reads working and the reports have shown no turn since.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::{auth_settle, settles_first};
use crate::protocol::SessionStatus;
use crate::session::LiveSession;
use crate::state::AppState;
use crate::terminal::program_status::{ReportedState, RootMove};

/// What a provider with a status integration of its own declares its program status reports say.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportedSignals {
    /// A `blocked` root record is the program waiting on the user, and its end the wait's end.
    pub wait_on_user: bool,
    /// A root record going from `working` straight to `idle` is a cancelled turn.
    pub cancel: bool,
}

impl ReportedSignals {
    /// The declaration of a provider whose reports say neither.
    pub const NONE: Self = Self {
        wait_on_user: false,
        cancel: false,
    };
}

/// What a session's reports have said of the signals its provider declares.
pub(in crate::session) struct DeclaredSignals {
    declared: ReportedSignals,
    /// The program has reported, so its reports say whether it waits on the user and its title
    /// no longer does.
    taken_over: AtomicBool,
    /// The program's last report is a wait on the user that counts.
    waits: AtomicBool,
    root: Mutex<Root>,
    /// How many reported cancels have been acted on, whether or not they closed a turn.
    #[cfg(test)]
    cancels_settled: std::sync::atomic::AtomicU64,
    /// How many reported cancels have been taken up, counted before anything is spawned for one.
    #[cfg(test)]
    cancels_started: std::sync::atomic::AtomicU64,
}

#[derive(Default)]
struct Root {
    /// The root record's state as last reported.
    state: Option<ReportedState>,
    /// How many moves the root record has made, for telling whether a held wait still stands.
    moves: u64,
}

impl DeclaredSignals {
    pub(in crate::session) fn new(declared: ReportedSignals) -> Self {
        Self {
            declared,
            taken_over: AtomicBool::new(false),
            waits: AtomicBool::new(false),
            root: Mutex::default(),
            #[cfg(test)]
            cancels_settled: std::sync::atomic::AtomicU64::new(0),
            #[cfg(test)]
            cancels_started: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// How many reported cancels have been taken up, each counted on the thread that took the
    /// report in, before anything was spawned for it.
    #[cfg(test)]
    pub(in crate::session) fn cancels_started(&self) -> u64 {
        self.cancels_started.load(Ordering::Acquire)
    }

    /// How many reported cancels have been acted on, whether or not they closed a turn.
    #[cfg(test)]
    pub(in crate::session) fn cancels_settled(&self) -> u64 {
        self.cancels_settled.load(Ordering::Acquire)
    }

    /// Whether the program's last report is a wait on the user that counts.
    pub(in crate::session) fn waits(&self) -> bool {
        self.waits.load(Ordering::Acquire)
    }

    /// Whether the reports say whether the program waits on the user, in place of its title.
    pub(in crate::session) fn say_the_wait(&self) -> bool {
        self.taken_over.load(Ordering::Acquire)
    }
}

impl LiveSession {
    /// Takes `moves` of the root status record for what the session's provider declares they say:
    /// its wait on the user and a cancelled turn. Called on the PTY reader thread, so nothing here
    /// writes to the store or a terminal: what a signal sets going runs on the runtime.
    pub(super) fn note_declared_signals(&self, moves: &[RootMove]) {
        let declared = self.signals.declared;
        if declared == ReportedSignals::NONE {
            return;
        }
        for root in moves {
            let (previous, moved) = {
                let mut last = self.signals.root.lock().expect("signals lock poisoned");
                last.moves += 1;
                (std::mem::replace(&mut last.state, root.state), last.moves)
            };
            if declared.wait_on_user {
                // The title's own wait goes with its taking over, or it would stand for good.
                if !self.signals.taken_over.swap(true, Ordering::AcqRel) {
                    self.action_required.store(false, Ordering::Release);
                }
                self.note_declared_wait(root, moved);
            }
            if declared.cancel
                && previous == Some(ReportedState::Working)
                && root.state == Some(ReportedState::Idle)
            {
                self.close_reported_cancel();
            }
        }
    }

    /// Raises the hand for a `blocked` root record and lowers it for any other, `moved` being the
    /// count of the root's moves at this one.
    fn note_declared_wait(&self, root: &RootMove, moved: u64) {
        if root.state != Some(ReportedState::Blocked) {
            self.set_declared_wait(false);
        } else if !settles_first(root) {
            self.set_declared_wait(true);
        } else if let Some((state, runtime)) = self.bound() {
            // A sign-in the program only checks is gone before anyone could act on it.
            let (id, launch_id, settle) = (self.id.clone(), self.launch_id.clone(), auth_settle());
            runtime.spawn(async move {
                tokio::time::sleep(settle).await;
                let Some(live) = state.live_session(&id) else {
                    return;
                };
                let stands = live.launch_id == launch_id
                    && live
                        .signals
                        .root
                        .lock()
                        .expect("signals lock poisoned")
                        .moves
                        == moved;
                if stands {
                    live.set_declared_wait(true);
                }
            });
        }
    }

    /// Records whether the program waits on the user by its reports and, where that changed, has
    /// the session published and what the change owes done: its owner told of a wait, or the
    /// messages held for the session let go once the wait is over.
    fn set_declared_wait(&self, waiting: bool) {
        if self.signals.waits.swap(waiting, Ordering::AcqRel) == waiting {
            return;
        }
        self.publish_declared_wait();
    }

    /// Publishes the session as its declared wait now has it shown, and does what the wait owes.
    /// Nothing until the session is bound to the daemon's state; `Self::observe_titles` calls
    /// this once it is, for a wait reported before.
    pub(in crate::session) fn publish_declared_wait(&self) {
        let Some((state, runtime)) = self.bound() else {
            return;
        };
        let waiting = self.signals.waits();
        let id = self.id.clone();
        let launch_id = self.launch_id.clone();
        // Telling the owner and letting messages go both write into a terminal, which blocks.
        runtime.spawn_blocking(move || {
            let Some(live) = state
                .live_session(&id)
                .filter(|live| live.launch_id == launch_id)
            else {
                return;
            };
            let Ok(record) = state.session_record(&id) else {
                return;
            };
            state.publish_session(&record);
            if waiting {
                crate::session::interaction::watch::ensure(&state, &live);
                crate::messaging::owner_notices::tell_owner_it_waits(&state, &id);
            } else {
                state.flush_outbox(&id, state.agent_status(&record));
            }
        });
    }

    /// Puts the session at its prompt and closes its turn as cancelled, after a root record that
    /// went from `working` straight to `idle`, once the hook events posted before have been
    /// applied: whatever they say of the session wins. Only while the session still reads
    /// `Working` and the reports have shown no turn since, and in one step with the session's
    /// record watch, where it has one, so a prompt landing just then does not have its own turn
    /// closed. The turn is closed as one a title back at the prompt closes
    /// (`AppState::end_turn_off_title`), so a turn end of the agent's own that still comes is its
    /// real end.
    fn close_reported_cancel(&self) {
        #[cfg(test)]
        self.signals.cancels_started.fetch_add(1, Ordering::AcqRel);
        let Some((state, runtime)) = self.bound() else {
            return;
        };
        let (id, launch_id) = (self.id.clone(), self.launch_id.clone());
        let stops = self.title_activity().stops();
        runtime.clone().spawn(async move {
            state.hook_queues.settled(&id).await;
            #[cfg(test)]
            let settled = (state.clone(), id.clone());
            let close = move || close_cancelled(&state, &id, &launch_id, stops);
            let _ = runtime.spawn_blocking(close).await;
            #[cfg(test)]
            if let Some(live) = settled.0.live_session(&settled.1) {
                live.signals.cancels_settled.fetch_add(1, Ordering::AcqRel);
            }
        });
    }

    /// How many cancels the session's reports have had taken up, each counted on the thread that
    /// took the report in, before anything was spawned for it: what a test of a provider's
    /// declared signals asserts on to say a report started no cancel, which no wait can show.
    #[cfg(test)]
    pub(crate) fn reported_cancels_started(&self) -> u64 {
        self.signals.cancels_started()
    }

    /// How many cancels the session's reports have had acted on, whether or not they closed a
    /// turn: what a test waits on before it looks at the turn, as the status moves first.
    #[cfg(test)]
    pub(crate) fn reported_cancels_settled(&self) -> u64 {
        self.signals.cancels_settled()
    }

    /// The daemon's state and runtime, once the session is bound to them.
    fn bound(&self) -> Option<(Arc<AppState>, tokio::runtime::Handle)> {
        let observer = self.title.lock().expect("title observer lock poisoned");
        Some((observer.state.upgrade()?, observer.runtime.clone()?))
    }
}

/// The write of [`LiveSession::close_reported_cancel`], `stops` being how many times the reports
/// had shown a turn stop when the cancel was reported.
fn close_cancelled(state: &Arc<AppState>, id: &str, launch_id: &str, stops: u64) {
    let unchanged = state.live_session(id).is_some_and(|live| {
        let activity = live.title_activity();
        live.launch_id == launch_id && activity.stops() == stops && !activity.last_said_running()
    });
    if !unchanged {
        return;
    }
    let turn_was_open = state.turn_is_open(id);
    let close = || {
        // What the agent held for the turn's end and dropped with it is queued again, as after a
        // cancel a hook or the status reports told, before the status moves: an interrupt returns
        // once the session leaves `Working`, and its caller reading `get_session` then must find
        // those messages counted as queued, or it would send them again. Only while the agent
        // still reads as working, which the move below needs too: one that has moved on ended
        // the turn some other way, which queued what it had to. Queuing twice for one cancel
        // queues nothing more (`LiveSession::take_dropped_at_cancel` empties the list). A cancel
        // with no turn open drops nothing of a turn.
        let working = state
            .session_record(id)
            .is_ok_and(|record| state.agent_status(&record) == SessionStatus::Working);
        if turn_was_open && working {
            state.write_dropped_messages_again(id);
        }
        let moved = state.report_paused_if(id, SessionStatus::Working);
        if matches!(moved, Ok(true)) {
            state.end_turn_off_title(id);
        }
        moved
    };
    let moved = match state.current_record_watch(id) {
        Some(generation) => state.while_record_watch_current(id, generation, close),
        None => Some(close()),
    };
    match moved {
        Some(Ok(true)) => {
            tracing::debug!(
                session = %id,
                "the program reported its turn cancelled; the session is back at its prompt"
            );
            // Nothing else is coming to release a message queued while the turn ran.
            state.spawn_flush_outbox(id, SessionStatus::Idle);
        }
        Some(Ok(false)) | None => {
            tracing::debug!(
                session = %id,
                "the program reported its turn cancelled, but the session had already moved on"
            );
        }
        Some(Err(err)) => {
            tracing::debug!(
                session = %id, %err,
                "returning the session to its prompt after its reported cancel failed"
            );
        }
    }
}
