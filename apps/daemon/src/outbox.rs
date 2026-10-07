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
//! Recovery from the ordinary case — nothing written, because the session could not take it — has no
//! timer of its own: the message waits at the front of the queue for the next drain, which something
//! outside triggers (a status a hook reported, or another message being queued).

use std::collections::{HashMap, HashSet, VecDeque};
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
}

#[derive(Default)]
pub struct Outbox {
    queues: Mutex<HashMap<String, VecDeque<String>>>,
    /// The sessions a drainer currently owns.
    draining: Mutex<HashSet<String>>,
}

impl Outbox {
    /// Accepts a message for a session, to be written as soon as it can take one.
    pub fn push(&self, id: &str, text: &str) {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .entry(id.to_string())
            .or_default()
            .push_back(text.to_string());
    }

    /// Writes out whatever is queued for this session, oldest first.
    ///
    /// **Blocks** on the PTY write, which polls for seconds when the child is not draining.
    pub fn drain(&self, session: &LiveSession) -> Drain {
        self.drain_with(&session.id, &mut |text| term::send_message(session, text))
    }

    /// The body of [`Self::drain`], with the fragment test and the write as parameters so the queue's
    /// own rules can be exercised without a PTY behind them.
    ///
    /// `fragmented` is read **under the claim**, and again before every write. That is what makes it
    /// enforceable: only the claim holder can set the mark, so a reading taken inside the claim
    /// cannot go stale while this drain runs. Read outside it, the mark could be set between the
    /// reading and the write — which is the same check-then-act hole the mark exists to close.
    fn drain_with(
        &self,
        id: &str,
        write: &mut impl FnMut(&str) -> Result<(), PartialWrite>,
    ) -> Drain {
        // Re-claimed around each pass. A producer that pushed while the previous pass was between
        // its last pop and releasing the claim would otherwise leave a queue with nothing to drain
        // it — and for a session producing no further events, nothing would come along to retry.
        // The claim is released before the queue is re-read, so there is no ordering in which the
        // producer both fails to claim and lets this loop exit.
        loop {
            if !self.claim(id) {
                // Another drainer owns it and will take whatever was just pushed.
                return Drain::Pending;
            }
            let outcome = self.drain_once(id, write);
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
    ) -> Drain {
        loop {
            let Some(text) = self.pop(id) else {
                return Drain::Clear;
            };
            match write(&text) {
                Ok(()) => {}
                // Nothing went in, so the session simply could not take it — most likely a modal its
                // reported state does not show. Put it back at the front and stop: letting the next
                // one through would reorder them.
                Err(partial) if partial.written == 0 => {
                    tracing::warn!(session = %id, error = %partial.error, "a queued message could not be written yet");
                    self.push_front(id, text);
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

    fn pop(&self, id: &str) -> Option<String> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        let queue = queues.get_mut(id)?;
        let next = queue.pop_front();
        if queue.is_empty() {
            queues.remove(id);
        }
        next
    }

    fn push_front(&self, id: &str, text: String) {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .entry(id.to_string())
            .or_default()
            .push_front(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            outbox.drain_with("s", &mut |text| {
                written.push(text.to_string());
                Ok(())
            }),
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
            outbox.drain_with("s", &mut |text| {
                written.push(text.to_string());
                if !pushed_late {
                    pushed_late = true;
                    // Stands in for a producer arriving as this pass finishes: by the time the
                    // drainer re-reads the queue the message is in it.
                    outbox.push("s", "late arrival");
                }
                Ok(())
            }),
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
            outbox.drain_with("s", &mut |_| Err(partial(0))),
            Drain::Pending
        );
        assert_eq!(outbox.len("s"), 2);

        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with("s", &mut |text| {
                written.push(text.to_string());
                Ok(())
            }),
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
            outbox.drain_with("s", &mut |_| Err(partial(3))),
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
        assert_eq!(outbox.drain_with("s", &mut |_| Ok(())), Drain::Pending);
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
}
