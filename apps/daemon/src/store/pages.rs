//! The report-panel pages sessions have pushed, and the submission of each one's form.

use super::Result;
use rusqlite::{params, OptionalExtension, Row};

use super::rows::enum_to_text;
use super::Store;
use crate::protocol::{Page, Role, Submission};

/// What [`Store::mark_page_submitted`] came to.
#[derive(Debug, PartialEq, Eq)]
pub enum PageMark {
    /// Marked, at this revision of its submission (`Page::submission_revision`).
    Marked(i64),
    /// There is no such page.
    Unknown,
    /// A newer page of its session has taken its place.
    NotCurrent,
    /// Its form was submitted already.
    AlreadySubmitted,
}

impl Store {
    pub fn get_page(&self, id: &str) -> Result<Option<Page>> {
        let conn = self.lock();
        let page = conn
            .query_row(
                "SELECT id, session_id, content, submission, submission_revision, created_at
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
            "INSERT INTO pages (id, session_id, content, submission, submission_revision,
                                created_at)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6
             WHERE NOT EXISTS (SELECT 1 FROM sessions
                               WHERE id = ?2 AND role = ?7)",
            params![
                page.id,
                page.session_id,
                serde_json::to_string(&page.content)?,
                page.submission
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                page.submission_revision,
                page.created_at,
                enum_to_text(&Role::Worker),
            ],
        )?;
        Ok(inserted > 0)
    }

    #[cfg(test)]
    pub fn insert_page(&self, page: &Page) -> Result<()> {
        self.lock().execute(
            "INSERT INTO pages (id, session_id, content, submission, submission_revision,
                                created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                page.id,
                page.session_id,
                serde_json::to_string(&page.content)?,
                page.submission
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                page.submission_revision,
                page.created_at,
            ],
        )?;
        Ok(())
    }

    /// Oldest first: `created_at` alone can tie at millisecond resolution, so `id` breaks the tie
    /// deterministically rather than leaving the order to SQLite's whim.
    pub fn list_pages(&self, session_id: &str) -> Result<Vec<Page>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, content, submission, submission_revision, created_at
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
             WHERE s.role = ?1
             ORDER BY p.session_id",
        )?;
        let ids = stmt
            .query_map(params![enum_to_text(&Role::Worker)], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }

    /// Marks the page as submitted with `submission`, in one statement that also requires it to be
    /// the newest page of its session and not to be submitted yet, so that of several submissions
    /// racing for it exactly one is marked, and that raises its submission revision. Says why a
    /// submission was not marked.
    pub fn mark_page_submitted(&self, id: &str, submission: &Submission) -> Result<PageMark> {
        let conn = self.lock();
        let marked: Option<i64> = conn
            .query_row(
                "UPDATE pages SET submission = ?2, submission_revision = submission_revision + 1
                 WHERE id = ?1 AND submission IS NULL
                   AND id = (SELECT id FROM pages AS newest
                             WHERE newest.session_id = pages.session_id
                             ORDER BY created_at DESC, id DESC LIMIT 1)
                 RETURNING submission_revision",
                params![id, serde_json::to_string(submission)?],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(revision) = marked {
            return Ok(PageMark::Marked(revision));
        }
        // Read under the same hold of the connection, so that the reason is the state that refused.
        let state: Option<(bool, bool)> = conn
            .query_row(
                "SELECT submission IS NOT NULL,
                        id = (SELECT id FROM pages AS newest
                              WHERE newest.session_id = pages.session_id
                              ORDER BY created_at DESC, id DESC LIMIT 1)
                 FROM pages WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(match state {
            None => PageMark::Unknown,
            Some((_, false)) => PageMark::NotCurrent,
            Some(_) => PageMark::AlreadySubmitted,
        })
    }

    /// Takes the submission mark set at `revision` off the page again, for a delivery that
    /// certainly failed, and raises the revision. Returns the revision of the cleared submission,
    /// or `None` when that mark is no longer there to take: the page was deleted meanwhile, or its
    /// submission has changed since, which a stale clearing must not undo.
    pub fn clear_page_submission(&self, id: &str, revision: i64) -> Result<Option<i64>> {
        let cleared = self
            .lock()
            .query_row(
                "UPDATE pages SET submission = NULL, submission_revision = submission_revision + 1
                 WHERE id = ?1 AND submission IS NOT NULL AND submission_revision = ?2
                 RETURNING submission_revision",
                params![id, revision],
                |row| row.get(0),
            )
            .optional()?;
        Ok(cleared)
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
    let unreadable = |index: usize, err: serde_json::Error| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, err.into())
    };
    let content: String = row.get(2)?;
    let submission: Option<String> = row.get(3)?;
    Ok(Page {
        id: row.get(0)?,
        session_id: row.get(1)?,
        content: serde_json::from_str(&content).map_err(|err| unreadable(2, err))?,
        submission: submission
            .map(|text| serde_json::from_str(&text))
            .transpose()
            .map_err(|err| unreadable(3, err))?,
        submission_revision: row.get(4)?,
        created_at: row.get(5)?,
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

    /// A page's content and its submission are stored as JSON and read back as they were, the
    /// order of a submission's fields included.
    #[test]
    fn a_pages_content_and_submission_round_trip() {
        let path = temp_db("page-round-trip");
        let store = Store::open(&path).expect("store");
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&console_session("session-a")).unwrap();
        let content = crate::protocol::PageContent::parse(&serde_json::json!({
            "title": "T",
            "body": [
                { "type": "number", "name": "n", "label": "N", "min": 1, "max": 2.5, "step": 0.5 },
                { "type": "submit" },
            ],
        }))
        .unwrap();
        let page = Page {
            content,
            ..crate::test_support::test_page("a-1", "session-a", 1)
        };
        assert!(store.insert_page_unless_worker(&page).unwrap());

        let read = store.get_page("a-1").unwrap().unwrap();
        assert_eq!(read.content, page.content);
        assert!(read.submission.is_none());
        let submission = serde_json::json!({ "z": "last", "a": ["x", "y"], "n": 2 });
        let submission = submission.as_object().unwrap();
        assert_eq!(
            store.mark_page_submitted("a-1", submission).unwrap(),
            PageMark::Marked(1)
        );
        let read = store.get_page("a-1").unwrap().unwrap();
        assert_eq!(read.submission.as_ref(), Some(submission));
        assert_eq!(read.submission_revision, 1);
        assert_eq!(
            read.submission.unwrap().keys().collect::<Vec<_>>(),
            ["z", "a", "n"]
        );
        assert_eq!(
            store.list_pages("session-a").unwrap()[0]
                .submission
                .as_ref(),
            Some(submission)
        );
    }

    /// A page is marked submitted only once, and only while it is its session's newest; the store
    /// says which of the two refused, and a cleared mark can be set again. Each mark and each
    /// clearing raises the page's submission revision.
    #[test]
    fn a_page_is_marked_submitted_once_and_only_while_newest() {
        let path = temp_db("page-mark");
        let store = Store::open(&path).expect("store");
        store.insert_console(&console(None, None, None)).unwrap();
        for id in ["session-a", "session-b"] {
            store.insert_session(&console_session(id)).unwrap();
        }
        for page in [
            crate::test_support::test_page("old", "session-a", 1),
            crate::test_support::test_page("new", "session-a", 2),
            crate::test_support::test_page("other", "session-b", 0),
        ] {
            store.insert_page(&page).unwrap();
        }
        let sent = serde_json::json!({ "who": "Ada" });
        let sent = sent.as_object().unwrap();

        assert_eq!(
            store.mark_page_submitted("old", sent).unwrap(),
            PageMark::NotCurrent
        );
        assert_eq!(
            store.mark_page_submitted("nope", sent).unwrap(),
            PageMark::Unknown
        );
        assert!(store.get_page("old").unwrap().unwrap().submission.is_none());
        // Another session's newer page does not make this one stale.
        assert_eq!(
            store.mark_page_submitted("other", sent).unwrap(),
            PageMark::Marked(1)
        );
        assert_eq!(
            store.mark_page_submitted("new", sent).unwrap(),
            PageMark::Marked(1)
        );
        assert_eq!(
            store.mark_page_submitted("new", sent).unwrap(),
            PageMark::AlreadySubmitted
        );
        // A page that is both superseded and submitted reads as superseded.
        store
            .insert_page(&crate::test_support::test_page("newer", "session-a", 3))
            .unwrap();
        assert_eq!(
            store.mark_page_submitted("new", sent).unwrap(),
            PageMark::NotCurrent
        );

        assert_eq!(store.clear_page_submission("other", 1).unwrap(), Some(2));
        assert_eq!(store.clear_page_submission("other", 1).unwrap(), None);
        assert_eq!(
            store.mark_page_submitted("other", sent).unwrap(),
            PageMark::Marked(3)
        );
        assert_eq!(
            store
                .get_page("other")
                .unwrap()
                .unwrap()
                .submission_revision,
            3
        );
    }

    /// Clearing takes off only the mark it names by revision: a clearing that comes late, after
    /// the page was cleared and marked again, leaves the newer mark and its revision as they are.
    #[test]
    fn a_late_clearing_does_not_take_off_a_newer_mark() {
        let path = temp_db("page-late-clear");
        let store = Store::open(&path).expect("store");
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&console_session("session-a")).unwrap();
        store
            .insert_page(&crate::test_support::test_page("form", "session-a", 1))
            .unwrap();
        let first = serde_json::json!({ "who": "Ada" });
        let second = serde_json::json!({ "who": "Grace" });

        assert_eq!(
            store
                .mark_page_submitted("form", first.as_object().unwrap())
                .unwrap(),
            PageMark::Marked(1)
        );
        assert_eq!(store.clear_page_submission("form", 1).unwrap(), Some(2));
        assert_eq!(
            store
                .mark_page_submitted("form", second.as_object().unwrap())
                .unwrap(),
            PageMark::Marked(3)
        );
        // The first submission's clearing, run again late, names a mark that is gone.
        assert_eq!(store.clear_page_submission("form", 1).unwrap(), None);
        let page = store.get_page("form").unwrap().unwrap();
        assert_eq!(page.submission.as_ref(), second.as_object());
        assert_eq!(page.submission_revision, 3);
        // A clearing of the mark that is there still works.
        assert_eq!(store.clear_page_submission("form", 3).unwrap(), Some(4));
    }
}
