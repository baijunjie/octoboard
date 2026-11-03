//! The report-panel pages a console session's projects have reported.

use super::Result;
use rusqlite::{params, OptionalExtension, Row};

use super::rows::enum_to_text;
use super::Store;
use crate::protocol::{OrchestrationRole, Page};

impl Store {
    pub fn get_page(&self, id: &str) -> Result<Option<Page>> {
        let conn = self.lock();
        let page = conn
            .query_row(
                "SELECT id, session_id, html, created_at
                 FROM pages WHERE id = ?1",
                params![id],
                read_page,
            )
            .optional()?;
        Ok(page)
    }

    /// Stores `page` unless its session is a worker by now (see [`Session::is_worker`]), checked
    /// in the same statement so that a binding written meanwhile cannot leave a page behind it.
    /// Returns whether it was stored.
    ///
    /// [`Session::is_worker`]: crate::protocol::Session::is_worker
    pub fn insert_page_unless_worker(&self, page: &Page) -> Result<bool> {
        let inserted = self.lock().execute(
            "INSERT INTO pages (id, session_id, html, created_at)
             SELECT ?1, ?2, ?3, ?4
             WHERE NOT EXISTS (SELECT 1 FROM sessions
                               WHERE id = ?2 AND orchestration_role = ?5)",
            params![
                page.id,
                page.session_id,
                page.html,
                page.created_at,
                enum_to_text(&OrchestrationRole::Worker),
            ],
        )?;
        Ok(inserted > 0)
    }

    #[cfg(test)]
    pub fn insert_page(&self, page: &Page) -> Result<()> {
        self.lock().execute(
            "INSERT INTO pages (id, session_id, html, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![page.id, page.session_id, page.html, page.created_at],
        )?;
        Ok(())
    }

    /// Oldest first: `created_at` alone can tie at millisecond resolution, so `id` breaks the tie
    /// deterministically rather than leaving the order to SQLite's whim.
    pub fn list_pages(&self, session_id: &str) -> Result<Vec<Page>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, html, created_at
             FROM pages WHERE session_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt
            .query_map(params![session_id], read_page)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Deletes pages of the session `session_id` and returns the ids it removed: the `named` ones,
    /// each once and in the order given, or with `None` every page it has, oldest first. Checked
    /// and deleted in one transaction, so a page two callers delete at once is reported by only
    /// one of them. A named id that is not one of the session's pages (another session's, one
    /// already gone, one that never was) deletes nothing and comes back as the `Err`.
    pub fn delete_pages(
        &self,
        session_id: &str,
        named: Option<&[String]>,
    ) -> Result<std::result::Result<Vec<String>, String>> {
        let conn = self.lock();
        let tx = conn.unchecked_transaction()?;
        let ids: Vec<String> = match named {
            Some(named) => {
                let mut ids: Vec<String> = Vec::new();
                for id in named {
                    if !ids.contains(id) {
                        ids.push(id.clone());
                    }
                }
                ids
            }
            None => {
                let mut stmt = tx.prepare(
                    "SELECT id FROM pages WHERE session_id = ?1 ORDER BY created_at, id",
                )?;
                let ids = stmt
                    .query_map(params![session_id], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<String>>>()?;
                ids
            }
        };
        let mut deleted = Vec::with_capacity(ids.len());
        for id in ids {
            let removed: Option<String> = tx
                .query_row(
                    "DELETE FROM pages WHERE id = ?1 AND session_id = ?2 RETURNING id",
                    params![id, session_id],
                    |row| row.get(0),
                )
                .optional()?;
            match removed {
                Some(removed) => deleted.push(removed),
                // Dropping the transaction rolls back the ones deleted before this one.
                None => return Ok(Err(id)),
            }
        }
        tx.commit()?;
        Ok(Ok(deleted))
    }

    /// The ids of the sessions that have a page and are workers (see [`Session::is_worker`]),
    /// whose pages no tool of theirs can list.
    ///
    /// [`Session::is_worker`]: crate::protocol::Session::is_worker
    pub fn session_ids_with_pages_and_worker_role(&self) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT p.session_id FROM pages p
             JOIN sessions s ON s.id = p.session_id
             WHERE s.orchestration_role = ?1
             ORDER BY p.session_id",
        )?;
        let ids = stmt
            .query_map(params![enum_to_text(&OrchestrationRole::Worker)], |row| {
                row.get(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }

    /// The id of the session's newest page, or `None` if it has none yet. The ordering is the exact
    /// reverse of [`Self::list_pages`]'s, so the two never disagree about which page is newest when
    /// `created_at` ties.
    pub fn newest_page_id(&self, session_id: &str) -> Result<Option<String>> {
        let id = self
            .lock()
            .query_row(
                "SELECT id FROM pages WHERE session_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }
}

fn read_page(row: &Row<'_>) -> rusqlite::Result<Page> {
    Ok(Page {
        id: row.get(0)?,
        session_id: row.get(1)?,
        html: row.get(2)?,
        created_at: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::store::test_support::*;

    /// Pages belong to the console session that pushed them: listing and "the newest" are per
    /// console session even within one console, a tie on `created_at` still resolves to the same
    /// page in both, and deleting a console session takes its pages and no one else's.
    #[test]
    fn pages_are_scoped_to_their_console_session_and_deleted_with_it() {
        let path = temp_db("pages-per-console-session");
        let store = Store::open(&path).expect("store");
        store.insert_console(&console(None, None, None)).unwrap();
        for id in ["session-a", "session-b"] {
            store.insert_session(&console_session(id)).unwrap();
        }
        for page in [
            crate::test_support::test_page("a-1", "session-a", 1),
            crate::test_support::test_page("b-1", "session-b", 2),
            crate::test_support::test_page("a-2", "session-a", 3),
            crate::test_support::test_page("a-3", "session-a", 3),
        ] {
            store.insert_page(&page).unwrap();
        }

        let ids = |owner: &str| -> Vec<String> {
            store
                .list_pages(owner)
                .unwrap()
                .into_iter()
                .map(|page| page.id)
                .collect()
        };
        assert_eq!(ids("session-a"), ["a-1", "a-2", "a-3"]);
        assert_eq!(ids("session-b"), ["b-1"]);
        assert_eq!(
            store.newest_page_id("session-a").unwrap().as_deref(),
            Some("a-3")
        );
        assert_eq!(
            store.newest_page_id("session-b").unwrap().as_deref(),
            Some("b-1")
        );

        store.delete_session("session-a").unwrap();
        assert!(ids("session-a").is_empty());
        assert_eq!(ids("session-b"), ["b-1"]);

        store.delete_console("console-1").unwrap();
        assert!(ids("session-b").is_empty());
        assert!(store.get_page("b-1").unwrap().is_none());
    }

    /// `delete_pages` checks and removes in one transaction: it reports only what it removed, a
    /// foreign or already-deleted id rolls the whole call back, and a repeated delete of the same
    /// page reports it once.
    #[test]
    fn delete_pages_reports_only_what_it_removed_and_rolls_back_on_a_foreign_id() {
        let path = temp_db("delete-pages");
        let store = Store::open(&path).expect("store");
        store.insert_console(&console(None, None, None)).unwrap();
        for id in ["session-a", "session-b"] {
            store.insert_session(&console_session(id)).unwrap();
        }
        for page in [
            crate::test_support::test_page("a-1", "session-a", 1),
            crate::test_support::test_page("a-2", "session-a", 2),
            crate::test_support::test_page("b-1", "session-b", 3),
        ] {
            store.insert_page(&page).unwrap();
        }
        let named = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();

        // A foreign id after an own one undoes the own one.
        let refused = store
            .delete_pages("session-a", Some(&named(&["a-1", "b-1"])))
            .unwrap();
        assert_eq!(refused, Err("b-1".to_string()));
        assert_eq!(store.list_pages("session-a").unwrap().len(), 2);
        assert_eq!(store.list_pages("session-b").unwrap().len(), 1);

        // A repeated id is removed, and reported, once.
        let done = store
            .delete_pages("session-a", Some(&named(&["a-2", "a-2"])))
            .unwrap();
        assert_eq!(done, Ok(named(&["a-2"])));
        // The same delete again finds it gone, so a second caller does not report it too.
        let again = store
            .delete_pages("session-a", Some(&named(&["a-2"])))
            .unwrap();
        assert_eq!(again, Err("a-2".to_string()));

        assert_eq!(
            store.delete_pages("session-a", None).unwrap(),
            Ok(named(&["a-1"]))
        );
        assert_eq!(store.delete_pages("session-a", None).unwrap(), Ok(vec![]));
        assert_eq!(store.list_pages("session-b").unwrap().len(), 1);
    }
}
