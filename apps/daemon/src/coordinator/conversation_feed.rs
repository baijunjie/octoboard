//! Following a session's conversation from a control connection (`subscribe_session_conversation`,
//! `read_session_conversation`, `unsubscribe_session_conversation`). What a connection follows is
//! its own, so these requests are served by a worker per connection ([`Connection`]), in the order
//! they were sent, rather than by the dispatcher in `mod.rs`; the feeds themselves are held in
//! `AppState::conversation_feeds` and filled by a task per followed session ([`follow`]).

mod follow;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use anyhow::Result;
use futures_util::FutureExt;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::protocol::{CodedError, Event, RequestBody};
use crate::records::feed::Page;
use crate::state::{AppState, FeedChannel};

/// The most conversations one connection follows at once.
pub const SUBSCRIPTION_LIMIT: usize = 16;

/// Requests queued for a connection's worker before the connection stops reading more.
const QUEUE_CAPACITY: usize = 64;

/// The serving of one connection's conversation requests. Dropping it ends them, and with them
/// every subscription of the connection.
pub struct Connection {
    requests: mpsc::Sender<(Option<String>, RequestBody)>,
    outbound: mpsc::Sender<Event>,
    worker: JoinHandle<()>,
}

impl Connection {
    /// Starts serving; replies and events go to `outbound`.
    pub fn start(state: Arc<AppState>, outbound: mpsc::Sender<Event>) -> Self {
        let (requests, queue) = mpsc::channel(QUEUE_CAPACITY);
        let worker = tokio::spawn(serve(state, outbound.clone(), queue));
        Self {
            requests,
            outbound,
            worker,
        }
    }

    /// Queues a request, which must be one of the three conversation requests. A request the
    /// worker can no longer take is answered with an error all the same.
    pub async fn submit(&self, id: Option<String>, body: RequestBody) {
        if let Err(refused) = self.requests.send((id, body)).await {
            let failure =
                anyhow::anyhow!("the daemon is no longer serving this connection's conversations");
            let _ = self
                .outbound
                .send(Event::from_error(refused.0 .0, &failure))
                .await;
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

/// One followed conversation of a connection; dropping it unsubscribes.
struct Subscription {
    state: Arc<AppState>,
    channel: Arc<FeedChannel>,
    forwarder: Option<JoinHandle<()>>,
    /// Told apart from the connection's other subscriptions to the same session, before and
    /// after this one, by the number its forwarder reports its end with.
    generation: u64,
}

impl Subscription {
    /// Ends the subscription with nothing more sent for it: the forwarder is stopped and awaited,
    /// so that what the connection sends next (a reply that supersedes it) comes after anything it
    /// forwarded.
    async fn end(mut self) {
        if let Some(forwarder) = self.forwarder.take() {
            forwarder.abort();
            let _ = forwarder.await;
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(forwarder) = &self.forwarder {
            forwarder.abort();
        }
        self.state.conversation_feeds.leave(&self.channel);
    }
}

async fn serve(
    state: Arc<AppState>,
    outbound: mpsc::Sender<Event>,
    mut queue: mpsc::Receiver<(Option<String>, RequestBody)>,
) {
    let mut followed: HashMap<String, Subscription> = HashMap::new();
    // The subscriptions whose feed ended, by session and generation: they are dropped, which
    // frees the slot.
    let (ended_tx, mut ended) = mpsc::unbounded_channel::<(String, u64)>();
    let mut generations = 0;
    loop {
        // A notice already queued is handled before the queued requests, so a request the client
        // sent after being told of the end finds the subscription gone. A notice that is not
        // queued yet when a request is taken is handled after it.
        let (id, body) = tokio::select! {
            biased;
            Some((session, generation)) = ended.recv() => {
                if followed
                    .get(&session)
                    .is_some_and(|held| held.generation == generation)
                {
                    followed.remove(&session);
                    let told = outbound
                        .send(Event::SessionConversationEnded { session })
                        .await;
                    if told.is_err() {
                        return;
                    }
                }
                continue;
            }
            request = queue.recv() => match request {
                Some(request) => request,
                None => return,
            },
        };
        let handling = handle(&state, &mut followed, id.clone(), body);
        let handled = match AssertUnwindSafe(handling).catch_unwind().await {
            Ok(Ok(handled)) => handled,
            Ok(Err(err)) => (Event::from_error(id, &err), None),
            Err(_) => {
                tracing::error!(request = ?id, "a conversation request handler panicked");
                let failure = anyhow::anyhow!("the daemon failed while handling the request");
                (Event::from_error(id, &failure), None)
            }
        };
        let (reply, started) = handled;
        // The reply goes first: the events that follow it are the changes after what it holds.
        if outbound.send(reply).await.is_err() {
            return;
        }
        if let Some((session, mut subscription, events)) = started {
            generations += 1;
            subscription.generation = generations;
            subscription.forwarder = Some(tokio::spawn(forward(
                Arc::clone(&subscription.channel),
                events,
                outbound.clone(),
                ended_tx.clone(),
                generations,
            )));
            followed.insert(session, subscription);
        }
    }
}

/// What a subscribing request started: the session, its subscription and the events to forward.
type Started = (String, Subscription, broadcast::Receiver<Event>);

async fn handle(
    state: &Arc<AppState>,
    followed: &mut HashMap<String, Subscription>,
    id: Option<String>,
    body: RequestBody,
) -> Result<(Event, Option<Started>)> {
    match body {
        RequestBody::SubscribeSessionConversation { session, limit } => {
            state.session_record(&session)?;
            if !followed.contains_key(&session) && followed.len() >= SUBSCRIPTION_LIMIT {
                return Err(CodedError::conversation_subscriptions_exceeded(
                    &session,
                    SUBSCRIPTION_LIMIT,
                ));
            }
            let (channel, started) = state.conversation_feeds.join(&session);
            // From here the subscription is dropped, and so left, wherever this ends.
            let subscription = Subscription {
                state: Arc::clone(state),
                channel: Arc::clone(&channel),
                forwarder: None,
                generation: 0,
            };
            if started {
                follow::start(state, Arc::clone(&channel));
            }
            let subscribed = if channel.wait_ready().await {
                channel.subscribe(Page::size(limit))
            } else {
                None
            };
            let Some((snapshot, events)) = subscribed else {
                // The feed stopped before or since it was ready: the session may be gone by now,
                // else it cannot be followed.
                state.session_record(&session)?;
                anyhow::bail!("the conversation of session {session} cannot be followed now");
            };
            // A repeat replaces the earlier subscription, joined before it is dropped; what that
            // one forwarded comes before the reply.
            if let Some(earlier) = followed.remove(&session) {
                earlier.end().await;
            }
            let reply = Event::SessionConversationSubscribed {
                id,
                session: session.clone(),
                epoch: snapshot.epoch,
                entries: snapshot.page.entries,
                has_earlier: snapshot.page.has_earlier,
                pending: snapshot.pending,
            };
            Ok((reply, Some((session, subscription, events))))
        }
        RequestBody::UnsubscribeSessionConversation { session } => {
            if let Some(subscription) = followed.remove(&session) {
                subscription.end().await;
            }
            Ok((Event::Ack { id }, None))
        }
        RequestBody::ReadSessionConversation {
            session,
            epoch,
            before,
            limit,
        } => {
            state.session_record(&session)?;
            let subscription = followed
                .get(&session)
                .ok_or_else(|| CodedError::conversation_not_followed(&session))?;
            let (epoch, page) = subscription
                .channel
                .read(epoch, before, Page::size(limit))
                .ok_or_else(|| CodedError::conversation_not_followed(&session))?;
            let reply = Event::SessionConversation {
                id,
                session,
                epoch,
                entries: page.entries,
                has_earlier: page.has_earlier,
            };
            Ok((reply, None))
        }
        _ => anyhow::bail!("not a conversation request"),
    }
}

/// Sends a connection the events of a feed. One that fell behind the feed is sent the
/// conversation whole instead of what it missed. When the feed ends (its session was deleted, or
/// the daemon failed while reading it), or this panics, the worker is told through `ended`, with
/// the subscription's `generation`: it drops that subscription and sends
/// `session_conversation_ended`.
async fn forward(
    channel: Arc<FeedChannel>,
    events: broadcast::Receiver<Event>,
    outbound: mpsc::Sender<Event>,
    ended: mpsc::UnboundedSender<(String, u64)>,
    generation: u64,
) {
    let forwarding = AssertUnwindSafe(forward_events(&channel, events, &outbound));
    match forwarding.catch_unwind().await {
        // The connection's queue is closed: it is going, and needs no word.
        Ok(false) => {}
        Ok(true) => {
            let _ = ended.send((channel.session.clone(), generation));
        }
        Err(_) => {
            tracing::error!(session = %channel.session, "a conversation forwarder panicked");
            let _ = ended.send((channel.session.clone(), generation));
        }
    }
}

/// Forwards until the feed ends (`true`) or the connection's queue closes (`false`).
async fn forward_events(
    channel: &FeedChannel,
    mut events: broadcast::Receiver<Event>,
    outbound: &mpsc::Sender<Event>,
) -> bool {
    loop {
        let event = tokio::select! {
            biased;
            () = channel.ended() => return true,
            event = events.recv() => match event {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // Listening again together with taking the conversation: what changes
                    // meanwhile is sent after it, and an entry sent twice is told by its `seq`.
                    let Some((replaced, listening)) = channel.resubscribe() else {
                        return true;
                    };
                    events = listening;
                    replaced
                }
                Err(broadcast::error::RecvError::Closed) => return true,
            },
        };
        if outbound.send(event).await.is_err() {
            return false;
        }
    }
}
