//! `AppState`'s side of the per-session write queues in [`super::outbox`]: queuing a message,
//! flushing the queue into the running agent, the holds on a queue, and the reasons nothing is
//! written into a session for now.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::outbox::{Drain, FirstTicket, HoldKind, PromptSignal, Ticket};
use crate::protocol::{notice_code, AgentId, Event, Notice, SessionStatus};
use crate::session::dialogs::Sighting;
use crate::session::LiveSession;
use crate::state::AppState;

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

/// A session's first message, whose place is reserved before the session exists. The pause lives
/// until normal delivery has accepted it, or a failed handoff drops the reservation.
pub(crate) struct FirstMessage {
    pause: OutboxPause,
    ticket: Option<FirstTicket>,
}

impl FirstMessage {
    pub(crate) fn id(&self) -> &str {
        &self.pause.id
    }

    pub(crate) fn enqueue(&mut self, text: &str) -> Result<Ticket> {
        let first = self
            .ticket
            .take()
            .ok_or_else(|| anyhow::anyhow!("the first message was already queued"))?;
        self.pause.state.outbox.push_first(first, text)
    }
}

impl Drop for FirstMessage {
    fn drop(&mut self) {
        if let Some(first) = self.ticket.take() {
            self.pause.state.outbox.cancel_first(first);
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
    /// A turn stopped on the account's usage limit, and every write would open another that fails
    /// the same way; see [`crate::messaging::usage_limit`].
    UsageLimit,
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
            Self::UsageLimit => "usage_limit",
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
/// ones part-written into a terminal, so the notice says nothing of an input line. One a restart
/// held ([`AppState::hold_outbox_for_restart`]) goes as what is queued for any process that ends
/// goes, with nothing announced.
pub struct OutboxHold {
    state: Arc<AppState>,
    id: String,
    kind: HoldKind,
    successor: std::sync::Mutex<Option<String>>,
}

impl std::fmt::Debug for OutboxHold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutboxHold")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
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
        let for_sharing = self.kind == HoldKind::Sharing;
        let target = match &successor {
            // Already where it is to go.
            Some(successor) if *successor == self.id => successor,
            Some(successor) => {
                let mut lost = 0;
                for (text, _) in self.state.outbox.take(&self.id) {
                    if self.state.outbox.push(successor, &text).is_none() {
                        lost += 1;
                    }
                }
                // Too late for the successor: a planned stop has saved the queues already.
                self.state
                    .tell_held_dropped(successor, lost, for_sharing, true);
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
            if self.kind == HoldKind::Restart {
                let dropped = self.state.outbox.forget(target);
                if dropped > 0 {
                    tracing::warn!(session = %target, dropped, "the session ended with messages still queued");
                }
            } else {
                self.state.drop_held_queue(target, for_sharing);
            }
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let (state, id) = (self.state.clone(), target.clone());
            let status = state.agent_status(&record);
            runtime.spawn_blocking(move || state.flush_outbox(&id, status));
        }
    }
}

#[cfg(test)]
thread_local! {
    // Draw a fixture's screen after its message was written, before its send wait expires.
    static ON_SEND_WAIT: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

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

/// How long a message written when `before` prompts had been reported is given to be reported
/// sent, `wait` being what a later prompt of a launch is given: the first prompt's report comes
/// from the launch's first hook, which some hosts start cold
/// (`platform::shell::FIRST_HOOK_SLOWDOWN`).
fn send_wait(wait: Duration, before: u64) -> Duration {
    if before == 0 {
        wait * crate::platform::shell::FIRST_HOOK_SLOWDOWN
    } else {
        wait
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

    fn input_held(&self) -> bool {
        self.live.input_swap_active()
    }

    fn usage_limit_held(&self) -> bool {
        self.live.usage_limit.holds()
    }

    fn sent(&self) -> u64 {
        self.live.prompts_sent()
    }

    /// Only for a provider that declares its writes confirmed by its report of the prompt it sent
    /// (`DeliveryQuirks::confirms_by_reported_prompt`): Claude Code, the one agent seen to take a
    /// write and not send it. Read now rather than taken from the status the drain was started
    /// with, which can be stale by the time a message reaches the front: a write that lands
    /// mid-turn is queued by the agent and sent only when the turn ends, which no short wait would
    /// see.
    fn expects_send(&self) -> bool {
        self.wait().is_some()
            && self
                .live
                .agent
                .descriptor()
                .capabilities
                .delivery
                .confirms_by_reported_prompt
            && self
                .state
                .session_record(&self.live.id)
                .is_ok_and(|record| self.state.agent_status(&record) == SessionStatus::Idle)
    }

    fn wait_past(&self, before: u64) -> bool {
        #[cfg(test)]
        ON_SEND_WAIT.with(|on_wait| {
            let on_wait = on_wait.borrow_mut().take();
            if let Some(on_wait) = on_wait {
                on_wait();
            }
        });
        let deadline = Instant::now() + send_wait(self.wait().unwrap_or_default(), before);
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

    fn mark_not_submitted(&self, before: u64) {
        self.live.mark_not_submitted(before);
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

    fn writes_stopped(&self) -> bool {
        self.state.handover_barrier_raised()
    }
}

impl AppState {
    /// Counts a prompt the agent of `live` reported sending ([`LiveSession::note_prompt_sent`]),
    /// and withdraws the `message_not_sent` notice of a message it confirms was sent after all.
    pub fn note_prompt_sent(&self, live: &LiveSession, prompt: Option<&str>) {
        if live.note_prompt_sent(prompt) {
            self.broadcast(Event::SessionNoticeWithdrawn {
                session: live.id.clone(),
                code: notice_code::MESSAGE_NOT_SENT,
            });
        }
    }

    /// Accepts a message for a session, to be written as soon as it can take one. Queued rather
    /// than written even when the session looks ready, so order is structural rather than something
    /// each caller has to get right.
    ///
    /// `None`, with nothing queued, once a planned stop has saved the queues for the next daemon
    /// ([`super::outbox::Outbox::push`]).
    pub fn queue_message(&self, id: &str, text: &str) -> Option<Ticket> {
        self.cancel_recoveries_for_queued(id);
        let ticket = self.outbox.push(id, text)?;
        self.note_activity(id);
        Some(ticket)
    }

    /// [`Self::queue_message`], unless a planned stop has saved the queues for the next daemon:
    /// then `None`, and nothing is queued (see [`super::outbox::Outbox::push_unless_sealed`]).
    pub fn queue_message_unless_sealed(&self, id: &str, text: &str) -> Option<Ticket> {
        self.cancel_recoveries_for_queued(id);
        let ticket = self.outbox.push_unless_sealed(id, text)?;
        self.note_activity(id);
        Some(ticket)
    }

    /// [`Self::queue_message_unless_sealed`] for the user's own message, which a hold for the
    /// account's usage limit does not hold (see [`super::outbox::Outbox::push_past_usage_limit`]).
    pub(crate) fn queue_users_message(&self, id: &str, text: &str) -> Option<Ticket> {
        self.cancel_recoveries_silently(id);
        let ticket = self.outbox.push_past_usage_limit(id, text)?;
        self.note_activity(id);
        Some(ticket)
    }

    /// Reserves a new session's first automatic message before the session can receive reports.
    pub(crate) fn reserve_first_message(self: &Arc<Self>, id: &str) -> Result<FirstMessage> {
        anyhow::ensure!(
            self.store.get_session(id)?.is_none(),
            "a first message must be reserved before the session exists"
        );
        let pause = self.pause_outbox(id);
        let ticket = self.outbox.reserve_first(id)?;
        Ok(FirstMessage {
            pause,
            ticket: Some(ticket),
        })
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

    /// Tells the session of each of its messages the agent's own queue refused that it has not been
    /// told of yet; whichever caller asks first tells it.
    pub(crate) fn announce_refusals(&self, id: &str, agent: AgentId) {
        for (_, reason) in self.outbox.refusals_to_announce(id) {
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_REFUSED,
                    format!(
                        "{} refused a message {} wrote for this session, so it was not sent: \
                         {reason}. Later messages are still sent.",
                        agent.label(),
                        crate::APP_NAME
                    ),
                    &[("agent", agent.label()), ("detail", &reason)],
                )
                .about(id),
            );
        }
    }

    /// Writes out whatever is queued for this session, and says what became of it.
    ///
    /// **Blocks** on the PTY write and on the wait for an idle Claude Code session to send what was
    /// written — twice over for a message a dialog discarded, which is written again in the same
    /// call — so callers on an async path go through [`Self::spawn_flush_outbox`].
    ///
    /// Writes nothing once a planned stop has raised its handover barrier: the queue stays, for the
    /// stop to save for the next daemon. A drain under way then is waited for, and stops before its
    /// next message.
    pub fn flush_outbox(&self, id: &str, status: SessionStatus) -> Drain {
        let Ok(_admission) = self.admit_change() else {
            return Drain::Pending;
        };
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
        self.announce_refusals(id, live.agent);
        if matches!(outcome, Drain::Unsent(_)) && still_running {
            // The user can inspect a retry that submits the current composer, including any new
            // edits they made. A later message must not repeat the already written payload. A
            // prompt reported after the message withdraws the notice.
            live.notice_unsent(|retry| {
                let notice = if retry {
                    Notice::new(
                        notice_code::MESSAGE_NOT_SENT,
                        format!(
                            "{} wrote a message into this session, but {} has not confirmed \
                             sending it. After three seconds its submit key is retried once, \
                             without repeating the text, and later messages may proceed. Answer \
                             pending dialogs first.",
                            crate::APP_NAME,
                            live.agent.label()
                        ),
                        &[("agent", live.agent.label()), ("submit_retry", "once")],
                    )
                } else {
                    Notice::new(
                        notice_code::MESSAGE_NOT_SENT,
                        format!(
                            "{} did not send a message {} wrote into this session, so it is \
                             waiting in the session's input box. {} writes nothing more into the \
                             session until a prompt is sent.",
                            live.agent.label(),
                            crate::APP_NAME,
                            crate::APP_NAME
                        ),
                        &[("agent", live.agent.label())],
                    )
                };
                self.broadcast(notice.about(id));
            });
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
        // A message `Drain::Fragmented` lost was announced as `typing_blocked`, which says what
        // holds there; this notice's warning that the next message runs together with it does
        // not, and nothing behind it was dropped.
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
                None | Some(
                    WriteHold::UnsentMessage
                        | WriteHold::SubmissionUncertain
                        | WriteHold::UsageLimit
                )
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

    /// [`Self::retry_pending_outbox`] on the calling thread, for a caller off the runtime that may
    /// block on the write.
    pub(crate) fn flush_pending_outbox(&self, id: &str) {
        if self.outbox.len(id) > 0 {
            if let Ok(record) = self.session_record(id) {
                self.flush_outbox(id, self.agent_status(&record));
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
        self.hold_outbox_for(id, HoldKind::Handoff)
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
        self.hold_outbox_for(id, HoldKind::Sharing)
    }

    /// [`Self::hold_outbox`] for a session a switch of its account or a change of its model
    /// restarts: what is queued is kept across the end of its process and written into the process
    /// a relaunch starts, behind that relaunch's instruction ([`Self::queue_relaunch_instruction`]),
    /// once it has started ([`Self::relaunched_after_restart`]). It is not reported in
    /// [`Self::write_hold`], the session being shown as switching or restarting meanwhile, and it
    /// does not hold back what the user types into the terminal.
    pub(crate) fn hold_outbox_for_restart(self: &Arc<Self>, id: &str) -> OutboxHold {
        self.hold_outbox_for(id, HoldKind::Restart)
    }

    fn hold_outbox_for(self: &Arc<Self>, id: &str, kind: HoldKind) -> OutboxHold {
        self.outbox.hold(id, kind);
        OutboxHold {
            state: self.clone(),
            id: id.to_string(),
            kind,
            successor: std::sync::Mutex::new(None),
        }
    }

    /// Forgets what is queued for `id`, a session that will never take it, and says so as a
    /// `sharing_messages_dropped` notice when `for_sharing` or `handoff_messages_dropped` otherwise,
    /// when anything was queued.
    pub(crate) fn drop_held_queue(&self, id: &str, for_sharing: bool) {
        let dropped = self.outbox.forget(id);
        self.tell_held_dropped(id, dropped, for_sharing, false);
    }

    /// Tells every client that `dropped` messages held back for `id` were dropped, as
    /// [`Self::drop_held_queue`] does, when there were any; with `stopping`, because a planned
    /// stop had saved the queues for the next daemon before they could be forwarded to `id`.
    fn tell_held_dropped(&self, id: &str, dropped: usize, for_sharing: bool, stopping: bool) {
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
        let why = if stopping {
            "the daemon is stopping and had already saved its queued messages for the next daemon"
        } else {
            why
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
        } else if self.outbox.held_for(id) == Some(HoldKind::Sharing) {
            Some(WriteHold::SharingChange)
        } else if self.outbox.held_for(id) == Some(HoldKind::Handoff) {
            Some(WriteHold::Handoff)
        } else if live.usage_limit.confirmed() {
            Some(WriteHold::UsageLimit)
        } else if live.submission_uncertain() {
            Some(WriteHold::SubmissionUncertain)
        } else if live.unsent_waiting() {
            Some(WriteHold::UnsentMessage)
        } else {
            None
        }
    }

    /// Queues `instruction`, what a relaunch of the session `id` gives it: at the front, ahead of
    /// what a restart that holds the queue carries over ([`Self::hold_outbox_for_restart`]), and
    /// otherwise like any message.
    pub(crate) fn queue_relaunch_instruction(&self, id: &str, instruction: &str) {
        if self.outbox.held_for(id) == Some(HoldKind::Restart) {
            if self
                .outbox
                .push_relaunch_instruction(id, instruction)
                .is_some()
            {
                self.note_activity(id);
            } else {
                tracing::warn!(session = %id, "the relaunch instruction was refused by a sealed queue");
            }
        } else {
            self.queue_message(id, instruction);
        }
    }

    /// The relaunch of the session `id` has started its process: what a restart holds for it is
    /// written into that process from now on, as into any session, though still kept if the
    /// process ends. Whether a restart held its queue.
    pub(crate) fn relaunched_after_restart(&self, id: &str) -> bool {
        let restarted = self.outbox.held_for(id) == Some(HoldKind::Restart);
        if restarted {
            self.outbox.hold(id, HoldKind::Relaunched);
        }
        restarted
    }

    /// The relaunch of the session `id` after a restart failed: its instruction is taken back, and
    /// what the restart carried over is held again for whatever brings the session back up.
    /// Whether a restart holds the queue.
    pub(crate) fn relaunch_failed_after_restart(&self, id: &str) -> bool {
        if !matches!(
            self.outbox.held_for(id),
            Some(HoldKind::Restart | HoldKind::Relaunched)
        ) {
            return false;
        }
        self.outbox.withdraw_relaunch_instruction(id);
        self.outbox.hold(id, HoldKind::Restart);
        true
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
    use crate::protocol::{AgentId, Code, Event, SessionStatus};
    use crate::state::test_support::{insert_record, session_record};
    use crate::test_support::{app_state, fake_live_session, PATIENCE};
    use std::time::{Duration, Instant};

    #[test]
    fn the_first_prompt_of_a_launch_is_waited_on_as_its_host_starts_a_first_hook() {
        let wait = Duration::from_secs(3);
        assert_eq!(
            send_wait(wait, 0),
            wait * crate::platform::shell::FIRST_HOOK_SLOWDOWN
        );
        assert_eq!(send_wait(wait, 1), wait);
        assert_eq!(send_wait(wait, 7), wait);
        assert_eq!(send_wait(Duration::ZERO, 0), Duration::ZERO);
        assert_eq!(
            crate::platform::shell::FIRST_HOOK_SLOWDOWN == 1,
            !cfg!(windows),
            "only Windows starts a first hook cold"
        );
    }

    #[test]
    fn submit_retry_respects_hook_permissions_and_lifecycle_holds() {
        let (state, dir) = app_state("submit-retry-permissions");
        insert_record(&state, &dir, "s", SessionStatus::WaitingUser);
        let live = fake_live_session("s", AgentId::CLAUDE, 120, 40, "sleep 30");
        state.register_live(live.clone());
        assert!(!state.permits_automatic_input("s"));
        state.apply_hook_status("s", SessionStatus::Idle).unwrap();
        live.mark_unsent(0);
        assert!(state.permits_automatic_input("s"));
        state.outbox.hold("s", HoldKind::Handoff);
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
        let live = fake_live_session("s", AgentId::CODEX, 80, 24, "sleep 30").with_auto_review();
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
        let live = fake_live_session("s", AgentId::CLAUDE, 120, 40, "sleep 30").with_composer();
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
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
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
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
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
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::ZERO);
        let mut events = state.subscribe();
        state.queue_message("s", "go on");
        state.queue_message("s", "and then");

        let screen = live.clone();
        ON_SEND_WAIT.with(|on_wait| {
            *on_wait.borrow_mut() = Some(Box::new(move || {
                screen.on_output(LAUNCH_DIALOG.as_bytes());
            }))
        });
        let outcome = state.flush_outbox("s", SessionStatus::Idle);

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
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::ZERO);
        state.queue_message("s", "go on");

        let screen = live.clone();
        ON_SEND_WAIT.with(|on_wait| {
            *on_wait.borrow_mut() = Some(Box::new(move || {
                screen.on_output(LAUNCH_DIALOG.as_bytes());
                screen.on_output(CLEAR_SCREEN.as_bytes());
            }))
        });
        let before = live.input_writes();
        let outcome = state.flush_outbox("s", SessionStatus::Idle);

        // Written again, and what the second write came to is the answer: the stand-in sends no
        // prompt, so this time it is left unsent.
        assert_eq!(
            live.input_writes(),
            before + 2,
            "the discarded message was written again"
        );
        assert_eq!(outcome, Drain::Unsent(0));
        assert_eq!(state.outbox.len("s"), 0);
        assert!(live.unsent_waiting());
        assert_eq!(state.write_hold("s"), Some(WriteHold::UnsentMessage));
        assert_eq!(live.look_for_dialog().current, None);
    }

    /// The user is told a message was not sent, and a prompt the agent reports after it, which
    /// says it was sent after all, withdraws that; a later prompt withdraws nothing more.
    #[test]
    fn a_late_report_of_an_unsent_message_withdraws_its_notice() {
        let (state, dir) = app_state("state-unsent-withdrawn");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::ZERO);
        let mut events = state.subscribe();
        state.queue_message("s", "go on");

        assert_eq!(
            state.flush_outbox("s", SessionStatus::Idle),
            Drain::Unsent(0)
        );
        let mut drained = Vec::new();
        while let Ok(event) = events.try_recv() {
            drained.push(event);
        }
        let notice = drained.iter().find_map(|event| match event {
            Event::SessionNotice {
                session,
                code,
                params,
                ..
            } => Some((
                session.clone(),
                code.as_str(),
                params.get("submit_retry").cloned(),
            )),
            _ => None,
        });
        assert_eq!(
            notice,
            Some((
                "s".to_string(),
                "message_not_sent",
                Some("once".to_string())
            ))
        );

        state.note_prompt_sent(&live, Some("go on"));
        let withdrawn: Vec<(String, Code)> = std::iter::from_fn(|| events.try_recv().ok())
            .filter_map(|event| match event {
                Event::SessionNoticeWithdrawn { session, code } => Some((session, code)),
                _ => None,
            })
            .collect();
        assert_eq!(
            withdrawn,
            vec![("s".to_string(), notice_code::MESSAGE_NOT_SENT)]
        );
        assert!(!live.unsent_waiting());

        state.note_prompt_sent(&live, Some("later"));
        assert!(std::iter::from_fn(|| events.try_recv().ok())
            .all(|event| !matches!(event, Event::SessionNoticeWithdrawn { .. })));
    }

    /// A view of background agents takes the message it is written while it comes up: the message
    /// is not queued again and holds nothing, the sender is told it did not reach the conversation,
    /// and the user is told where it went.
    #[test]
    fn a_message_a_view_of_background_agents_took_is_announced_and_not_queued_again() {
        let (state, dir) = app_state("state-agent-view-diverted");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::ZERO);
        let mut events = state.subscribe();
        state.queue_message("s", "go on");
        state.queue_message("s", "and then");

        let screen = live.clone();
        ON_SEND_WAIT.with(|on_wait| {
            *on_wait.borrow_mut() = Some(Box::new(move || {
                screen.on_output("  \u{25ef} main\r\n".as_bytes());
            }))
        });
        let outcome = state.flush_outbox("s", SessionStatus::Idle);

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

    /// While a turn's usage limit holds a session, Octoboard's messages stay queued, the user's own
    /// go past them, the hold is reported after the holds that come before it, and queuing does not
    /// end the automatic switch's wait for quota; once it lets go the held ones follow in order.
    #[test]
    fn a_usage_limit_hold_keeps_octoboard_s_messages_and_lets_the_user_s_through() {
        let (state, dir) = app_state("state-usage-limit-hold");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        let judgement = live.usage_limit.begin_judging(1);
        live.usage_limit.judged(judgement, true);
        assert_eq!(state.write_hold("s"), Some(WriteHold::UsageLimit));
        assert!(
            state.permits_automatic_input("s"),
            "a draft is still restored"
        );

        state.quota_recovery.wait_for_test(&state, "s");
        assert!(state.quota_recovery.is_waiting_for_test("s"));
        state.queue_message("s", "from the owner");
        assert!(
            state.quota_recovery.is_waiting_for_test("s"),
            "a held message opens no turn"
        );
        let before = live.input_writes();
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Idle),
            Drain::HeldForUsageLimit
        );
        assert_eq!(live.input_writes(), before);

        state.queue_users_message("s", "mine");
        assert!(!state.quota_recovery.is_waiting_for_test("s"));
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Idle),
            Drain::HeldForUsageLimit
        );
        assert_eq!(live.input_writes(), before + 1, "the user's message went");
        assert_eq!(state.outbox.len("s"), 1);

        // A handoff's hold comes first.
        let hold = state.hold_outbox("s");
        assert_eq!(state.write_hold("s"), Some(WriteHold::Handoff));
        drop(hold);
        // A restart's is not reported at all.
        let hold = state.hold_outbox_for_restart("s");
        assert_eq!(state.write_hold("s"), Some(WriteHold::UsageLimit));
        hold.forward_to("s");
        drop(hold);
        assert_eq!(state.outbox.len("s"), 1, "kept where it was");

        assert!(live.usage_limit.release_armed_at(1));
        assert_eq!(state.write_hold("s"), None);
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Clear);
        assert_eq!(live.input_writes(), before + 2);
    }

    /// A restart's hold keeps the queue across the end of the process it restarts, and leaves it
    /// for the relaunch when told to; without a relaunch it goes, with nothing announced.
    #[test]
    fn a_restart_s_hold_keeps_the_queue_for_the_relaunched_process() {
        let (state, dir) = app_state("state-restart-hold");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let hold = state.hold_outbox_for_restart("s");
        state.queue_message("s", "one");
        let was_current = state.remove_live(&live);
        state.after_live_removed("s", was_current);
        assert_eq!(state.outbox.len("s"), 1);
        let claim = state.begin_launch("s").unwrap();
        hold.forward_to("s");
        drop(hold);
        assert_eq!(state.outbox.len("s"), 1, "left for the launch to write");
        drop(claim);

        let mut events = state.subscribe();
        let hold = state.hold_outbox_for_restart("s");
        drop(hold);
        assert_eq!(state.outbox.len("s"), 0);
        assert!(events.try_recv().is_err(), "nothing announced");
    }

    /// The hold for the usage limit has its place among the reasons nothing is written: after the
    /// agent's dialogs, actions and errors and after a sharing change or a handoff, before a
    /// submission that may have been accepted and a message left unsent.
    #[test]
    fn a_usage_limit_hold_takes_its_place_among_the_reasons_for_a_hold() {
        let (state, dir) = app_state("state-usage-limit-precedence");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        let judgement = live.usage_limit.begin_judging(1);
        live.usage_limit.judged(judgement, true);
        live.mark_unsent(live.prompts_sent());
        live.mark_submission_uncertain(live.prompts_sent());
        assert_eq!(
            state.write_hold("s"),
            Some(WriteHold::UsageLimit),
            "before an uncertain submission and an unsent message"
        );
        let hold = state.write_hold("s").unwrap();
        assert_eq!(hold.code(), "usage_limit");
        assert_eq!(hold.dialog(), None);

        let handoff = state.hold_outbox("s");
        assert_eq!(state.write_hold("s"), Some(WriteHold::Handoff));
        let sharing = state.hold_outbox_for_sharing("s");
        assert_eq!(state.write_hold("s"), Some(WriteHold::SharingChange));
        live.mark_agent_error();
        assert_eq!(state.write_hold("s"), Some(WriteHold::AgentError));
        live.on_output(LAUNCH_DIALOG.as_bytes());
        assert!(matches!(
            state.write_hold("s"),
            Some(WriteHold::AgentDialog(_))
        ));
        drop((handoff, sharing));
    }

    /// Codex's usage prompt, drawn as its screen shows when the account is out of credits.
    const CODEX_CREDIT_LIMIT: &str = "You've reached your workspace credit limit\r\nYour workspace is out of credits. Ask your workspace owner to add more. Your included usage resets at 10:09 on 14 Nov.\r\n\u{203a} 1. Request credits\r\nPress a number to choose\r\n";

    /// While Codex shows its usage prompt, everything Octoboard writes stays queued, since the
    /// Enter ending a message would pick "Request credits"; the session reads as waiting for the
    /// user, but is still one the automatic account switch may switch; once the prompt is gone the
    /// queue is written, oldest first.
    #[test]
    fn codex_s_usage_prompt_holds_the_queue_until_it_is_gone() {
        use crate::test_support::{idle_stand_in_of, AcceptingServer};
        let (state, dir) = app_state("state-codex-usage-prompt");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let mut record = state.session_record("s").unwrap();
        record.agent = AgentId::CODEX;
        state.store.update_session(&record).unwrap();
        let server = AcceptingServer::new();
        let live = idle_stand_in_of("s", AgentId::CODEX).with_transport(server.clone());
        state.register_live(live.clone());
        assert_eq!(state.write_hold("s"), None);

        live.on_output(CODEX_CREDIT_LIMIT.as_bytes());
        let hold = state.write_hold("s").expect("held");
        assert_eq!(hold.code(), "agent_dialog");
        assert_eq!(hold.dialog(), Some("codex_usage_limit"));
        assert_eq!(
            state
                .wire_session(state.session_record("s").unwrap())
                .status,
            SessionStatus::WaitingUser
        );
        assert!(live.waits_for_user());
        assert!(
            !live.waits_for_user_beyond_usage_limit(),
            "the automatic switch still treats it as switchable"
        );

        state.queue_message("s", "first");
        state.queue_message("s", "second");
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Pending);
        assert!(server.offered().is_empty(), "{:?}", server.offered());
        assert_eq!(state.outbox.len("s"), 2);

        live.on_output(CLEAR_SCREEN.as_bytes());
        assert_eq!(state.write_hold("s"), None);
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Clear);
        assert_eq!(server.offered(), ["first", "second"]);
    }

    /// A relaunch after a restart puts its instruction ahead of what the restart carried over, and
    /// what is queued is written once the relaunch has started a process; a relaunch that fails
    /// takes its instruction back and leaves the rest held for a restore.
    #[test]
    fn a_relaunch_s_instruction_goes_ahead_of_what_a_restart_carried() {
        let (state, dir) = app_state("state-relaunch-instruction");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let hold = state.hold_outbox_for_restart("s");
        state.queue_message("s", "carried");
        state.queue_relaunch_instruction("s", "carry on");
        assert!(state.outbox.blocks_writes("s"));

        assert!(state.relaunch_failed_after_restart("s"));
        assert_eq!(state.outbox.len("s"), 1, "the instruction is taken back");
        assert!(state.outbox.blocks_writes("s"));

        state.queue_relaunch_instruction("s", "carry on again");
        assert!(state.relaunched_after_restart("s"));
        assert!(
            !state.outbox.blocks_writes("s"),
            "written into the relaunch"
        );
        assert!(state.outbox.is_held("s"), "and kept if it ends");
        assert_eq!(state.take_queued("s"), ["carry on again", "carried"]);
        drop(hold);

        // With no restart holding the queue, an instruction is queued like any message.
        state.queue_message("s", "first");
        state.queue_relaunch_instruction("s", "then this");
        assert!(!state.relaunch_failed_after_restart("s"));
        assert_eq!(state.take_queued("s"), ["first", "then this"]);
    }
}
