//! The messages a planned stop leaves queued in the outbox, carried over to the next daemon:
//! written by the daemon that stops to `restart-outbox.json` in the data directory (owner-only,
//! replaced whole) and read and deleted by the next daemon as it starts, before it adopts anything.
//!
//! Once the handover barrier is up no drain writes any more (`super::outbox`), so what is still
//! queued when the terminals have been handed over is exactly what no session has read: each
//! message is saved whole, in its queue's order, with an id that stays the message's across the
//! handover (the stop's token and its ticket), the session it is for, the launch of that session's
//! process and the store row it is kept in, if it is. A message written only in part is never in
//! the queue to be saved: it was reported lost, as a message cut short always is, and is never
//! written again whole. The outbox takes no message once saved, and a delivery asked for then is
//! refused with `daemon_shutting_down`, which the sender may try again once the next daemon is up.
//!
//! The next daemon queues the messages again as it reads them, in order, each id once, and once
//! its adoption is over writes them into each session it adopted with the same launch: the process
//! the messages were queued for. A session that is not running that launch any more gets none of
//! them, as a session whose process ended with messages queued gets none; a message kept in the
//! store stays there for the session's next resume.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::hostfs::replace::{remove_replaced, replace_beside, Permissions};
use crate::state::AppState;

/// The version of the file's format this build writes and reads; a file of another is dropped.
const VERSION: u32 = 1;

/// How long the queues are kept open, once no message is accepted any more, for a drain still
/// writing to put back the message the handover refused (see `Outbox::seal`): such a write is
/// refused at once once the daemon has parted from the holders, but a message written just before
/// may still be waited on to be sent, a few seconds at most.
const SEAL_DRAIN_WAIT: Duration = Duration::from_secs(3);

#[derive(Debug, Serialize, Deserialize)]
struct OutboxFile {
    #[serde(default)]
    version: u32,
    messages: Vec<SavedMessage>,
}

/// One message queued and not yet written, as saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SavedMessage {
    /// The message's id across the handover: the stop's token and the message's ticket.
    id: String,
    /// The session it is queued for.
    session: String,
    /// The launch of the session's process it was queued for.
    launch: String,
    text: String,
    /// The store row it is also kept in (`Store::add_kept_message`), deleted once it is written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kept_row: Option<i64>,
}

/// Reads the file at `path` and deletes it, so that no later start queues its messages again. A
/// file that does not read, or is of another version, gives nothing and is deleted too.
/// **Blocks.**
fn load_and_delete(path: &Path) -> Option<OutboxFile> {
    let content = match std::fs::read(path) {
        Ok(content) => content,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %path.display(), %err, "reading the messages the previous daemon left queued failed");
            }
            return None;
        }
    };
    if let Err(err) = remove_replaced(path) {
        tracing::warn!(path = %path.display(), %err, "removing the messages the previous daemon left queued failed");
    }
    match serde_json::from_slice::<OutboxFile>(&content) {
        Ok(file) if file.version == VERSION => Some(file),
        Ok(file) => {
            tracing::warn!(path = %path.display(), version = file.version, "the messages the previous daemon left queued are of a version this build does not read; they are not written");
            None
        }
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "the messages the previous daemon left queued do not read; they are not written");
            None
        }
    }
}

impl AppState {
    /// Takes what is queued in the outbox, which takes no message from now on, and writes it to
    /// `path` for the next daemon, replacing whatever is there; with nothing queued for a running
    /// session, removes the file. Called by a planned stop once its terminals are handed over, when
    /// no drain begins another write. Returns how many messages were saved. **Blocks**, for up to
    /// [`SEAL_DRAIN_WAIT`] on a drain still writing, and on the file.
    pub fn save_restart_outbox(&self, path: &Path) -> usize {
        let token = uuid::Uuid::new_v4().simple().to_string();
        let mut sealed = self.outbox.seal(SEAL_DRAIN_WAIT);
        sealed.sort_by(|a, b| a.0.cmp(&b.0));
        let mut messages = Vec::new();
        for (session, queue) in sealed {
            let Some(live) = self.live_session(&session) else {
                // Nothing would ever write it: a session with no process takes no queue.
                if !queue.is_empty() {
                    tracing::warn!(session = %session, dropped = queue.len(), "messages queued for a session with no process are not carried to the next daemon");
                }
                continue;
            };
            messages.extend(
                queue
                    .into_iter()
                    .map(|(ticket, text, kept_row)| SavedMessage {
                        id: format!("{token}-{ticket}"),
                        session: session.clone(),
                        launch: live.launch_id.clone(),
                        text,
                        kept_row,
                    }),
            );
        }
        let saved = messages.len();
        let written = if messages.is_empty() {
            remove_replaced(path)
        } else {
            let file = OutboxFile {
                version: VERSION,
                messages,
            };
            serde_json::to_vec(&file)
                .map_err(std::io::Error::other)
                .and_then(|bytes| replace_beside(path, &bytes, Permissions::Private))
        };
        match written {
            Ok(()) => {
                if saved > 0 {
                    tracing::info!(saved, "saved the queued messages for the next daemon");
                }
                saved
            }
            Err(err) => {
                tracing::error!(path = %path.display(), %err, saved, "saving the queued messages for the next daemon failed; they are lost");
                0
            }
        }
    }

    /// Queues again, in order, the messages the previous daemon's planned stop left at `path`,
    /// deleting the file, and writes them once the adoption is over into the sessions adopted with
    /// the launch each was queued for; the rest are forgotten. Called at start, once the adoption
    /// has begun and before anything is adopted. **Blocks** on the file.
    pub fn load_restart_outbox(self: &Arc<Self>, path: &Path) {
        let Some(file) = load_and_delete(path) else {
            return;
        };
        let mut seen = HashSet::new();
        let mut sessions: Vec<(String, String)> = Vec::new();
        for message in file.messages {
            if !seen.insert(message.id.clone()) {
                continue;
            }
            match message.kept_row {
                Some(row) => {
                    self.outbox.push_kept(&message.session, &message.text, row);
                }
                None => {
                    self.outbox.push(&message.session, &message.text);
                }
            }
            let pair = (message.session, message.launch);
            if !sessions.contains(&pair) {
                sessions.push(pair);
            }
        }
        tracing::info!(
            messages = seen.len(),
            "queued the messages the previous daemon left for its successor"
        );
        let state = Arc::clone(self);
        self.after_adoption(move || state.deliver_restart_outbox(&sessions));
    }

    /// Writes what [`Self::load_restart_outbox`] queued into each of `sessions` adopted running
    /// the launch it names, and forgets what is queued for the others.
    fn deliver_restart_outbox(self: &Arc<Self>, sessions: &[(String, String)]) {
        for (session, launch) in sessions {
            let same_launch = self
                .live_session(session)
                .is_some_and(|live| live.launch_id == *launch);
            if !same_launch {
                let dropped = self.outbox.forget(session);
                if dropped > 0 {
                    tracing::warn!(session = %session, dropped, "the session no longer runs the process the previous daemon queued messages for; they are not written");
                }
                continue;
            }
            // A drain goes to a blocking thread of the runtime the adoption ends on.
            if tokio::runtime::Handle::try_current().is_ok() {
                self.retry_pending_outbox(session);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::messaging::delivery::{write_message, WhenBlocked};
    use crate::protocol::{error_code, AgentId, SessionStatus};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, code_of, fake_live_session};

    /// What is queued for a running session at a planned stop is written owner-only, read back in
    /// order, kept rows included, and queued in the next daemon for the session it adopts running
    /// the same launch; reading the file deletes it, and the stopping daemon takes no message
    /// after saving, a delivery asked for then being refused as one to try again.
    #[tokio::test]
    async fn queued_messages_cross_a_planned_stop_in_order_to_the_same_launch() {
        let (state, dir) = app_state("restart-outbox-round-trip");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        state.queue_message("s", "one");
        state.queue_kept_message("s", "kept", 7);
        state.queue_message("s", "two");
        let path = dir.join("restart-outbox.json");
        assert_eq!(state.save_restart_outbox(&path), 3);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "the file is open to others: {mode:o}");
        }
        let refused = write_message(&state, "s", "late", WhenBlocked::Queue)
            .expect_err("a message was taken after the queues were saved");
        assert_eq!(code_of(&refused), error_code::DAEMON_SHUTTING_DOWN);
        assert_eq!(state.queued_count("s"), 0);

        let (next, _next_dir) = app_state("restart-outbox-round-trip-next");
        next.begin_adoption();
        next.load_restart_outbox(&path);
        assert!(!path.exists(), "the file was left for a later start");
        next.register_live(live.clone());
        next.end_adoption();
        assert!(next.outbox.is_row_queued(7));
        assert_eq!(next.take_queued("s"), ["one", "kept", "two"]);
    }

    /// A session the next daemon does not run in the launch the messages were queued for gets
    /// none of them.
    #[tokio::test]
    async fn messages_for_a_launch_no_longer_running_are_forgotten() {
        let (state, dir) = app_state("restart-outbox-gone");
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        state.queue_message("s", "one");
        let path = dir.join("restart-outbox.json");
        assert_eq!(state.save_restart_outbox(&path), 1);

        let (next, _next_dir) = app_state("restart-outbox-gone-next");
        next.begin_adoption();
        next.load_restart_outbox(&path);
        next.end_adoption();
        assert_eq!(next.queued_count("s"), 0);
    }

    /// With nothing queued, a planned stop leaves no file, and removes one an earlier stop left.
    #[tokio::test]
    async fn nothing_queued_leaves_no_file() {
        let (state, dir) = app_state("restart-outbox-empty");
        let path = dir.join("restart-outbox.json");
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(state.save_restart_outbox(&path), 0);
        assert!(!path.exists());
    }
}
