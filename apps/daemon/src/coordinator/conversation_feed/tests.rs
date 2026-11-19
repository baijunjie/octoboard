//! The conversation requests of a connection, over the test provider's records
//! (`adapter::test_provider`): which entries and pending ones its reader and companion declare is
//! all the generic feed knows of them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::mpsc;

use super::*;
use crate::protocol::{error_code, AgentId, ConversationBody, ConversationEntry, SessionStatus};
use crate::test_support::{
    console_with_project_and_accounts, project_session, ScratchDir, PATIENCE,
};

const AGENT_ID: &str = "0b0e3a2c-6a52-4a49-8b64-0f3c1a1d9b11";

struct Client {
    state: Arc<AppState>,
    connection: Connection,
    events: mpsc::Receiver<Event>,
    dir: ScratchDir,
}

impl Client {
    fn new() -> Self {
        let (state, dir) = console_with_project_and_accounts("conversation-feed");
        let (outbound, events) = mpsc::channel(256);
        let connection = Connection::start(state.clone(), outbound);
        Self {
            state,
            connection,
            events,
            dir,
        }
    }

    /// A session of the test agent whose record is `<AGENT_ID>.jsonl` under its own home.
    fn session(&self, id: &str, agent_session_id: Option<&str>) {
        let mut session = project_session(id, AgentId::TEST, SessionStatus::Idle);
        session.agent_session_id = agent_session_id.map(str::to_owned);
        session.config_dir = Some(self.dir.join("home").to_string_lossy().into_owned());
        self.state.store.insert_session(&session).unwrap();
    }

    fn record(&self) -> PathBuf {
        self.record_of(AGENT_ID)
    }

    fn record_of(&self, agent_session_id: &str) -> PathBuf {
        self.dir
            .join("home")
            .join("sessions")
            .join(format!("{agent_session_id}.jsonl"))
    }

    fn write(&self, lines: &[&str]) {
        write_lines(&self.record(), lines, false);
    }

    fn append(&self, lines: &[&str]) {
        write_lines(&self.record(), lines, true);
    }

    /// Puts a new file in the record's place, as an agent that writes its record anew and renames
    /// it over the old one does.
    fn replace(&self, lines: &[&str]) {
        let staged = self.record().with_extension("staged");
        write_lines(&staged, lines, false);
        std::fs::rename(&staged, self.record()).unwrap();
    }

    async fn send(&self, id: &str, body: RequestBody) {
        self.connection.submit(Some(id.to_owned()), body).await;
    }

    async fn next(&mut self) -> Event {
        tokio::time::timeout(PATIENCE, self.events.recv())
            .await
            .expect("an event in time")
            .expect("an open connection")
    }

    async fn subscribe(&mut self, session: &str, limit: Option<u32>) -> Event {
        self.send(
            "sub",
            RequestBody::SubscribeSessionConversation {
                session: session.to_owned(),
                limit,
            },
        )
        .await;
        self.next().await
    }
}

fn write_lines(path: &Path, lines: &[&str], append: bool) {
    use std::io::Write;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
        .unwrap();
    // One write for the whole batch: the daemon polls the file meanwhile, and a poll between two
    // writes would split the batch into two events.
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    file.write_all(text.as_bytes()).unwrap();
}

fn texts(entries: &[ConversationEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| match &entry.body {
            ConversationBody::User { text } | ConversationBody::Assistant { text } => text.clone(),
            ConversationBody::ToolCall(call) => format!("{}:{:?}", call.call_id, call.status),
            other => format!("{other:?}"),
        })
        .collect()
}

fn error_of(event: Event) -> crate::protocol::Code {
    match event {
        Event::Error { code, .. } => code,
        other => panic!("an error, not {other:?}"),
    }
}

#[tokio::test]
async fn subscribing_answers_the_latest_page_and_follows_appends_updates_and_pending() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[
        r#"{"user":"hello"}"#,
        r#"{"assistant":"hi"}"#,
        r#"{"call":"c1"}"#,
    ]);
    let Event::SessionConversationSubscribed {
        id,
        session,
        epoch,
        entries,
        has_earlier,
        pending,
    } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    assert_eq!((id.as_deref(), session.as_str()), (Some("sub"), "s1"));
    assert!(epoch > 0 && !has_earlier && pending.is_empty());
    assert_eq!(texts(&entries), ["hello", "hi", "c1:Running"]);

    client.append(&[
        r#"{"assistant":"more"}"#,
        r#"{"result":"c1","output":"done"}"#,
    ]);
    let Event::SessionConversationChanged {
        epoch: changed_epoch,
        appended,
        updated,
        pending,
        ..
    } = client.next().await
    else {
        panic!("a change");
    };
    assert_eq!(changed_epoch, epoch);
    assert_eq!(texts(&appended), ["more"]);
    assert_eq!(texts(&updated), ["c1:Completed"]);
    assert_eq!(updated[0].seq, 2);
    assert!(pending.is_none());

    // The companion beside the record, which the provider declared, shows what is in progress.
    write_lines(
        &client.record().with_extension("live"),
        &[r#"{"pending":"typing"}"#],
        false,
    );
    let Event::SessionConversationChanged {
        pending, appended, ..
    } = client.next().await
    else {
        panic!("a change");
    };
    assert!(appended.is_empty());
    assert_eq!(pending.expect("pending changed").len(), 1);
}

#[tokio::test]
async fn a_record_that_is_rewritten_replaces_the_conversation_under_a_new_epoch() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[
        r#"{"user":"one"}"#,
        r#"{"assistant":"two"}"#,
        r#"{"user":"three"}"#,
    ]);
    let Event::SessionConversationSubscribed { epoch, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    client.replace(&[r#"{"user":"fresh"}"#]);
    let Event::SessionConversationReplaced {
        epoch: next,
        entries,
        ..
    } = client.next().await
    else {
        panic!("a replacement");
    };
    assert!(next > epoch);
    assert_eq!(texts(&entries), ["fresh"]);

    // A reader that drops entries clients hold replaces it too.
    client.append(&[r#"{"assistant":"a"}"#, r#"{"assistant":"b"}"#]);
    assert!(matches!(
        client.next().await,
        Event::SessionConversationChanged { .. }
    ));
    client.append(&[r#"{"truncate":1}"#]);
    let Event::SessionConversationReplaced {
        epoch: after,
        entries,
        ..
    } = client.next().await
    else {
        panic!("a replacement");
    };
    assert!(after > next);
    assert_eq!(texts(&entries), ["fresh"]);
}

#[tokio::test]
async fn a_session_with_no_record_is_empty_and_picks_it_up_when_it_appears() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    let Event::SessionConversationSubscribed { entries, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    assert!(entries.is_empty());
    client.write(&[r#"{"user":"late"}"#]);
    let Event::SessionConversationReplaced { entries, .. } = client.next().await else {
        panic!("a replacement");
    };
    assert_eq!(texts(&entries), ["late"]);
}

#[tokio::test]
async fn earlier_pages_are_read_by_seq_and_a_stale_epoch_gets_an_empty_page() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    let lines: Vec<String> = (0..5)
        .map(|n| format!(r#"{{"assistant":"m{n}"}}"#))
        .collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    client.write(&lines);
    let Event::SessionConversationSubscribed {
        epoch,
        entries,
        has_earlier,
        ..
    } = client.subscribe("s1", Some(2)).await
    else {
        panic!("a subscribed reply");
    };
    assert_eq!(texts(&entries), ["m3", "m4"]);
    assert!(has_earlier);
    let read = |epoch, before| RequestBody::ReadSessionConversation {
        session: "s1".to_owned(),
        epoch,
        before,
        limit: Some(2),
    };
    client.send("r1", read(epoch, 3)).await;
    let Event::SessionConversation {
        entries,
        has_earlier,
        epoch: same,
        ..
    } = client.next().await
    else {
        panic!("a page");
    };
    assert_eq!(
        (texts(&entries), has_earlier, same),
        (["m1", "m2"].map(String::from).to_vec(), true, epoch)
    );
    client.send("r2", read(epoch, 1)).await;
    let Event::SessionConversation {
        entries,
        has_earlier,
        ..
    } = client.next().await
    else {
        panic!("a page");
    };
    assert_eq!(
        (texts(&entries), has_earlier),
        (vec!["m0".to_owned()], false)
    );
    client.send("r3", read(epoch - 1, 3)).await;
    let Event::SessionConversation {
        entries,
        epoch: current,
        ..
    } = client.next().await
    else {
        panic!("a page");
    };
    assert!(entries.is_empty());
    assert_eq!(current, epoch);
}

#[tokio::test]
async fn reading_needs_a_subscription_and_a_known_session_and_unsubscribing_is_idempotent() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    let read = |session: &str| RequestBody::ReadSessionConversation {
        session: session.to_owned(),
        epoch: 1,
        before: 1,
        limit: None,
    };
    client.send("r", read("s1")).await;
    assert_eq!(
        error_of(client.next().await),
        error_code::CONVERSATION_NOT_FOLLOWED
    );
    client.send("r", read("nope")).await;
    assert_eq!(error_of(client.next().await), error_code::UNKNOWN_SESSION);
    let unsubscribe = || RequestBody::UnsubscribeSessionConversation {
        session: "s1".to_owned(),
    };
    client.send("u", unsubscribe()).await;
    assert!(matches!(client.next().await, Event::Ack { .. }));
    assert!(matches!(
        client.subscribe("s1", None).await,
        Event::SessionConversationSubscribed { .. }
    ));
    assert_eq!(client.state.conversation_feeds.len(), 1);
    client.send("u", unsubscribe()).await;
    assert!(matches!(client.next().await, Event::Ack { .. }));
    client.send("r", read("s1")).await;
    assert_eq!(
        error_of(client.next().await),
        error_code::CONVERSATION_NOT_FOLLOWED
    );
    assert_eq!(client.state.conversation_feeds.len(), 0);
    let unknown = client.subscribe("nope", None).await;
    assert_eq!(error_of(unknown), error_code::UNKNOWN_SESSION);
}

#[tokio::test]
async fn a_connection_follows_at_most_sixteen_conversations() {
    let mut client = Client::new();
    for n in 0..=SUBSCRIPTION_LIMIT {
        client.session(&format!("s{n}"), None);
    }
    for n in 0..SUBSCRIPTION_LIMIT {
        let reply = client.subscribe(&format!("s{n}"), None).await;
        assert!(
            matches!(reply, Event::SessionConversationSubscribed { .. }),
            "{n}"
        );
    }
    let refused = client
        .subscribe(&format!("s{SUBSCRIPTION_LIMIT}"), None)
        .await;
    assert_eq!(
        error_of(refused),
        error_code::CONVERSATION_SUBSCRIPTIONS_EXCEEDED
    );
    // A repeat of one followed is no new subscription: it answers a fresh latest page.
    assert!(matches!(
        client.subscribe("s0", None).await,
        Event::SessionConversationSubscribed { .. }
    ));
    assert_eq!(client.state.conversation_feeds.len(), SUBSCRIPTION_LIMIT);
}

#[tokio::test(start_paused = true)]
async fn the_feed_ends_with_its_last_subscriber_and_a_connection_going() {
    let mut client = Client::new();
    client.session("s1", None);
    let Client { state, .. } = &client;
    let state = Arc::clone(state);
    // A second connection follows the same session: one feed for both.
    let (outbound, mut second_events) = mpsc::channel(256);
    let second = Connection::start(state.clone(), outbound);
    client.subscribe("s1", None).await;
    second
        .submit(
            Some("x".to_owned()),
            RequestBody::SubscribeSessionConversation {
                session: "s1".to_owned(),
                limit: None,
            },
        )
        .await;
    assert!(matches!(
        second_events.recv().await,
        Some(Event::SessionConversationSubscribed { .. })
    ));
    assert_eq!(state.conversation_feeds.len(), 1);
    drop(second);
    drop(client.connection);
    // The aborted workers drop their subscriptions as the runtime runs them out.
    tasks_idle().await;
    assert_eq!(state.conversation_feeds.len(), 0);
}

#[tokio::test]
async fn a_deleted_session_drops_its_feed() {
    let mut client = Client::new();
    client.session("s1", None);
    client.subscribe("s1", None).await;
    client.state.conversation_feeds.forget("s1");
    assert_eq!(client.state.conversation_feeds.len(), 0);
}

// -- What a connection is told, over the paused clock ---------------------------------------------
//
// The following task looks at the files every `POLL_EVERY` and finds them again every
// `RELOCATE_EVERY`, both of which a paused clock moves on whenever everything else is waiting, so
// these tests wait for the events themselves and prove an absence by letting the runtime go idle.

use crate::protocol::{
    CONVERSATION_PAGE_BYTES, CONVERSATION_PAGE_DEFAULT, CONVERSATION_PAGE_LIMIT,
};
use crate::records::feed::follow::{Follower, Read, POLL_EVERY};
use crate::records::feed::FeedOut;
use crate::records::Stretch;
use crate::test_support::clock::tasks_idle;

const OTHER_ID: &str = "0b0e3a2c-6a52-4a49-8b64-0f3c1a1d9b22";

impl Client {
    /// Lets the following task look at the files a few more times, then returns once everything
    /// that can run has run: what the connection was going to be sent has been sent by then.
    async fn quiet(&mut self) {
        tokio::time::advance(POLL_EVERY * 4).await;
        tasks_idle().await;
    }

    fn assert_nothing_pending(&mut self) {
        match self.events.try_recv() {
            Err(mpsc::error::TryRecvError::Empty) => {}
            other => panic!("nothing was to be sent, got {other:?}"),
        }
    }
}

fn numbered(count: usize) -> Vec<String> {
    (0..count)
        .map(|n| format!(r#"{{"assistant":"m{n}"}}"#))
        .collect()
}

fn refs(lines: &[String]) -> Vec<&str> {
    lines.iter().map(String::as_str).collect()
}

#[tokio::test(start_paused = true)]
async fn a_page_holds_fifty_entries_unless_asked_and_never_more_than_two_hundred() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    let lines = numbered(CONVERSATION_PAGE_LIMIT + 50);
    client.write(&refs(&lines));
    let page = |event| match event {
        Event::SessionConversationSubscribed {
            entries,
            has_earlier,
            ..
        } => (entries, has_earlier),
        other => panic!("a subscribed reply, not {other:?}"),
    };
    let (entries, has_earlier) = page(client.subscribe("s1", None).await);
    assert_eq!(entries.len(), CONVERSATION_PAGE_DEFAULT);
    assert!(has_earlier);
    assert_eq!(entries.last().unwrap().seq as usize, lines.len() - 1);
    let (entries, _) = page(client.subscribe("s1", Some(7)).await);
    assert_eq!(entries.len(), 7);
    let (entries, has_earlier) = page(client.subscribe("s1", Some(10_000)).await);
    assert_eq!(entries.len(), CONVERSATION_PAGE_LIMIT);
    assert!(has_earlier);
}

#[tokio::test(start_paused = true)]
async fn a_read_holds_fifty_entries_unless_asked_and_never_more_than_two_hundred() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    let lines = numbered(CONVERSATION_PAGE_LIMIT + 100);
    client.write(&refs(&lines));
    let Event::SessionConversationSubscribed { epoch, .. } = client.subscribe("s1", Some(1)).await
    else {
        panic!("a subscribed reply");
    };
    let before = lines.len() as u64 - 1;
    for (limit, expected) in [
        (None, CONVERSATION_PAGE_DEFAULT),
        (Some(3), 3),
        (Some(100_000), CONVERSATION_PAGE_LIMIT),
    ] {
        client
            .send(
                "r",
                RequestBody::ReadSessionConversation {
                    session: "s1".to_owned(),
                    epoch,
                    before,
                    limit,
                },
            )
            .await;
        let Event::SessionConversation {
            entries,
            has_earlier,
            ..
        } = client.next().await
        else {
            panic!("a page");
        };
        assert_eq!(entries.len(), expected, "{limit:?}");
        assert_eq!(entries.last().unwrap().seq, before - 1);
        assert!(has_earlier);
    }
}

/// A page ends sooner than its limit where its entries would pass a mebibyte serialized, but holds
/// one entry at least, whether it is the subscribe reply or a read.
#[tokio::test(start_paused = true)]
async fn a_page_is_kept_to_a_mebibyte_with_one_entry_at_least() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    let big = "x".repeat(CONVERSATION_PAGE_BYTES / 4);
    let line = format!(r#"{{"assistant":"{big}"}}"#);
    client.write(&[&line, &line, &line, &line, &line, &line]);
    let Event::SessionConversationSubscribed {
        epoch,
        entries,
        has_earlier,
        ..
    } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    assert!(has_earlier && !entries.is_empty() && entries.len() < 6);
    assert!(
        entries
            .iter()
            .map(|entry| serde_json::to_vec(entry).unwrap().len())
            .sum::<usize>()
            <= CONVERSATION_PAGE_BYTES
    );
    let earliest = entries[0].seq;
    client
        .send(
            "r",
            RequestBody::ReadSessionConversation {
                session: "s1".to_owned(),
                epoch,
                before: earliest,
                limit: Some(CONVERSATION_PAGE_LIMIT as u32),
            },
        )
        .await;
    let Event::SessionConversation { entries, .. } = client.next().await else {
        panic!("a page");
    };
    assert!(!entries.is_empty() && entries.len() < earliest as usize + 1);
    assert!(
        entries
            .iter()
            .map(|entry| serde_json::to_vec(entry).unwrap().len())
            .sum::<usize>()
            <= CONVERSATION_PAGE_BYTES
    );

    // One entry past the limit on its own is still sent.
    let huge = "y".repeat(CONVERSATION_PAGE_BYTES * 2);
    client.replace(&[&format!(r#"{{"assistant":"{huge}"}}"#)]);
    let Event::SessionConversationReplaced { entries, .. } = client.next().await else {
        panic!("a replacement");
    };
    assert_eq!(entries.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_session_is_followed_in_any_status() {
    let mut client = Client::new();
    let statuses = [
        SessionStatus::Starting,
        SessionStatus::Working,
        SessionStatus::WaitingUser,
        SessionStatus::Idle,
        SessionStatus::Interrupted,
        SessionStatus::Archived,
    ];
    for (n, status) in statuses.into_iter().enumerate() {
        let id = format!("s{n}");
        client.session(&id, None);
        let mut session = client.state.store.get_session(&id).unwrap().unwrap();
        session.status = status;
        client.state.store.update_session(&session).unwrap();
        assert!(
            matches!(
                client.subscribe(&id, None).await,
                Event::SessionConversationSubscribed { .. }
            ),
            "{status:?}"
        );
    }
}

/// Events go to the connections that follow the session only, and what one connection does leaves
/// the other's subscription alone.
#[tokio::test(start_paused = true)]
async fn events_go_only_to_the_connections_following_and_unsubscribing_stops_them() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.session("s2", None);
    client.write(&[r#"{"user":"one"}"#]);
    let (outbound, mut bystander) = mpsc::channel(256);
    let bystander_connection = Connection::start(client.state.clone(), outbound);
    let (outbound, mut second_events) = mpsc::channel(256);
    let second = Connection::start(client.state.clone(), outbound);
    // The bystander follows another session only.
    bystander_connection
        .submit(
            Some("b".to_owned()),
            RequestBody::SubscribeSessionConversation {
                session: "s2".to_owned(),
                limit: None,
            },
        )
        .await;
    second
        .submit(
            Some("x".to_owned()),
            RequestBody::SubscribeSessionConversation {
                session: "s1".to_owned(),
                limit: None,
            },
        )
        .await;
    client.subscribe("s1", None).await;
    assert!(matches!(
        bystander.recv().await,
        Some(Event::SessionConversationSubscribed { .. })
    ));
    assert!(matches!(
        second_events.recv().await,
        Some(Event::SessionConversationSubscribed { .. })
    ));

    client.append(&[r#"{"assistant":"two"}"#]);
    assert!(matches!(
        client.next().await,
        Event::SessionConversationChanged { .. }
    ));
    assert!(matches!(
        second_events.recv().await,
        Some(Event::SessionConversationChanged { .. })
    ));
    tasks_idle().await;
    assert!(
        matches!(bystander.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "a connection following another session was sent something"
    );

    // One connection leaving the feed leaves the other's subscription as it was.
    client
        .send(
            "u",
            RequestBody::UnsubscribeSessionConversation {
                session: "s1".to_owned(),
            },
        )
        .await;
    assert!(matches!(client.next().await, Event::Ack { .. }));
    client.append(&[r#"{"assistant":"three"}"#]);
    let Some(Event::SessionConversationChanged { appended, .. }) = second_events.recv().await
    else {
        panic!("a change");
    };
    assert_eq!(texts(&appended), ["three"]);
    client.quiet().await;
    client.assert_nothing_pending();
}

/// Subscribing again answers a fresh latest page, and the connection is then sent each change
/// once: the first subscription's forwarding does not go on beside the second's.
#[tokio::test(start_paused = true)]
async fn subscribing_again_answers_a_fresh_page_and_events_are_not_doubled() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#]);
    client.subscribe("s1", None).await;
    client.append(&[r#"{"assistant":"two"}"#]);
    assert!(matches!(
        client.next().await,
        Event::SessionConversationChanged { .. }
    ));
    let Event::SessionConversationSubscribed { entries, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    assert_eq!(texts(&entries), ["one", "two"]);

    client.append(&[r#"{"assistant":"three"}"#]);
    let Event::SessionConversationChanged { appended, .. } = client.next().await else {
        panic!("a change");
    };
    assert_eq!(texts(&appended), ["three"]);
    client.quiet().await;
    client.assert_nothing_pending();
    assert_eq!(client.state.conversation_feeds.len(), 1);
}

/// What was written between the reply and the events is told once: nothing in the reply comes
/// again, and nothing after it is missed.
#[tokio::test(start_paused = true)]
async fn the_events_after_a_reply_hold_exactly_what_the_reply_does_not() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#, r#"{"assistant":"two"}"#]);
    let Event::SessionConversationSubscribed { entries, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    assert_eq!(texts(&entries), ["one", "two"]);
    let mut seen = Vec::new();
    for n in 0..5 {
        client.append(&[&format!(r#"{{"assistant":"more{n}"}}"#)]);
        let Event::SessionConversationChanged { appended, .. } = client.next().await else {
            panic!("a change");
        };
        seen.extend(appended.iter().map(|entry| entry.seq));
    }
    assert_eq!(seen, [2, 3, 4, 5, 6]);
}

/// The agent's conversation moving to another file (what a Codex revert does) is a replacement
/// under a new epoch carrying what the new files say.
#[tokio::test(start_paused = true)]
async fn a_conversation_that_moves_to_another_file_replaces_the_one_held() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"old"}"#]);
    let Event::SessionConversationSubscribed { epoch, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    write_lines(
        &client.record_of(OTHER_ID),
        &[r#"{"user":"moved"}"#, r#"{"assistant":"on"}"#],
        false,
    );
    client
        .state
        .store
        .set_agent_session_id("s1", OTHER_ID)
        .unwrap();
    // The session's own record moved, which is acted on at the next look.
    let Event::SessionConversationReplaced {
        epoch: next,
        entries,
        ..
    } = client.next().await
    else {
        panic!("a replacement");
    };
    assert!(next > epoch);
    assert_eq!(texts(&entries), ["moved", "on"]);

    write_lines(
        &client.record_of(OTHER_ID),
        &[r#"{"assistant":"after"}"#],
        true,
    );
    let Event::SessionConversationChanged {
        epoch: same,
        appended,
        ..
    } = client.next().await
    else {
        panic!("a change");
    };
    assert_eq!((same, texts(&appended)), (next, vec!["after".to_owned()]));
}

/// A conversation that ends up with no subscriber is not read any more: a record written after
/// the last connection left shows up in no feed, and a new subscription reads it from its start.
#[tokio::test(start_paused = true)]
async fn nothing_is_kept_or_read_once_the_last_subscriber_has_gone() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#]);
    client.subscribe("s1", None).await;
    client
        .send(
            "u",
            RequestBody::UnsubscribeSessionConversation {
                session: "s1".to_owned(),
            },
        )
        .await;
    assert!(matches!(client.next().await, Event::Ack { .. }));
    assert_eq!(client.state.conversation_feeds.len(), 0);
    client.append(&[r#"{"assistant":"two"}"#]);
    client.quiet().await;
    client.assert_nothing_pending();
    let Event::SessionConversationSubscribed { entries, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    assert_eq!(texts(&entries), ["one", "two"]);
}

/// A connection that fell behind the feed is sent the conversation whole, and the changes after it
/// follow as they come.
#[tokio::test]
async fn a_lagging_connection_is_sent_a_fresh_replacement_and_then_the_changes() {
    let client = Client::new();
    let (channel, _) = client.state.conversation_feeds.join("s1");
    let mut out = FeedOut::default();
    out.user("one", None);
    channel.replace_with(out);
    let (snapshot, events) = channel
        .subscribe(CONVERSATION_PAGE_DEFAULT)
        .expect("a live feed");
    // More changes than the feed buffers for a connection that is not taking them.
    for n in 0..2000 {
        channel
            .apply(|out| out.assistant(&format!("m{n}"), None))
            .expect("applied");
    }
    let (outbound, mut sent) = mpsc::channel(1024);
    let (ended, _watching) = mpsc::unbounded_channel();
    let forwarding = tokio::spawn(forward(Arc::clone(&channel), events, outbound, ended, 0));
    let first = tokio::time::timeout(PATIENCE, sent.recv())
        .await
        .expect("an event in time")
        .expect("an open channel");
    let Event::SessionConversationReplaced {
        epoch,
        entries,
        has_earlier,
        ..
    } = first
    else {
        panic!("the conversation whole, not {first:?}");
    };
    assert!(epoch >= snapshot.epoch);
    assert_eq!(entries.len(), CONVERSATION_PAGE_DEFAULT);
    assert!(has_earlier);
    assert_eq!(
        texts(&entries).last().map(String::as_str),
        Some("m1999"),
        "the latest page as it stands"
    );
    channel
        .apply(|out| out.assistant("last", None))
        .expect("applied");
    let next = tokio::time::timeout(PATIENCE, sent.recv())
        .await
        .expect("an event in time")
        .expect("an open channel");
    let Event::SessionConversationChanged { appended, .. } = next else {
        panic!("a change, not {next:?}");
    };
    assert_eq!(texts(&appended), ["last"]);
    forwarding.abort();
}

/// The conversation requests are served by the connection: one that reaches the generic request
/// path is refused, and nothing is followed.
#[tokio::test]
async fn the_generic_request_path_refuses_the_conversation_requests() {
    let client = Client::new();
    client.session("s1", None);
    let bodies = [
        RequestBody::SubscribeSessionConversation {
            session: "s1".to_owned(),
            limit: None,
        },
        RequestBody::UnsubscribeSessionConversation {
            session: "s1".to_owned(),
        },
        RequestBody::ReadSessionConversation {
            session: "s1".to_owned(),
            epoch: 1,
            before: 1,
            limit: None,
        },
    ];
    for body in bodies {
        let result = crate::coordinator::handle(&client.state, Some("r".to_owned()), body).await;
        assert!(result.is_err());
    }
    assert_eq!(client.state.conversation_feeds.len(), 0);
}

// -- The real readers, fed through the feed -------------------------------------------------------

fn texts_of_page(channel: &FeedChannel) -> Vec<String> {
    texts(
        &channel
            .subscribe(CONVERSATION_PAGE_LIMIT)
            .expect("a live feed")
            .0
            .page
            .entries,
    )
}

/// Claude Code's transcript keeps what the user abandoned when they went back: the feed tells it as
/// a replacement under a new epoch, with what is left of the conversation.
#[test]
fn a_claude_rewind_replaces_what_the_subscribers_hold() {
    let capture = include_str!("../../../testdata/feed_claude_2.1.328.jsonl");
    let lines: Vec<&str> = capture.lines().collect();
    let dir = ScratchDir::new("conversation-feed-claude-rewind");
    let path = dir.join("transcript.jsonl");
    let before = &lines[..lines.len() - 2];
    write_lines(&path, before, false);

    let feeds = crate::state::ConversationFeeds::default();
    let (channel, _) = feeds.join("s1");
    let built = Follower::build(
        AgentId::CLAUDE,
        vec![Stretch {
            path: path.clone(),
            cut: None,
        }],
    );
    let mut follower = built.follower;
    channel.replace_with(built.out);
    let (snapshot, mut events) = channel
        .subscribe(CONVERSATION_PAGE_LIMIT)
        .expect("a live feed");
    assert!(texts(&snapshot.page.entries)
        .iter()
        .any(|text| text.contains("Compaction")));

    write_lines(&path, &lines[lines.len() - 2..], true);
    let Read::Lines(read) = follower.read_new() else {
        panic!("lines");
    };
    channel
        .apply(|out| follower.apply(read, out))
        .expect("applied");
    let Ok(Event::SessionConversationReplaced { epoch, entries, .. }) = events.try_recv() else {
        panic!("a replacement");
    };
    assert!(epoch > snapshot.epoch);
    let shown = texts(&entries);
    assert_eq!(
        shown[shown.len() - 2..],
        ["Start over with a different approach", "Starting over."]
    );
    assert!(shown.iter().any(|text| text == "Noted, reviewing now."));
    assert!(!shown.iter().any(|text| text.contains("Compaction")));
    assert_eq!(texts_of_page(&channel), shown);
}

/// Codex starts a new rollout segment when a thread goes back to an earlier turn: the earlier file
/// counts up to its cut, and the new one follows it whole.
#[test]
fn a_codex_revert_reads_the_earlier_file_to_its_cut_and_then_the_new_segment() {
    let dir = ScratchDir::new("conversation-feed-codex-revert");
    let old = include_str!("../../../testdata/handoff_codex_0.162.1_rollout.jsonl");
    let new = include_str!("../../../testdata/feed_codex_0.167.1_rollout.jsonl");
    let cut: usize = old.split_inclusive('\n').take(3).map(str::len).sum();
    let (old_path, new_path) = (dir.join("old.jsonl"), dir.join("new.jsonl"));
    std::fs::write(&old_path, old).unwrap();
    std::fs::write(&new_path, new).unwrap();
    let read = |stretches: Vec<Stretch>| {
        let built = Follower::build(AgentId::CODEX, stretches);
        texts(built.out.entries())
    };
    let abandoned = read(vec![Stretch {
        path: old_path.clone(),
        cut: None,
    }]);
    let kept = read(vec![Stretch {
        path: old_path.clone(),
        cut: Some(cut as u64),
    }]);
    let fresh = read(vec![Stretch {
        path: new_path.clone(),
        cut: None,
    }]);
    let both = read(vec![
        Stretch {
            path: old_path,
            cut: Some(cut as u64),
        },
        Stretch {
            path: new_path,
            cut: None,
        },
    ]);
    assert!(!kept.is_empty() && kept.len() < abandoned.len());
    assert_eq!(both, [kept, fresh].concat());
}

/// Waits for the connection to be told the feed of `session` ended, and then checks it no longer
/// follows it: the subscription is dropped before the event is sent.
async fn until_not_followed(client: &mut Client, session: &str) {
    let Event::SessionConversationEnded { session: ended } = client.next().await else {
        panic!("the feed's end is told first");
    };
    assert_eq!(ended, session);
    client
        .send(
            "r",
            RequestBody::ReadSessionConversation {
                session: session.to_owned(),
                epoch: 1,
                before: 1,
                limit: None,
            },
        )
        .await;
    assert_eq!(
        error_of(client.next().await),
        error_code::CONVERSATION_NOT_FOLLOWED
    );
}

/// A reader that panics ends its feed cleanly: the lock it held is given back, the connection's
/// subscription to it is dropped, and the connection goes on answering.
#[tokio::test(start_paused = true)]
async fn a_reader_that_panics_ends_the_feed_and_the_connection_goes_on() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#]);
    assert!(matches!(
        client.subscribe("s1", None).await,
        Event::SessionConversationSubscribed { .. }
    ));
    client.append(&[r#"{"panic":true}"#]);
    until_not_followed(&mut client, "s1").await;
    assert_eq!(client.state.conversation_feeds.len(), 0);

    client.replace(&[r#"{"user":"again"}"#]);
    let Event::SessionConversationSubscribed { entries, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    assert_eq!(texts(&entries), ["again"]);
}

/// A feed that is dropped under its subscribers (its session was deleted) ends their subscriptions
/// too, which frees their places among the sixteen.
#[tokio::test(start_paused = true)]
async fn a_feed_dropped_under_its_subscriber_ends_the_subscription() {
    let mut client = Client::new();
    client.session("s1", None);
    client.subscribe("s1", None).await;
    client.state.conversation_feeds.forget("s1");
    until_not_followed(&mut client, "s1").await;
    // Not told again, and the session may be followed anew while it exists.
    assert!(matches!(
        client.subscribe("s1", None).await,
        Event::SessionConversationSubscribed { .. }
    ));
}

/// A request the connection's worker can no longer take is still answered, with its id.
#[tokio::test]
async fn a_request_the_worker_cannot_take_is_answered_with_an_error() {
    let mut client = Client::new();
    client.connection.worker.abort();
    while !client.connection.requests.is_closed() {
        tokio::task::yield_now().await;
    }
    client
        .send(
            "late",
            RequestBody::UnsubscribeSessionConversation {
                session: "s1".to_owned(),
            },
        )
        .await;
    let Event::Error { id, .. } = client.next().await else {
        panic!("an error");
    };
    assert_eq!(id.as_deref(), Some("late"));
}

// -- When a feed ends, over the paused clock -------------------------------------------------------

use crate::records::feed::follow::{RELOCATE_BACKOFF, RELOCATE_EVERY};

impl Client {
    /// Makes a second connection to the same daemon that follows `session`, and returns it with
    /// what it is sent, after its subscribe was answered.
    async fn bystander(&self, session: &str) -> (Connection, mpsc::Receiver<Event>) {
        let (outbound, mut events) = mpsc::channel(256);
        let connection = Connection::start(self.state.clone(), outbound);
        connection
            .submit(
                Some("sub".to_owned()),
                RequestBody::SubscribeSessionConversation {
                    session: session.to_owned(),
                    limit: None,
                },
            )
            .await;
        assert!(matches!(
            events.recv().await,
            Some(Event::SessionConversationSubscribed { .. })
        ));
        (connection, events)
    }

    /// The path where an account of the test agent, other than the session's own directory, keeps
    /// the record of the session's agent session id: another place its conversation may be.
    fn other_account_record(&self) -> PathBuf {
        let home = self.dir.join("other-account");
        self.state
            .store
            .insert_account(&crate::protocol::Account {
                id: "test-other".to_owned(),
                agent: AgentId::TEST,
                name: "test-other".to_owned(),
                config_dir: home.to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();
        home.join("sessions").join(format!("{AGENT_ID}.jsonl"))
    }

    /// Reads events until the one that answers request `id`, and returns it with the events that
    /// came before it.
    async fn until_answer(&mut self, id: &str) -> (Event, Vec<Event>) {
        let mut before = Vec::new();
        loop {
            let event = self.next().await;
            let answers = match &event {
                Event::Ack { id: answered }
                | Event::SessionConversationSubscribed { id: answered, .. }
                | Event::Error { id: answered, .. } => answered.as_deref() == Some(id),
                _ => false,
            };
            if answers {
                return (event, before);
            }
            before.push(event);
        }
    }
}

/// A session that is deleted while followed ends its feed for every connection that follows it,
/// each is told, and a new subscribe is answered `unknown_session`.
#[tokio::test(start_paused = true)]
async fn a_session_deleted_while_followed_ends_its_feed_for_every_connection() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#]);
    client.subscribe("s1", None).await;
    let (_second, mut second_events) = client.bystander("s1").await;

    client.state.store.delete_session("s1").unwrap();
    assert!(matches!(
        client.next().await,
        Event::SessionConversationEnded { session } if session == "s1"
    ));
    assert!(matches!(
        second_events.recv().await,
        Some(Event::SessionConversationEnded { session }) if session == "s1"
    ));
    tasks_idle().await;
    assert_eq!(client.state.conversation_feeds.len(), 0);
    assert_eq!(
        error_of(client.subscribe("s1", None).await),
        error_code::UNKNOWN_SESSION
    );
    client.quiet().await;
    client.assert_nothing_pending();
    assert!(matches!(
        second_events.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}

/// A subscriber that is waiting for the first page of a feed that ends because its session was
/// deleted is answered `unknown_session`, with its id, and not told the conversation cannot be
/// followed.
#[tokio::test(start_paused = true)]
async fn a_session_deleted_while_a_subscriber_waits_for_the_first_page_is_unknown() {
    let mut client = Client::new();
    client.session("s1", None);
    // A feed that nothing fills yet, so the subscribe below waits for its first page.
    let (waiting_for, started) = client.state.conversation_feeds.join("s1");
    assert!(started);
    client
        .send(
            "sub",
            RequestBody::SubscribeSessionConversation {
                session: "s1".to_owned(),
                limit: None,
            },
        )
        .await;
    tasks_idle().await;
    client.assert_nothing_pending();

    client.state.store.delete_session("s1").unwrap();
    client.state.conversation_feeds.forget("s1");
    let Event::Error { id, code, .. } = client.next().await else {
        panic!("an error");
    };
    assert_eq!(id.as_deref(), Some("sub"));
    assert_eq!(code, error_code::UNKNOWN_SESSION);
    drop(waiting_for);
}

/// A reader that panics ends that feed only: the connection's other feeds go on, the one that
/// ended is told to it, and its place among the sixteen is free again.
#[tokio::test(start_paused = true)]
async fn a_reader_that_panics_ends_only_its_feed_and_frees_its_place() {
    let mut client = Client::new();
    client.session("s0", Some(AGENT_ID));
    client.session("s1", Some(OTHER_ID));
    client.write(&[r#"{"user":"one"}"#]);
    write_lines(&client.record_of(OTHER_ID), &[r#"{"user":"two"}"#], false);
    for n in 2..SUBSCRIPTION_LIMIT {
        client.session(&format!("s{n}"), None);
    }
    for n in 0..SUBSCRIPTION_LIMIT {
        assert!(matches!(
            client.subscribe(&format!("s{n}"), None).await,
            Event::SessionConversationSubscribed { .. }
        ));
    }
    client.session("extra", None);
    assert_eq!(
        error_of(client.subscribe("extra", None).await),
        error_code::CONVERSATION_SUBSCRIPTIONS_EXCEEDED
    );

    client.append(&[r#"{"panic":true}"#]);
    until_not_followed(&mut client, "s0").await;
    assert_eq!(
        client.state.conversation_feeds.len(),
        SUBSCRIPTION_LIMIT - 1
    );
    assert!(matches!(
        client.subscribe("extra", None).await,
        Event::SessionConversationSubscribed { .. }
    ));

    // The other feeds are followed still.
    write_lines(
        &client.record_of(OTHER_ID),
        &[r#"{"assistant":"after"}"#],
        true,
    );
    let Event::SessionConversationChanged {
        session, appended, ..
    } = client.next().await
    else {
        panic!("a change of the feed that did not panic");
    };
    assert_eq!(
        (session.as_str(), texts(&appended)),
        ("s1", vec!["after".to_owned()])
    );
}

/// A reader that panics on the conversation's first read leaves the subscribe answered with an
/// error that carries its id, and the connection goes on.
#[tokio::test(start_paused = true)]
async fn a_reader_that_panics_on_the_first_read_answers_the_subscribe_with_an_error() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#, r#"{"panic":true}"#]);
    let Event::Error { id, .. } = client.subscribe("s1", None).await else {
        panic!("an error");
    };
    assert_eq!(id.as_deref(), Some("sub"));
    assert_eq!(client.state.conversation_feeds.len(), 0);

    client.session("s2", Some(OTHER_ID));
    write_lines(&client.record_of(OTHER_ID), &[r#"{"user":"fine"}"#], false);
    let Event::SessionConversationSubscribed { entries, .. } = client.subscribe("s2", None).await
    else {
        panic!("a subscribed reply");
    };
    assert_eq!(texts(&entries), ["fine"]);
}

/// Changes the feed sent while the connection's forwarder had not yet run are the page of a
/// repeated subscribe too: nothing the first subscription's forwarder holds comes after the reply.
#[tokio::test(start_paused = true)]
async fn a_repeat_subscribe_sends_nothing_from_the_old_forwarder_after_its_reply() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#]);
    client.subscribe("s1", None).await;
    let (channel, _) = client.state.conversation_feeds.join("s1");
    for text in ["a", "b", "c"] {
        channel
            .apply(|out| out.assistant(text, None))
            .expect("applied");
    }
    client
        .send(
            "again",
            RequestBody::SubscribeSessionConversation {
                session: "s1".to_owned(),
                limit: None,
            },
        )
        .await;
    let (reply, _earlier) = client.until_answer("again").await;
    let Event::SessionConversationSubscribed { entries, .. } = reply else {
        panic!("a subscribed reply");
    };
    assert_eq!(texts(&entries), ["one", "a", "b", "c"]);
    tasks_idle().await;
    client.assert_nothing_pending();

    channel
        .apply(|out| out.assistant("after", None))
        .expect("applied");
    let Event::SessionConversationChanged { appended, .. } = client.next().await else {
        panic!("a change");
    };
    assert_eq!(texts(&appended), ["after"]);
    client.quiet().await;
    client.assert_nothing_pending();
    client.state.conversation_feeds.leave(&channel);
}

/// The same for an unsubscribe: what was forwarded comes before the ack, and nothing after it.
#[tokio::test(start_paused = true)]
async fn an_unsubscribe_sends_nothing_from_the_forwarder_after_its_ack() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#]);
    client.subscribe("s1", None).await;
    let (channel, _) = client.state.conversation_feeds.join("s1");
    for text in ["a", "b", "c"] {
        channel
            .apply(|out| out.assistant(text, None))
            .expect("applied");
    }
    client
        .send(
            "u",
            RequestBody::UnsubscribeSessionConversation {
                session: "s1".to_owned(),
            },
        )
        .await;
    let (ack, _earlier) = client.until_answer("u").await;
    assert!(matches!(ack, Event::Ack { .. }));
    tasks_idle().await;
    client.assert_nothing_pending();
    channel
        .apply(|out| out.assistant("after", None))
        .expect("applied");
    client.quiet().await;
    client.assert_nothing_pending();
    client.state.conversation_feeds.leave(&channel);
}

// -- Reading the session and the files again ------------------------------------------------------

/// Breaks every read of the session store for a moment, as a second connection to its database
/// that renames the table does, and puts it back when dropped.
struct SessionsUnreadable(rusqlite::Connection);

impl SessionsUnreadable {
    fn new(client: &Client) -> Self {
        let database = rusqlite::Connection::open(client.dir.join("octoboard.db")).unwrap();
        database
            .execute_batch(
                "PRAGMA legacy_alter_table = ON; ALTER TABLE sessions RENAME TO sessions_away;",
            )
            .unwrap();
        Self(database)
    }

    fn restore(self) {
        self.0
            .execute_batch("ALTER TABLE sessions_away RENAME TO sessions;")
            .unwrap();
    }
}

/// A session that cannot be read for a moment is no session gone: the feed goes on following the
/// record, and picks up a move of the conversation once the session reads again.
#[tokio::test(start_paused = true)]
async fn a_failure_to_read_the_session_does_not_end_the_feed() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#]);
    let Event::SessionConversationSubscribed { epoch, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    let unreadable = SessionsUnreadable::new(&client);
    client.append(&[r#"{"assistant":"two"}"#]);
    let Event::SessionConversationChanged { appended, .. } = client.next().await else {
        panic!("a change");
    };
    assert_eq!(texts(&appended), ["two"]);
    client.quiet().await;
    client.assert_nothing_pending();
    assert_eq!(client.state.conversation_feeds.len(), 1);

    unreadable.restore();
    write_lines(&client.record_of(OTHER_ID), &[r#"{"user":"moved"}"#], false);
    client
        .state
        .store
        .set_agent_session_id("s1", OTHER_ID)
        .unwrap();
    let Event::SessionConversationReplaced {
        epoch: next,
        entries,
        ..
    } = client.next().await
    else {
        panic!("the feed still follows the session");
    };
    assert!(next > epoch);
    assert_eq!(texts(&entries), ["moved"]);
}

/// How long the conversation took to be read from where it moved to, from the time this was called.
async fn replaced_after(client: &mut Client) -> (std::time::Duration, Vec<String>) {
    let from = tokio::time::Instant::now();
    let Event::SessionConversationReplaced { entries, .. } = client.next().await else {
        panic!("a replacement");
    };
    (from.elapsed(), texts(&entries))
}

/// A change of the session's agent session id is acted on at the next look, not after the
/// interval that applies while the files read cleanly.
#[tokio::test(start_paused = true)]
async fn a_changed_agent_session_id_is_looked_for_at_once() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"old"}"#]);
    client.subscribe("s1", None).await;
    write_lines(&client.record_of(OTHER_ID), &[r#"{"user":"moved"}"#], false);
    client
        .state
        .store
        .set_agent_session_id("s1", OTHER_ID)
        .unwrap();
    let (took, entries) = replaced_after(&mut client).await;
    assert_eq!(entries, ["moved"]);
    assert!(took <= POLL_EVERY * 2, "{took:?}");
}

/// So is a change of the account directory the session runs under.
#[tokio::test(start_paused = true)]
async fn a_changed_config_dir_is_looked_for_at_once() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"old"}"#]);
    client.subscribe("s1", None).await;
    let other_home = client.dir.join("other-home");
    write_lines(
        &other_home
            .join("sessions")
            .join(format!("{AGENT_ID}.jsonl")),
        &[r#"{"user":"elsewhere"}"#],
        false,
    );
    client
        .state
        .store
        .set_session_account("s1", None, Some(&other_home.to_string_lossy()))
        .unwrap();
    let (took, entries) = replaced_after(&mut client).await;
    assert_eq!(entries, ["elsewhere"]);
    assert!(took <= POLL_EVERY * 2, "{took:?}");
}

/// A head that is gone is looked for again at once, and the conversation is read from the
/// other file that holds it.
#[tokio::test(start_paused = true)]
async fn a_missing_head_is_looked_for_at_once() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"old"}"#]);
    client.subscribe("s1", None).await;
    let moved = client.other_account_record();
    write_lines(&moved, &[r#"{"user":"found"}"#], false);
    std::fs::remove_file(client.record()).unwrap();
    let (took, entries) = replaced_after(&mut client).await;
    assert_eq!(entries, ["found"]);
    assert!(took <= POLL_EVERY * 2, "{took:?}");
}

/// While no record is found it is looked for every `RELOCATE_EVERY`.
#[tokio::test(start_paused = true)]
async fn a_record_that_is_not_found_is_looked_for_every_few_seconds() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.subscribe("s1", None).await;
    client.write(&[r#"{"user":"late"}"#]);
    let (took, entries) = replaced_after(&mut client).await;
    assert_eq!(entries, ["late"]);
    assert!(took <= RELOCATE_EVERY + POLL_EVERY, "{took:?}");
}

/// While the head reads cleanly the files are looked for again only every `RELOCATE_BACKOFF`: a
/// newer file the agent started on its own is picked up then, and not sooner.
#[tokio::test(start_paused = true)]
async fn a_head_that_reads_cleanly_is_looked_past_every_fifteen_seconds() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"old"}"#]);
    crate::test_support::move_modification_time_back(&client.record());
    client.subscribe("s1", None).await;
    let newer = client.other_account_record();
    write_lines(&newer, &[r#"{"user":"newer"}"#], false);
    let (took, entries) = replaced_after(&mut client).await;
    assert_eq!(entries, ["newer"]);
    assert!(took >= RELOCATE_BACKOFF - POLL_EVERY, "{took:?}");
    assert!(took <= RELOCATE_BACKOFF + POLL_EVERY, "{took:?}");
}

/// A record written again with the entries it held (the files were found anew) is a change to
/// nobody: the epoch stays, and what is appended after it goes on as a change of that epoch.
#[tokio::test(start_paused = true)]
async fn a_record_rewritten_with_the_same_entries_keeps_the_epoch() {
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#, r#"{"assistant":"two"}"#]);
    let Event::SessionConversationSubscribed { epoch, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    client.replace(&[r#"{"user":"one"}"#, r#"{"assistant":"two"}"#]);
    client.quiet().await;
    client.assert_nothing_pending();

    client.append(&[r#"{"assistant":"three"}"#]);
    let Event::SessionConversationChanged {
        epoch: same,
        appended,
        ..
    } = client.next().await
    else {
        panic!("a change, not a replacement");
    };
    assert_eq!((same, texts(&appended)), (epoch, vec!["three".to_owned()]));
    assert_eq!(appended[0].seq, 2);
}

/// A read of the record that fails is tried again at the next look and replaces nothing: what was
/// added meanwhile arrives as an addition to the epoch held, once it can be read.
#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn a_read_error_is_retried_at_the_next_look_without_a_replacement() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: a plain query of this process's effective user.
    if unsafe { libc::geteuid() } == 0 {
        // Permissions do not stop the superuser, so no read fails.
        return;
    }
    let mut client = Client::new();
    client.session("s1", Some(AGENT_ID));
    client.write(&[r#"{"user":"one"}"#]);
    let Event::SessionConversationSubscribed { epoch, .. } = client.subscribe("s1", None).await
    else {
        panic!("a subscribed reply");
    };
    client.append(&[r#"{"assistant":"two"}"#]);
    let record = client.record();
    let chmod = move |mode| {
        std::fs::set_permissions(&record, std::fs::Permissions::from_mode(mode)).unwrap()
    };
    chmod(0o000);
    // The mode is put back whatever follows, so that the scratch directory can be deleted.
    let restore = scopeguard(move || chmod(0o644));
    client.quiet().await;
    client.assert_nothing_pending();
    assert_eq!(client.state.conversation_feeds.len(), 1);
    drop(restore);

    let Event::SessionConversationChanged {
        epoch: same,
        appended,
        ..
    } = client.next().await
    else {
        panic!("a change, not a replacement");
    };
    assert_eq!((same, texts(&appended)), (epoch, vec!["two".to_owned()]));
}

/// Runs `restore` when dropped.
#[cfg(unix)]
fn scopeguard<F: FnOnce()>(restore: F) -> impl Drop {
    struct Guard<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for Guard<F> {
        fn drop(&mut self) {
            if let Some(restore) = self.0.take() {
                restore();
            }
        }
    }
    Guard(Some(restore))
}
