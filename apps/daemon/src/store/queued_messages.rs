//! The session composer's queue of the user's unsent messages, and each session's default send
//! mode. A queue is small (`crate::state::composer::MAX_QUEUED`), so a change writes the
//! session's whole queue in one transaction: its rows are its own, and nothing else writes them.

use std::collections::{BTreeMap, HashMap};

use rusqlite::params;

use super::rows::{enum_from_row, enum_to_text};
use super::{Result, Store};
use crate::protocol::{
    QueuedMessage, QueuedMessageFailure, QueuedMessageState, QueuedMode, SendMode,
};

impl Store {
    /// Every session's composer queue, each in queue order, for the daemon to keep in memory from
    /// its start. A message is `waiting` as it is loaded: a hand-over a daemon was making when it
    /// stopped is gone with it.
    pub fn load_queued_messages(&self) -> Result<BTreeMap<String, Vec<QueuedMessage>>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT session_id, id, text, images, files, command, mode, failure, created_at
             FROM queued_messages ORDER BY session_id, position",
        )?;
        let rows = stmt.query_map([], |row| {
            let session: String = row.get(0)?;
            let images: String = row.get(3)?;
            let files: String = row.get(4)?;
            let failure: Option<String> = row.get(7)?;
            Ok((
                session,
                QueuedMessage {
                    id: row.get(1)?,
                    text: row.get(2)?,
                    images: serde_json::from_str(&images).unwrap_or_default(),
                    files: serde_json::from_str(&files).unwrap_or_default(),
                    command: row.get(5)?,
                    mode: enum_from_row::<QueuedMode>(row, 6)?,
                    state: QueuedMessageState::Waiting,
                    failure: failure
                        .and_then(|text| serde_json::from_str::<QueuedMessageFailure>(&text).ok()),
                    created_at: row.get(8)?,
                },
            ))
        })?;
        let mut queues: BTreeMap<String, Vec<QueuedMessage>> = BTreeMap::new();
        for row in rows {
            let (session, message) = row?;
            queues.entry(session).or_default().push(message);
        }
        Ok(queues)
    }

    /// The paths each queued message's own create or update moved into its session's composer
    /// directory, by message id; a message with none is left out.
    pub fn load_queued_owned(&self) -> Result<HashMap<String, Vec<String>>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, owned FROM queued_messages")?;
        let rows = stmt.query_map([], |row| {
            let owned: String = row.get(1)?;
            Ok((
                row.get::<_, String>(0)?,
                serde_json::from_str::<Vec<String>>(&owned).unwrap_or_default(),
            ))
        })?;
        let mut owned = HashMap::new();
        for row in rows {
            let (id, paths) = row?;
            if !paths.is_empty() {
                owned.insert(id, paths);
            }
        }
        Ok(owned)
    }

    /// Writes `messages` as the session's whole composer queue, in their order, with no files
    /// owned; see [`Self::replace_queued_messages_owning`].
    #[cfg(test)]
    pub fn replace_queued_messages(
        &self,
        session_id: &str,
        messages: &[QueuedMessage],
    ) -> Result<()> {
        self.replace_queued_messages_owning(session_id, messages, &HashMap::new())
    }

    /// Writes `messages` as the session's whole composer queue, in their order, each with the
    /// paths `owned` holds for it, in one transaction. A session that is gone has no rows
    /// written: the reference refuses them, and that is answered as the store answers any write
    /// it refuses.
    pub fn replace_queued_messages_owning(
        &self,
        session_id: &str,
        messages: &[QueuedMessage],
        owned: &HashMap<String, Vec<String>>,
    ) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM queued_messages WHERE session_id = ?1",
            params![session_id],
        )?;
        for (position, message) in messages.iter().enumerate() {
            tx.execute(
                "INSERT INTO queued_messages
                    (id, session_id, position, text, images, files, command, mode, failure,
                     created_at, owned)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    message.id,
                    session_id,
                    position as i64,
                    message.text,
                    serde_json::to_string(&message.images)?,
                    serde_json::to_string(&message.files)?,
                    message.command,
                    enum_to_text(&message.mode),
                    message
                        .failure
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    message.created_at,
                    serde_json::to_string(owned.get(&message.id).map_or(&[][..], Vec::as_slice))?,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Sets the session's default send mode for new messages. Says whether the session exists.
    pub fn set_session_send_mode(&self, id: &str, mode: SendMode) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE sessions SET send_mode = ?2 WHERE id = ?1",
            params![id, enum_to_text(&mode)],
        )?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::QueuedMessageFailureKind;
    use crate::store::test_support::{console, session, temp_db};

    const SESSION: &str = "session-1";

    fn store_with_session(name: &str) -> (Store, crate::test_support::ScratchFile) {
        let path = temp_db(name);
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_session(&session(None)).expect("session");
        (store, path)
    }

    fn message(id: &str, mode: QueuedMode) -> QueuedMessage {
        QueuedMessage {
            id: id.to_string(),
            text: format!("text of {id}"),
            images: vec!["/a.png".to_string()],
            files: vec!["src/main.rs".to_string()],
            command: false,
            mode,
            state: QueuedMessageState::Sending,
            failure: None,
            created_at: 5,
        }
    }

    /// A queue written is read back in its order, each message waiting whatever it was, and a
    /// later write replaces it whole.
    #[test]
    fn a_queue_is_written_whole_and_read_back_waiting_in_order() {
        let (store, _dir) = store_with_session("queued-messages-roundtrip");
        let mut second = message("b", QueuedMode::Manual);
        second.failure = Some(QueuedMessageFailure {
            kind: QueuedMessageFailureKind::NotDelivered,
            code: "message_not_taken".to_string(),
            params: [("detail".to_string(), "x".to_string())].into(),
            message: "not taken".to_string(),
        });
        store
            .replace_queued_messages(
                SESSION,
                &[message("a", QueuedMode::WhenIdle), second.clone()],
            )
            .unwrap();
        let loaded = store.load_queued_messages().unwrap();
        let queue = &loaded[SESSION];
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].id, "a");
        assert_eq!(queue[0].state, QueuedMessageState::Waiting);
        assert_eq!(queue[1].failure, second.failure);
        assert_eq!(queue[1].files, ["src/main.rs"]);

        store
            .replace_queued_messages(SESSION, &[message("b", QueuedMode::Manual)])
            .unwrap();
        let loaded = store.load_queued_messages().unwrap();
        assert_eq!(loaded[SESSION].len(), 1);
        store.replace_queued_messages(SESSION, &[]).unwrap();
        assert!(store.load_queued_messages().unwrap().is_empty());
    }

    /// Deleting the session takes its queue; its send mode is stored with it.
    #[test]
    fn the_send_mode_is_stored_and_the_queue_goes_with_the_session() {
        let (store, _dir) = store_with_session("queued-messages-cascade");
        assert_eq!(
            store.require_session(SESSION).unwrap().send_mode,
            SendMode::Now
        );
        assert!(store
            .set_session_send_mode(SESSION, SendMode::Manual)
            .unwrap());
        assert_eq!(
            store.require_session(SESSION).unwrap().send_mode,
            SendMode::Manual
        );
        assert!(!store
            .set_session_send_mode("gone", SendMode::Manual)
            .unwrap());
        store
            .replace_queued_messages(SESSION, &[message("a", QueuedMode::WhenIdle)])
            .unwrap();
        store.delete_session(SESSION).unwrap();
        assert!(store.load_queued_messages().unwrap().is_empty());
    }
}
