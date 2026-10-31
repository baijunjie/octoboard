//! The per-session FIFO for automatic messages, with one drainer per session.
//!
//! Each ticket keeps its first delivery-attempt time across retries. Draft protection can delay
//! writing only for a bounded grace period; afterwards a write may include unfinished user input.
//! Lifecycle holds, trust confirmations and agent dialogs remain separate reasons to wait.
//!
//! A physical partial write cannot be repeated without duplicating its accepted prefix. It and
//! the queue behind it are reported lost, because another paste would join the unterminated one.
//! An uncertain native submission instead consumes only its own ticket and retains later ones:
//! there is no terminal fragment, but repeating the request could deliver the message twice.
//!
//! A complete write missing its prompt acknowledgement briefly holds later messages. Its payload
//! never goes back into the queue; the running session owns the bounded submit-key retry.
//! For the delivery contract see docs/product/message-delivery.md's
//! "Messages held until a session can take them" section.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::agent_dialogs::Sighting;
use crate::ptyio::PartialWrite;
use crate::session::LiveSession;
use crate::term;

/// How long draft preservation may delay an automatic message before combined delivery is allowed.
pub(crate) const DRAFT_WAIT: Duration = Duration::from_secs(3);

/// What a drain amounted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drain {
    /// Nothing is left queued for this session.
    Clear,
    /// Something is still queued, and a later drain will take it.
    Pending,
    /// A message was written in part, so it is gone and the queue behind it with it.
    Lost,
    /// The native queue may have accepted this ticket; it is never submitted again.
    Uncertain(Ticket),
    /// The message with this ticket was written, but the agent did not report sending it. Its text
    /// is not repeated; later messages wait for a prompt acknowledgement or a bounded submit retry.
    Unsent(Ticket),
    /// The message with this ticket was written while a dialog came up that threw it away. It is
    /// back at the front of the queue, which waits for the dialog to be answered.
    Discarded(Ticket),
    /// The message with this ticket was written while a view of the agent's background agents came
    /// up, which took it in place of the conversation. It is not queued again; the rest of the
    /// queue waits for the view to be left.
    Diverted(Ticket),
}

/// What [`Outbox::push`] hands back for a message, for telling it apart in a drain's outcome.
pub type Ticket = u64;

/// What a drain learns from the agent about whether what it wrote was sent.
pub trait PromptSignal {
    /// How many prompts the agent has reported sending so far.
    fn sent(&self) -> u64;
    /// Whether a message written now should be followed by a report of it being sent: the agent
    /// reports each prompt it sends, and is at its prompt rather than mid-turn.
    fn expects_send(&self) -> bool;
    /// Waits, for a bounded time, until [`Self::sent`] has passed `before`; says whether it did.
    fn wait_past(&self, before: u64) -> bool;
    /// Records that a message written when the count stood at `before` was not sent.
    fn mark_unsent(&self, before: u64);
    /// Whether a message marked unsent still holds delivery before acknowledgement or submit retry.
    fn unsent_waiting(&self) -> bool;
    fn mark_submission_uncertain(&self, before: u64) {
        self.mark_unsent(before);
    }
    fn submission_uncertain(&self) -> bool {
        false
    }
    /// Looks at the agent's screen now: whether it shows a dialog only the user can answer, which
    /// a message written into it would answer destructively, and how many times dialogs of each
    /// kind have come up so far, including any that have gone again.
    fn sighting(&self) -> Sighting;
    /// Starts (`true`) or ends (`false`) having the screen looked at on every chunk of output, so
    /// that a dialog drawn and closed between two calls of [`Self::sighting`] is still counted.
    fn watch_screen(&self, on: bool);
    /// Whether the agent's folder-trust screen has been sighted and not answered, which a message
    /// written into it would answer.
    fn trust_holds_writes(&self) -> bool;
    /// Whether the agent is in the middle of a turn, read before a message is written so the
    /// message that opens a turn is not taken for one written into it.
    fn mid_turn(&self) -> bool {
        false
    }
    /// Records that a message went into the agent in the middle of a turn.
    fn note_written_mid_turn(&self) {}
    /// A complete message reached the CLI, whose native draft restoration is still pending.
    fn input_recovery_pending(&self) -> bool {
        false
    }
}

/// A screen watch that ends when it is dropped, so no return from a drain leaves it on.
struct ScreenWatch<'a, P: PromptSignal>(&'a P);

impl<'a, P: PromptSignal> ScreenWatch<'a, P> {
    fn start(prompts: &'a P) -> Self {
        prompts.watch_screen(true);
        Self(prompts)
    }
}

impl<P: PromptSignal> Drop for ScreenWatch<'_, P> {
    fn drop(&mut self) {
        self.0.watch_screen(false);
    }
}

type MessageQueue = VecDeque<(Ticket, String, Option<Instant>)>;

#[derive(Default)]
pub struct Outbox {
    queues: Mutex<HashMap<String, MessageQueue>>,
    /// The ticket the next message pushed gets.
    next_ticket: AtomicU64,
    /// The sessions a drainer currently owns.
    draining: Mutex<HashSet<String>>,
    /// The sessions whose queue is held: messages are still accepted but none is written, because
    /// the session is being handed off and what is queued for it goes to its successor, or
    /// restarted for a change of its account's sharing (the value is `true`).
    held: Mutex<HashMap<String, bool>>,
    /// The sessions whose queue is paused for a moment (`AppState::pause_outbox`), each with how
    /// many pauses are held on it: like a hold, nothing is written, but it is no handoff and has no
    /// successor.
    paused: Mutex<HashMap<String, usize>>,
    /// The messages that are also kept in the store (`Store::add_kept_message`), by ticket, and
    /// the row each is kept in. A row is deleted only once its message has been written into the
    /// session ([`Self::take_written`]), so a daemon that exits before the queue drains, or a
    /// process that ends first, leaves it for the next resume.
    kept: Mutex<Keeps>,
}

/// A session's drain claim, from [`Outbox::claim`].
struct DrainClaim<'a> {
    outbox: &'a Outbox,
    id: &'a str,
}

impl Drop for DrainClaim<'_> {
    fn drop(&mut self) {
        self.outbox
            .draining
            .lock()
            .expect("outbox claim lock poisoned")
            .remove(self.id);
    }
}

#[derive(Default)]
struct Keeps {
    /// Ticket to row, for a message still queued or being written.
    queued: HashMap<Ticket, i64>,
    /// The same the other way round, so a row is queued once however often it is read.
    tickets: HashMap<i64, Ticket>,
    /// The rows whose messages have been written, not yet deleted from the store.
    written: Vec<i64>,
}

impl Outbox {
    /// Accepts a message for a session, to be written as soon as it can take one.
    pub fn push(&self, id: &str, text: &str) -> Ticket {
        // Taken under the queue's lock, so that tickets follow the order messages are queued in.
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        queues
            .entry(id.to_string())
            .or_default()
            .push_back((ticket, text.to_string(), None));
        ticket
    }

    /// [`Self::push`] for a message kept in the store as row `row`, which goes once it is written.
    /// A row that already has a message queued or being written is not queued again, whatever
    /// session it is asked for: a queue held across a restart still has it.
    pub fn push_kept(&self, id: &str, text: &str, row: i64) -> Option<Ticket> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        let ticket = {
            let mut kept = self.kept.lock().expect("outbox kept lock poisoned");
            if kept.tickets.contains_key(&row) {
                return None;
            }
            let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
            kept.queued.insert(ticket, row);
            kept.tickets.insert(row, ticket);
            ticket
        };
        queues
            .entry(id.to_string())
            .or_default()
            .push_back((ticket, text.to_string(), None));
        Some(ticket)
    }

    /// Whether the message kept as row `row` is queued or being written.
    pub fn is_row_queued(&self, row: i64) -> bool {
        self.kept
            .lock()
            .expect("outbox kept lock poisoned")
            .tickets
            .contains_key(&row)
    }

    /// The rows of kept messages written into their sessions since the last call, for the store to
    /// delete.
    pub fn take_written(&self) -> Vec<i64> {
        std::mem::take(&mut self.kept.lock().expect("outbox kept lock poisoned").written)
    }

    /// The message with this ticket is in its session, so its row, if it has one, may go.
    fn mark_written(&self, ticket: Ticket) {
        let mut kept = self.kept.lock().expect("outbox kept lock poisoned");
        if let Some(row) = kept.queued.remove(&ticket) {
            kept.tickets.remove(&row);
            kept.written.push(row);
        }
    }

    /// The messages with these tickets will not be written by this queue, so their rows stay.
    fn unkeep(&self, tickets: impl Iterator<Item = Ticket>) {
        let mut kept = self.kept.lock().expect("outbox kept lock poisoned");
        for ticket in tickets {
            if let Some(row) = kept.queued.remove(&ticket) {
                kept.tickets.remove(&row);
            }
        }
    }

    /// Writes out whatever is queued for this session, oldest first.
    ///
    /// **Blocks** on the PTY write, which polls for seconds when the child is not draining, and on
    /// the wait for a written message to be sent.
    pub fn drain(&self, session: &LiveSession, prompts: &impl PromptSignal) -> Drain {
        self.drain_with(
            &session.id,
            &mut |ticket, text, allow_mixed| {
                term::send_message_with_id(
                    session,
                    text,
                    &format!("{}:{ticket}", session.launch_id),
                    allow_mixed,
                )
            },
            prompts,
        )
    }

    /// The body of [`Self::drain`], with the write and the agent's prompt signal as parameters so
    /// the queue's own rules can be exercised without a PTY or an agent behind them.
    fn drain_with(
        &self,
        id: &str,
        write: &mut impl FnMut(Ticket, &str, bool) -> Result<(), PartialWrite>,
        prompts: &impl PromptSignal,
    ) -> Drain {
        // Re-claimed around each pass. A producer that pushed while the previous pass was between
        // its last pop and releasing the claim would otherwise leave a queue with nothing to drain
        // it — and for a session producing no further events, nothing would come along to retry.
        // The claim is released before the queue is re-read, so there is no ordering in which the
        // producer both fails to claim and lets this loop exit.
        loop {
            if self.is_held(id) || self.is_paused(id) {
                return Drain::Pending;
            }
            let Some(claim) = self.claim(id) else {
                // Another drainer owns it and will take whatever was just pushed.
                return Drain::Pending;
            };
            let outcome = self.drain_once(id, write, prompts);
            drop(claim);
            match outcome {
                Drain::Clear if self.len(id) > 0 => continue,
                outcome => return outcome,
            }
        }
    }

    fn drain_once(
        &self,
        id: &str,
        write: &mut impl FnMut(Ticket, &str, bool) -> Result<(), PartialWrite>,
        prompts: &impl PromptSignal,
    ) -> Drain {
        loop {
            // Re-read for every message: a hold taken while this pass runs stops it at the next one.
            if self.is_held(id) || self.is_paused(id) {
                return Drain::Pending;
            }
            if prompts.unsent_waiting()
                || prompts.submission_uncertain()
                || prompts.trust_holds_writes()
            {
                return Drain::Pending;
            }
            // Asked afresh for every message: once one is sent the agent reads as working, and what
            // follows is queued by the agent until that turn ends.
            let checked = prompts.expects_send();
            // From before the look at the screen until the write has been waited on, so that a
            // dialog coming up in between, and gone again, is counted all the same. The look is
            // also the mark the count is compared with.
            let _watch = checked.then(|| ScreenWatch::start(prompts));
            let seen = prompts.sighting();
            if seen.current.is_some() {
                return Drain::Pending;
            }
            let Some((ticket, text, protection_started)) = self.pop(id) else {
                return Drain::Clear;
            };
            let protection_started = protection_started.unwrap_or_else(Instant::now);
            let before = prompts.sent();
            let mid_turn = prompts.mid_turn();
            let written = write(ticket, &text, protection_started.elapsed() >= DRAFT_WAIT);
            if mid_turn && written.is_ok() {
                prompts.note_written_mid_turn();
            }
            match written {
                Ok(()) if prompts.input_recovery_pending() => {
                    self.mark_written(ticket);
                    return Drain::Pending;
                }
                Ok(()) if !checked || prompts.wait_past(before) => self.mark_written(ticket),
                Ok(()) => {
                    let now = prompts.sighting();
                    if now.discarded_seen > seen.discarded_seen {
                        // Its paste went to a dialog that discarded it, so unlike an unsent
                        // message nothing of it is in the input box.
                        tracing::warn!(session = %id, "a dialog discarded a written message");
                        self.push_front(id, ticket, text, protection_started);
                        return Drain::Discarded(ticket);
                    }
                    if now.diverted_seen > seen.diverted_seen {
                        tracing::warn!(session = %id, "a view of background agents took a message");
                        self.mark_written(ticket);
                        return Drain::Diverted(ticket);
                    }
                    tracing::warn!(session = %id, "a written message was not sent by the agent");
                    // Its text is in the input box, which is as far as a written message goes.
                    self.mark_written(ticket);
                    prompts.mark_unsent(before);
                    return Drain::Unsent(ticket);
                }
                // Nothing went in, so the session simply could not take it — most likely a modal its
                // reported state does not show. Put it back at the front and stop: letting the next
                // one through would reorder them.
                Err(partial) if partial.written == 0 => {
                    if partial.error.kind() != std::io::ErrorKind::WouldBlock {
                        tracing::warn!(session = %id, error = %partial.error, "a queued message could not be written yet");
                    }
                    self.push_front(id, ticket, text, protection_started);
                    return Drain::Pending;
                }
                Err(partial)
                    if partial.error.get_ref().is_some_and(|error| {
                        error.is::<crate::codex_transport::UncertainSubmission>()
                    }) =>
                {
                    self.mark_written(ticket);
                    prompts.mark_submission_uncertain(before);
                    tracing::error!(session = %id, error = %partial.error,
                        "the native queue did not confirm accepting a message; it will not be resent");
                    return Drain::Uncertain(ticket);
                }
                Err(partial) => {
                    // The queue behind it goes too. The next message written into this session is
                    // run together with the fragment, so it would arrive spoiled — delivering it
                    // spoiled is worse than saying it was not delivered.
                    // The sender is told these messages were lost, so their rows are done with
                    // too, and a resume does not deliver what was reported undelivered.
                    self.mark_written(ticket);
                    let abandoned = self.forget_as_done(id);
                    tracing::error!(
                        session = %id,
                        written = partial.written,
                        error = %partial.error,
                        abandoned,
                        "a message was only partly written; it and the rest of the queue have been \
                         dropped"
                    );
                    return Drain::Lost;
                }
            }
        }
    }

    /// How many messages are still queued for this session.
    pub fn len(&self, id: &str) -> usize {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .get(id)
            .map(|queue| queue.len())
            .unwrap_or(0)
    }

    /// Whether anything is queued for this session or being written into it now.
    pub fn is_pending(&self, id: &str) -> bool {
        self.len(id) > 0
            || self
                .draining
                .lock()
                .expect("outbox claim lock poisoned")
                .contains(id)
    }

    /// [`Self::forget`] for a queue that is reported lost: the rows of its kept messages go too.
    fn forget_as_done(&self, id: &str) -> usize {
        let queue = self
            .queues
            .lock()
            .expect("outbox lock poisoned")
            .remove(id)
            .unwrap_or_default();
        for (ticket, _, _) in &queue {
            self.mark_written(*ticket);
        }
        queue.len()
    }

    /// Forgets this session's queue and says how much went with it. Called when its process is gone
    /// — there is nothing to write to, and a resume starts the agent at its prompt rather than
    /// replaying a queue it never saw — and when a write left the input line holding a fragment.
    pub fn forget(&self, id: &str) -> usize {
        let queue = self
            .queues
            .lock()
            .expect("outbox lock poisoned")
            .remove(id)
            .unwrap_or_default();
        self.unkeep(queue.iter().map(|(ticket, _, _)| *ticket));
        queue.len()
    }

    /// Stops writing this session's queue until [`Self::release_hold`]. Pushing still works.
    pub fn hold(&self, id: &str, for_sharing: bool) {
        self.held
            .lock()
            .expect("outbox hold lock poisoned")
            .insert(id.to_string(), for_sharing);
    }

    /// Stops writing this session's queue until every pause taken has been ended with
    /// [`Self::resume`]. Pushing still works.
    pub fn pause(&self, id: &str) {
        *self
            .paused
            .lock()
            .expect("outbox pause lock poisoned")
            .entry(id.to_string())
            .or_default() += 1;
    }

    /// Ends one pause; the queue is written again once none is left.
    pub fn resume(&self, id: &str) {
        let mut paused = self.paused.lock().expect("outbox pause lock poisoned");
        if let Some(count) = paused.get_mut(id) {
            *count -= 1;
            if *count == 0 {
                paused.remove(id);
            }
        }
    }

    pub fn is_paused(&self, id: &str) -> bool {
        self.paused
            .lock()
            .expect("outbox pause lock poisoned")
            .contains_key(id)
    }

    pub fn release_hold(&self, id: &str) {
        self.held
            .lock()
            .expect("outbox hold lock poisoned")
            .remove(id);
    }

    pub fn is_held(&self, id: &str) -> bool {
        self.held
            .lock()
            .expect("outbox hold lock poisoned")
            .contains_key(id)
    }

    /// Whether the hold on this session's queue is for a change of its account's sharing.
    pub fn is_held_for_sharing(&self, id: &str) -> bool {
        self.held
            .lock()
            .expect("outbox hold lock poisoned")
            .get(id)
            .copied()
            .unwrap_or(false)
    }

    /// Removes and returns what is queued for this session, oldest first, each with the row it is
    /// kept in, if it is.
    pub fn take(&self, id: &str) -> Vec<(String, Option<i64>)> {
        let queue = self
            .queues
            .lock()
            .expect("outbox lock poisoned")
            .remove(id)
            .unwrap_or_default();
        // Whoever takes it forwards it, and what is kept for the session is theirs to deal with.
        let kept = self.kept.lock().expect("outbox kept lock poisoned");
        let rows: Vec<Option<i64>> = queue
            .iter()
            .map(|(ticket, _, _)| kept.queued.get(ticket).copied())
            .collect();
        drop(kept);
        self.unkeep(queue.iter().map(|(ticket, _, _)| *ticket));
        queue
            .into_iter()
            .zip(rows)
            .map(|((_, text, _), row)| (text, row))
            .collect()
    }

    /// Makes the caller the session's one drainer until the claim is dropped, or `None` when
    /// another drainer is. Released on `Drop`, so a write that panics cannot leave the session's
    /// queue with a claim nobody holds, and so never drained again.
    fn claim<'a>(&'a self, id: &'a str) -> Option<DrainClaim<'a>> {
        let claimed = self
            .draining
            .lock()
            .expect("outbox claim lock poisoned")
            .insert(id.to_string());
        claimed.then(|| DrainClaim { outbox: self, id })
    }

    fn pop(&self, id: &str) -> Option<(Ticket, String, Option<Instant>)> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        let queue = queues.get_mut(id)?;
        let next = queue.pop_front();
        if queue.is_empty() {
            queues.remove(id);
        }
        next
    }

    fn push_front(&self, id: &str, ticket: Ticket, text: String, protection_started: Instant) {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .entry(id.to_string())
            .or_default()
            .push_front((ticket, text, Some(protection_started)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_dialogs::{Dialog, WrittenMessage};
    use std::cell::Cell;

    /// An agent mid-turn, or one that reports no prompts at all: nothing written is waited on.
    struct MidTurn;

    impl PromptSignal for MidTurn {
        fn sent(&self) -> u64 {
            0
        }
        fn expects_send(&self) -> bool {
            false
        }
        fn wait_past(&self, _before: u64) -> bool {
            unreachable!("nothing is waited on mid-turn")
        }
        fn mark_unsent(&self, _before: u64) {
            unreachable!("nothing is checked mid-turn")
        }
        fn unsent_waiting(&self) -> bool {
            false
        }
        fn sighting(&self) -> Sighting {
            Sighting::default()
        }
        fn watch_screen(&self, _on: bool) {
            unreachable!("nothing is checked mid-turn")
        }
        fn trust_holds_writes(&self) -> bool {
            false
        }
    }

    /// An agent at its prompt that sends a written message, or none while `sends` is off, and is
    /// busy with a turn once anything is sent. [`Self::prompt_sent`] is any prompt reported sent,
    /// the agent's own or one from elsewhere, such as the user pressing Enter.
    struct AtPrompt {
        sent: Cell<u64>,
        sends: Cell<bool>,
        waits: Cell<u32>,
        unsent: Cell<Option<u64>>,
        busy: Cell<bool>,
        dialog: Cell<Option<Dialog>>,
        /// What the screen has shown so far.
        sighting: Cell<Sighting>,
        /// A dialog that comes up while the next message is waited on, and whether it is gone
        /// again by the time the wait ends.
        dialog_on_wait: Cell<Option<(Dialog, bool)>>,
        /// Screen watches in force, and the most that ever were.
        watching: Cell<i32>,
        watched_during_wait: Cell<bool>,
        trust_holds: Cell<bool>,
        recovering_input: Cell<bool>,
    }

    impl AtPrompt {
        fn new(sends: bool) -> Self {
            Self {
                sent: Cell::new(0),
                sends: Cell::new(sends),
                waits: Cell::new(0),
                unsent: Cell::new(None),
                busy: Cell::new(false),
                dialog: Cell::new(None),
                sighting: Cell::new(Sighting::default()),
                dialog_on_wait: Cell::new(None),
                watching: Cell::new(0),
                watched_during_wait: Cell::new(false),
                trust_holds: Cell::new(false),
                recovering_input: Cell::new(false),
            }
        }

        /// The screen now shows `dialog`, or none.
        fn show(&self, dialog: Option<Dialog>) {
            self.dialog.set(dialog);
            self.sighting.set(self.sighting.get().after(dialog));
        }

        fn prompt_sent(&self) {
            self.sent.set(self.sent.get() + 1);
            self.busy.set(true);
        }
    }

    impl PromptSignal for AtPrompt {
        fn sent(&self) -> u64 {
            self.sent.get()
        }

        fn input_recovery_pending(&self) -> bool {
            self.recovering_input.get()
        }
        fn expects_send(&self) -> bool {
            !self.busy.get()
        }
        fn wait_past(&self, before: u64) -> bool {
            self.waits.set(self.waits.get() + 1);
            self.watched_during_wait.set(self.watching.get() > 0);
            if let Some((dialog, gone_again)) = self.dialog_on_wait.take() {
                self.show(Some(dialog));
                if gone_again {
                    self.show(None);
                }
            }
            if self.sends.get() {
                self.prompt_sent();
            }
            self.sent.get() > before
        }
        fn mark_unsent(&self, before: u64) {
            self.unsent.set(Some(before));
        }
        fn unsent_waiting(&self) -> bool {
            self.unsent
                .get()
                .is_some_and(|before| self.sent.get() <= before)
        }
        fn sighting(&self) -> Sighting {
            self.sighting.get()
        }
        fn watch_screen(&self, on: bool) {
            self.watching
                .set(self.watching.get() + if on { 1 } else { -1 });
        }
        fn trust_holds_writes(&self) -> bool {
            self.trust_holds.get()
        }
    }

    fn dialog(written: WrittenMessage) -> Dialog {
        Dialog {
            code: "test_dialog",
            written,
        }
    }

    fn partial(written: usize) -> PartialWrite {
        PartialWrite {
            written,
            error: std::io::Error::new(std::io::ErrorKind::TimedOut, "no room"),
        }
    }

    #[test]
    fn everything_queued_is_written_in_order() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(())
                },
                &MidTurn
            ),
            Drain::Clear
        );
        assert_eq!(written, vec!["first", "second"]);
        assert_eq!(outbox.len("s"), 0);
    }

    /// A paused queue is written only once every pause on it has ended, so two that overlap do not
    /// let it go early.
    #[test]
    fn a_paused_queue_waits_for_the_last_pause_to_end() {
        let outbox = Outbox::default();
        outbox.push("s", "held");
        outbox.pause("s");
        outbox.pause("s");
        let mut written = Vec::new();
        let mut drain = |outbox: &Outbox| {
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(())
                },
                &MidTurn,
            )
        };
        assert_eq!(drain(&outbox), Drain::Pending);
        outbox.resume("s");
        assert_eq!(drain(&outbox), Drain::Pending);
        outbox.resume("s");
        assert_eq!(drain(&outbox), Drain::Clear);
        assert_eq!(written, vec!["held"]);
    }

    /// A message queued while a drain is already under way is written by that drain rather than left
    /// for a later one. This does not reach the outer re-claim loop — landing between the final pop
    /// and the release needs a second thread, and the argument for that window is the ordering noted
    /// in `drain_with` rather than something a test can arrange.
    #[test]
    fn a_message_queued_during_a_drain_is_written_by_it() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let mut written = Vec::new();
        let mut pushed_late = false;
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    if !pushed_late {
                        pushed_late = true;
                        // Stands in for a producer arriving as this pass finishes: by the time the
                        // drainer re-reads the queue the message is in it.
                        outbox.push("s", "late arrival");
                    }
                    Ok(())
                },
                &MidTurn
            ),
            Drain::Clear
        );
        assert_eq!(written, vec!["first", "late arrival"]);
    }

    /// A message kept in the store is queued once per row however often it is pushed, its row is
    /// handed on once it is written, a queue that is forgotten or taken leaves the row alone, and a
    /// queue reported lost counts as done.
    #[test]
    fn a_kept_message_is_queued_once_and_its_row_goes_when_it_is_written_or_lost() {
        let outbox = Outbox::default();
        assert!(outbox.push_kept("s", "kept", 7).is_some());
        assert!(outbox.push_kept("s", "kept", 7).is_none());
        assert_eq!(outbox.len("s"), 1);
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Ok(()), &MidTurn),
            Drain::Clear
        );
        assert_eq!(outbox.take_written(), [7]);
        // Written, the row may be queued again by a later resume that still finds it.
        assert!(outbox.push_kept("s", "kept", 7).is_some());

        // Taken, it reports its row and is no longer queued.
        let taken = outbox.take("s");
        assert_eq!(taken, [("kept".to_string(), Some(7))]);
        assert!(outbox.push_kept("s", "kept", 7).is_some());
        outbox.forget("s");
        assert!(outbox.take_written().is_empty());

        // Lost: reported to the sender as undelivered, so its row and those behind it go.
        outbox.push_kept("s", "first", 1);
        outbox.push_kept("s", "second", 2);
        outbox.push("s", "plain");
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Err(partial(3)), &MidTurn),
            Drain::Lost
        );
        let mut rows = outbox.take_written();
        rows.sort();
        assert_eq!(rows, [1, 2]);
    }

    /// Nothing went in, so the session could not take it: it stays at the front, and the one behind
    /// it must not be let past.
    #[test]
    fn a_message_that_could_not_be_written_stays_at_the_front() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Err(partial(0)), &MidTurn),
            Drain::Pending
        );
        assert_eq!(outbox.len("s"), 2);

        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(())
                },
                &MidTurn
            ),
            Drain::Clear
        );
        assert_eq!(written, vec!["first", "second"]);
    }

    /// A half-written message leaves an unterminated paste in the session's input line, so retrying
    /// it would deliver the fragment twice and writing the next one would append it inside the
    /// fragment. Both it and the queue behind it go, and the loss is reported.
    #[test]
    fn a_half_written_message_takes_the_queue_with_it_and_is_reported() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Err(partial(3)), &MidTurn),
            Drain::Lost
        );
        assert_eq!(outbox.len("s"), 0);
    }

    #[test]
    fn only_one_drainer_at_a_time_owns_a_session() {
        let outbox = Outbox::default();
        let claim = outbox.claim("s");
        assert!(claim.is_some());
        assert!(outbox.claim("s").is_none());
        drop(claim);
        assert!(outbox.claim("s").is_some());
    }

    #[test]
    fn a_drain_whose_write_panics_leaves_the_session_drainable() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            outbox.drain_with("s", &mut |_, _, _| panic!("the write failed"), &MidTurn)
        }));
        assert!(panicked.is_err());
        assert!(outbox.claim("s").is_some());
    }

    #[test]
    fn a_drain_a_concurrent_drainer_already_owns_reports_pending() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let _claim = outbox.claim("s").unwrap();
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Ok(()), &MidTurn),
            Drain::Pending
        );
        assert_eq!(outbox.len("s"), 1);
    }

    #[test]
    fn forgetting_a_session_reports_what_it_threw_away() {
        let outbox = Outbox::default();
        outbox.push("s", "one");
        outbox.push("s", "two");
        assert_eq!(outbox.forget("s"), 2);
        assert_eq!(outbox.len("s"), 0);
        assert_eq!(outbox.forget("s"), 0);
    }

    /// A message an idle agent sends goes through, and what follows it in the same pass is not
    /// waited on: the agent is busy with the first and queues the rest itself.
    #[test]
    fn a_message_the_agent_sends_is_written_and_the_rest_follow_unchecked() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        let agent = AtPrompt::new(true);
        let mut written = Vec::new();
        let outcome = outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                Ok(())
            },
            &agent,
        );
        assert_eq!(outcome, Drain::Clear);
        assert_eq!(written, vec!["first", "second"]);
        assert_eq!(agent.waits.get(), 1);
    }

    /// A message the agent does not send is reported by its ticket and not put back, and nothing
    /// more is written — not even a message queued afterwards — until the agent next reports a
    /// prompt sent.
    #[test]
    fn a_message_the_agent_does_not_send_holds_the_queue_until_the_next_prompt() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first");
        outbox.push("s", "second");
        let agent = AtPrompt::new(false);
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(())
        };
        assert_eq!(
            outbox.drain_with("s", &mut write, &agent),
            Drain::Unsent(first)
        );
        outbox.push("s", "third");
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
        assert_eq!(outbox.len("s"), 2);

        agent.prompt_sent();
        agent.sends.set(true);
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
        assert_eq!(written, vec!["first", "second", "third"]);
    }

    /// Nothing is written while a dialog is up, and the queue goes on once it is gone.
    #[test]
    fn nothing_is_written_while_a_dialog_is_up() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let agent = AtPrompt::new(true);
        agent.show(Some(dialog(WrittenMessage::Draft)));
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(())
        };
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
        assert_eq!(outbox.len("s"), 1);

        agent.show(None);
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
        assert_eq!(written, vec!["first"]);
    }

    /// A trust screen sighted while a drain runs stops it at the next message.
    #[test]
    fn nothing_is_written_while_the_trust_screen_holds_writes() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let agent = AtPrompt::new(true);
        agent.trust_holds.set(true);
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(())
        };
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
        assert_eq!(outbox.len("s"), 1);
        assert!(written.is_empty());
    }

    /// A dialog that came up around a write threw the message away: it goes back at the front
    /// under its ticket, the agent is not marked as holding an unsent message, and the message
    /// behind it is not let past. The dialog is gone again by the end of the wait, as one a write
    /// closes is, and the screen was being watched throughout.
    #[test]
    fn a_message_a_dialog_threw_away_goes_back_at_the_front() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first");
        outbox.push("s", "second");
        let agent = AtPrompt::new(false);
        agent
            .dialog_on_wait
            .set(Some((dialog(WrittenMessage::Discarded), true)));
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(())
        };
        assert_eq!(
            outbox.drain_with("s", &mut write, &agent),
            Drain::Discarded(first)
        );
        assert!(agent.watched_during_wait.get());
        assert_eq!(agent.watching.get(), 0);
        assert_eq!(outbox.len("s"), 2);
        assert!(!agent.unsent_waiting());

        agent.sends.set(true);
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
        assert_eq!(written, vec!["first", "first", "second"]);
    }

    /// A view that took the message has delivered it, to the wrong place: it is not put back, no
    /// unsent mark holds the queue, and the message behind it waits for the view to be left.
    #[test]
    fn a_message_a_view_took_is_not_put_back() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first");
        outbox.push("s", "second");
        let agent = AtPrompt::new(false);
        agent
            .dialog_on_wait
            .set(Some((dialog(WrittenMessage::Diverted), false)));
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(())
        };
        assert_eq!(
            outbox.drain_with("s", &mut write, &agent),
            Drain::Diverted(first)
        );
        assert_eq!(outbox.len("s"), 1);
        assert!(!agent.unsent_waiting());

        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
        agent.show(None);
        agent.sends.set(true);
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
        assert_eq!(written, vec!["first", "second"]);
    }

    /// A draft a dialog kept (the idle resume dialog) is in the input box, so it counts as unsent.
    #[test]
    fn a_message_left_as_a_draft_by_a_dialog_is_unsent() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first");
        let agent = AtPrompt::new(false);
        agent
            .dialog_on_wait
            .set(Some((dialog(WrittenMessage::Draft), false)));
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Ok(()), &agent),
            Drain::Unsent(first)
        );
        assert_eq!(outbox.len("s"), 0);
    }
    #[test]
    fn a_written_message_with_pending_draft_recovery_is_not_retried_or_marked_unsent() {
        let outbox = Outbox::default();
        outbox.push("s", "already written");
        outbox.push("s", "still queued");
        let agent = AtPrompt::new(false);
        agent.recovering_input.set(true);
        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(())
                },
                &agent
            ),
            Drain::Pending
        );
        assert_eq!(written, ["already written"]);
        assert_eq!(outbox.len("s"), 1);
        assert!(!agent.unsent_waiting());
        assert_eq!(agent.waits.get(), 0);
    }

    #[test]
    fn uncertain_native_acceptance_preserves_later_messages_without_resending() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first");
        outbox.push("s", "second");
        let agent = AtPrompt::new(true);
        let mut attempts = Vec::new();
        let outcome = outbox.drain_with(
            "s",
            &mut |ticket, text, _| {
                attempts.push((ticket, text.to_string()));
                Err(PartialWrite {
                    written: 1,
                    error: std::io::Error::other(crate::codex_transport::UncertainSubmission(
                        "connection closed after add".into(),
                    )),
                })
            },
            &agent,
        );
        assert_eq!(outcome, Drain::Uncertain(first));
        assert_eq!(attempts, [(first, "first".to_string())]);
        assert_eq!(outbox.len("s"), 1);
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| panic!("held"), &agent),
            Drain::Pending
        );
        agent.prompt_sent();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |ticket, text, _| {
                    attempts.push((ticket, text.to_string()));
                    Ok(())
                },
                &agent
            ),
            Drain::Clear
        );
        assert_eq!(attempts[1].1, "second");
    }
    #[test]
    fn an_unsubmitted_draft_does_not_prevent_delivery_after_the_grace_period() {
        let outbox = Outbox::default();
        let live = crate::test_support::fake_live_session(
            "bounded-draft",
            crate::protocol::Agent::Claude,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.on_output(include_bytes!("../testdata/claude_input_restored.bin"));
        live.write_terminal_input(b"unfinished user input").unwrap();
        outbox.push(&live.id, "automatic message");
        let before = live.input_writes();
        assert_eq!(outbox.drain(&live, &MidTurn), Drain::Pending);
        assert_eq!(live.input_writes(), before);
        let queued = outbox.pop(&live.id).unwrap();
        outbox.push_front(&live.id, queued.0, queued.1, Instant::now() - DRAFT_WAIT);
        assert_eq!(outbox.drain(&live, &MidTurn), Drain::Clear);
        assert_eq!(live.input_writes(), before + 1);
        assert_eq!(outbox.len(&live.id), 0);
    }

    #[test]
    fn draft_wait_expires_for_the_same_ticket_across_retries() {
        let outbox = Outbox::default();
        let ticket = outbox.push("s", "automatic");
        let mut allowances = Vec::new();
        let mut write = |seen, _: &str, allow_mixed| {
            assert_eq!(seen, ticket);
            allowances.push(allow_mixed);
            Err(PartialWrite {
                written: 0,
                error: std::io::ErrorKind::WouldBlock.into(),
            })
        };
        assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
        let queued = outbox.pop("s").unwrap();
        assert!(queued.2.is_some());
        outbox.push_front("s", queued.0, queued.1, Instant::now() - DRAFT_WAIT);
        assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
        assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
        assert_eq!(allowances, [false, true, true]);
    }
}
