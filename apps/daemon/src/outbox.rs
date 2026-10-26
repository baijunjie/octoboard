//! The per-session queue for messages Octoboard writes into a running agent: a report reaching a
//! console session, an instruction the console session sent, an instruction handed to a session
//! being reopened.
//!
//! Four properties, each of which something upstream depends on:
//!
//! - **Order-preserving.** A message that cannot be written yet does not let a later one past it.
//!   Reports from several project sessions arriving at one console session, and a console
//!   session's follow-up landing after the report it answers, both rest on this.
//! - **Nothing is dropped once accepted**, with three exceptions: a half-written message (below),
//!   whose drain reports the loss; a message a view of the agent's took instead of the
//!   conversation (below), which is reported too; and a session whose process ends, which takes
//!   its queue with it. Short of those, whoever queued a message was told not to send it again and
//!   is right. A message kept in the store for a session that was interrupted is backed by its row
//!   there, which goes only once the message is written (or lost in a half-write), so a process
//!   ending before then leaves it for the next resume.
//! - **A held queue is kept and not written.** A handoff holds the original's queue: messages are
//!   still accepted, a drain in progress stops at its next message, and the queue survives the
//!   session's process ending, so that its holder can forward it to the session that takes over
//!   (`AppState::hold_outbox`).
//! - **One drainer per session.** Two of them interleaving between a pop and its write would
//!   reorder the queue.
//!
//! It is not the only writer to a session's PTY: the terminal socket writes the user's own
//! keystrokes straight through (`server.rs`), which is deliberate — a keystroke must not wait behind
//! a queue — and means the queue serialises Octoboard's own messages against each other, not against
//! the person typing. Keystrokes still skip the queue, but they wait behind a message write that is
//! already in flight on the session's input lock and can no longer interleave with it.
//!
//! **A half-written message is dropped, not retried.** A PTY write can be accepted in part and then
//! time out, leaving a truncated bracketed paste in the child's input buffer with no terminator;
//! writing the message again would deliver that prefix twice, and writing the *next* one would
//! append it inside the dangling paste. So the message goes no further, the rest of the queue is
//! abandoned with it — the next message written into that session is run together with the fragment,
//! so it would arrive spoiled, and delivering it spoiled is worse than reporting it undelivered. The
//! drain reports the loss so the sender whose call hit it can be told rather than left believing its
//! message was accepted. A sender whose message was already queued behind it is not told; the user's
//! notice and the log line are all there is for those.
//!
//! Nothing here tries to *recover* the input line. What state it is actually left in, and what clears
//! it, is not established for any of the three agents; what is known is that the next write closes
//! the fragment, which bounds the damage at one spoiled message rather than a session that stays
//! unusable. Refusing to write until some signal said the line was clean was tried and abandoned:
//! no available signal proves it, and a wrong one leaves a console silently unable to orchestrate.
//!
//! **A message written but not sent holds the queue.** Claude Code can take a write and still not
//! send it, leaving the text in its input box (see "Writing into a running session" in
//! `docs/agent-cli-reference.md`). So a message written to an agent that reports each prompt it
//! sends and is at its prompt, not mid-turn, is followed by a short wait for that report. With none
//! the message is reported unsent, and nothing more is written into the session until the agent
//! next reports a prompt sent — whoever sends it — since whatever came next would land in the same
//! input box. The message itself is not put back: its text is already in the session. A message
//! written mid-turn is not checked, since the agent sends nothing until its turn ends. The mark
//! that holds the queue is the running process's, kept beside its count of prompts sent
//! ([`PromptSignal`]), so it goes with that process and never holds the next one's queue.
//!
//! **A message a dialog threw away is put back.** A modal dialog that appears at launch (Claude
//! Code's "Resume this conversation?") discards a paste and lets the Enter pick a row, so the
//! message is nowhere. Such a dialog is held for before any write (`PromptSignal::sighting`), and
//! when one is seen to have come up between a write and the end of the wait for its report, the
//! message goes back to the front of the queue with its ticket and no unsent mark is set. The
//! screen is looked at on every chunk of output while a checked write is in flight
//! (`PromptSignal::watch_screen`), because the write itself can close such a dialog within
//! milliseconds, leaving nothing on screen to see once the wait is over.
//!
//! **A message a view took is not put back.** Claude Code's views of its background agents take
//! what is written into them: it reaches a subagent or starts another session. Writing it again
//! would deliver it twice, so it is neither queued again nor marked unsent; the drain reports it
//! and stops, and the queue waits behind the view like any other dialog.
//!
//! Recovery from the ordinary case — nothing written, because the session could not take it — has
//! no timer of its own: the message waits at the front of the queue for the next drain, which
//! something outside triggers (a status a hook reported, or another message being queued). The
//! same goes for a queue held behind an unsent message: the hook reporting the next prompt sent
//! triggers the drain that goes on.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::agent_dialogs::Sighting;
use crate::ptyio::PartialWrite;
use crate::session::LiveSession;
use crate::term;

/// What a drain amounted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drain {
    /// Nothing is left queued for this session.
    Clear,
    /// Something is still queued, and a later drain will take it.
    Pending,
    /// A message was written in part, so it is gone and the queue behind it with it.
    Lost,
    /// The message with this ticket was written, but the agent did not report sending it. Its text
    /// is in the session's input box, and the rest of the queue waits for the next prompt sent.
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
    /// Whether a message marked unsent is still waiting, which it is until the count moves past
    /// the one it was marked at.
    fn unsent_waiting(&self) -> bool;
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

#[derive(Default)]
pub struct Outbox {
    queues: Mutex<HashMap<String, VecDeque<(Ticket, String)>>>,
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
    /// The messages that are also kept in the store (`Store::add_pending_info`), by ticket, and
    /// the row each is kept in. A row is deleted only once its message has been written into the
    /// session ([`Self::take_written`]), so a daemon that exits before the queue drains, or a
    /// process that ends first, leaves it for the next resume.
    kept: Mutex<Keeps>,
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
            .push_back((ticket, text.to_string()));
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
            .push_back((ticket, text.to_string()));
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
            &mut |text| term::send_message(session, text),
            prompts,
        )
    }

    /// The body of [`Self::drain`], with the write and the agent's prompt signal as parameters so
    /// the queue's own rules can be exercised without a PTY or an agent behind them.
    ///
    /// `fragmented` is read **under the claim**, and again before every write. That is what makes it
    /// enforceable: only the claim holder can set the mark, so a reading taken inside the claim
    /// cannot go stale while this drain runs. Read outside it, the mark could be set between the
    /// reading and the write — which is the same check-then-act hole the mark exists to close.
    fn drain_with(
        &self,
        id: &str,
        write: &mut impl FnMut(&str) -> Result<(), PartialWrite>,
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
            if !self.claim(id) {
                // Another drainer owns it and will take whatever was just pushed.
                return Drain::Pending;
            }
            let outcome = self.drain_once(id, write, prompts);
            self.release(id);
            match outcome {
                Drain::Clear if self.len(id) > 0 => continue,
                outcome => return outcome,
            }
        }
    }

    fn drain_once(
        &self,
        id: &str,
        write: &mut impl FnMut(&str) -> Result<(), PartialWrite>,
        prompts: &impl PromptSignal,
    ) -> Drain {
        loop {
            // Re-read for every message: a hold taken while this pass runs stops it at the next one.
            if self.is_held(id) || self.is_paused(id) {
                return Drain::Pending;
            }
            if prompts.unsent_waiting() || prompts.trust_holds_writes() {
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
            let Some((ticket, text)) = self.pop(id) else {
                return Drain::Clear;
            };
            let before = prompts.sent();
            let mid_turn = prompts.mid_turn();
            let written = write(&text);
            if mid_turn && written.is_ok() {
                prompts.note_written_mid_turn();
            }
            match written {
                Ok(()) if !checked || prompts.wait_past(before) => self.mark_written(ticket),
                Ok(()) => {
                    let now = prompts.sighting();
                    if now.discarded_seen > seen.discarded_seen {
                        // Its paste went to a dialog that discarded it, so unlike an unsent
                        // message nothing of it is in the input box.
                        tracing::warn!(session = %id, "a dialog discarded a written message");
                        self.push_front(id, ticket, text);
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
                    tracing::warn!(session = %id, error = %partial.error, "a queued message could not be written yet");
                    self.push_front(id, ticket, text);
                    return Drain::Pending;
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
        for (ticket, _) in &queue {
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
        self.unkeep(queue.iter().map(|(ticket, _)| *ticket));
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
            .map(|(ticket, _)| kept.queued.get(ticket).copied())
            .collect();
        drop(kept);
        self.unkeep(queue.iter().map(|(ticket, _)| *ticket));
        queue
            .into_iter()
            .zip(rows)
            .map(|((_, text), row)| (text, row))
            .collect()
    }

    fn claim(&self, id: &str) -> bool {
        self.draining
            .lock()
            .expect("outbox claim lock poisoned")
            .insert(id.to_string())
    }

    fn release(&self, id: &str) {
        self.draining
            .lock()
            .expect("outbox claim lock poisoned")
            .remove(id);
    }

    fn pop(&self, id: &str) -> Option<(Ticket, String)> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        let queue = queues.get_mut(id)?;
        let next = queue.pop_front();
        if queue.is_empty() {
            queues.remove(id);
        }
        next
    }

    fn push_front(&self, id: &str, ticket: Ticket, text: String) {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .entry(id.to_string())
            .or_default()
            .push_front((ticket, text));
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
                &mut |text| {
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
                &mut |text| {
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
                &mut |text| {
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
            outbox.drain_with("s", &mut |_| Ok(()), &MidTurn),
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
            outbox.drain_with("s", &mut |_| Err(partial(3)), &MidTurn),
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
            outbox.drain_with("s", &mut |_| Err(partial(0)), &MidTurn),
            Drain::Pending
        );
        assert_eq!(outbox.len("s"), 2);

        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |text| {
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
            outbox.drain_with("s", &mut |_| Err(partial(3)), &MidTurn),
            Drain::Lost
        );
        assert_eq!(outbox.len("s"), 0);
    }

    #[test]
    fn only_one_drainer_at_a_time_owns_a_session() {
        let outbox = Outbox::default();
        assert!(outbox.claim("s"));
        assert!(!outbox.claim("s"));
        outbox.release("s");
        assert!(outbox.claim("s"));
    }

    #[test]
    fn a_drain_a_concurrent_drainer_already_owns_reports_pending() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        assert!(outbox.claim("s"));
        assert_eq!(
            outbox.drain_with("s", &mut |_| Ok(()), &MidTurn),
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
            &mut |text| {
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
        let mut write = |text: &str| {
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
        let mut write = |text: &str| {
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
        let mut write = |text: &str| {
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
        let mut write = |text: &str| {
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
        let mut write = |text: &str| {
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
            outbox.drain_with("s", &mut |_| Ok(()), &agent),
            Drain::Unsent(first)
        );
        assert_eq!(outbox.len("s"), 0);
    }
}
