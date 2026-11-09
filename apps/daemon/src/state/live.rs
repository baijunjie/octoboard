//! The running sessions with the ids being launched, and the questions asked of them.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;

use super::sharing_lock::SharingLockEntry;
use super::AppState;
use crate::protocol::{Session, SessionStatus};
use crate::session::LiveSession;

#[derive(Default)]
pub(super) struct LiveSessions {
    pub(super) sessions: HashMap<String, Arc<LiveSession>>,
    pub(super) launching: HashSet<String>,
    /// The sessions in `launching` whose claim came from [`AppState::begin_switch`], so is not a
    /// launch of their process.
    pub(super) switching: HashSet<String>,
    /// The sessions in `switching` held by an account switch, which clients are shown
    /// (`Session::switching_account`).
    pub(super) account_switches: HashSet<String>,
    /// The sessions in `launching` whose launch will hand the agent a prompt
    /// (`Session::prompt_on_launch`).
    pub(super) prompted_launches: HashSet<String>,
    /// The sessions in `switching` that a change of their model is restarting, which clients are
    /// shown starting ([`AppState::mark_model_change`]).
    pub(super) model_changes: HashSet<String>,
    /// The accounts locked for a change of their sharing, by id.
    pub(super) sharing_locks: HashMap<String, SharingLockEntry>,
}

/// A session running or holding a claim, as a change of sharing sees it.
pub struct Running {
    pub session: Session,
    /// Working, which refuses a change of its account's sharing: not idle (see
    /// [`AppState::activity_of`]), or claimed by anything but such a change.
    pub working: bool,
    /// Claimed by a change of its account's sharing, which restarts it.
    pub restarting: bool,
}

impl AppState {
    /// Keeps `text` in the store for the session `id`, to be queued at its next resume, and returns
    /// the row it is kept in, unless it has a process or is being launched, in which case it
    /// returns `None` and keeps nothing. Shared information is kept this way, and so are the
    /// reports and notices for an owner (`crate::messaging::delivery`'s `write_to_owner`). The
    /// check and the insert are one step under the `live` lock, which [`Self::begin_launch`] takes
    /// for writing before it reads the store, so a launch that begins either sees the row or is
    /// seen by this check, and a message is never kept for a session that has already read what was
    /// kept. (An archive can still land between this check and the insert; its rows are then
    /// cleared by the next archive or delete of the session, and are harmless until a reopen.)
    pub fn keep_message_if_not_running(&self, id: &str, text: &str) -> Result<Option<i64>> {
        let live = self.live.read().expect("live sessions lock poisoned");
        if live.sessions.contains_key(id) || live.launching.contains(id) {
            return Ok(None);
        }
        Ok(Some(self.store.add_kept_message(id, text)?))
    }

    pub fn live_session(&self, id: &str) -> Option<Arc<LiveSession>> {
        self.live
            .read()
            .expect("live sessions lock poisoned")
            .sessions
            .get(id)
            .cloned()
    }

    /// The sessions running or holding a claim, each once, as a change of sharing sees them.
    pub fn running_sessions(&self) -> Result<Vec<Running>> {
        let live = self.live.read().expect("live sessions lock poisoned");
        self.running_of(&live)
    }

    pub(super) fn running_of(&self, live: &LiveSessions) -> Result<Vec<Running>> {
        let ids: HashSet<&String> = live.sessions.keys().chain(&live.launching).collect();
        let mut running = Vec::new();
        for id in ids {
            let Some(session) = self.store.get_session(id)? else {
                continue;
            };
            let restarting = live
                .sharing_locks
                .values()
                .any(|entry| entry.restarting.contains(id));
            let working = !restarting
                && (live.launching.contains(id)
                    || live
                        .sessions
                        .get(id)
                        .is_none_or(|process| !self.is_idle_with(&session, process)));
            running.push(Running {
                session,
                working,
                restarting,
            });
        }
        Ok(running)
    }

    /// Whether `session` is idle: its process is at the prompt, waiting for the user's next
    /// message, with nothing on screen only the user can answer, no message of Octoboard's
    /// queued, being written or left unsent in its input box, none whose acceptance by its agent's
    /// own queue is still unconfirmed, and nothing left running in the background.
    fn is_idle_with(&self, session: &Session, process: &LiveSession) -> bool {
        session.status == SessionStatus::Idle
            && !process.waits_for_user()
            && !process.unsent_waiting()
            && !process.submission_uncertain()
            && !self.outbox.is_pending(&session.id)
            && self.background_tasks_of(&session.id).is_empty()
    }

    /// Whether `session`, its stored record, is working (`Some(true)`), idle (`Some(false)`), or has
    /// no process, for telling whether its account's sharing state changed.
    pub(super) fn activity_of(&self, session: &Session) -> Option<bool> {
        self.live_session(&session.id)
            .map(|process| !self.is_idle_with(session, &process))
    }

    pub fn live_sessions(&self) -> Vec<Arc<LiveSession>> {
        self.live
            .read()
            .expect("live sessions lock poisoned")
            .sessions
            .values()
            .cloned()
            .collect()
    }

    /// Whether the session has a process right now, or its claim is held. This is the line
    /// archiving a console session draws, and it is not the session's status: an interrupted
    /// session has none and an archived one may still be exiting (its record says archived, so
    /// callers ask about the ones that are not).
    pub fn has_process(&self, id: &str) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.sessions.contains_key(id) || live.launching.contains(id)
    }

    /// Whether any session matching is running or still being launched or resumed. Deleting a
    /// console or a project in that state is refused rather than silently killing them.
    pub fn has_live_sessions_where(&self, matches: impl Fn(&Session) -> bool) -> Result<bool> {
        for live in self.live_sessions() {
            if let Some(record) = self.store.get_session(&live.id)? {
                if matches(&record) {
                    return Ok(true);
                }
            }
        }
        self.has_launching_sessions_where(matches)
    }

    pub fn register_live(self: &Arc<Self>, session: Arc<LiveSession>) {
        session.enter_crash_cleanup();
        // Its holder can be out of reach for a while, the program going on: clients are told
        // (`Session::holder_link`, read when the record is published). Asked for before the
        // session is registered, so that no change between the two goes unpublished.
        let (state, live) = (Arc::downgrade(self), Arc::downgrade(&session));
        session.on_link_changed(move || {
            if let (Some(state), Some(live)) = (state.upgrade(), live.upgrade()) {
                state.publish_session_by_id(&live.id);
            }
        });
        self.process_ledger
            .launch_started(&session.launch_id, &session.id, session.process_id());
        let mut live = self.live.write().expect("live sessions lock poisoned");
        live.sessions.insert(session.id.clone(), session.clone());
        drop(live);
        // A launch that was past its claim when the daemon began to shut down: the stop of the
        // sessions has listed the running ones already, so this one is ended here. The exit
        // watcher records it interrupted, as for the rest. A planned stop ends nothing: the
        // session's terminal is left running in its holder for the next daemon.
        if self.shutting_down.load(Ordering::Acquire) && !self.is_handing_over() {
            let (late, ledger) = (session.clone(), Arc::clone(&self.process_ledger));
            std::thread::spawn(move || {
                ledger.scan_before_ending();
                late.end();
            });
        }
        session.observe_titles(self);
        if let Some(transport) = &session.transport {
            let (state, id) = (Arc::downgrade(self), session.id.clone());
            let process = Arc::downgrade(&session);
            let (held_state, held_id) = (state.clone(), id.clone());
            let (released_state, released_id) = (state.clone(), id.clone());
            transport.attach(crate::session::term::TransportEvents {
                // Off the caller's thread: the write blocks, and this can be called on the runtime.
                ready: Box::new(move || {
                    if let Some(state) = state.upgrade() {
                        let id = id.clone();
                        std::thread::spawn(move || state.flush_pending_outbox(&id));
                    }
                }),
                // Ended as the agent ending by itself would be: nobody asked for it.
                server_gone: Box::new(move || {
                    if let Some(process) = process.upgrade() {
                        std::thread::spawn(move || process.terminate());
                    }
                }),
                held: Box::new(move |notice| {
                    if let Some(state) = held_state.upgrade() {
                        state.broadcast(notice.about(&held_id));
                    }
                }),
                released: Box::new(move |code| {
                    if let Some(state) = released_state.upgrade() {
                        state.broadcast(crate::protocol::Event::SessionNoticeWithdrawn {
                            session: released_id.clone(),
                            code,
                        });
                    }
                }),
            });
        }
        // Its holder's link dropping loses what the program prints meanwhile: the screen is no
        // longer exact, which clients are told, and the program is made to draw it again.
        let (state, live) = (Arc::downgrade(self), Arc::downgrade(&session));
        session.on_resumed(move || {
            let (Some(state), Some(live)) = (state.upgrade(), live.upgrade()) else {
                return;
            };
            live.mark_screen_inexact();
            state.publish_session_by_id(&live.id);
            if let Err(err) = live.nudge_repaint() {
                tracing::warn!(session = %live.id, %err, "asking the program to redraw its screen failed");
            }
        });
        // Where it runs, for a start that cannot link to its holder to know the holder runs this
        // launch of the session.
        if let Some(held_at) = session.held_at() {
            let record = crate::store::HeldTerminalRecord {
                kind: crate::store::HeldKind::Session,
                owner_id: session.id.clone(),
                holder_id: held_at.holder_id,
                terminal_id: held_at.terminal_id,
                launch_id: session.launch_id.clone(),
            };
            if let Err(err) = self.store.record_held_terminal(&record) {
                tracing::warn!(session = %session.id, %err, "recording where the session's launch runs failed");
            }
        }
    }

    /// Drops a session from the live map — but only if it is still *this* session, which is what
    /// it returns. Comparing by id would let a finished session's exit watcher evict the live
    /// successor that reused its id. What follows the removal is [`Self::after_live_removed`]'s;
    /// `coordinator::exit_watch::unregister_live` runs the whole sequence.
    pub(crate) fn remove_live(&self, session: &Arc<LiveSession>) -> bool {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        let is_current = live
            .sessions
            .get(&session.id)
            .is_some_and(|current| Arc::ptr_eq(current, session));
        if is_current {
            live.sessions.remove(&session.id);
            // Under the same lock as the removal: a relaunch claiming this id the moment it leaves
            // the map would otherwise queue an instruction that this cleanup then threw away.
            // A held queue is its holder's to forward or discard.
            let dropped = if self.outbox.is_held(&session.id) {
                0
            } else {
                self.outbox.forget(&session.id)
            };
            if dropped > 0 {
                tracing::warn!(session = %session.id, dropped, "the session ended with messages still queued");
            }
        }
        is_current
    }

    /// What the daemon drops once session `id` has left the live map ([`Self::remove_live`]):
    /// the sharing status is published again when it was the current session (`was_current`),
    /// and its bookkeeping goes either way.
    pub(crate) fn after_live_removed(&self, id: &str, was_current: bool) {
        if was_current {
            self.publish_account_sharing();
        }
        self.forget_session_bookkeeping(id);
    }

    /// Whether a change of sharing has claimed `id` to restart it.
    pub(super) fn is_restarting(&self, id: &str) -> bool {
        self.live
            .read()
            .expect("live sessions lock poisoned")
            .sharing_locks
            .values()
            .any(|entry| entry.restarting.contains(id))
    }
}
