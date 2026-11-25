//! The hook events waiting to be applied, one ordered queue per session.
//!
//! The hook route answers before an event is applied (see `server::callbacks::hook_callback`), so
//! what its events do to the session must still happen in the order they arrived: a `Stop` applied
//! before the `PreToolUse` that preceded it would leave a finished session looking busy. A queue
//! exists only while a worker is draining it, so an idle session holds nothing.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::Notify;

use bytes::Bytes;

/// A hook event waiting to be applied: the body as it was posted, and the name of the event when
/// the route was given one beside it (its `event` query parameter), for a payload that names none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedHook {
    pub body: Bytes,
    pub launch: Option<String>,
    pub event: Option<String>,
}

#[cfg(test)]
impl From<Bytes> for QueuedHook {
    /// An event posted with no name beside its body.
    fn from(body: Bytes) -> Self {
        Self {
            body,
            launch: None,
            event: None,
        }
    }
}

/// How many events one session may have waiting, the one being applied included. An agent fires a
/// handful of hooks per tool call and each is applied in well under a millisecond, so a queue this
/// long means the session is wedged or the caller is not an agent, so past it the newest event
/// takes the place of the one last in line, and the queue stops growing. The tail event is lost,
/// and the turn bookkeeping it carried (a turn's start or credits, a Stop's report, the agent's
/// session id, usage) can be lost with it. That is accepted: a queue this long means the session
/// is wedged. The status an event carries is last-writer-wins, so the newest event is the one
/// that matters most, and the queue's head, the one being applied, is never touched.
const MAX_QUEUED: usize = 256;

/// How long [`HookQueues::settled`] waits for a session's queue to drain before giving up; a
/// session's program status reports settle within the same bound
/// (`LiveSession::settle_program_status`).
const SETTLE_BOUND: Duration = Duration::from_secs(2);

/// [`SETTLE_BOUND`], widened to `PATIENCE` under test so a test holding an event back is not
/// raced by it, and lowered by `SETTLE_OVERRIDE` for the test proving it runs out.
pub(crate) fn settle_bound() -> Duration {
    #[cfg(test)]
    {
        SETTLE_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    SETTLE_BOUND
}

#[cfg(test)]
thread_local! {
    static SETTLE_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

#[derive(Default)]
pub struct HookQueues {
    sessions: Mutex<HashMap<String, VecDeque<QueuedHook>>>,
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
    pub fn push(&self, session: &str, event: impl Into<QueuedHook>) -> Push {
        let event = event.into();
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
    pub fn next(&self, session: &str) -> Option<QueuedHook> {
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
        let _ = tokio::time::timeout(settle_bound(), wait).await;
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

    fn hook(body: &'static str) -> QueuedHook {
        Bytes::from(body).into()
    }

    #[test]
    fn events_come_out_in_arrival_order_and_the_queue_goes_when_drained() {
        let queues = HookQueues::default();
        assert_eq!(queues.push("s", hook("a")), Push::StartWorker);
        assert_eq!(queues.push("s", hook("b")), Push::Queued);
        assert_eq!(queues.push("t", hook("x")), Push::StartWorker);

        assert_eq!(queues.next("s").unwrap().body, "a");
        assert!(queues.is_busy());
        assert!(queues.complete("s"));
        assert_eq!(queues.next("s").unwrap().body, "b");
        assert!(!queues.complete("s"));
        assert!(queues.next("s").is_none());
        assert_eq!(queues.push("s", hook("c")), Push::StartWorker);
    }

    /// The event's name travels beside its body, through the queue in order, and is absent for a
    /// body that came with none; the body is never rewritten to carry it.
    #[test]
    fn an_events_name_stays_beside_its_body_through_the_queue() {
        let queues = HookQueues::default();
        let named = |body: &'static str, event: &str| QueuedHook {
            body: Bytes::from(body),
            launch: None,
            event: Some(event.to_string()),
        };
        assert_eq!(
            queues.push("s", named("a", "PreToolUse")),
            Push::StartWorker
        );
        assert_eq!(queues.push("s", hook("b")), Push::Queued);
        assert_eq!(queues.push("s", named("c", "Stop")), Push::Queued);

        assert_eq!(queues.next("s"), Some(named("a", "PreToolUse")));
        assert!(queues.complete("s"));
        assert_eq!(queues.next("s").unwrap().event, None);
        assert_eq!(queues.next("s").unwrap().body, "b");
        assert!(queues.complete("s"));
        assert_eq!(queues.next("s"), Some(named("c", "Stop")));
        assert_eq!(QueuedHook::from(Bytes::from("x")).event, None);
    }

    #[test]
    fn a_full_queue_replaces_its_tail_with_what_comes_after() {
        let queues = HookQueues::default();
        assert_eq!(queues.push("s", hook("0")), Push::StartWorker);
        for _ in 1..MAX_QUEUED {
            assert_eq!(queues.push("s", hook("e")), Push::Queued);
        }
        assert_eq!(queues.push("s", hook("late")), Push::Coalesced);
        assert_eq!(queues.next("s").unwrap().body, "0");
        for _ in 1..MAX_QUEUED - 1 {
            assert!(queues.complete("s"));
        }
        assert_eq!(queues.next("s").unwrap().body, "e");
        assert!(queues.complete("s"));
        assert_eq!(queues.next("s").unwrap().body, "late");
        assert!(!queues.complete("s"));
    }

    #[tokio::test(start_paused = true)]
    async fn settled_returns_once_the_queue_drains_and_gives_up_on_a_wedged_one() {
        SETTLE_OVERRIDE.with(|bound| bound.set(Some(SETTLE_BOUND)));
        let queues = std::sync::Arc::new(HookQueues::default());
        queues.settled("s").await;

        queues.push("s", hook("a"));
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

        queues.push("s", hook("b"));
        let started = tokio::time::Instant::now();
        queues.settled("s").await;
        assert!(started.elapsed() >= SETTLE_BOUND);
    }
}
