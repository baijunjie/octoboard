//! The facade over the per-session write queues in `crate::messaging::outbox`: queuing a message,
//! flushing the queue into the running agent, the holds on a queue, and the reasons nothing is
//! written into a session for now.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::AppState;
use crate::messaging::outbox::{Drain, PromptSignal, Ticket};
use crate::protocol::{notice_code, Agent, Notice, SessionStatus};
use crate::session::dialogs::Sighting;
use crate::session::LiveSession;

/// A session's queue paused by [`AppState::pause_outbox`].
pub struct OutboxPause {
    state: Arc<AppState>,
    id: String,
}

impl Drop for OutboxPause {
    fn drop(&mut self) {
        self.state.outbox.resume(&self.id);
        // Whatever was queued meanwhile waited on this; nothing else is coming to release it.
        if let Ok(session) = self.state.session_record(&self.id) {
            let status = self.state.agent_status(&session);
            self.state.spawn_flush_outbox(&self.id, status);
        }
    }
}

/// Why nothing is written into a running session for now; see [`AppState::write_hold`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteHold {
    /// The agent has not yet sent the signal that it is past its startup screens: its first hook,
    /// or for Codex its first terminal title; for Claude Code also bracketed paste coming on.
    Starting,
    /// The agent's folder-trust confirmation is up and has not been answered.
    TrustConfirmation,
    /// The agent is showing a dialog only the user can answer, which a message's paste and Enter
    /// would answer destructively; see [`crate::session::dialogs`].
    AgentDialog(crate::session::dialogs::Dialog),
    /// Codex's terminal title says it is waiting on the user.
    AgentAction,
    /// The agent stopped on an error and may be showing a prompt only the user can answer; held for
    /// [`crate::session::AGENT_ERROR_SETTLE`] or until a prompt is sent there, whichever is first.
    AgentError,
    /// The session is being handed off to another agent.
    Handoff,
    /// The session is being restarted for a change of its account's sharing, and is resumed
    /// afterwards.
    SharingChange,
    /// A message written into it was not sent and is still in its input box.
    UnsentMessage,
    /// A native submission may have been accepted; it must not be submitted again.
    SubmissionUncertain,
}

impl WriteHold {
    pub fn code(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::TrustConfirmation => "trust_confirmation",
            Self::AgentDialog(_) => "agent_dialog",
            Self::AgentAction => "agent_action",
            Self::AgentError => "agent_error",
            Self::Handoff => "handoff",
            Self::SharingChange => "sharing_change",
            Self::UnsentMessage => "unsent_message",
            Self::SubmissionUncertain => "submission_uncertain",
        }
    }

    /// Which dialog, for [`Self::AgentDialog`].
    pub fn dialog(self) -> Option<&'static str> {
        match self {
            Self::AgentDialog(dialog) => Some(dialog.code),
            _ => None,
        }
    }
}

/// A session's queue held by [`AppState::hold_outbox`]. While it lives the queue is kept, whatever
/// becomes of the session's process. Dropping it ends the hold, and where the queue goes depends on
/// whether the holder has named a successor ([`Self::forward_to`]): to the successor, in order,
/// or, without one, to the session itself if it is still running and nowhere if it is not, since a
/// session without a process takes nothing and would leave the queue stranded. A queue dropped for
/// that is announced as `handoff_messages_dropped`, or as `sharing_messages_dropped` for a hold taken
/// for a sharing change ([`AppState::hold_outbox_for_sharing`]): these are messages held back, never
/// ones part-written into a terminal, so the notice says nothing of an input line.
pub struct OutboxHold {
    state: Arc<AppState>,
    id: String,
    /// Held for a change of an account's sharing, not a handoff; only the notice for a drop differs.
    for_sharing: bool,
    successor: std::sync::Mutex<Option<String>>,
}

impl OutboxHold {
    /// From now on whatever is queued for the held session goes to `successor` when the hold ends.
    pub fn forward_to(&self, successor: &str) {
        *self.successor.lock().expect("hold lock poisoned") = Some(successor.to_string());
    }
}

impl Drop for OutboxHold {
    fn drop(&mut self) {
        let successor = self.successor.lock().expect("hold lock poisoned").take();
        let target = match &successor {
            Some(successor) => {
                for (text, _) in self.state.outbox.take(&self.id) {
                    self.state.outbox.push(successor, &text);
                }
                successor
            }
            None => &self.id,
        };
        self.state.outbox.release_hold(&self.id);
        let record = self.state.store.get_session(target).ok().flatten();
        // Judged by the process as well as the status: between a process leaving the live map and
        // the exit being saved the record still reads running.
        let writable = record
            .filter(|record| !record.status.has_no_process())
            .filter(|_| self.state.live_session(target).is_some());
        let Some(record) = writable else {
            // A successor being launched is no process yet but will be: a resume saves it running
            // and queues its instruction first. The launch flushes what is queued, and
            // `AppState::remove_live` forgets it if the launch fails.
            if successor.is_some() && self.state.is_launching(target) {
                return;
            }
            // Nothing to write to, and a queue left in the map would be replayed on a later resume.
            self.state.drop_held_queue(target, self.for_sharing);
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let (state, id) = (self.state.clone(), target.clone());
            let status = state.agent_status(&record);
            runtime.spawn_blocking(move || state.flush_outbox(&id, status));
        }
    }
}

/// How long a message written to an idle session is given to be reported sent before it is taken
/// to be sitting unsent in the input box. A prompt's `UserPromptSubmit` normally arrives well
/// within a second of the Enter that sent it.
pub(super) const SEND_WAIT: Duration = Duration::from_secs(3);

/// A running session's prompts as its hooks report them, for the outbox to tell a message the agent
/// sent from one it left in the input box.
struct HookPrompts<'a> {
    state: &'a AppState,
    live: &'a LiveSession,
}

impl HookPrompts<'_> {
    fn wait(&self) -> Option<Duration> {
        *self
            .state
            .send_wait
            .lock()
            .expect("send wait lock poisoned")
    }
}

impl PromptSignal for HookPrompts<'_> {
    fn mark_submission_uncertain(&self, before: u64) {
        self.live.mark_submission_uncertain(before);
    }

    fn submission_uncertain(&self) -> bool {
        self.live.submission_uncertain()
    }

    fn input_recovery_pending(&self) -> bool {
        self.live.input_recovery_pending().is_some()
    }

    fn sent(&self) -> u64 {
        self.live.prompts_sent()
    }

    /// Claude Code alone, the one agent seen to take a write and not send it. Read now rather than
    /// taken from the status the drain was started with, which can be stale by the time a message
    /// reaches the front: a write that lands mid-turn is queued by the agent and sent only when
    /// the turn ends, which no short wait would see.
    fn expects_send(&self) -> bool {
        self.wait().is_some()
            && self.live.agent == Agent::Claude
            && self
                .state
                .session_record(&self.live.id)
                .is_ok_and(|record| self.state.agent_status(&record) == SessionStatus::Idle)
    }

    fn wait_past(&self, before: u64) -> bool {
        let deadline = Instant::now() + self.wait().unwrap_or_default();
        while self.live.prompts_sent() <= before {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }

    fn mark_unsent(&self, before: u64) {
        self.live.mark_unsent(before);
    }

    fn unsent_waiting(&self) -> bool {
        self.live.unsent_waiting()
    }

    fn sighting(&self) -> Sighting {
        self.live.look_for_dialog()
    }

    fn watch_screen(&self, on: bool) {
        self.live.watch_screen(on);
    }

    fn trust_holds_writes(&self) -> bool {
        self.live.trust.holds_writes()
    }

    fn mid_turn(&self) -> bool {
        self.state
            .session_record(&self.live.id)
            .is_ok_and(|record| self.state.agent_status(&record) == SessionStatus::Working)
    }

    fn note_written_mid_turn(&self) {
        self.state.note_written_mid_turn(&self.live.id);
    }
}

impl AppState {
    /// Accepts a message for a session, to be written as soon as it can take one. Queued rather
    /// than written even when the session looks ready, so order is structural rather than something
    /// each caller has to get right.
    pub fn queue_message(&self, id: &str, text: &str) -> Ticket {
        self.quota_recovery.cancel(id);
        let ticket = self.outbox.push(id, text);
        self.note_activity(id);
        ticket
    }

    /// [`Self::queue_message`] for a message kept in the store as `row` (see
    /// [`Self::keep_message_if_not_running`]): the row is deleted once the message has been written
    /// into the session, not when it is queued, so a queue that is lost leaves it for the next
    /// resume.
    pub fn queue_kept_message(&self, id: &str, text: &str, row: i64) {
        if self.outbox.push_kept(id, text, row).is_some() {
            self.note_activity(id);
        }
    }

    /// How many messages are kept in the store for the session and not on the queue, which counts
    /// the ones that are (see [`Self::queued_count`]).
    pub fn kept_unqueued_count(&self, id: &str) -> Result<usize> {
        Ok(self
            .store
            .kept_messages(id)?
            .iter()
            .filter(|(row, _)| !self.outbox.is_row_queued(*row))
            .count())
    }

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

    /// [`Self::queue_message`] for a session a change of its account's sharing is restarting, and
    /// `None` for any other. The check and the push are one step under the `live` lock, which
    /// [`AccountSharingLock::end_restart`] takes to let the session go before it ends the hold, so
    /// a message is never pushed after the hold has dropped what it held.
    ///
    /// [`AccountSharingLock::end_restart`]: super::AccountSharingLock::end_restart
    pub(crate) fn queue_if_restarting(&self, id: &str, text: &str) -> Option<Ticket> {
        let ticket = {
            let live = self.live.read().expect("live sessions lock poisoned");
            live.sharing_locks
                .values()
                .any(|entry| entry.restarting.contains(id))
                .then(|| self.outbox.push(id, text))
        }?;
        // After the lock: it reads the live sessions again.
        self.note_activity(id);
        Some(ticket)
    }

    /// Tells the session's account a message queued, written or left unsent may have changed
    /// whether the session is working.
    fn note_activity(&self, id: &str) {
        if let Ok(Some(session)) = self.store.get_session(id) {
            self.note_sharing_activity(&session);
        }
    }

    /// Writes out whatever is queued for this session, and says what became of it.
    ///
    /// **Blocks** on the PTY write and on the wait for an idle Claude Code session to send what was
    /// written — twice over for a message a dialog discarded, which is written again in the same
    /// call — so callers on an async path go through [`Self::spawn_flush_outbox`].
    pub fn flush_outbox(&self, id: &str, status: SessionStatus) -> Drain {
        self.flush_outbox_once(id, status, true)
    }

    /// [`Self::flush_outbox`]; `again` says whether a message a dialog discarded may be written
    /// again here, which is allowed once per call so that a dialog every write brings back cannot
    /// keep the caller waiting.
    fn flush_outbox_once(&self, id: &str, status: SessionStatus, again: bool) -> Drain {
        // Each check below leaves the queue pending, and nothing retries a pending queue by itself:
        // whatever lifts a hold — a hook, a screen watch seeing the screen go, a timer, the person
        // answering in the terminal — has to call for a drain on every path it can lift by, and a
        // hold must never wait on an event that may not come. The same goes for a message put back
        // on the queue while no drain is running.
        //
        // A write while a modal dialog is up has its trailing Enter confirm whatever option is
        // highlighted. Check both the caller's hook state and the live interaction below.
        if !matches!(status, SessionStatus::Working | SessionStatus::Idle) {
            return Drain::Pending;
        }
        // And never a session on its way out. Archiving records the status before the process is
        // gone, so a hook from the dying process can arrive reporting work while the live entry it
        // would be drained through is still registered. Nothing is waiting on this answer: a sender
        // is refused before it ever queues for a session without a process, and what is held for an
        // interrupted one is in the store (`Store::add_kept_message`), not in the queue.
        match self.store.get_session(id) {
            Ok(Some(record)) if !record.status.has_no_process() => {}
            _ => return Drain::Pending,
        }
        let Some(live) = self.live_session(id) else {
            return Drain::Pending;
        };
        // Nor while the agent may be showing its trust screen, which the paste's trailing Enter
        // would accept (Codex, Grok Build) or decline (Claude Code); see `crate::session::trust`.
        // The press that answers it, its first hook (for Claude Code, with bracketed paste on by
        // then or coming on after it), the end of the watch or, for an agent whose hook may never
        // come, its fallback releases the queue.
        if live.trust.holds_writes() || live.needs_user_action() {
            return Drain::Pending;
        }
        // Nor while it shows a dialog of its own, looked for now rather than taken from the watch,
        // which polls: a dialog that came up since its last look would be answered by the paste.
        // The drain looks again before each message.
        if live.look_for_dialog().current.is_some() {
            return Drain::Pending;
        }
        // Nor just after an error the agent stopped on, which can leave a prompt that no hook
        // announces and the screen may not show yet; for `AGENT_ERROR_SETTLE`, or until a prompt
        // is sent.
        if live.agent_error() {
            return Drain::Pending;
        }
        let outcome = self.outbox.drain(
            &live,
            &HookPrompts {
                state: self,
                live: &live,
            },
        );
        // What was kept in the store is now in the session, or at least in its input box.
        let written = self.outbox.take_written();
        if !written.is_empty() {
            if let Err(err) = self.store.delete_kept_messages(&written) {
                tracing::warn!(
                    session = %id, %err,
                    "deleting information written into a session failed"
                );
            }
        }
        let still_running = self
            .live_session(id)
            .is_some_and(|current| Arc::ptr_eq(&current, &live));
        if matches!(outcome, Drain::Unsent(_)) && still_running {
            // The user can inspect a retry that submits the current composer, including any new
            // edits they made. A later message must not repeat the already written payload.
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_NOT_SENT,
                    format!(
                        "{} wrote a message into this session, but {} has not confirmed sending it. \
                         After three seconds its submit key is retried once, without repeating \
                         the text, and later messages may proceed. Answer pending dialogs first.",
                        crate::APP_NAME,
                        live.agent.label()
                    ),
                    &[("agent", live.agent.label()), ("submit_retry", "once")],
                )
                .about(id),
            );
        }
        if matches!(outcome, Drain::Diverted(_)) && still_running {
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_DIVERTED_BY_VIEW,
                    format!(
                        "{} was showing a view of its background agents while {} wrote a message \
                         into this session, so the message went there rather than to the \
                         conversation. Check the session.",
                        live.agent.label(),
                        crate::APP_NAME
                    ),
                    &[("agent", live.agent.label())],
                )
                .about(id),
            );
        }
        if matches!(outcome, Drain::Discarded(_)) && still_running {
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_DISCARDED_BY_DIALOG,
                    format!(
                        "{} showed a dialog while {} wrote a message into this session and \
                         discarded the message. {} will write it again once the dialog has been \
                         answered.",
                        live.agent.label(),
                        crate::APP_NAME,
                        crate::APP_NAME
                    ),
                    &[("agent", live.agent.label())],
                )
                .about(id),
            );
            // The dialog has usually been answered by the message's own Enter, and taken down,
            // within the write, and answering it runs no hook: with no drain under way any more
            // and no dialog left for the watch to see go, nothing else would write the message
            // again. One still up is left to the watch, which drains when it goes. What the second
            // drain came to is what the sender is told, except a pending one, which does not say
            // what was written: the first outcome still holds for whatever went in before the
            // discarded message. A second discard by a dialog gone before anything looks waits for
            // the next drain.
            if again && live.look_for_dialog().current.is_none() {
                if let Ok(record) = self.session_record(id) {
                    return match self.flush_outbox_once(id, self.agent_status(&record), false) {
                        Drain::Pending => outcome,
                        other => other,
                    };
                }
            }
        }
        if matches!(outcome, Drain::Uncertain(_)) && still_running {
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_ACCEPTANCE_UNCONFIRMED,
                    format!(
                        "{} did not confirm that it accepted a message {} wrote into this \
                         session. The message may already be queued and will not be sent \
                         again. Later messages are held for at most three seconds or until \
                         another prompt is sent; check the conversation before deciding whether \
                         to send it again.",
                        live.agent.label(),
                        crate::APP_NAME
                    ),
                    &[("agent", live.agent.label())],
                )
                .about(id),
            );
        }
        if outcome == Drain::Lost {
            // Nobody is waiting on a return value for most of these drains, and a message Octoboard
            // accepted and then could not deliver is the user's business — the more so because the
            // session's input line is the thing left in a state only they can see.
            self.broadcast(
                Notice::new(
                    notice_code::QUEUED_MESSAGES_DROPPED,
                    format!(
                        "{} dropped what it had queued for this session: {}",
                        crate::APP_NAME,
                        crate::session::term::FRAGMENT_HAZARD
                    ),
                    &[],
                )
                .about(id),
            );
        }
        self.note_activity(id);
        outcome
    }

    pub(crate) fn permits_automatic_input(&self, id: &str) -> bool {
        self.session_record(id).is_ok_and(|record| {
            matches!(
                self.agent_status(&record),
                SessionStatus::Idle | SessionStatus::Working
            ) && matches!(
                self.write_hold(id),
                None | Some(WriteHold::UnsentMessage | WriteHold::SubmissionUncertain)
            )
        })
    }

    pub(crate) fn retry_pending_outbox(self: &Arc<Self>, id: &str) {
        if self.outbox.len(id) > 0 {
            if let Ok(record) = self.session_record(id) {
                self.spawn_flush_outbox(id, self.agent_status(&record));
            }
        }
    }

    /// Drains this session's queue off the runtime. Used wherever releasing a message is a side
    /// effect of something else — a status change reported by a hook — and the caller must not wait
    /// for a PTY write: every agent gives its hooks a few seconds and renders a timed-out one to
    /// the user.
    pub fn spawn_flush_outbox(self: &Arc<Self>, id: &str, status: SessionStatus) {
        let state = self.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || state.flush_outbox(&id, status));
    }

    /// Holds a session's queue for as long as the guard lives: messages for it are accepted and
    /// queued, none is written. A handoff holds the original's, so that nothing reaches a session
    /// whose work is being moved; see [`OutboxHold`].
    pub fn hold_outbox(self: &Arc<Self>, id: &str) -> OutboxHold {
        self.hold_outbox_for(id, false)
    }

    /// Pauses the session's queue for as long as the guard lives: messages are accepted and queued,
    /// none is written, and the drain it lets go of is started when the guard drops. Unlike
    /// [`Self::hold_outbox`] it is no hold a client or a caller is told of: it lasts the few seconds
    /// an interrupt types into the session (`crate::session::interrupt`).
    pub fn pause_outbox(self: &Arc<Self>, id: &str) -> OutboxPause {
        self.outbox.pause(id);
        OutboxPause {
            state: self.clone(),
            id: id.to_string(),
        }
    }

    /// [`Self::hold_outbox`] for a session a change of its account's sharing restarts; the only
    /// difference is the notice for messages dropped when it is not relaunched.
    pub fn hold_outbox_for_sharing(self: &Arc<Self>, id: &str) -> OutboxHold {
        self.hold_outbox_for(id, true)
    }

    fn hold_outbox_for(self: &Arc<Self>, id: &str, for_sharing: bool) -> OutboxHold {
        self.outbox.hold(id, for_sharing);
        OutboxHold {
            state: self.clone(),
            id: id.to_string(),
            for_sharing,
            successor: std::sync::Mutex::new(None),
        }
    }

    /// Forgets what is queued for `id`, a session that will never take it, and says so as a
    /// `sharing_messages_dropped` notice when `for_sharing` or `handoff_messages_dropped` otherwise,
    /// when anything was queued.
    pub(super) fn drop_held_queue(&self, id: &str, for_sharing: bool) {
        let dropped = self.outbox.forget(id);
        if dropped == 0 {
            return;
        }
        let (code, during, why) = if for_sharing {
            (
                notice_code::SHARING_MESSAGES_DROPPED,
                "while its account's sharing changed",
                "the session was not restarted",
            )
        } else {
            (
                notice_code::HANDOFF_MESSAGES_DROPPED,
                "during a handoff",
                "the session has no process to receive them",
            )
        };
        self.broadcast(
            Notice::new(
                code,
                format!(
                    "{} dropped {dropped} message(s) held back for this session {during}, \
                     because {why}",
                    crate::APP_NAME
                ),
                &[],
            )
            .about(id),
        );
    }

    /// Removes and returns what is queued for a session, oldest first.
    #[cfg(test)]
    pub fn take_queued(&self, id: &str) -> Vec<String> {
        self.take_queued_with_rows(id)
            .into_iter()
            .map(|(text, _)| text)
            .collect()
    }

    /// Removes and returns what is queued for a session, oldest first, each message with the row of
    /// the store it is kept in, if it is (see [`Self::queue_kept_message`]).
    pub fn take_queued_with_rows(&self, id: &str) -> Vec<(String, Option<i64>)> {
        self.outbox.take(id)
    }

    /// How many messages are queued for a session and not yet written into it.
    pub fn queued_count(&self, id: &str) -> usize {
        self.outbox.len(id)
    }

    /// What keeps [`Self::flush_outbox`] from writing into a running session apart from the status
    /// a hook reported, which a caller reads off the record: a session can read as idle and still
    /// hold everything queued for it. `None` for one with no process running, or nothing holding
    /// it.
    pub fn write_hold(&self, id: &str) -> Option<WriteHold> {
        let live = self.live_session(id)?;
        if live.trust.holds_writes() {
            return Some(if live.trust.waiting() {
                WriteHold::TrustConfirmation
            } else {
                WriteHold::Starting
            });
        }
        if let Some(dialog) = live.look_for_dialog().current {
            Some(WriteHold::AgentDialog(dialog))
        } else if live.needs_user_action() {
            Some(WriteHold::AgentAction)
        } else if live.agent_error() {
            Some(WriteHold::AgentError)
        } else if self.outbox.is_held_for_sharing(id) {
            Some(WriteHold::SharingChange)
        } else if self.outbox.is_held(id) {
            Some(WriteHold::Handoff)
        } else if live.submission_uncertain() {
            Some(WriteHold::SubmissionUncertain)
        } else if live.unsent_waiting() {
            Some(WriteHold::UnsentMessage)
        } else {
            None
        }
    }

    /// Throws away anything queued for a session that will never take it — a relaunch that failed,
    /// so nothing would ever release what was queued ahead of it.
    pub fn discard_outbox(&self, id: &str) {
        self.outbox.forget(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::outbox::Drain;
    use crate::protocol::{Agent, Event, SessionStatus};
    use crate::state::test_support::{insert_record, session_record};
    use crate::test_support::{app_state, fake_live_session, PATIENCE};
    use std::time::{Duration, Instant};

    #[test]
    fn submit_retry_respects_hook_permissions_and_lifecycle_holds() {
        let (state, dir) = app_state("submit-retry-permissions");
        insert_record(&state, &dir, "s", SessionStatus::WaitingUser);
        let live = fake_live_session("s", Agent::Claude, 120, 40, "sleep 30");
        state.register_live(live.clone());
        assert!(!state.permits_automatic_input("s"));
        state.apply_hook_status("s", SessionStatus::Idle).unwrap();
        live.mark_unsent(0);
        assert!(state.permits_automatic_input("s"));
        state.outbox.hold("s", false);
        assert!(!state.permits_automatic_input("s"));
        state.outbox.release_hold("s");
        assert!(state.permits_automatic_input("s"));
        let mut record = state.session_record("s").unwrap();
        record.status = SessionStatus::Interrupted;
        state.save_session(&record).unwrap();
        assert!(!state.permits_automatic_input("s"));
    }

    #[tokio::test]
    async fn answering_a_codex_manual_approval_releases_queued_messages_off_the_reader() {
        let (state, dir) = app_state("codex-approval-outbox");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", Agent::Codex, 80, 24, "sleep 30").with_auto_review();
        state.register_live(live.clone());
        live.on_output(include_bytes!("../../testdata/codex_working_title.bin"));
        assert!(
            !live.needs_user_action(),
            "ordinary auto-review work raises no hand"
        );
        live.on_output(include_bytes!("../../testdata/codex_approval_title.bin"));
        state.queue_message("s", "Queued while waiting");
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Pending
        );
        assert_eq!(state.outbox.len("s"), 1);
        let reader = live.clone();
        std::thread::spawn(move || {
            reader.on_output(include_bytes!("../../testdata/codex_working_title.bin"));
        })
        .join()
        .unwrap();
        let deadline = Instant::now() + PATIENCE;
        while state.outbox.len("s") != 0 {
            assert!(
                Instant::now() < deadline,
                "answering must release the message without another hook"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!live.needs_user_action());
    }

    #[tokio::test]
    async fn a_prompt_hook_before_the_composer_redraw_is_retried_without_another_hook() {
        let (state, dir) = app_state("draft-redraw-retry");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", Agent::Claude, 120, 40, "sleep 30").with_composer();
        state.register_live(live.clone());
        live.on_output(include_bytes!("../../testdata/claude_input_restored.bin"));
        live.write_terminal_input(b"typed\r").unwrap();
        live.note_prompt_sent(Some("typed"));
        state.queue_message("s", "peer message");
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Pending
        );
        crate::session::dialogs::watch(&state, &live);
        live.on_output(b"\x1b[2J\x1b[H");
        live.on_output(include_bytes!(
            "../../testdata/claude_input_placeholder.bin"
        ));
        let deadline = Instant::now() + PATIENCE;
        while state.outbox.len("s") > 0 {
            assert!(
                Instant::now() < deadline,
                "the clean composer never released the queued message"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    const LAUNCH_DIALOG: &str = "  Resume this conversation?\r\n  \u{276f} 1. Resume\r\n    2. Start a new conversation\r\n  Enter to confirm \u{b7} Esc to resume\r\n";
    const CLEAR_SCREEN: &str = "\x1b[2J\x1b[H";

    /// A dialog on the screen holds the writes, shows the session as waiting for the user, and the
    /// queue goes in once the screen shows none.
    #[test]
    fn an_agent_dialog_on_the_screen_holds_writes_until_it_is_gone() {
        let (state, dir) = app_state("state-agent-dialog");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        assert_eq!(state.write_hold("s"), None);

        live.on_output(LAUNCH_DIALOG.as_bytes());
        let hold = state.write_hold("s").expect("held");
        assert_eq!(hold.code(), "agent_dialog");
        assert_eq!(hold.dialog(), Some("claude_resume_at_launch"));
        assert_eq!(
            state
                .wire_session(state.session_record("s").unwrap())
                .status,
            SessionStatus::WaitingUser
        );

        state.queue_message("s", "go on");
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Pending);
        assert_eq!(state.outbox.len("s"), 1);

        live.on_output(CLEAR_SCREEN.as_bytes());
        assert_eq!(state.write_hold("s"), None);
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Clear);
        assert_eq!(state.outbox.len("s"), 0);
    }

    /// The watch publishes the session when a dialog comes and goes, and writes out what waited.
    #[tokio::test]
    async fn the_dialog_watch_raises_the_hand_and_releases_the_queue() {
        let (state, dir) = app_state("state-agent-dialog-watch");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        crate::session::dialogs::watch(&state, &live);
        let mut events = state.subscribe();

        live.on_output(LAUNCH_DIALOG.as_bytes());
        let deadline = Instant::now() + PATIENCE;
        loop {
            match events.try_recv() {
                Ok(Event::SessionUpserted { session }) => {
                    assert_eq!(session.status, SessionStatus::WaitingUser);
                    break;
                }
                _ => {
                    assert!(Instant::now() < deadline, "the hand was not raised");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }

        state.queue_message("s", "go on");
        live.on_output(CLEAR_SCREEN.as_bytes());
        let deadline = Instant::now() + PATIENCE;
        while state.outbox.len("s") != 0 {
            assert!(Instant::now() < deadline, "the queue was not released");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// A launch dialog that comes up as a message is written discards it: the message is queued
    /// again, no unsent mark holds the queue, and the user is told what happened.
    #[test]
    fn a_message_a_launch_dialog_discarded_is_queued_again_and_announced() {
        let (state, dir) = app_state("state-agent-dialog-discard");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::from_secs(2));
        let mut events = state.subscribe();
        state.queue_message("s", "go on");
        state.queue_message("s", "and then");

        let screen = live.clone();
        let dialog = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            screen.on_output(LAUNCH_DIALOG.as_bytes());
        });
        let outcome = state.flush_outbox("s", SessionStatus::Idle);
        dialog.join().unwrap();

        assert_eq!(outcome, Drain::Discarded(0));
        assert_eq!(state.outbox.len("s"), 2);
        assert!(!live.unsent_waiting());
        assert!(matches!(
            state.write_hold("s"),
            Some(WriteHold::AgentDialog(_))
        ));
        let mut announced = None;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionNotice {
                session,
                code,
                params,
                ..
            } = event
            {
                announced = Some((session, code, params));
            }
        }
        let (session, code, params) = announced.expect("a notice");
        assert_eq!(
            (session.as_str(), code.as_str()),
            ("s", "message_discarded_by_dialog")
        );
        assert_eq!(params.get("agent").map(String::as_str), Some("Claude Code"));
    }

    /// The dialog a write closes within milliseconds is gone before anything looks at the screen
    /// after the write, and the message is still known to have been discarded: the screen is looked
    /// at on every chunk of output while the write is waited on. With the dialog gone, the message
    /// is written again at once.
    #[test]
    fn a_dialog_drawn_and_closed_inside_the_write_still_discards_the_message() {
        let (state, dir) = app_state("state-agent-dialog-closed");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::from_secs(1));
        state.queue_message("s", "go on");

        let screen = live.clone();
        let dialog = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            screen.on_output(LAUNCH_DIALOG.as_bytes());
            screen.on_output(CLEAR_SCREEN.as_bytes());
        });
        let outcome = state.flush_outbox("s", SessionStatus::Idle);
        dialog.join().unwrap();

        // Written again, and what the second write came to is the answer: the stand-in sends no
        // prompt, so this time it is left unsent.
        assert_eq!(outcome, Drain::Unsent(0));
        assert_eq!(state.outbox.len("s"), 0);
        assert!(live.unsent_waiting());
        assert_eq!(state.write_hold("s"), Some(WriteHold::UnsentMessage));
        assert_eq!(live.look_for_dialog().current, None);
    }

    /// A view of background agents takes the message it is written while it comes up: the message
    /// is not queued again and holds nothing, the sender is told it did not reach the conversation,
    /// and the user is told where it went.
    #[test]
    fn a_message_a_view_of_background_agents_took_is_announced_and_not_queued_again() {
        let (state, dir) = app_state("state-agent-view-diverted");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::from_secs(1));
        let mut events = state.subscribe();
        state.queue_message("s", "go on");
        state.queue_message("s", "and then");

        let screen = live.clone();
        let view = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            screen.on_output("  \u{25ef} main\r\n".as_bytes());
        });
        let outcome = state.flush_outbox("s", SessionStatus::Idle);
        view.join().unwrap();

        assert_eq!(outcome, Drain::Diverted(0));
        assert_eq!(state.outbox.len("s"), 1);
        assert!(!live.unsent_waiting());
        assert!(matches!(
            state.write_hold("s"),
            Some(WriteHold::AgentDialog(_))
        ));
        let mut announced = None;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionNotice { session, code, .. } = event {
                announced = Some((session, code));
            }
        }
        let (session, code) = announced.expect("a notice");
        assert_eq!(
            (session.as_str(), code.as_str()),
            ("s", "message_diverted_by_view")
        );
    }

    /// A flush has nothing to write to while no process is registered, and must not take the queue
    /// apart on the way to finding that out. The queue's own properties are
    /// `crate::messaging::outbox`'s to test; what matters here is that the status gate and the
    /// live-session check come first.
    #[test]
    fn a_flush_with_nowhere_to_write_leaves_the_queue_alone() {
        let (state, _dir) = app_state("state-outbox");
        state.queue_message("s", "first");
        state.queue_message("s", "second");

        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Pending);
        assert_eq!(
            state.flush_outbox("s", SessionStatus::WaitingUser),
            Drain::Pending
        );
        assert_eq!(state.outbox.len("s"), 2);

        // A relaunch that failed has nothing left that would ever release these.
        state.discard_outbox("s");
        assert_eq!(state.outbox.len("s"), 0);
    }

    /// A handoff holds the queue of the session it moves. The queue is not forgotten when that
    /// session's process ends meanwhile; it goes to the successor the holder names, and is
    /// discarded, not stranded, when the session has nowhere to be written and no successor.
    #[test]
    fn a_held_queue_outlives_its_sessions_process_and_goes_where_its_holder_says() {
        let (state, dir) = app_state("state-held-queue");
        insert_record(&state, &dir, "t", SessionStatus::Idle);
        let successor = crate::test_support::idle_stand_in("t");
        state.register_live(successor.clone());
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let hold = state.hold_outbox("s");
        state.queue_message("s", "one");
        let was_current = state.remove_live(&live);
        state.after_live_removed("s", was_current);
        assert_eq!(state.outbox.len("s"), 1);

        // No runtime here, so the successor's queue is left for a flush to write.
        hold.forward_to("t");
        drop(hold);
        assert_eq!((state.outbox.len("s"), state.outbox.len("t")), (0, 1));

        let hold = state.hold_outbox("u");
        state.queue_message("u", "two");
        drop(hold);
        assert_eq!(state.outbox.len("u"), 0);

        // A successor with no process takes nothing either, and keeps nothing for a later resume.
        let mut events = state.subscribe();
        let hold = state.hold_outbox("v");
        state.queue_message("v", "three");
        hold.forward_to("w");
        drop(hold);
        assert_eq!((state.outbox.len("v"), state.outbox.len("w")), (0, 0));
        match events.try_recv().expect("a notice") {
            Event::SessionNotice { session, code, .. } => {
                assert_eq!(
                    (session.as_str(), code.as_str()),
                    ("w", "handoff_messages_dropped")
                );
            }
            other => panic!("expected a notice, got {other:?}"),
        }

        // A hold taken for a sharing change says so when it drops what it held.
        let mut events = state.subscribe();
        let hold = state.hold_outbox_for_sharing("z");
        state.queue_message("z", "five");
        drop(hold);
        assert_eq!(state.outbox.len("z"), 0);
        match events.try_recv().expect("a notice") {
            Event::SessionNotice { session, code, .. } => {
                assert_eq!(
                    (session.as_str(), code.as_str()),
                    ("z", "sharing_messages_dropped")
                );
            }
            other => panic!("expected a notice, got {other:?}"),
        }

        // Unless it is being launched: the launch will flush what is queued for it.
        state
            .store
            .insert_session(&session_record("x", SessionStatus::Idle))
            .unwrap();
        let claim = state.begin_launch("x").unwrap();
        let hold = state.hold_outbox("y");
        state.queue_message("y", "four");
        hold.forward_to("x");
        drop(hold);
        assert_eq!(state.outbox.len("x"), 1);
        drop(claim);
    }
}
