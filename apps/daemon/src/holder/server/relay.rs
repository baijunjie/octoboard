//! A helper's request on its way to the daemon over the daemon link, and the daemon's answer on
//! its way back.
//!
//! A request nobody waits on (a report) is queued and handed to the daemon in order by a task of
//! its own, so the helper is answered at once whether or not a daemon is linked. While none is,
//! the queue keeps the newest of them, at most [`QUEUE_LIMIT`] and [`QUEUE_BYTES`] of bodies, for
//! the next daemon that links. A request
//! its helper waits on (a question) is sent at once to a daemon that is linked, and held up to
//! the holder's deadline for one to link otherwise. Once sent it is never sent again: a link
//! that drops while it is waiting fails it, since the daemon may have acted on it.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::{oneshot, watch, Notify};
use tokio::time::Instant;

use super::terminal::{frame, Sink};
use crate::holder::protocol::{FromHolder, RelayRoute};

/// Reports kept while no daemon is linked. Past it the oldest is dropped: a daemon that is away
/// for long is better told what happened last than nothing.
const QUEUE_LIMIT: usize = 1024;

/// The most bytes of bodies the queue of reports holds while no daemon is linked. Past it the
/// oldest are dropped first, as past [`QUEUE_LIMIT`].
const QUEUE_BYTES: usize = 16 * 1024 * 1024;

/// What the daemon answered a question with.
#[derive(Debug)]
pub(super) struct Answer {
    pub(super) status: u16,
    pub(super) body: String,
}

/// Why a question got no answer.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failure {
    /// No daemon linked within the deadline.
    NoDaemon,
    /// The link was lost, or went away, after the question was sent.
    Lost,
}

struct Linked {
    generation: u64,
    sink: Sink,
}

struct Waiting {
    generation: u64,
    reply: oneshot::Sender<Answer>,
}

#[derive(Default)]
struct State {
    link: Option<Linked>,
    queue: VecDeque<FromHolder>,
    /// The bytes of the bodies in `queue`.
    queued_bytes: usize,
    waiting: HashMap<u64, Waiting>,
}

/// The bytes a queued request holds.
fn body_len(relay: &FromHolder) -> usize {
    match relay {
        FromHolder::Relay { body, .. } => body.len(),
        _ => 0,
    }
}

impl State {
    fn push_back(&mut self, relay: FromHolder) {
        self.queued_bytes += body_len(&relay);
        self.queue.push_back(relay);
    }

    fn push_front(&mut self, relay: FromHolder) {
        self.queued_bytes += body_len(&relay);
        self.queue.push_front(relay);
    }

    fn pop_front(&mut self) -> Option<FromHolder> {
        let relay = self.queue.pop_front()?;
        self.queued_bytes -= body_len(&relay);
        Some(relay)
    }
}

pub(super) struct Relays {
    state: Mutex<State>,
    next_req: AtomicU64,
    /// How long a question waits for a daemon to link.
    deadline: Duration,
    /// Something is queued, or a daemon linked.
    wake: Notify,
    linked: watch::Sender<bool>,
}

impl Relays {
    /// Starts the task that hands queued reports over, so it needs a runtime.
    pub(super) fn new(deadline: Duration) -> Arc<Self> {
        let relays = Arc::new(Self {
            state: Mutex::new(State::default()),
            next_req: AtomicU64::new(1),
            deadline,
            wake: Notify::new(),
            linked: watch::channel(false).0,
        });
        tokio::spawn(relays.clone().hand_over());
        relays
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn relay(
        &self,
        route: RelayRoute,
        param: String,
        body: Vec<u8>,
        waits: bool,
    ) -> (u64, FromHolder) {
        let req = self.next_req.fetch_add(1, Ordering::Relaxed);
        (
            req,
            FromHolder::Relay {
                req,
                route,
                param,
                body,
                waits,
            },
        )
    }

    /// A daemon has linked and finished its handshake: reports go to it, queued ones first.
    pub(super) fn attach(&self, generation: u64, sink: Sink) {
        self.state().link = Some(Linked { generation, sink });
        self.linked.send_replace(true);
        self.wake.notify_one();
    }

    /// The link `generation` is over. The questions it was carrying fail, and so would those of no
    /// other link, which is the point: nothing is sent again.
    pub(super) fn detach(&self, generation: u64) {
        let mut state = self.state();
        state
            .waiting
            .retain(|_, waiting| waiting.generation != generation);
        if state
            .link
            .as_ref()
            .is_some_and(|link| link.generation == generation)
        {
            state.link = None;
            self.linked.send_replace(false);
        }
    }

    /// Queues a report for the daemon. Never waits.
    pub(super) fn report(&self, route: RelayRoute, param: String, body: Vec<u8>) {
        let (_, relay) = self.relay(route, param, body, false);
        {
            let mut state = self.state();
            let size = body_len(&relay);
            while state.queue.len() >= QUEUE_LIMIT
                || (!state.queue.is_empty() && state.queued_bytes + size > QUEUE_BYTES)
            {
                state.pop_front();
                tracing::warn!(
                    limit = QUEUE_LIMIT,
                    bytes = QUEUE_BYTES,
                    "the report queue is full; the oldest report was dropped"
                );
            }
            state.push_back(relay);
        }
        self.wake.notify_one();
    }

    /// Sends the daemon a question and waits for its answer. Cancelling the future (the helper
    /// went away) tells the daemon the question is gone.
    pub(super) async fn ask(
        self: &Arc<Self>,
        route: RelayRoute,
        param: String,
        body: Vec<u8>,
    ) -> Result<Answer, Failure> {
        let give_up = Instant::now() + self.deadline;
        let mut linked = self.linked.subscribe();
        let (req, relay, sink, replied) = loop {
            {
                let mut state = self.state();
                if let Some(link) = &state.link {
                    let (sink, generation) = (link.sink.clone(), link.generation);
                    let (reply, replied) = oneshot::channel();
                    let (req, relay) = self.relay(route, param.clone(), body.clone(), true);
                    state.waiting.insert(req, Waiting { generation, reply });
                    break (req, relay, sink, replied);
                }
            }
            let linked_in_time = tokio::time::timeout_at(give_up, linked.wait_for(|up| *up))
                .await
                .is_ok();
            if !linked_in_time {
                return Err(Failure::NoDaemon);
            }
        };
        let mut asked = Asked {
            relays: self.clone(),
            req,
            sink: sink.clone(),
            open: true,
        };
        if sink.send(frame(&relay)).await.is_err() {
            return Err(Failure::Lost);
        }
        let answer = replied.await;
        asked.open = false;
        answer.map_err(|_| Failure::Lost)
    }

    /// The daemon's answer to the question `req`.
    pub(super) fn answer(&self, req: u64, answer: Answer) {
        if let Some(waiting) = self.state().waiting.remove(&req) {
            waiting.reply.send(answer).ok();
        }
    }

    /// Hands the queued reports to the daemon in order, whenever there are some and one is linked.
    async fn hand_over(self: Arc<Self>) {
        loop {
            self.wake.notified().await;
            loop {
                let (sink, relay) = {
                    let mut state = self.state();
                    let Some(link) = &state.link else { break };
                    let sink = link.sink.clone();
                    let Some(relay) = state.pop_front() else {
                        break;
                    };
                    (sink, relay)
                };
                if sink.send(frame(&relay)).await.is_err() {
                    // The link is going away; the next daemon is handed this one first.
                    self.state().push_front(relay);
                    break;
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn queued(&self) -> usize {
        self.state().queue.len()
    }
}

/// A question that is out and unanswered. Dropped while it is, it tells the daemon to stop.
struct Asked {
    relays: Arc<Relays>,
    req: u64,
    sink: Sink,
    open: bool,
}

impl Drop for Asked {
    fn drop(&mut self) {
        if !self.open {
            return;
        }
        let answered = self.relays.state().waiting.remove(&self.req).is_none();
        if answered {
            return;
        }
        let (sink, req) = (self.sink.clone(), self.req);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                sink.send(frame(&FromHolder::RelayGone { req })).await.ok();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::extract::ws::Message;
    use tokio::sync::mpsc;

    use super::*;

    const PATIENCE: Duration = crate::test_support::PATIENCE;

    fn link() -> (Sink, mpsc::Receiver<Message>) {
        mpsc::channel(64)
    }

    async fn next(frames: &mut mpsc::Receiver<Message>) -> FromHolder {
        let Message::Text(text) = tokio::time::timeout(PATIENCE, frames.recv())
            .await
            .expect("a frame in time")
            .expect("a frame")
        else {
            panic!("not a text frame");
        };
        serde_json::from_str(&text).expect("a message")
    }

    fn report_of(frame: &FromHolder) -> (&str, &[u8]) {
        match frame {
            FromHolder::Relay {
                param,
                body,
                waits: false,
                ..
            } => (param.as_str(), body.as_slice()),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn reports_made_while_no_daemon_is_linked_reach_the_next_one_in_order() {
        let relays = Relays::new(PATIENCE);
        for n in 0..5 {
            relays.report(RelayRoute::Hook, "s".into(), vec![n]);
        }
        assert_eq!(relays.queued(), 5);

        let (sink, mut frames) = link();
        relays.attach(1, sink);
        for n in 0..5 {
            assert_eq!(report_of(&next(&mut frames).await), ("s", &[n][..]));
        }
        // And a report made once linked follows the earlier ones.
        relays.report(RelayRoute::GitRef, "s".into(), vec![9]);
        assert_eq!(report_of(&next(&mut frames).await), ("s", &[9][..]));
        assert_eq!(relays.queued(), 0);
    }

    #[tokio::test]
    async fn a_full_queue_drops_the_oldest_report() {
        let relays = Relays::new(PATIENCE);
        for n in 0..QUEUE_LIMIT + 3 {
            relays.report(
                RelayRoute::Hook,
                "s".into(),
                (n as u32).to_be_bytes().to_vec(),
            );
        }
        assert_eq!(relays.queued(), QUEUE_LIMIT);
        let (sink, mut frames) = mpsc::channel(QUEUE_LIMIT + 8);
        relays.attach(1, sink);
        let first = next(&mut frames).await;
        assert_eq!(report_of(&first).1, 3u32.to_be_bytes());
    }

    #[tokio::test]
    async fn a_queue_over_its_bytes_drops_the_oldest_reports_first() {
        let relays = Relays::new(PATIENCE);
        let big = QUEUE_BYTES / 2 + 1;
        for n in 0..3u8 {
            relays.report(RelayRoute::Hook, "s".into(), vec![n; big]);
        }
        assert_eq!(relays.queued(), 1, "only the newest of three fits");
        let (sink, mut frames) = link();
        relays.attach(1, sink);
        assert_eq!(report_of(&next(&mut frames).await).1[0], 2);
    }

    #[tokio::test]
    async fn a_question_is_sent_at_once_and_answered_by_its_request_number() {
        let relays = Relays::new(PATIENCE);
        let (sink, mut frames) = link();
        relays.attach(1, sink);
        let asking = tokio::spawn({
            let relays = relays.clone();
            async move {
                relays
                    .ask(RelayRoute::RevertGate, "s".into(), b"{}".to_vec())
                    .await
            }
        });
        let FromHolder::Relay {
            req,
            route,
            param,
            waits,
            ..
        } = next(&mut frames).await
        else {
            panic!("not a relay");
        };
        assert_eq!(
            (route, param.as_str(), waits),
            (RelayRoute::RevertGate, "s", true)
        );
        relays.answer(
            req,
            Answer {
                status: 200,
                body: "null".into(),
            },
        );
        let answer = asking.await.unwrap().expect("answered");
        assert_eq!((answer.status, answer.body.as_str()), (200, "null"));
    }

    #[tokio::test]
    async fn a_question_with_no_daemon_fails_once_the_deadline_passes() {
        let relays = Relays::new(Duration::from_millis(150));
        let started = std::time::Instant::now();
        let failed = relays
            .ask(RelayRoute::Mcp, "token".into(), Vec::new())
            .await
            .expect_err("no daemon");
        assert_eq!(failed, Failure::NoDaemon);
        assert!(started.elapsed() >= Duration::from_millis(150));
    }

    #[tokio::test]
    async fn a_question_waits_for_a_daemon_that_links_within_the_deadline() {
        let relays = Relays::new(PATIENCE);
        let asking = tokio::spawn({
            let relays = relays.clone();
            async move {
                relays
                    .ask(RelayRoute::Mcp, "token".into(), Vec::new())
                    .await
            }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (sink, mut frames) = link();
        relays.attach(1, sink);
        let FromHolder::Relay { req, .. } = next(&mut frames).await else {
            panic!("not a relay");
        };
        relays.answer(
            req,
            Answer {
                status: 200,
                body: String::new(),
            },
        );
        assert!(asking.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn a_link_that_drops_fails_the_question_it_was_carrying_at_once() {
        let relays = Relays::new(PATIENCE);
        let (sink, mut frames) = link();
        relays.attach(1, sink);
        let asking = tokio::spawn({
            let relays = relays.clone();
            async move {
                relays
                    .ask(RelayRoute::RevertGate, "s".into(), Vec::new())
                    .await
            }
        });
        next(&mut frames).await;
        relays.detach(1);
        let failed = tokio::time::timeout(Duration::from_secs(2), asking)
            .await
            .expect("failed at once, not at the deadline")
            .unwrap()
            .expect_err("lost");
        assert_eq!(failed, Failure::Lost);
    }

    #[tokio::test]
    async fn a_link_taken_over_fails_its_questions_but_leaves_the_newer_link_in_place() {
        let relays = Relays::new(PATIENCE);
        let (older, mut older_frames) = link();
        relays.attach(1, older);
        let asking = tokio::spawn({
            let relays = relays.clone();
            async move {
                relays
                    .ask(RelayRoute::RevertGate, "s".into(), Vec::new())
                    .await
            }
        });
        next(&mut older_frames).await;
        let (newer, mut newer_frames) = link();
        relays.attach(2, newer);
        relays.detach(1);
        assert_eq!(asking.await.unwrap().expect_err("lost"), Failure::Lost);
        relays.report(RelayRoute::Hook, "s".into(), vec![1]);
        assert_eq!(report_of(&next(&mut newer_frames).await), ("s", &[1][..]));
    }

    #[tokio::test]
    async fn a_question_whose_helper_goes_away_tells_the_daemon() {
        let relays = Relays::new(PATIENCE);
        let (sink, mut frames) = link();
        relays.attach(1, sink);
        let asking = tokio::spawn({
            let relays = relays.clone();
            async move {
                relays
                    .ask(RelayRoute::Mcp, "token".into(), Vec::new())
                    .await
            }
        });
        let FromHolder::Relay { req, .. } = next(&mut frames).await else {
            panic!("not a relay");
        };
        asking.abort();
        let FromHolder::RelayGone { req: gone } = next(&mut frames).await else {
            panic!("not a relay_gone");
        };
        assert_eq!(gone, req);
        // A late answer to it finds nobody.
        relays.answer(
            req,
            Answer {
                status: 200,
                body: String::new(),
            },
        );
    }
}
