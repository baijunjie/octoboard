//! A session's extension channel: the way a session's automatic messages reach an agent whose
//! provider runs an extension inside the agent's process, instead of its terminal.
//!
//! The daemon owns the channel ([`Channel`], a [`MessageTransport`]). The extension only forwards:
//! it starts this executable in its `channel` mode
//! (`crate::cli::channel`), and that helper long-polls `POST /channel/<session>` for the messages
//! to send as user messages and posts what came of each. The route's body and answer are
//! [`PollRequest`] and [`PollReply`]; `apps/daemon/PROTOCOL.md`, "`POST /channel/:session`", is
//! their contract.
//!
//! What a [`Channel::send`] answers:
//!
//! - No helper is polling, or none picked the message up in time: nothing of it left the daemon,
//!   and it is declined, so the terminal takes it. A channel that has gone costs the session only
//!   the channel.
//! - Handed over and reported `taken`: the agent has it ([`Sent::Queued`]).
//! - Handed over and reported `refused`: the agent refused it ([`RefusedByAgent`]).
//! - Handed over and reported `returned`: the extension could not give it to the agent and nothing
//!   of it is left there, so it is declined.
//! - Handed over with no outcome in time: the agent may have it ([`UncertainSubmission`]), and it
//!   is not sent again by any route.
//!
//! A message is handed over again only to the helper it was handed to (a poll names its helper's
//! instance), and only to a poll of it that does not list the message as held, until its outcome
//! comes: an answer lost on its way to the helper loses no message, and the helper tells a repeat
//! by its id. Another helper is never handed it: the extension that started the first one may
//! have sent it to the agent already, and the one that started the next (the extension reloaded)
//! does not know, so the message is left to its outcome window and ends uncertain.
//!
//! A message reported `returned` after it was `taken` is one the agent gave back unsent, which
//! the session's queue then writes again ([`MessageTransport::on_returned`]), whether the report
//! comes after the send has answered or while it is still on its way to answering.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::term::{
    GivenBack, MessageTransport, QueueStart, RefusedByAgent, Sent, TransportEvents,
    UncertainSubmission,
};
use crate::platform::term::PartialWrite;

/// The ids of the messages the agent took that a channel remembers, the latest ones: a message
/// reported `returned` is passed on only while its id is among them.
const TAKEN_KEPT: usize = 64;

/// The windows of a channel.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    /// How long a message waits for a poll to pick it up before it is declined.
    pub(crate) pickup: Duration,
    /// How long a message handed over waits for its outcome before its delivery is uncertain.
    pub(crate) outcome: Duration,
    /// How long a poll waits for a message before it is answered with none.
    pub(crate) poll: Duration,
    /// How long after its last poll ended a helper still counts as connected: the gap between
    /// one poll's answer and the next poll.
    pub(crate) gap: Duration,
}

impl Timing {
    /// The windows a session's channel runs with. A poll is answered well inside the time its
    /// helper waits for an answer (`crate::cli::channel`), and a handed-over message waits no
    /// longer for its outcome than a write into the terminal may take.
    #[cfg(not(test))]
    pub(crate) fn standard() -> Self {
        Self {
            pickup: Duration::from_secs(2),
            outcome: Duration::from_secs(10),
            poll: Duration::from_secs(20),
            gap: Duration::from_secs(3),
        }
    }

    /// In tests, the windows a stand-in must answer inside are the tests' patience.
    #[cfg(test)]
    pub(crate) fn standard() -> Self {
        Self {
            pickup: crate::test_support::PATIENCE,
            outcome: crate::test_support::PATIENCE,
            poll: Duration::from_secs(20),
            gap: crate::test_support::PATIENCE,
        }
    }
}

/// What a poll posts: which helper it is, the outcomes of messages handed over before, the ids of
/// the messages the helper holds without an outcome yet, and whether it waits for messages. Every
/// field may be left out.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct PollRequest {
    /// The helper's instance: an id it made up when it started, the same in each of its polls. A
    /// message handed over is handed over again only to the instance it was handed to. Polls
    /// that name none count as one instance.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) instance: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) outcomes: Vec<Reported>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) holding: Vec<String>,
    /// Whether the poll waits for messages to hand over. One that only reports outcomes does
    /// not, and is answered at once with no messages.
    #[serde(default)]
    pub(crate) wait: bool,
}

/// What came of one message handed over, as the extension reports it: one JSON line on the
/// helper's standard input, passed on as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Reported {
    pub(crate) id: String,
    pub(crate) outcome: Outcome,
    /// Why, for a refusal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    /// The agent has the message: it runs it, or holds it for its running turn.
    Taken,
    /// The agent refused the message; nothing of it was taken.
    Refused,
    /// Nothing of the message is left with the agent, its input box included: it could not be
    /// given to the agent, or the agent gave it back unsent after it was taken.
    Returned,
    /// An outcome this build does not know, which says nothing.
    #[serde(other)]
    Unknown,
}

/// A poll's answer: the messages to send, in order.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PollReply {
    #[serde(default)]
    pub(crate) messages: Vec<Handed>,
}

/// One message to send as a user message: one JSON line on the helper's standard output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Handed {
    /// The id its outcome is reported under.
    pub(crate) id: String,
    pub(crate) text: String,
}

struct Outgoing {
    id: String,
    text: String,
    /// The instance of the helper it was handed to, once it was handed over.
    handed_to: Option<String>,
    outcome: Option<(Outcome, Option<String>)>,
    /// Reported `returned` after its outcome `taken`, before the send has answered for it: the
    /// agent gave it back, which the send passes on once it has answered.
    given_back: bool,
}

#[derive(Default)]
struct State {
    /// The messages a [`Channel::send`] waits on, in the order they were sent.
    outgoing: VecDeque<Outgoing>,
    /// The polls waiting for a message now.
    polls: usize,
    /// When the last poll that waited ended.
    last_poll: Option<Instant>,
    /// The ids of the latest messages the agent took.
    taken: VecDeque<String>,
    returned: Option<Arc<GivenBack>>,
    /// The session has ended: nothing is handed over any more.
    closed: bool,
}

impl State {
    fn connected(&self, gap: Duration) -> bool {
        self.polls > 0 || self.last_poll.is_some_and(|ended| ended.elapsed() < gap)
    }

    fn of(&self, id: &str) -> Option<&Outgoing> {
        self.outgoing.iter().find(|message| message.id == id)
    }

    fn remove(&mut self, id: &str) -> Option<Outgoing> {
        let at = self.outgoing.iter().position(|message| message.id == id)?;
        self.outgoing.remove(at)
    }
}

/// A session's extension channel, on the daemon's side.
pub struct Channel {
    state: Mutex<State>,
    /// Wakes the sends: a message was handed over, an outcome came, or the channel closed.
    changed: Condvar,
    /// Wakes the waiting poll: a message is there to hand over, or the channel closed.
    wake: tokio::sync::Notify,
    timing: Timing,
}

impl Channel {
    pub(crate) fn new() -> Self {
        Self::with_timing(Timing::standard())
    }

    pub(crate) fn with_timing(timing: Timing) -> Self {
        Self {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            wake: tokio::sync::Notify::new(),
            timing,
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Serves one poll of the session's helper: takes in the outcomes it reports, and, for a poll
    /// that waits, answers with the messages to hand over once there are some, or with none once
    /// the poll's window has run out. `None` once the session has ended, which the helper ends on.
    /// Dropping the future is the helper hanging up: the messages it was about to be answered
    /// with are handed over again to its next poll.
    pub(crate) async fn poll(&self, request: PollRequest) -> Option<PollReply> {
        self.report(request.outcomes);
        if !request.wait {
            return (!self.state().closed).then(PollReply::default);
        }
        let _polling = Polling::begin(self);
        let deadline = tokio::time::Instant::now() + self.timing.poll;
        loop {
            {
                let mut state = self.state();
                if state.closed {
                    return None;
                }
                let messages: Vec<Handed> = state
                    .outgoing
                    .iter_mut()
                    .filter(|message| {
                        message.outcome.is_none()
                            && match &message.handed_to {
                                None => true,
                                // Another instance is never handed it: the extension behind the
                                // first may have sent it already.
                                Some(instance) => {
                                    *instance == request.instance
                                        && !request.holding.contains(&message.id)
                                }
                            }
                    })
                    .map(|message| {
                        message.handed_to = Some(request.instance.clone());
                        Handed {
                            id: message.id.clone(),
                            text: message.text.clone(),
                        }
                    })
                    .collect();
                if !messages.is_empty() {
                    self.changed.notify_all();
                    return Some(PollReply { messages });
                }
            }
            if tokio::time::timeout_at(deadline, self.wake.notified())
                .await
                .is_err()
            {
                return Some(PollReply::default());
            }
        }
    }

    /// Takes in what the extension reports of messages handed over: the outcome a send waits on,
    /// or, for a message the agent took before, that it gave it back.
    fn report(&self, outcomes: Vec<Reported>) {
        let mut given_back = Vec::new();
        let returned = {
            let mut state = self.state();
            for reported in outcomes {
                if reported.outcome == Outcome::Unknown {
                    continue;
                }
                let waiting = state
                    .outgoing
                    .iter_mut()
                    .find(|message| message.id == reported.id && message.handed_to.is_some());
                if let Some(message) = waiting {
                    let earlier = message.outcome.as_ref().map(|(outcome, _)| *outcome);
                    match earlier {
                        None => message.outcome = Some((reported.outcome, reported.reason)),
                        // Given back before the send has woken to answer that it was taken: kept
                        // for the send to pass on, since the session knows no such message yet.
                        Some(Outcome::Taken) if reported.outcome == Outcome::Returned => {
                            message.given_back = true;
                        }
                        // The first outcome stands.
                        Some(_) => {}
                    }
                } else if reported.outcome == Outcome::Returned {
                    if let Some(at) = state.taken.iter().position(|id| *id == reported.id) {
                        state.taken.remove(at);
                        given_back.push(reported.id);
                    }
                }
            }
            self.changed.notify_all();
            state.returned.clone()
        };
        // Outside the lock: the session's queue takes its own.
        if let Some(returned) = returned {
            for id in &given_back {
                returned(id);
            }
        }
    }

    /// The helper's connection is as good as gone: the gap after its last poll has run out.
    #[cfg(test)]
    pub(crate) fn forget_last_poll(&self) {
        self.state().last_poll = None;
    }

    /// How many polls wait for a message now.
    #[cfg(test)]
    pub(crate) fn waiting_polls(&self) -> usize {
        self.state().polls
    }
}

/// A poll that waits, counted as the helper's connection while it does.
struct Polling<'a>(&'a Channel);

impl<'a> Polling<'a> {
    fn begin(channel: &'a Channel) -> Self {
        channel.state().polls += 1;
        Self(channel)
    }
}

impl Drop for Polling<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state();
        state.polls -= 1;
        state.last_poll = Some(Instant::now());
    }
}

fn declined(reason: &str) -> Result<Sent, PartialWrite> {
    Ok(Sent::Declined(reason.to_string()))
}

impl MessageTransport for Channel {
    fn send(&self, text: &str, logical_id: &str) -> Result<Sent, PartialWrite> {
        let mut state = self.state();
        if state.closed || !state.connected(self.timing.gap) {
            return declined("no extension is connected to the session's channel");
        }
        state.outgoing.push_back(Outgoing {
            id: logical_id.to_string(),
            text: text.to_string(),
            handed_to: None,
            outcome: None,
            given_back: false,
        });
        self.wake.notify_one();

        let handed_over = |state: &State| {
            state
                .of(logical_id)
                .is_some_and(|message| message.handed_to.is_some())
        };
        let (mut state, _) = self
            .changed
            .wait_timeout_while(state, self.timing.pickup, |state| {
                !state.closed && !handed_over(state)
            })
            .unwrap_or_else(PoisonError::into_inner);
        if !handed_over(&state) {
            // Under the lock that hands a message over, so no poll takes it after this.
            state.remove(logical_id);
            return declined("the extension did not take the message from the session's channel");
        }

        let (mut state, _) = self
            .changed
            .wait_timeout_while(state, self.timing.outcome, |state| {
                !state.closed
                    && state
                        .of(logical_id)
                        .is_some_and(|message| message.outcome.is_none())
            })
            .unwrap_or_else(PoisonError::into_inner);
        let message = state.remove(logical_id);
        let given_back = message.as_ref().is_some_and(|message| message.given_back);
        match message.and_then(|message| message.outcome) {
            // Taken, and given back before this thread woke: the report found no taken message
            // to give back, so it is passed on here, where the session can know the message.
            Some((Outcome::Taken, _)) if given_back => {
                let returned = state.returned.clone();
                // Outside the lock: the session's queue takes its own.
                drop(state);
                if let Some(returned) = returned {
                    returned(logical_id);
                }
                Ok(Sent::Queued)
            }
            Some((Outcome::Taken, _)) => {
                if state.taken.len() == TAKEN_KEPT {
                    state.taken.pop_front();
                }
                state.taken.push_back(logical_id.to_string());
                Ok(Sent::Queued)
            }
            Some((Outcome::Refused, reason)) => Err(PartialWrite {
                written: 0,
                error: std::io::Error::other(RefusedByAgent(
                    reason.unwrap_or_else(|| "the agent refused the message".to_string()),
                )),
            }),
            Some((Outcome::Returned, _)) => {
                declined("the extension could not give the message to the agent")
            }
            Some((Outcome::Unknown, _)) | None => Err(PartialWrite {
                written: 0,
                error: std::io::Error::other(UncertainSubmission(
                    "the extension was handed the message and did not say what came of it"
                        .to_string(),
                )),
            }),
        }
    }

    /// The channel tells the session nothing of its own: a channel that has gone leaves the
    /// terminal running, and no message is held back for it.
    fn attach(&self, _: TransportEvents) {}

    fn on_returned(&self, returned: Box<GivenBack>) {
        self.state().returned = Some(Arc::from(returned));
    }

    /// The agent runs what it took by itself, and gives back what it does not.
    fn start_queue(&self) -> QueueStart {
        QueueStart::Nothing
    }

    fn stop(&self) {
        self.state().closed = true;
        self.changed.notify_all();
        self.wake.notify_waiters();
        self.wake.notify_one();
    }

    fn channel(&self) -> Option<&Channel> {
        Some(self)
    }
}

/// The transport of a launch whose own server gave `server` (none where it has no server or the
/// server did not start) and whose provider `declared` an extension channel or not, and whether
/// it is the channel: a server takes the channel's place.
pub(crate) fn transport_of(
    server: Option<Arc<dyn MessageTransport>>,
    declared: bool,
) -> (Option<Arc<dyn MessageTransport>>, bool) {
    match server {
        Some(server) => (Some(server), false),
        None if declared => (Some(Arc::new(Channel::new())), true),
        None => (None, false),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::test_support::PATIENCE;

    fn timing() -> Timing {
        Timing {
            pickup: PATIENCE,
            outcome: PATIENCE,
            poll: PATIENCE,
            gap: PATIENCE,
        }
    }

    fn waiting(holding: &[&str]) -> PollRequest {
        waiting_as("", holding)
    }

    /// A waiting poll of the helper `instance`.
    fn waiting_as(instance: &str, holding: &[&str]) -> PollRequest {
        PollRequest {
            instance: instance.to_string(),
            outcomes: Vec::new(),
            holding: holding.iter().map(|id| id.to_string()).collect(),
            wait: true,
        }
    }

    fn outcome(id: &str, outcome: Outcome, reason: Option<&str>) -> PollRequest {
        outcomes(&[(id, outcome, reason)])
    }

    /// One post of several outcomes, taken in under one lock in the order given.
    fn outcomes(reported: &[(&str, Outcome, Option<&str>)]) -> PollRequest {
        PollRequest {
            instance: String::new(),
            outcomes: reported
                .iter()
                .map(|(id, outcome, reason)| Reported {
                    id: id.to_string(),
                    outcome: *outcome,
                    reason: reason.map(str::to_string),
                })
                .collect(),
            holding: Vec::new(),
            wait: false,
        }
    }

    /// Sends `text` as `id` on a thread of its own, since a send blocks until its outcome is in.
    fn sending(
        channel: &Arc<Channel>,
        text: &'static str,
        id: &'static str,
    ) -> tokio::task::JoinHandle<Result<Sent, PartialWrite>> {
        let channel = channel.clone();
        tokio::task::spawn_blocking(move || channel.send(text, id))
    }

    /// A poll that waits, started and seen waiting.
    async fn polling(
        channel: &Arc<Channel>,
        holding: &'static [&'static str],
    ) -> tokio::task::JoinHandle<Option<PollReply>> {
        let before = channel.waiting_polls();
        let poll = {
            let channel = channel.clone();
            tokio::spawn(async move { channel.poll(waiting(holding)).await })
        };
        let seen = channel.clone();
        assert!(
            crate::test_support::within(PATIENCE, move || seen.waiting_polls() > before).await,
            "the poll never waited"
        );
        poll
    }

    fn handed(id: &str, text: &str) -> Option<PollReply> {
        Some(PollReply {
            messages: vec![Handed {
                id: id.to_string(),
                text: text.to_string(),
            }],
        })
    }

    /// A message sent while a helper polls is handed to that poll, and is the agent's once the
    /// helper reports it taken.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_handed_over_and_reported_taken_is_queued() {
        let channel = Arc::new(Channel::with_timing(timing()));
        let poll = polling(&channel, &[]).await;
        let sent = sending(&channel, "the text", "m1");
        assert_eq!(poll.await.unwrap(), handed("m1", "the text"));
        assert_eq!(
            channel.poll(outcome("m1", Outcome::Taken, None)).await,
            Some(PollReply::default())
        );
        assert_eq!(sent.await.unwrap().expect("sent"), Sent::Queued);
    }

    /// A message the agent refused fails as refused, with the reason the extension gave, and one
    /// the extension could not give to the agent is declined, for the terminal to take.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_refusal_fails_the_message_and_one_given_straight_back_is_declined() {
        let channel = Arc::new(Channel::with_timing(timing()));
        let poll = polling(&channel, &[]).await;
        let sent = sending(&channel, "the text", "m1");
        poll.await.unwrap();
        channel
            .poll(outcome("m1", Outcome::Refused, Some("too long")))
            .await;
        let refused = sent.await.unwrap().expect_err("refused");
        assert_eq!(refused.written, 0);
        let reason = refused
            .error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<RefusedByAgent>())
            .expect("a refusal by the agent");
        assert_eq!(reason.0, "too long");

        let poll = polling(&channel, &[]).await;
        let sent = sending(&channel, "another", "m2");
        poll.await.unwrap();
        channel.poll(outcome("m2", Outcome::Returned, None)).await;
        assert!(matches!(sent.await.unwrap(), Ok(Sent::Declined(_))));
    }

    /// A message handed over whose outcome never comes is uncertain: the agent may have it, so it
    /// is never offered to the terminal.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_handed_over_with_no_outcome_in_time_is_uncertain() {
        let channel = Arc::new(Channel::with_timing(Timing {
            outcome: Duration::from_millis(50),
            ..timing()
        }));
        let poll = polling(&channel, &[]).await;
        let sent = sending(&channel, "the text", "m1");
        poll.await.unwrap();
        let failed = sent.await.unwrap().expect_err("uncertain");
        assert!(failed
            .error
            .get_ref()
            .is_some_and(|inner| inner.is::<UncertainSubmission>()));
        // Its late outcome is nothing the channel waits on any more.
        channel.poll(outcome("m1", Outcome::Taken, None)).await;
    }

    /// With no helper polling, a message is declined at once; with one that polled and went
    /// away, it is declined once the gap after its last poll has run out; and one no poll picks
    /// up in time is declined and never handed over afterwards.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_no_helper_takes_is_declined_and_never_handed_over_later() {
        let channel = Arc::new(Channel::with_timing(Timing {
            pickup: Duration::from_millis(50),
            ..timing()
        }));
        assert!(matches!(channel.send("early", "m0"), Ok(Sent::Declined(_))));

        // A helper that hung up: its poll is dropped while it waits.
        polling(&channel, &[]).await.abort();
        let gone = channel.clone();
        assert!(crate::test_support::within(PATIENCE, move || gone.waiting_polls() == 0).await);
        // Inside the gap the message waits to be picked up, and is declined when it is not.
        let sent = sending(&channel, "late", "m1");
        assert!(matches!(sent.await.unwrap(), Ok(Sent::Declined(_))));
        channel.forget_last_poll();
        assert!(matches!(channel.send("later", "m2"), Ok(Sent::Declined(_))));

        // Nothing declined is left for a poll that comes after.
        let poll = polling(&channel, &[]).await;
        let sent = sending(&channel, "now", "m3");
        assert_eq!(poll.await.unwrap(), handed("m3", "now"));
        channel.poll(outcome("m3", Outcome::Taken, None)).await;
        assert_eq!(sent.await.unwrap().expect("sent"), Sent::Queued);
    }

    /// A message handed to a poll whose answer was lost is handed over again to the same
    /// helper's next poll, which does not list it as held; a poll that holds it is not handed it
    /// again.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_is_handed_over_again_until_the_helper_holds_it() {
        let channel = Arc::new(Channel::with_timing(timing()));
        let poll = polling(&channel, &[]).await;
        let sent = sending(&channel, "the text", "m1");
        assert_eq!(poll.await.unwrap(), handed("m1", "the text"));
        // The helper never read that answer.
        assert_eq!(
            channel.poll(waiting(&[])).await,
            handed("m1", "the text"),
            "handed over again"
        );
        // It holds it now: its next poll waits for another message.
        let poll = polling(&channel, &["m1"]).await;
        let second = sending(&channel, "the next", "m2");
        assert_eq!(poll.await.unwrap(), handed("m2", "the next"));
        for id in ["m1", "m2"] {
            channel.poll(outcome(id, Outcome::Taken, None)).await;
        }
        assert_eq!(sent.await.unwrap().expect("sent"), Sent::Queued);
        assert_eq!(second.await.unwrap().expect("sent"), Sent::Queued);
    }

    /// A message the agent gave back after taking it is reported to the session once; an id the
    /// agent never took, or one already given back, is reported to nobody.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_taken_message_given_back_is_reported_once() {
        let channel = Arc::new(Channel::with_timing(timing()));
        let given_back = Arc::new(Mutex::new(Vec::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        {
            let (given_back, calls) = (given_back.clone(), calls.clone());
            channel.on_returned(Box::new(move |id| {
                calls.fetch_add(1, Ordering::SeqCst);
                given_back.lock().unwrap().push(id.to_string());
            }));
        }
        let poll = polling(&channel, &[]).await;
        let sent = sending(&channel, "the text", "m1");
        poll.await.unwrap();
        channel.poll(outcome("m1", Outcome::Taken, None)).await;
        assert_eq!(sent.await.unwrap().expect("sent"), Sent::Queued);

        channel
            .poll(outcome("never-taken", Outcome::Returned, None))
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        channel.poll(outcome("m1", Outcome::Returned, None)).await;
        channel.poll(outcome("m1", Outcome::Returned, None)).await;
        assert_eq!(*given_back.lock().unwrap(), ["m1"]);
    }

    /// A message handed to one helper is never handed to another: the extension that started the
    /// first may have sent it, and the one that started the second (the extension reloaded) does
    /// not know. The second helper is handed only what no helper was handed yet, and the first
    /// one's own next poll is still handed its message again.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_handed_to_one_helper_is_never_handed_to_another() {
        let channel = Arc::new(Channel::with_timing(timing()));
        let poll = {
            let before = channel.waiting_polls();
            let (polled, seen) = (channel.clone(), channel.clone());
            let poll = tokio::spawn(async move { polled.poll(waiting_as("first", &[])).await });
            assert!(
                crate::test_support::within(PATIENCE, move || seen.waiting_polls() > before).await,
                "the poll never waited"
            );
            poll
        };
        let sent = sending(&channel, "the text", "m1");
        assert_eq!(poll.await.unwrap(), handed("m1", "the text"));

        // The helper that replaced it holds nothing, and is handed only the next message.
        let second = sending(&channel, "the next", "m2");
        assert_eq!(
            channel.poll(waiting_as("second", &[])).await,
            handed("m2", "the next")
        );
        // The first helper, had it only lost the answer, is handed its message again.
        assert_eq!(
            channel.poll(waiting_as("first", &[])).await,
            handed("m1", "the text")
        );
        for id in ["m1", "m2"] {
            channel.poll(outcome(id, Outcome::Taken, None)).await;
        }
        assert_eq!(sent.await.unwrap().expect("sent"), Sent::Queued);
        assert_eq!(second.await.unwrap().expect("sent"), Sent::Queued);
    }

    /// A message reported given back before its send has woken to answer that it was taken is
    /// not lost: the send answers that it was taken and then tells the session it was given
    /// back, once. Both outcomes come in one post, which the channel takes in under one lock, so
    /// the send cannot have woken between them.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_given_back_before_its_send_answered_is_reported_by_the_send() {
        let channel = Arc::new(Channel::with_timing(timing()));
        let given_back = Arc::new(Mutex::new(Vec::new()));
        {
            let given_back = given_back.clone();
            channel.on_returned(Box::new(move |id| {
                given_back.lock().unwrap().push(id.to_string());
            }));
        }
        let poll = polling(&channel, &[]).await;
        let sent = sending(&channel, "the text", "m1");
        poll.await.unwrap();
        channel
            .poll(outcomes(&[
                ("m1", Outcome::Taken, None),
                ("m1", Outcome::Returned, None),
            ]))
            .await;
        assert_eq!(sent.await.unwrap().expect("sent"), Sent::Queued);
        assert_eq!(*given_back.lock().unwrap(), ["m1"]);
        // It was given back once: the channel no longer holds it as taken.
        channel.poll(outcome("m1", Outcome::Returned, None)).await;
        assert_eq!(*given_back.lock().unwrap(), ["m1"]);
    }

    /// Once the session has ended, a waiting poll and every later one is answered that the
    /// channel is gone, and nothing is handed over.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_stopped_channel_ends_its_polls_and_declines() {
        let channel = Arc::new(Channel::with_timing(timing()));
        let poll = polling(&channel, &[]).await;
        channel.stop();
        assert_eq!(poll.await.unwrap(), None);
        assert_eq!(channel.poll(waiting(&[])).await, None);
        assert_eq!(channel.poll(PollRequest::default()).await, None);
        assert!(matches!(channel.send("late", "m1"), Ok(Sent::Declined(_))));
    }

    /// The route's body and answer on the wire: every field of a poll may be left out, its
    /// helper's instance included (as a helper that predates it leaves it out), an outcome
    /// this build does not know reads as one that says nothing, and a message is `id` and `text`.
    #[test]
    fn the_polls_wire_form() {
        let empty: PollRequest = serde_json::from_str("{}").unwrap();
        assert!(empty.outcomes.is_empty() && empty.holding.is_empty() && !empty.wait);
        assert!(empty.instance.is_empty());
        let request: PollRequest = serde_json::from_str(
            r#"{"outcomes":[{"id":"a","outcome":"taken"},{"id":"b","outcome":"refused","reason":"r"},
                {"id":"c","outcome":"returned"},{"id":"d","outcome":"later-kind","extra":1}],
                "holding":["e"],"wait":true,"instance":"i","later":0}"#,
        )
        .unwrap();
        assert_eq!(
            request
                .outcomes
                .iter()
                .map(|reported| reported.outcome)
                .collect::<Vec<_>>(),
            [
                Outcome::Taken,
                Outcome::Refused,
                Outcome::Returned,
                Outcome::Unknown
            ]
        );
        assert_eq!(request.outcomes[1].reason.as_deref(), Some("r"));
        assert_eq!(request.holding, ["e"]);
        assert_eq!(request.instance, "i");
        assert!(request.wait);
        assert_eq!(
            serde_json::to_value(handed("m1", "text").unwrap()).unwrap(),
            serde_json::json!({"messages": [{"id": "m1", "text": "text"}]})
        );
    }

    /// A launch's own server takes the channel's place, and a launch that declares no channel
    /// gets none.
    #[test]
    fn a_launch_gets_a_channel_only_where_it_declares_one_and_has_no_server() {
        let server: Arc<dyn MessageTransport> = Arc::new(Channel::new());
        let (transport, is_channel) = transport_of(Some(server.clone()), true);
        assert!(Arc::ptr_eq(&transport.unwrap(), &server) && !is_channel);
        let (transport, is_channel) = transport_of(None, true);
        assert!(transport.unwrap().channel().is_some() && is_channel);
        let (transport, is_channel) = transport_of(None, false);
        assert!(transport.is_none() && !is_channel);
    }
}
