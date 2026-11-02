//! Messages held for interrupted sessions until they are resumed and the message is written into
//! them.

use super::Result;
use rusqlite::params;

use super::Store;

impl Store {
    /// Holds `text`, a message for the interrupted session `session_id`, until it is resumed and
    /// the message has been written into it. Returns the row it is kept in.
    pub fn add_kept_message(&self, session_id: &str, text: &str) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO kept_messages (session_id, text) VALUES (?1, ?2)",
            params![session_id, text],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// What is held for the session with the row each is kept in, oldest first.
    pub fn kept_messages(&self, session_id: &str) -> Result<Vec<(i64, String)>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT id, text FROM kept_messages WHERE session_id = ?1 ORDER BY id")?;
        let rows = stmt
            .query_map(params![session_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Drops the held messages in these rows, which have been written into their session, in one
    /// transaction.
    pub fn delete_kept_messages(&self, rows: &[i64]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        for row in rows {
            tx.execute("DELETE FROM kept_messages WHERE id = ?1", params![row])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Drops everything held for the session, which will not take it.
    pub fn clear_kept_messages(&self, session_id: &str) -> Result<()> {
        self.lock().execute(
            "DELETE FROM kept_messages WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }
}
