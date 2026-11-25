//! The task that fills one followed conversation's feed: it reads the conversation's files, then
//! looks at them again every [`POLL_EVERY`], and ends when the feed is dropped or the session is
//! gone. The reading itself is `records::feed::follow`; this decides when, and finds the files.
//!
//! The files are looked for again at once when the session's own record says they may have moved
//! (its agent session id, account directory or hinted record path changed) or the head is
//! missing, every [`RELOCATE_EVERY`] while none is found, and every [`RELOCATE_BACKOFF`] while
//! the head reads cleanly. A feed that cannot go on (a reader panicked, the blocking task
//! failed) ends: the connections following it drop their subscriptions.
//!
//! A conversation kept as rows (`records::rows`) has no files to look at: it is read whole
//! through the provider when the feed opens, when an event of the session comes in
//! (`Hints::stirred`), when the session's process starts or ends and when the session names
//! another conversation or account, and at no other time. A reading that fails is tried again at
//! the next of those.

use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::time::Instant;

use crate::protocol::{AgentId, Session};
use crate::records::feed::follow::{
    Follower, Lines, Read, POLL_EVERY, RELOCATE_BACKOFF, RELOCATE_EVERY,
};
use crate::records::feed::FeedOut;
use crate::records::{history_branch, Locator, Stretch};
use crate::state::{AppState, FeedChannel};

/// Starts the task for the feed `channel` of its session.
pub(super) fn start(state: &Arc<AppState>, channel: Arc<FeedChannel>) {
    tokio::spawn(run(Arc::clone(state), channel));
}

/// Drops the feed when the task ends, however it does.
struct Finish {
    state: Arc<AppState>,
    channel: Arc<FeedChannel>,
}

impl Drop for Finish {
    fn drop(&mut self) {
        self.state.conversation_feeds.finish(&self.channel);
    }
}

/// What of the session's own record decides where its conversation is looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Whereabouts {
    agent_session_id: Option<String>,
    config_dir: Option<String>,
    hint: Option<PathBuf>,
}

/// What the task holds between two looks.
#[derive(Default)]
struct Following {
    agent: Option<AgentId>,
    follower: Option<Follower>,
    /// The entries the follower's reader works on. Parsing changes this, outside the feed's lock;
    /// the feed is then given what changed ([`FeedChannel::publish`]).
    out: FeedOut,
    /// Where the conversation was last looked for.
    whereabouts: Option<Whereabouts>,
    /// Whether the feed has been given the conversation once, which makes it ready.
    published: bool,
    /// Whether the record's file was found missing, and the files looked for because of it.
    missing: bool,
    /// Whether this look looked for the files again.
    relocated: bool,
    /// For a conversation kept as rows: what its last reading was made at.
    rows_read: Option<RowsRead>,
}

/// What decides when a conversation kept as rows is read again: a change of any of these.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RowsRead {
    agent_session_id: Option<String>,
    config_dir: Option<String>,
    /// How many of the session's events had come in (`Hints::stirred`).
    stirred: u64,
    /// Whether the session had a process.
    live: bool,
}

impl Following {
    /// How long until the files are looked for again, if nothing says sooner.
    fn relocate_after(&self) -> std::time::Duration {
        if self.follower.is_some() && !self.missing {
            RELOCATE_BACKOFF
        } else {
            RELOCATE_EVERY
        }
    }
}

async fn run(state: Arc<AppState>, channel: Arc<FeedChannel>) {
    let _finish = Finish {
        state: Arc::clone(&state),
        channel: Arc::clone(&channel),
    };
    let mut following = Following::default();
    let mut relocate_at = Instant::now();
    while !channel.is_stopped() {
        let relocate = Instant::now() >= relocate_at;
        let step = {
            let state = Arc::clone(&state);
            let channel = Arc::clone(&channel);
            tokio::task::spawn_blocking(move || step(&state, &channel, following, relocate)).await
        };
        match step {
            Ok(Some(next)) => following = next,
            // The session is gone: nothing more to follow.
            Ok(None) => return,
            Err(err) => {
                tracing::warn!(session = %channel.session, %err, "following a conversation failed");
                return;
            }
        }
        if following.relocated {
            relocate_at = Instant::now() + following.relocate_after();
        }
        tokio::time::sleep(POLL_EVERY).await;
    }
}

/// One look: reads what was added, and finds the files again when `relocate` or the session's
/// own record says to. `None` when the session no longer exists, or a reader panicked.
fn step(
    state: &AppState,
    channel: &FeedChannel,
    following: Following,
    relocate: bool,
) -> Option<Following> {
    step_with(state, channel, following, relocate, Locator::default)
}

/// [`step`], resolving the accounts through a locator `locator` makes, one for each search or
/// reading: a listing is not kept from one look to the next.
fn step_with(
    state: &AppState,
    channel: &FeedChannel,
    mut following: Following,
    relocate: bool,
    locator: impl Fn() -> Locator<'static>,
) -> Option<Following> {
    following.relocated = false;
    let mut relocate = relocate;
    if let (Some(agent), Some(follower)) = (following.agent, following.follower.as_mut()) {
        match follower.read_new() {
            Read::Unchanged => following.missing = false,
            Read::Lines(lines) => {
                following.missing = false;
                apply(channel, &mut following, lines)?;
            }
            Read::Restarted => {
                following.missing = false;
                let stretches = follower.stretches().to_vec();
                rebuild(channel, &mut following, agent, stretches)?;
            }
            Read::Missing => {
                // Looked for at once, the first time; after that on the usual interval.
                relocate |= !following.missing;
                following.missing = true;
            }
        }
    }
    // The session's own record: gone ends the feed, a failure to read it is tried again at the
    // next look, and a move of its conversation is acted on at once.
    let session = match state.store.get_session(&channel.session) {
        Ok(Some(session)) => Some(session),
        Ok(None) => return None,
        Err(err) => {
            tracing::warn!(session = %channel.session, %err, "reading a followed session failed");
            None
        }
    };
    if let Some(session) = session
        .as_ref()
        .filter(|session| crate::records::rows::shaped(session.agent).is_some())
    {
        return rows_step(state, channel, following, session, &locator());
    }
    if let Some(session) = &session {
        let whereabouts = Whereabouts {
            agent_session_id: session.agent_session_id.clone(),
            config_dir: session.config_dir.clone(),
            hint: state.record_hints.hint(session),
        };
        if following.whereabouts.as_ref() != Some(&whereabouts) {
            relocate = true;
        }
        if relocate {
            following.whereabouts = Some(whereabouts);
        }
    }
    if let (true, Some(session)) = (relocate, session) {
        following.relocated = true;
        following.agent = Some(session.agent);
        let located = locate(state, &session, locator());
        let unchanged = matches!(
            (&located, &following.follower),
            (Some(stretches), Some(follower)) if stretches.as_slice() == follower.stretches()
        );
        match located {
            Some(stretches) if !unchanged => {
                rebuild(channel, &mut following, session.agent, stretches)?;
            }
            // A session with no record yet is an empty conversation that keeps being looked for.
            None if !following.published => {
                following.out = FeedOut::default();
                channel.replace_with(&mut following.out);
                following.published = true;
            }
            _ => {}
        }
    }
    Some(following)
}

/// Hands the lines a follower read to its reader and publishes what they changed. `None` when
/// the reader panicked, which ends the feed with nothing of what it half-applied published.
fn apply(channel: &FeedChannel, following: &mut Following, lines: Lines) -> Option<()> {
    let follower = following.follower.as_mut()?;
    let out = &mut following.out;
    // Parsed outside the feed's lock, so that no request waits on it.
    let parsed = std::panic::catch_unwind(AssertUnwindSafe(|| follower.apply(lines, out)));
    if parsed.is_err() {
        tracing::error!(session = %channel.session, "a feed reader panicked");
        channel.stop();
        return None;
    }
    channel.publish(&mut following.out);
    Some(())
}

/// One look at a conversation kept as rows: reads it through the provider when something says
/// it may have changed since the last reading (see the module header), and gives the feed what
/// the rows added, or the conversation again from its start. `None` when a reader panicked.
fn rows_step(
    state: &AppState,
    channel: &FeedChannel,
    mut following: Following,
    session: &Session,
    locator: &Locator,
) -> Option<Following> {
    let due = RowsRead {
        agent_session_id: session.agent_session_id.clone(),
        config_dir: session.config_dir.clone(),
        stirred: state.record_hints.stirred(&session.id),
        live: state.live_session(&session.id).is_some(),
    };
    if following.rows_read.as_ref() == Some(&due) {
        return Some(following);
    }
    let same_conversation = following.rows_read.as_ref().is_some_and(|read| {
        read.agent_session_id == due.agent_session_id && read.config_dir == due.config_dir
    });
    following.rows_read = Some(due);
    following.agent = Some(session.agent);
    let rows = crate::records::rows::read(locator, session).and_then(Result::ok);
    let Some(rows) = rows else {
        // A session with no conversation to read yet is an empty one until a reading succeeds.
        if !following.published || !same_conversation {
            following.follower = None;
            following.out = FeedOut::default();
            channel.replace_with(&mut following.out);
            following.published = true;
        }
        return Some(following);
    };
    let read = match following.follower.as_mut().filter(|_| same_conversation) {
        Some(follower) => follower.read_rows(&rows),
        None => Read::Restarted,
    };
    match read {
        Read::Unchanged => {}
        Read::Lines(lines) => apply(channel, &mut following, lines)?,
        Read::Restarted | Read::Missing => {
            let agent = session.agent;
            let Ok(built) =
                std::panic::catch_unwind(AssertUnwindSafe(|| Follower::build_rows(agent, rows)))
            else {
                tracing::error!(session = %channel.session, "a feed reader panicked");
                channel.stop();
                return None;
            };
            following.out = built.out;
            channel.replace_with(&mut following.out);
            following.follower = Some(built.follower);
            following.published = true;
        }
    }
    Some(following)
}

/// Reads the conversation in `stretches` from its start and gives it to the feed. `None` when a
/// reader panicked.
fn rebuild(
    channel: &FeedChannel,
    following: &mut Following,
    agent: AgentId,
    stretches: Vec<Stretch>,
) -> Option<()> {
    let Ok(built) =
        std::panic::catch_unwind(AssertUnwindSafe(|| Follower::build(agent, stretches)))
    else {
        tracing::error!(session = %channel.session, "a feed reader panicked");
        channel.stop();
        return None;
    };
    following.out = built.out;
    channel.replace_with(&mut following.out);
    following.follower = Some(built.follower);
    following.published = true;
    Some(())
}

/// The files `session`'s conversation is read from, when its record is found and whole.
fn locate(state: &AppState, session: &Session, locator: Locator) -> Option<Vec<Stretch>> {
    let mut locator = locator.with_store(&state.store);
    let accounts = state.store.list_accounts().ok()?;
    history_branch(&state.record_hints, &mut locator, &accounts, session)?.stretches
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::adapter::test_provider::{row_reads, ROW_DATABASE};
    use crate::protocol::{ConversationBody, SessionStatus, CONVERSATION_PAGE_LIMIT};
    use crate::records::fixtures::{row_database, ROW_ID, ROW_TURNS};
    use crate::test_support::{console_with_project_and_accounts, project_session, ScratchDir};

    /// A followed session of the test-only provider's row-shaped variant, its account's database
    /// and its feed, looked at one step at a time on the test's own thread.
    struct Followed {
        state: Arc<AppState>,
        dir: ScratchDir,
        session: Session,
        channel: Arc<FeedChannel>,
        following: Option<Following>,
    }

    impl Followed {
        fn new(label: &str) -> Self {
            let (state, dir) = console_with_project_and_accounts(label);
            let mut session = project_session("s1", AgentId::TEST, SessionStatus::Idle);
            session.agent_session_id = Some(ROW_ID.to_owned());
            session.config_dir = Some(dir.join("account").to_string_lossy().into_owned());
            state.store.insert_session(&session).unwrap();
            let (channel, _) = state.conversation_feeds.join("s1");
            Self {
                state,
                dir,
                session,
                channel,
                following: Some(Following::default()),
            }
        }

        fn account(&self) -> std::path::PathBuf {
            self.dir.join("account")
        }

        /// One look, and how many times it asked the provider for the rows.
        fn step(&mut self) -> usize {
            let before = row_reads();
            let following = self.following.take().expect("the feed goes on");
            self.following = step_with(&self.state, &self.channel, following, false, || {
                Locator::with_shell(HashMap::new())
            });
            assert!(self.following.is_some(), "the session is on record");
            row_reads() - before
        }

        /// The feed's epoch and the text of each entry it holds.
        fn feed(&self) -> (u64, Vec<String>) {
            let (snapshot, _) = self
                .channel
                .subscribe(CONVERSATION_PAGE_LIMIT)
                .expect("a live feed");
            let texts = snapshot
                .page
                .entries
                .iter()
                .map(|entry| match &entry.body {
                    ConversationBody::User { text } | ConversationBody::Assistant { text } => {
                        text.clone()
                    }
                    other => format!("{other:?}"),
                })
                .collect();
            (snapshot.epoch, texts)
        }
    }

    const FIRST_TURN: &str = concat!(
        r#"{"id":"a","parent":null,"user":"first"},"#,
        r#"{"id":"b","parent":"a","assistant":"reply one","tokens":100}"#,
    );

    /// A conversation kept as rows is read when the feed opens, when an event of its session
    /// came in, when the session's process starts or ends and when the session names another
    /// conversation, and at no other look: rows that changed with none of those are not seen.
    #[test]
    fn a_row_conversation_is_read_again_only_when_something_says_it_changed() {
        let _declared = crate::providers::test_provider::with_row_records();
        let mut followed = Followed::new("conversation-feed-rows-due");
        let other = r#"{"id":"x","parent":null,"user":"elsewhere"}"#;
        let account = followed.account();
        let database = |rows: &str| {
            row_database(&account, &[(ROW_ID, rows), ("row_other", other)]);
        };

        database(FIRST_TURN);
        assert_eq!(followed.step(), 1);
        let (opened, texts) = followed.feed();
        assert_eq!(texts, ["first", "reply one"]);

        // The rows grew and nothing said so.
        database(ROW_TURNS);
        assert_eq!(followed.step(), 0);
        assert_eq!(followed.step(), 0);
        assert_eq!(followed.feed().1.len(), 2);

        // An event of the session.
        followed.state.record_hints.stir("s1");
        assert_eq!(followed.step(), 1);
        assert_eq!(
            followed.feed(),
            (
                opened,
                ["first", "reply one", "second", "reply two"]
                    .map(str::to_owned)
                    .to_vec()
            )
        );
        assert_eq!(followed.step(), 0);

        // The session's process starts, then ends.
        let live = crate::test_support::idle_stand_in_of("s1", AgentId::TEST);
        followed.state.register_live(live.clone());
        assert_eq!(followed.step(), 1);
        assert_eq!(followed.step(), 0);
        assert!(followed.state.remove_live(&live.clone()));
        assert_eq!(followed.step(), 1);
        assert_eq!(followed.step(), 0);

        // The session names another conversation: that one replaces what the feed held.
        followed.session.agent_session_id = Some("row_other".to_owned());
        followed
            .state
            .store
            .update_session(&followed.session)
            .unwrap();
        assert_eq!(followed.step(), 1);
        let (moved, texts) = followed.feed();
        assert_eq!(texts, ["elsewhere"]);
        assert!(moved > opened);
        assert_eq!(followed.step(), 0);
    }

    /// A reading that fails before any succeeded gives an empty conversation, so the feed is
    /// ready; one that fails after a success keeps what the feed holds. Either is tried again at
    /// the session's next event, not at the next look.
    #[test]
    fn a_row_reading_that_fails_is_empty_at_first_and_keeps_the_feed_later() {
        let _declared = crate::providers::test_provider::with_row_records();
        let mut followed = Followed::new("conversation-feed-rows-failed");

        // No database yet.
        assert_eq!(followed.step(), 1);
        assert!(followed.following.as_ref().unwrap().published);
        assert_eq!(followed.feed().1, Vec::<String>::new());
        assert_eq!(followed.step(), 0);

        row_database(&followed.account(), &[(ROW_ID, FIRST_TURN)]);
        followed.state.record_hints.stir("s1");
        assert_eq!(followed.step(), 1);
        let (read, texts) = followed.feed();
        assert_eq!(texts, ["first", "reply one"]);

        // The database cannot be read for a while.
        std::fs::remove_file(followed.account().join(ROW_DATABASE)).unwrap();
        followed.state.record_hints.stir("s1");
        assert_eq!(followed.step(), 1);
        assert_eq!(followed.feed(), (read, texts));
        assert_eq!(followed.step(), 0);

        // Readable again, grown: the rows added follow the ones held.
        row_database(&followed.account(), &[(ROW_ID, ROW_TURNS)]);
        followed.state.record_hints.stir("s1");
        assert_eq!(followed.step(), 1);
        let (epoch, texts) = followed.feed();
        assert_eq!(epoch, read);
        assert_eq!(texts, ["first", "reply one", "second", "reply two"]);
    }
}
