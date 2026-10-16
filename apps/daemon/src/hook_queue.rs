//! The hook events waiting to be applied, one ordered queue per session.
//!
//! The hook route answers before an event is applied (see `server::hook_callback`), so what its
//! events do to the session must still happen in the order they arrived: a `Stop` applied before
//! the `PreToolUse` that preceded it would leave a finished session looking busy. A queue exists
//! only while a worker is draining it, so an idle session holds nothing.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::Notify;

use bytes::Bytes;

/// How many events one session may have waiting, the one being applied included. An agent fires a
/// handful of hooks per tool call and each is applied in well under a millisecond, so a queue this
/// long means the session is wedged or the caller is not an agent, so past it the newest event
/// takes the place of the one last in line, and the queue stops growing. The tail event is lost,
/// and the turn bookkeeping it carried (a turn's start or credits, a Stop's report, the agent's
/// session id, usage) can be lost with it. That is accepted: a queue this long means the session
/// is wedged. The status an event carries is last-writer-wins, so the newest event is the one
/// that matters most, and the queue's head, the one being applied, is never touched.
const MAX_QUEUED: usize = 256;

/// How long [`HookQueues::settled`] waits for a session's queue to drain before giving up.
const SETTLE_BOUND: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct HookQueues {
    sessions: Mutex<HashMap<String, VecDeque<Bytes>>>,
    /// Woken whenever a session's queue goes away.
    drained: Notify,
}

/// What a [`HookQueues::push`] asks of its caller.
#[derive(Debug, PartialEq, Eq)]
pub enum Push {
    /// The queue was empty: the caller starts the worker that drains it.
    StartWorker,
    /// A worker is already draining the session's queue and will reach the event.
    Queued,
    /// The queue is full: the event took the place of the one last in line.
    Coalesced,
}

impl HookQueues {
    pub fn push(&self, session: &str, event: Bytes) -> Push {
        let mut sessions = self.sessions.lock().expect("hook queue lock poisoned");
        match sessions.get_mut(session) {
            Some(queue) if queue.len() >= MAX_QUEUED => {
                // At least two long, so the tail is never the event being applied.
                *queue.back_mut().expect("a full queue is not empty") = event;
                Push::Coalesced
            }
            Some(queue) => {
                queue.push_back(event);
                Push::Queued
            }
            None => {
                sessions.insert(session.to_string(), VecDeque::from([event]));
                Push::StartWorker
            }
        }
    }

    /// The event to apply next, which stays queued until [`Self::complete`], so the session reads
    /// as busy for as long as it is being applied.
    pub fn next(&self, session: &str) -> Option<Bytes> {
        let sessions = self.sessions.lock().expect("hook queue lock poisoned");
        sessions.get(session)?.front().cloned()
    }

    /// Marks the event [`Self::next`] returned as applied. Returns whether another is waiting; when
    /// none is, the queue is gone in the same step, so a push after it starts a worker of its own.
    pub fn complete(&self, session: &str) -> bool {
        let mut sessions = self.sessions.lock().expect("hook queue lock poisoned");
        let Some(queue) = sessions.get_mut(session) else {
            return false;
        };
        queue.pop_front();
        if queue.is_empty() {
            sessions.remove(session);
            self.drained.notify_waiters();
            return false;
        }
        true
    }

    /// Drops the session's queue, for a worker that is going away without having drained it.
    pub fn abandon(&self, session: &str) {
        self.sessions
            .lock()
            .expect("hook queue lock poisoned")
            .remove(session);
        self.drained.notify_waiters();
    }

    /// Waits until the session has no event waiting or being applied, for a handler that reads or
    /// writes what those events change (the session's turn): the hook route answers before an event
    /// is applied, so a call the agent makes right after its hook would otherwise run first. Bounded,
    /// so a wedged queue delays such a call by [`SETTLE_BOUND`] at most.
    pub async fn settled(&self, session: &str) {
        let wait = async {
            loop {
                let woken = self.drained.notified();
                tokio::pin!(woken);
                // Registered before the check, so a drain between the two is not missed.
                woken.as_mut().enable();
                if !self
                    .sessions
                    .lock()
                    .expect("hook queue lock poisoned")
                    .contains_key(session)
                {
                    return;
                }
                woken.await;
            }
        };
        let _ = tokio::time::timeout(SETTLE_BOUND, wait).await;
    }

    /// Whether any event, of any session, is waiting or being applied.
    #[cfg(test)]
    pub fn is_busy(&self) -> bool {
        !self
            .sessions
            .lock()
            .expect("hook queue lock poisoned")
            .is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_come_out_in_arrival_order_and_the_queue_goes_when_drained() {
        let queues = HookQueues::default();
        assert_eq!(queues.push("s", Bytes::from("a")), Push::StartWorker);
        assert_eq!(queues.push("s", Bytes::from("b")), Push::Queued);
        assert_eq!(queues.push("t", Bytes::from("x")), Push::StartWorker);

        assert_eq!(queues.next("s").unwrap(), "a");
        assert!(queues.is_busy());
        assert!(queues.complete("s"));
        assert_eq!(queues.next("s").unwrap(), "b");
        assert!(!queues.complete("s"));
        assert!(queues.next("s").is_none());
        assert_eq!(queues.push("s", Bytes::from("c")), Push::StartWorker);
    }

    #[test]
    fn a_full_queue_replaces_its_tail_with_what_comes_after() {
        let queues = HookQueues::default();
        assert_eq!(queues.push("s", Bytes::from("0")), Push::StartWorker);
        for _ in 1..MAX_QUEUED {
            assert_eq!(queues.push("s", Bytes::from("e")), Push::Queued);
        }
        assert_eq!(queues.push("s", Bytes::from("late")), Push::Coalesced);
        assert_eq!(queues.next("s").unwrap(), "0");
        for _ in 1..MAX_QUEUED - 1 {
            assert!(queues.complete("s"));
        }
        assert_eq!(queues.next("s").unwrap(), "e");
        assert!(queues.complete("s"));
        assert_eq!(queues.next("s").unwrap(), "late");
        assert!(!queues.complete("s"));
    }

    #[tokio::test(start_paused = true)]
    async fn settled_returns_once_the_queue_drains_and_gives_up_on_a_wedged_one() {
        let queues = std::sync::Arc::new(HookQueues::default());
        queues.settled("s").await;

        queues.push("s", Bytes::from("a"));
        let waiting = {
            let queues = queues.clone();
            tokio::spawn(async move { queues.settled("s").await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiting.is_finished());
        queues.complete("s");
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .expect("settled returns when the queue drains")
            .unwrap();

        queues.push("s", Bytes::from("b"));
        let started = tokio::time::Instant::now();
        queues.settled("s").await;
        assert!(started.elapsed() >= SETTLE_BOUND);
    }
}
