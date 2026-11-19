//! The task that fills one followed conversation's feed: it reads the conversation's files, then
//! looks at them again every [`POLL_EVERY`], and ends when the feed is dropped or the session is
//! gone. The reading itself is `records::feed::follow`; this decides when, and finds the files.
//!
//! The files are looked for again at once when the session's own record says they may have moved
//! (its agent session id, account directory or hinted record path changed) or the head is
//! missing, every [`RELOCATE_EVERY`] while none is found, and every [`RELOCATE_BACKOFF`] while
//! the head reads cleanly. A feed that cannot go on (a reader panicked, the blocking task
//! failed) ends: the connections following it drop their subscriptions.

use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::time::Instant;

use crate::protocol::{AgentId, Session};
use crate::records::feed::follow::{Follower, Read, POLL_EVERY, RELOCATE_BACKOFF, RELOCATE_EVERY};
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
struct Following {
    agent: Option<AgentId>,
    follower: Option<Follower>,
    /// Where the conversation was last looked for.
    whereabouts: Option<Whereabouts>,
    /// Whether the feed has been given the conversation once, which makes it ready.
    published: bool,
    /// Whether the record's file was found missing, and the files looked for because of it.
    missing: bool,
    /// Whether this look looked for the files again.
    relocated: bool,
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
    let mut following = Following {
        agent: None,
        follower: None,
        whereabouts: None,
        published: false,
        missing: false,
        relocated: false,
    };
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
    mut following: Following,
    relocate: bool,
) -> Option<Following> {
    following.relocated = false;
    let mut relocate = relocate;
    if let (Some(agent), Some(follower)) = (following.agent, following.follower.as_mut()) {
        match follower.read_new() {
            Read::Unchanged => following.missing = false,
            Read::Lines(lines) => {
                following.missing = false;
                // `FeedChannel::apply` catches a reader's panic; the feed then ends.
                let applied = channel.apply(|out| follower.apply(lines, out));
                if applied.is_err() {
                    tracing::error!(session = %channel.session, "a feed reader panicked");
                    return None;
                }
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
        let located = locate(state, &session);
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
                channel.replace_with(FeedOut::default());
                following.published = true;
            }
            _ => {}
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
        return None;
    };
    channel.replace_with(built.out);
    following.follower = Some(built.follower);
    following.published = true;
    Some(())
}

/// The files `session`'s conversation is read from, when its record is found and whole.
fn locate(state: &AppState, session: &Session) -> Option<Vec<Stretch>> {
    let accounts = state.store.list_accounts().ok()?;
    history_branch(
        &state.record_hints,
        &mut Locator::default(),
        &accounts,
        session,
    )?
    .stretches
}
