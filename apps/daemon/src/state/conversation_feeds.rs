//! The conversations clients follow (`subscribe_session_conversation`): per followed session, the
//! entries as the record reads so far, the epoch they belong to and the events that tell the
//! connections following it what changed. Derived from the agent's record and held in memory
//! only while a connection follows the session: the last one leaving, or the session's deletion,
//! drops it. What reads the record is the following task (`coordinator::conversation_feed`); this
//! is what it fills and the connections read.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::{broadcast, watch};

use crate::protocol::{Event, PendingEntry, CONVERSATION_PAGE_BYTES, CONVERSATION_PAGE_DEFAULT};
use crate::records::feed::{Changes, FeedOut, Page};

/// Events a connection may lag behind by before it is sent the conversation whole again.
const EVENT_CAPACITY: usize = 256;

/// Every followed conversation, by session id.
#[derive(Default)]
pub struct ConversationFeeds {
    epochs: Arc<AtomicU64>,
    followed: Mutex<HashMap<String, Followed>>,
}

struct Followed {
    channel: Arc<FeedChannel>,
    subscribers: usize,
}

impl ConversationFeeds {
    /// Adds a subscriber to the session's feed, which is made, and answered as `started`, for the
    /// first one: its caller then starts what fills it.
    pub fn join(&self, session: &str) -> (Arc<FeedChannel>, bool) {
        let mut followed = self
            .followed
            .lock()
            .expect("conversation feed lock poisoned");
        if let Some(entry) = followed.get_mut(session) {
            entry.subscribers += 1;
            return (Arc::clone(&entry.channel), false);
        }
        let channel = Arc::new(FeedChannel::new(session, Arc::clone(&self.epochs)));
        followed.insert(
            session.to_owned(),
            Followed {
                channel: Arc::clone(&channel),
                subscribers: 1,
            },
        );
        (channel, true)
    }

    /// Takes a subscriber away; the last one's leaving drops the feed and stops its task.
    pub fn leave(&self, channel: &Arc<FeedChannel>) {
        let mut followed = self
            .followed
            .lock()
            .expect("conversation feed lock poisoned");
        let Some(entry) = followed.get_mut(&channel.session) else {
            return;
        };
        if !Arc::ptr_eq(&entry.channel, channel) {
            return;
        }
        entry.subscribers -= 1;
        if entry.subscribers == 0 {
            followed.remove(&channel.session);
            channel.stop();
        }
    }

    /// Drops the feed of `session` whatever follows it: the session is gone.
    pub fn forget(&self, session: &str) {
        let removed = self
            .followed
            .lock()
            .expect("conversation feed lock poisoned")
            .remove(session);
        if let Some(entry) = removed {
            entry.channel.stop();
        }
    }

    /// Drops `channel` when its task ends by itself, so the next subscriber starts a new one.
    pub fn finish(&self, channel: &Arc<FeedChannel>) {
        let mut followed = self
            .followed
            .lock()
            .expect("conversation feed lock poisoned");
        if followed
            .get(&channel.session)
            .is_some_and(|entry| Arc::ptr_eq(&entry.channel, channel))
        {
            followed.remove(&channel.session);
        }
        drop(followed);
        channel.stop();
    }

    /// How many sessions are followed.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.followed
            .lock()
            .expect("conversation feed lock poisoned")
            .len()
    }
}

/// One session's feed.
pub struct FeedChannel {
    pub session: String,
    state: Mutex<FeedState>,
    epochs: Arc<AtomicU64>,
    events: broadcast::Sender<Event>,
    ready: watch::Sender<bool>,
    stopped: watch::Sender<bool>,
}

struct FeedState {
    epoch: u64,
    out: FeedOut,
}

/// A provider's reader panicked while its lines were applied to a feed.
#[derive(Debug)]
pub struct ReaderPanicked;

/// The conversation as a subscriber starts with it.
pub struct Snapshot {
    pub epoch: u64,
    pub page: Page,
    pub pending: Vec<PendingEntry>,
}

impl FeedChannel {
    fn new(session: &str, epochs: Arc<AtomicU64>) -> Self {
        Self {
            session: session.to_owned(),
            state: Mutex::new(FeedState {
                epoch: 0,
                out: FeedOut::default(),
            }),
            epochs,
            events: broadcast::channel(EVENT_CAPACITY).0,
            ready: watch::channel(false).0,
            stopped: watch::channel(false).0,
        }
    }

    /// Stops what fills the feed, wakes whoever waits for it to be ready and tells the connections
    /// that follow it it has ended ([`Self::ended`]).
    pub fn stop(&self) {
        self.stopped.send_replace(true);
        self.ready.send_replace(true);
    }

    pub fn is_stopped(&self) -> bool {
        *self.stopped.borrow()
    }

    /// Resolves once the feed has stopped: a connection that follows it is then to drop its
    /// subscription, since nothing will be sent to it any more.
    pub async fn ended(&self) {
        let _ = self.stopped.subscribe().wait_for(|stopped| *stopped).await;
    }

    /// Waits until the conversation was read once; `false` when the feed stopped before.
    pub async fn wait_ready(&self) -> bool {
        let mut ready = self.ready.subscribe();
        let _ = ready.wait_for(|ready| *ready).await;
        !self.is_stopped()
    }

    /// The latest page of at most `limit` entries, and a receiver of everything that changes after
    /// it: taken together, so that nothing is lost between them or repeated.
    /// `None` for a feed that has stopped, which nobody is to be given any more (a reader's panic
    /// leaves half-applied entries in it).
    pub fn subscribe(&self, limit: usize) -> Option<(Snapshot, broadcast::Receiver<Event>)> {
        let state = self.lock();
        if self.is_stopped() {
            return None;
        }
        let snapshot = Snapshot {
            epoch: state.epoch,
            page: state.out.page(None, limit),
            pending: state.out.pending().to_vec(),
        };
        Some((snapshot, self.events.subscribe()))
    }

    /// The conversation whole, for a connection that missed events, and a receiver of what
    /// changes after it, taken together as [`Self::subscribe`] does; `None` for a feed that has
    /// stopped, as there.
    pub fn resubscribe(&self) -> Option<(Event, broadcast::Receiver<Event>)> {
        let state = self.lock();
        if self.is_stopped() {
            return None;
        }
        Some((self.replaced_event(&state), self.events.subscribe()))
    }

    /// The page before the entry `before` of the conversation as it stands at `epoch`; an empty
    /// one, with the current epoch, when the client's entries are of an earlier epoch.
    /// `None` for a feed that has stopped, as for [`Self::subscribe`].
    pub fn read(&self, epoch: u64, before: u64, limit: usize) -> Option<(u64, Page)> {
        let state = self.lock();
        if self.is_stopped() {
            return None;
        }
        if epoch != state.epoch {
            return Some((state.epoch, Page::default()));
        }
        Some((state.epoch, state.out.page(Some(before), limit)))
    }

    /// Lets `read` add lines to the entries and sends what that changed, in events of at most
    /// [`CONVERSATION_PAGE_BYTES`] of entries each. Entries a reader dropped that clients hold
    /// replace the conversation, under a new epoch. A `read` that panics (a provider's reader) is
    /// caught here, inside the lock a panic through it would poison: nothing of what it
    /// half-applied is sent, and the feed is stopped, so that nobody goes on to read it.
    pub fn apply(&self, read: impl FnOnce(&mut FeedOut)) -> Result<(), ReaderPanicked> {
        let mut state = self.lock();
        if std::panic::catch_unwind(AssertUnwindSafe(|| read(&mut state.out))).is_err() {
            self.stop();
            return Err(ReaderPanicked);
        }
        let changes = state.out.take_changes();
        if changes.replaced {
            state.epoch = self.epochs.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = self.events.send(self.replaced_event(&state));
        } else {
            for event in changed_events(&self.session, state.epoch, changes) {
                let _ = self.events.send(event);
            }
        }
        Ok(())
    }

    /// Puts the entries of a conversation read afresh in place of the held ones and sends it to
    /// the subscribers as the conversation whole, under a new epoch; the first one makes the feed
    /// ready. When the entries are the ones held, the epoch stays and only what the agent is still
    /// producing is sent, if that changed.
    pub fn replace_with(&self, mut out: FeedOut) {
        out.take_changes();
        {
            let mut state = self.lock();
            if state.epoch != 0 && state.out.entries() == out.entries() {
                let pending_changed = state.out.pending() != out.pending();
                state.out = out;
                if pending_changed {
                    let _ = self.events.send(Event::SessionConversationChanged {
                        session: self.session.clone(),
                        epoch: state.epoch,
                        appended: Vec::new(),
                        updated: Vec::new(),
                        pending: Some(state.out.pending().to_vec()),
                    });
                }
            } else {
                state.epoch = self.epochs.fetch_add(1, Ordering::SeqCst) + 1;
                state.out = out;
                let _ = self.events.send(self.replaced_event(&state));
            }
        }
        self.ready.send_replace(true);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FeedState> {
        // Nothing that can panic runs under it but the reader, whose panic is caught there.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn replaced_event(&self, state: &FeedState) -> Event {
        let page = state.out.page(None, CONVERSATION_PAGE_DEFAULT);
        Event::SessionConversationReplaced {
            session: self.session.clone(),
            epoch: state.epoch,
            entries: page.entries,
            has_earlier: page.has_earlier,
            pending: state.out.pending().to_vec(),
        }
    }
}

/// What changed as events, in order (the updated entries, then the appended ones), each event
/// holding at most [`CONVERSATION_PAGE_BYTES`] of entries serialized, one entry at least, and
/// what is pending with the last of them (or alone, when no entry changed).
fn changed_events(session: &str, epoch: u64, changes: Changes) -> Vec<Event> {
    let Changes {
        appended,
        updated,
        pending,
        ..
    } = changes;
    let event = |appended, updated, pending| Event::SessionConversationChanged {
        session: session.to_owned(),
        epoch,
        appended,
        updated,
        pending,
    };
    let mut events = Vec::new();
    let (mut run_appended, mut run_updated, mut bytes) = (Vec::new(), Vec::new(), 0);
    let entries = updated
        .into_iter()
        .map(|entry| (entry, true))
        .chain(appended.into_iter().map(|entry| (entry, false)));
    for (entry, is_update) in entries {
        let size = serde_json::to_vec(&entry).map_or(0, |json| json.len());
        if bytes > 0 && bytes + size > CONVERSATION_PAGE_BYTES {
            events.push(event(
                std::mem::take(&mut run_appended),
                std::mem::take(&mut run_updated),
                None,
            ));
            bytes = 0;
        }
        bytes += size;
        if is_update {
            run_updated.push(entry);
        } else {
            run_appended.push(entry);
        }
    }
    if !run_appended.is_empty() || !run_updated.is_empty() || pending.is_some() {
        events.push(event(run_appended, run_updated, pending));
    }
    events
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast::error::TryRecvError;

    use super::*;
    use crate::protocol::ConversationBody;
    use crate::records::feed::Pending;

    fn feeds() -> ConversationFeeds {
        ConversationFeeds::default()
    }

    fn texts(entries: &[crate::protocol::ConversationEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match &entry.body {
                ConversationBody::User { text } | ConversationBody::Assistant { text } => {
                    text.clone()
                }
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// A feed with the entries `lines` (each a user message), made ready.
    fn ready(feeds: &ConversationFeeds, session: &str, lines: &[&str]) -> Arc<FeedChannel> {
        let (channel, _) = feeds.join(session);
        let mut out = FeedOut::default();
        for line in lines {
            out.user(line, None);
        }
        channel.replace_with(out);
        channel
    }

    #[test]
    fn the_first_subscriber_starts_the_feed_and_the_last_one_leaving_drops_and_stops_it() {
        let feeds = feeds();
        let (first, started) = feeds.join("s");
        assert!(started);
        let (second, started) = feeds.join("s");
        assert!(!started && Arc::ptr_eq(&first, &second));
        assert_eq!(feeds.len(), 1);

        feeds.leave(&first);
        assert!(!first.is_stopped() && feeds.len() == 1);
        feeds.leave(&second);
        assert!(first.is_stopped() && feeds.len() == 0);

        // The next subscriber starts a feed of its own.
        let (next, started) = feeds.join("s");
        assert!(started && !Arc::ptr_eq(&first, &next));
        // A leave of the dropped feed does not take a subscriber off the new one.
        feeds.leave(&first);
        assert!(!next.is_stopped() && feeds.len() == 1);
    }

    #[test]
    fn feeds_of_other_sessions_are_independent() {
        let feeds = feeds();
        let (a, _) = feeds.join("a");
        let (b, started) = feeds.join("b");
        assert!(started && feeds.len() == 2);
        feeds.leave(&a);
        assert!(a.is_stopped() && !b.is_stopped() && feeds.len() == 1);
    }

    #[test]
    fn forgetting_a_session_stops_its_feed_whoever_follows_it() {
        let feeds = feeds();
        let (channel, _) = feeds.join("s");
        feeds.join("s");
        let (other, _) = feeds.join("other");
        feeds.forget("s");
        assert!(channel.is_stopped() && feeds.len() == 1 && !other.is_stopped());
        feeds.forget("never followed");
        assert_eq!(feeds.len(), 1);
    }

    /// A task that ends by itself drops its own feed only: one the next subscriber started since
    /// is not touched.
    #[test]
    fn a_finished_feed_is_dropped_so_the_next_subscriber_starts_another() {
        let feeds = feeds();
        let (old, _) = feeds.join("s");
        feeds.finish(&old);
        assert!(old.is_stopped() && feeds.len() == 0);
        let (new, started) = feeds.join("s");
        assert!(started);
        feeds.finish(&old);
        assert!(!new.is_stopped() && feeds.len() == 1);
    }

    #[tokio::test]
    async fn a_subscriber_waits_until_the_conversation_was_read_once() {
        let feeds = feeds();
        let (channel, _) = feeds.join("s");
        let waiting = tokio::spawn({
            let channel = Arc::clone(&channel);
            async move { channel.wait_ready().await }
        });
        channel.replace_with(FeedOut::default());
        assert!(waiting.await.unwrap());
        assert!(channel.wait_ready().await);
    }

    #[tokio::test]
    async fn a_feed_stopped_before_it_was_ready_lets_the_waiter_go_with_false() {
        let feeds = feeds();
        let (channel, _) = feeds.join("s");
        let waiting = tokio::spawn({
            let channel = Arc::clone(&channel);
            async move { channel.wait_ready().await }
        });
        feeds.forget("s");
        assert!(!waiting.await.unwrap());
    }

    #[test]
    fn a_replacement_gives_a_new_epoch_and_tells_the_subscribers_the_whole_conversation() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &["one"]);
        let (snapshot, mut events) = channel.subscribe(50).expect("a live feed");
        let first = snapshot.epoch;
        assert!(first > 0);

        let mut out = FeedOut::default();
        out.user("two", None);
        out.assistant("answer", None);
        out.set_pending(vec![Pending::Assistant("typing".to_owned())]);
        channel.replace_with(out);
        let Ok(Event::SessionConversationReplaced {
            session,
            epoch,
            entries,
            has_earlier,
            pending,
        }) = events.try_recv()
        else {
            panic!("a replacement");
        };
        assert_eq!(session, "s");
        assert!(epoch > first);
        assert_eq!(texts(&entries), ["two", "answer"]);
        assert!(!has_earlier);
        assert_eq!(pending.len(), 1);
        // What a new subscriber is given is the same conversation under the same epoch.
        let (snapshot, _) = channel.subscribe(50).expect("a live feed");
        assert_eq!(snapshot.epoch, epoch);
        assert_eq!(texts(&snapshot.page.entries), ["two", "answer"]);
        assert_eq!(snapshot.pending.len(), 1);
    }

    #[test]
    fn epochs_never_repeat_across_feeds() {
        let feeds = feeds();
        let a = ready(&feeds, "a", &[]);
        let b = ready(&feeds, "b", &[]);
        let epochs = [
            a.subscribe(1).expect("a live feed").0.epoch,
            b.subscribe(1).expect("a live feed").0.epoch,
        ];
        assert_ne!(epochs[0], epochs[1]);
        assert!(epochs.iter().all(|epoch| *epoch > 0));
    }

    /// What is added reaches the subscribers as `appended`, a result as an update of the entry,
    /// pending entries only when they changed, and nothing at all when nothing did.
    #[test]
    fn changes_reach_the_subscribers_and_no_change_sends_nothing() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &["one"]);
        let (snapshot, mut events) = channel.subscribe(50).expect("a live feed");
        channel.apply(|_| {}).expect("applied");
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));

        channel
            .apply(|out| out.assistant("two", None))
            .expect("applied");
        let Ok(Event::SessionConversationChanged {
            epoch,
            appended,
            updated,
            pending,
            ..
        }) = events.try_recv()
        else {
            panic!("a change");
        };
        assert_eq!(epoch, snapshot.epoch);
        assert_eq!((appended.len(), appended[0].seq), (1, 1));
        assert!(updated.is_empty() && pending.is_none());

        channel
            .apply(|out| out.set_pending(vec![Pending::Assistant("x".to_owned())]))
            .expect("applied");
        let Ok(Event::SessionConversationChanged { pending, .. }) = events.try_recv() else {
            panic!("a change");
        };
        assert_eq!(pending.map(|pending| pending.len()), Some(1));
        channel
            .apply(|out| out.set_pending(Vec::new()))
            .expect("applied");
        let Ok(Event::SessionConversationChanged { pending, .. }) = events.try_recv() else {
            panic!("a change");
        };
        assert_eq!(pending, Some(Vec::new()));
    }

    /// A reader that drops entries a subscriber was given makes a replacement under a new epoch
    /// in place of a change; dropping ones nobody had is no event.
    #[test]
    fn dropping_entries_that_were_sent_replaces_the_conversation() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &["one", "two", "three"]);
        let (snapshot, mut events) = channel.subscribe(50).expect("a live feed");
        channel
            .apply(|out| {
                out.user("four", None);
                out.truncate(3);
            })
            .expect("applied");
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));

        channel.apply(|out| out.truncate(1)).expect("applied");
        let Ok(Event::SessionConversationReplaced { epoch, entries, .. }) = events.try_recv()
        else {
            panic!("a replacement");
        };
        assert!(epoch > snapshot.epoch);
        assert_eq!(texts(&entries), ["one"]);
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
        // The entries after it are appended from where the conversation stands.
        channel
            .apply(|out| out.assistant("again", None))
            .expect("applied");
        let Ok(Event::SessionConversationChanged {
            epoch: same,
            appended,
            ..
        }) = events.try_recv()
        else {
            panic!("a change");
        };
        assert_eq!((same, appended[0].seq), (epoch, 1));
    }

    /// The page and the receiver are taken together: what changed before is in the page, what
    /// changes after comes as an event, and nothing is in both.
    #[test]
    fn a_subscription_loses_and_repeats_nothing_between_the_page_and_the_events() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &["one"]);
        channel
            .apply(|out| out.assistant("before", None))
            .expect("applied");
        let (snapshot, mut events) = channel.subscribe(50).expect("a live feed");
        channel
            .apply(|out| out.assistant("after", None))
            .expect("applied");
        assert_eq!(texts(&snapshot.page.entries), ["one", "before"]);
        let Ok(Event::SessionConversationChanged { appended, .. }) = events.try_recv() else {
            panic!("a change");
        };
        assert_eq!(texts(&appended), ["after"]);
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn a_page_of_an_earlier_epoch_is_empty_and_names_the_current_one() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &["a", "b", "c", "d"]);
        let (snapshot, _) = channel.subscribe(2).expect("a live feed");
        assert_eq!(texts(&snapshot.page.entries), ["c", "d"]);
        let (epoch, page) = channel.read(snapshot.epoch, 2, 10).expect("a live feed");
        assert_eq!(
            (epoch, texts(&page.entries), page.has_earlier),
            (snapshot.epoch, vec!["a".to_owned(), "b".to_owned()], false)
        );
        let (epoch, page) = channel
            .read(snapshot.epoch - 1, 2, 10)
            .expect("a live feed");
        assert_eq!(epoch, snapshot.epoch);
        assert!(page.entries.is_empty() && !page.has_earlier);
        let (epoch, page) = channel
            .read(snapshot.epoch + 1, 2, 10)
            .expect("a live feed");
        assert_eq!(epoch, snapshot.epoch);
        assert!(page.entries.is_empty());
    }

    /// A connection that lagged is given the conversation whole as it stands, pending included,
    /// together with a receiver of what changes after it.
    #[test]
    fn the_replacement_for_a_lagging_connection_is_the_conversation_as_it_stands() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &["one"]);
        channel
            .apply(|out| {
                out.assistant("two", None);
                out.set_pending(vec![Pending::Assistant("typing".to_owned())]);
            })
            .expect("applied");
        let (replacement, mut listening) = channel.resubscribe().expect("a live feed");
        let Event::SessionConversationReplaced {
            epoch,
            entries,
            pending,
            ..
        } = replacement
        else {
            panic!("a replacement");
        };
        assert_eq!(epoch, channel.subscribe(1).expect("a live feed").0.epoch);
        assert_eq!(texts(&entries), ["one", "two"]);
        assert_eq!(pending.len(), 1);
        // What changes after it reaches the receiver it came with, and nothing before does.
        assert!(matches!(listening.try_recv(), Err(TryRecvError::Empty)));
        channel
            .apply(|out| out.assistant("three", None))
            .expect("applied");
        let Ok(Event::SessionConversationChanged { appended, .. }) = listening.try_recv() else {
            panic!("a change");
        };
        assert_eq!(texts(&appended), ["three"]);
    }

    /// A conversation read again that holds the entries clients hold changes nothing for them
    /// but what the agent is still producing.
    #[test]
    fn a_rebuild_with_the_entries_held_keeps_the_epoch_and_sends_only_the_pending_change() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &["one", "two"]);
        let (snapshot, mut events) = channel.subscribe(50).expect("a live feed");
        let same = || {
            let mut out = FeedOut::default();
            out.user("one", None);
            out.user("two", None);
            out
        };
        channel.replace_with(same());
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
        let mut typing = same();
        typing.set_pending(vec![Pending::Assistant("typing".to_owned())]);
        channel.replace_with(typing);
        let Ok(Event::SessionConversationChanged {
            epoch,
            appended,
            updated,
            pending,
            ..
        }) = events.try_recv()
        else {
            panic!("a pending change");
        };
        assert_eq!(epoch, snapshot.epoch);
        assert!(appended.is_empty() && updated.is_empty());
        assert_eq!(pending.map(|pending| pending.len()), Some(1));
        assert_eq!(
            channel.subscribe(50).expect("a live feed").0.epoch,
            snapshot.epoch
        );

        // Another conversation replaces it under a new epoch.
        let mut other = FeedOut::default();
        other.user("three", None);
        channel.replace_with(other);
        let Ok(Event::SessionConversationReplaced { epoch, .. }) = events.try_recv() else {
            panic!("a replacement");
        };
        assert!(epoch > snapshot.epoch);
    }

    /// What one pass changed goes out in events that each hold at most a page's bytes of
    /// entries, in order, with what is pending in the last.
    #[test]
    fn changes_larger_than_a_page_are_split_into_events() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &[]);
        let (_, mut events) = channel.subscribe(50).expect("a live feed");
        let size = CONVERSATION_PAGE_BYTES / 3;
        channel
            .apply(|out| {
                for _ in 0..5 {
                    out.assistant(&"x".repeat(size), None);
                }
                out.set_pending(vec![Pending::Assistant("typing".to_owned())]);
            })
            .expect("applied");
        let mut seqs = Vec::new();
        let mut pending = Vec::new();
        while let Ok(event) = events.try_recv() {
            let Event::SessionConversationChanged {
                appended,
                pending: now,
                ..
            } = event
            else {
                panic!("a change");
            };
            assert!(serde_json::to_vec(&appended).unwrap().len() <= CONVERSATION_PAGE_BYTES);
            seqs.extend(appended.iter().map(|entry| entry.seq));
            pending.push(now.is_some());
        }
        assert_eq!(seqs, [0, 1, 2, 3, 4]);
        assert!(pending.len() >= 2, "{pending:?}");
        assert_eq!(pending.iter().filter(|set| **set).count(), 1);
        assert_eq!(pending.last(), Some(&true));
    }

    /// Entries the clients hold that changed go out before the new ones, in events of a page's
    /// bytes each, and the pending entries ride the last event only.
    #[test]
    fn updated_and_appended_entries_are_split_in_order_with_the_pending_in_the_last_event() {
        use crate::protocol::{ToolCategory, ToolStatus};
        use crate::records::feed::{ToolCall, ToolOutcome};

        let call = |id: &str| ToolCall {
            call_id: id.to_owned(),
            name: "Bash".to_owned(),
            category: ToolCategory::Command,
            title: None,
            input: serde_json::json!({ "command": "x".repeat(20_000) }),
            paths: Vec::new(),
            status: ToolStatus::Running,
            output: None,
            exit_code: None,
        };
        let feeds = feeds();
        let (channel, _) = feeds.join("s");
        let mut out = FeedOut::default();
        let calls = 80;
        for n in 0..calls {
            out.tool_call(call(&format!("c{n}")), None);
        }
        channel.replace_with(out);
        let (_, mut events) = channel.subscribe(50).expect("a live feed");
        channel
            .apply(|out| {
                for n in 0..calls {
                    out.tool_result(
                        &format!("c{n}"),
                        ToolOutcome {
                            status: ToolStatus::Completed,
                            output: Some("ok".to_owned()),
                            exit_code: Some(0),
                        },
                    );
                }
                out.assistant("tail", None);
                out.set_pending(vec![Pending::Assistant("typing".to_owned())]);
            })
            .expect("applied");
        let mut updated_seqs = Vec::new();
        let mut appended_texts = Vec::new();
        let mut pending_in = Vec::new();
        let mut count = 0;
        while let Ok(event) = events.try_recv() {
            let Event::SessionConversationChanged {
                appended,
                updated,
                pending,
                ..
            } = event
            else {
                panic!("a change");
            };
            assert!(
                serde_json::to_vec(&updated).unwrap().len()
                    + serde_json::to_vec(&appended).unwrap().len()
                    <= CONVERSATION_PAGE_BYTES
            );
            updated_seqs.extend(updated.iter().map(|entry| entry.seq));
            appended_texts.extend(texts(&appended));
            pending_in.push(pending.is_some());
            count += 1;
        }
        assert!(count >= 2);
        assert_eq!(updated_seqs, (0..calls as u64).collect::<Vec<_>>());
        assert_eq!(appended_texts, ["tail"]);
        assert_eq!(pending_in.iter().filter(|set| **set).count(), 1);
        assert_eq!(pending_in.last(), Some(&true));
    }

    /// An entry that alone passes the limit still goes out, whole.
    #[test]
    fn an_entry_larger_than_a_page_is_sent_alone() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &[]);
        let (_, mut events) = channel.subscribe(50).expect("a live feed");
        channel
            .apply(|out| out.assistant(&"x".repeat(CONVERSATION_PAGE_BYTES * 2), None))
            .expect("applied");
        assert!(matches!(
            events.try_recv(),
            Ok(Event::SessionConversationChanged { appended, .. }) if appended.len() == 1
        ));
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn a_stopped_feed_resolves_ended_for_the_connections_following_it() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &["one"]);
        let waiting = tokio::spawn({
            let channel = Arc::clone(&channel);
            async move { channel.ended().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        feeds.forget("s");
        tokio::time::timeout(crate::test_support::PATIENCE, waiting)
            .await
            .expect("ended in time")
            .unwrap();
    }

    /// A reader that panics half-way sends nothing of what it applied, stops the feed, and leaves
    /// the lock usable.
    #[test]
    fn a_reader_that_panics_is_caught_and_broadcasts_nothing() {
        let feeds = feeds();
        let channel = ready(&feeds, "s", &["one"]);
        let (snapshot, mut events) = channel.subscribe(50).expect("a live feed");
        let applied = channel.apply(|out| {
            out.assistant("half", None);
            panic!("a reader failed");
        });
        assert!(applied.is_err());
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
        assert!(channel.is_stopped());
        // Nobody is served the half-applied entry, and the lock is not poisoned.
        assert!(channel.subscribe(50).is_none());
        assert!(channel.resubscribe().is_none());
        assert!(channel.read(snapshot.epoch, u64::MAX, 50).is_none());
    }
}
