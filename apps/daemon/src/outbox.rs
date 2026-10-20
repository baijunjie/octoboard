//! The per-session queue for messages Octoboard writes into a running agent: a report reaching a
//! console session, an instruction the console session sent, an instruction handed to a session
//! being reopened.
//!
//! Three properties, each of which something upstream depends on:
//!
//! - **Order-preserving.** A message that cannot be written yet does not let a later one past it.
//!   Reports from several project sessions arriving at one console session, and a console
//!   session's follow-up landing after the report it answers, both rest on this.
//! - **Nothing is dropped once accepted**, with two exceptions: a half-written message (below),
//!   whose drain reports the loss; and a session whose process ends, which takes its queue with it.
//!   Short of those, whoever queued a message was told not to send it again and is right.
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
//! Recovery from the ordinary case — nothing written, because the session could not take it — has
//! no timer of its own: the message waits at the front of the queue for the next drain, which
//! something outside triggers (a status a hook reported, or another message being queued). The
//! same goes for a queue held behind an unsent message: the hook reporting the next prompt sent
//! triggers the drain that goes on.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

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
}

#[derive(Default)]
pub struct Outbox {
    queues: Mutex<HashMap<String, VecDeque<(Ticket, String)>>>,
    /// The ticket the next message pushed gets.
    next_ticket: AtomicU64,
    /// The sessions a drainer currently owns.
    draining: Mutex<HashSet<String>>,
    /// The sessions whose queue is held: messages are still accepted but none is written, because
    /// the session is being handed off and what is queued for it goes to its successor.
    held: Mutex<HashSet<String>>,
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
            if self.is_held(id) {
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
            if self.is_held(id) {
                return Drain::Pending;
            }
            if prompts.unsent_waiting() {
                return Drain::Pending;
            }
            let Some((ticket, text)) = self.pop(id) else {
                return Drain::Clear;
            };
            // Asked afresh for every message: once one is sent the agent reads as working, and what
            // follows is queued by the agent until that turn ends.
            let checked = prompts.expects_send();
            let before = prompts.sent();
            match write(&text) {
                Ok(()) if !checked || prompts.wait_past(before) => {}
                Ok(()) => {
                    tracing::warn!(session = %id, "a written message was not sent by the agent");
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
                    let abandoned = self.forget(id);
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

    /// Forgets this session's queue and says how much went with it. Called when its process is gone
    /// — there is nothing to write to, and a resume starts the agent at its prompt rather than
    /// replaying a queue it never saw — and when a write left the input line holding a fragment.
    pub fn forget(&self, id: &str) -> usize {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .remove(id)
            .map(|queue| queue.len())
            .unwrap_or(0)
    }

    /// Stops writing this session's queue until [`Self::release_hold`]. Pushing still works.
    pub fn hold(&self, id: &str) {
        self.held
            .lock()
            .expect("outbox hold lock poisoned")
            .insert(id.to_string());
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
            .contains(id)
    }

    /// Removes and returns what is queued for this session, oldest first.
    pub fn take(&self, id: &str) -> Vec<String> {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .remove(id)
            .map(|queue| queue.into_iter().map(|(_, text)| text).collect())
            .unwrap_or_default()
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
    }

    impl AtPrompt {
        fn new(sends: bool) -> Self {
            Self {
                sent: Cell::new(0),
                sends: Cell::new(sends),
                waits: Cell::new(0),
                unsent: Cell::new(None),
                busy: Cell::new(false),
            }
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
}
