//! A session's place in the orchestration tree and what hangs on it: its binding to an owner, its
//! lock and the notice of a change of owner.

use super::Result;
use rusqlite::{params, OptionalExtension};

use super::rows::enum_to_text;
use super::Store;
use crate::protocol::{Role, Session, SessionKind, SessionStatus};

impl Store {
    /// Binds the project session `id`, currently bound to `from` (`None` for unbound), to `owner`,
    /// with `role` as its new role, in one statement that matches only while that still holds, so
    /// of two changes racing for it only one lands. It also matches only while `owner` can own
    /// sessions — it is a coordinator — and, with `childless`, only while no session is bound to
    /// `id`: the two conditions that keep orchestration two levels deep cannot be undone by a
    /// binding that lands between the caller's check and this write. Returns the record as it now
    /// stands, or `None` when nothing changed. What else `owner` may be is the caller's to check.
    /// With `owe_notice` the session is marked owed a notice of its new owner
    /// ([`Self::mark_owner_notice`]) in the same statement, so the change and the mark land
    /// together. Any parting note a release left owed is dropped: the release is no longer the
    /// latest change of owner.
    pub fn set_binding(
        &self,
        id: &str,
        from: Option<&str>,
        owner: &str,
        role: Role,
        childless: bool,
        owe_notice: bool,
    ) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET bound_to = ?3, role = ?4,
                    owner_notice_pending = owner_notice_pending + ?8,
                    owner_notice_note = NULL
             WHERE id = ?1 AND kind = ?5 AND bound_to IS ?2
               AND EXISTS (SELECT 1 FROM sessions AS owner WHERE owner.id = ?3
                             AND owner.role = ?6)
               AND (?7 = 0
                    OR NOT EXISTS (SELECT 1 FROM sessions AS bound WHERE bound.bound_to = ?1))",
            params![
                id,
                from,
                owner,
                enum_to_text(&role),
                enum_to_text(&SessionKind::Project),
                enum_to_text(&Role::Coordinator),
                childless,
                owe_notice
            ],
        )
    }

    /// How many sessions are bound to `owner`, in any status, counted in the store.
    pub fn count_bound_to(&self, owner: &str) -> Result<usize> {
        Ok(self.lock().query_row(
            "SELECT COUNT(*) FROM sessions WHERE bound_to = ?1",
            params![owner],
            |row| row.get(0),
        )?)
    }

    /// Unbinds the project session `id`, currently bound to `from`: `bound_to` cleared and the
    /// session made a coordinator, in one statement that matches only while `id` is still bound
    /// to `from`, so of two changes racing for it only one lands. The sessions bound to `id` stay
    /// bound to it. Returns the record as it now stands, or `None` when nothing changed. With
    /// `owe_notice` the session is marked owed a notice ([`Self::mark_owner_notice`]) in the same
    /// statement, and `note`, the releasing owner's parting words, is kept with it
    /// ([`Self::owner_notice_note`]); without, there is nobody to keep it for and it is dropped.
    pub fn clear_binding(
        &self,
        id: &str,
        from: &str,
        owe_notice: bool,
        note: Option<&str>,
    ) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET bound_to = NULL, role = ?6,
                    owner_notice_pending = owner_notice_pending + ?4,
                    owner_notice_note = CASE WHEN ?4 THEN ?5 END
             WHERE id = ?1 AND kind = ?3 AND bound_to IS ?2",
            params![
                id,
                from,
                enum_to_text(&SessionKind::Project),
                owe_notice,
                note,
                enum_to_text(&Role::Coordinator)
            ],
        )
    }

    /// Moves the sessions bound to `from` to `to`, and, with `coordinator_of`, binds `to`, an
    /// unbound project session, to that console session as a coordinator, all in one transaction.
    /// Returns the ids of the sessions moved, or `None` when that binding did not apply (`to` was
    /// bound meanwhile, or `coordinator_of` is not a console session) and nothing changed. What may
    /// own what is the caller's to check.
    pub fn transfer_binding(
        &self,
        from: &str,
        to: &str,
        coordinator_of: Option<&str>,
    ) -> Result<Option<Vec<String>>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        if let Some(owner) = coordinator_of {
            let bound = tx.execute(
                "UPDATE sessions SET bound_to = ?2, role = ?5, owner_notice_note = NULL
                 WHERE id = ?1 AND kind = ?3 AND bound_to IS NULL
                   AND EXISTS (SELECT 1 FROM sessions AS owner WHERE owner.id = ?2 AND owner.kind = ?4)",
                params![
                    to,
                    owner,
                    enum_to_text(&SessionKind::Project),
                    enum_to_text(&SessionKind::Console),
                    enum_to_text(&Role::Coordinator)
                ],
            )?;
            if bound == 0 {
                return Ok(None);
            }
        }
        let moved = tx
            .prepare("SELECT id FROM sessions WHERE bound_to = ?1")?
            .query_map(params![from], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        tx.execute(
            "UPDATE sessions SET bound_to = ?2 WHERE bound_to = ?1",
            params![from, to],
        )?;
        tx.commit()?;
        Ok(Some(moved))
    }

    pub fn set_session_locked(&self, id: &str, locked: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET locked = ?2 WHERE id = ?1",
            params![id, locked],
        )?;
        Ok(())
    }

    /// Locks session `id` unless it is archived, deciding that and writing the lock in one
    /// statement, so a lock cannot land on a session archived a moment before. Returns false when
    /// it refused for that reason; an unknown session changes nothing and is not refused, as
    /// [`Self::set_session_locked`] does.
    pub fn set_session_locked_unless_archived(&self, id: &str) -> Result<bool> {
        let archived_status = enum_to_text(&SessionStatus::Archived);
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE sessions SET locked = 1 WHERE id = ?1 AND status != ?2",
            params![id, archived_status],
        )?;
        if changed != 0 {
            return Ok(true);
        }
        let is_archived = conn
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?1 AND status = ?2",
                params![id, archived_status],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(!is_archived)
    }

    /// Records that session `id` is owed a notice of a change of its owner: it was not running to
    /// be told, and its next launch delivers it. Kept apart from the session record, which clients
    /// read. The notice carries no text, since it is built from the binding at delivery, so a later
    /// change replaces an earlier pending one. Each call counts up, which is what lets a launch
    /// that delivered the notice clear only what it saw ([`Self::clear_owner_notice`]).
    pub fn mark_owner_notice(&self, id: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET owner_notice_pending = owner_notice_pending + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// [`Self::mark_owner_notice`] for a release whose message could not be written to a running
    /// session: the former owner's parting `note` is kept with the mark, in the same statement,
    /// only while the session is still unbound, so a change of owner since leaves no stale note.
    pub fn mark_owner_notice_with_note(&self, id: &str, note: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET owner_notice_pending = owner_notice_pending + 1,
                    owner_notice_note = CASE WHEN bound_to IS NULL THEN ?2 END
             WHERE id = ?1",
            params![id, note],
        )?;
        Ok(())
    }

    /// The count of notices of a change of its owner session `id` is owed, 0 for none (see
    /// [`Self::mark_owner_notice`]).
    pub fn owner_notice_pending(&self, id: &str) -> Result<i64> {
        Ok(self
            .lock()
            .query_row(
                "SELECT owner_notice_pending FROM sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// The parting note a release left with the notice session `id` is owed, if it left one and no
    /// change of owner has come since (see [`Self::clear_binding`]).
    pub fn owner_notice_note(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT owner_notice_note FROM sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    /// Clears the notice owed to session `id`, and the note kept with it, if its count is still
    /// `seen`, the one a launch read before it delivered the notice: a change marked since stays
    /// owed.
    pub fn clear_owner_notice(&self, id: &str, seen: i64) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET owner_notice_pending = 0, owner_notice_note = NULL
             WHERE id = ?1 AND owner_notice_pending = ?2",
            params![id, seen],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::SessionStatus;

    use crate::store::test_support::*;

    #[test]
    fn session_locks_persist_and_guard_every_store_removal() {
        let path = temp_db("session-locked");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_project(&project(false)).unwrap();
        let mut record = session(None);
        record.project_id = Some("project-1".to_string());
        store.insert_session(&record).unwrap();
        assert!(!store.get_session("session-1").unwrap().unwrap().locked);
        store.set_session_locked("session-1", true).unwrap();
        assert!(store
            .set_session_title("session-1", "Renamed")
            .unwrap()
            .is_some());
        assert!(store.get_session("session-1").unwrap().unwrap().locked);
        assert!(store.archive_session("session-1", 1, None).is_err());
        assert!(store.delete_session("session-1").is_err());
        assert!(store.delete_session_if_unused("session-1").is_err());
        assert!(store.delete_project("project-1").is_err());
        assert!(store.delete_console("console-1").is_err());
        assert!(store.get_project("project-1").unwrap().is_some());
        store.set_session_locked("session-1", false).unwrap();
        store.archive_session("session-1", 1, None).unwrap();
        store.set_session_locked("session-1", true).unwrap();
        assert!(store.delete_session_if_archived("session-1").is_err());
        drop(store);
        let store = Store::open(&path).unwrap();
        assert!(store.get_session("session-1").unwrap().unwrap().locked);
        store.set_session_locked("session-1", false).unwrap();
        assert!(store
            .delete_session_if_archived("session-1")
            .unwrap()
            .is_some());
    }

    /// A project session record for the binding tests: unbound unless `bound_to` says otherwise.
    fn project_session(id: &str, bound_to: Option<&str>) -> Session {
        let mut built = session(None);
        built.id = id.to_string();
        built.kind = SessionKind::Project;
        built.bound_to = bound_to.map(str::to_string);
        if bound_to.is_some() {
            built.role = Role::Worker;
        }
        built
    }

    /// A session with no parent is a coordinator after every binding operation: the owner checks
    /// read the stored role alone, so a write that cleared `bound_to` and left a worker behind
    /// would leave a session that owns nothing and is nobody's.
    #[test]
    fn every_session_without_a_parent_is_a_coordinator_after_each_binding_operation() {
        let path = temp_db("unparented-coordinators");
        let store = Store::open(&path).expect("store");
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&session(None)).unwrap();
        for id in ["a", "b"] {
            store.insert_session(&project_session(id, None)).unwrap();
        }
        let check = |step: &str| {
            for session in store.list_sessions().unwrap() {
                if session.bound_to.is_none() {
                    assert_eq!(
                        session.role,
                        Role::Coordinator,
                        "{} after {step}",
                        session.id
                    );
                }
            }
        };
        check("the inserts");
        store
            .set_binding("a", None, "session-1", Role::Worker, false, false)
            .unwrap()
            .expect("bound as a worker");
        check("binding a worker");
        store
            .clear_binding("a", "session-1", false, None)
            .unwrap()
            .expect("released");
        check("releasing it");
        store
            .set_binding("a", None, "session-1", Role::Coordinator, false, false)
            .unwrap()
            .expect("bound as a coordinator");
        check("binding a coordinator");
        store
            .transfer_binding("a", "b", Some("session-1"))
            .unwrap()
            .expect("transferred");
        check("a transfer");
        store
            .clear_binding("b", "session-1", false, None)
            .unwrap()
            .expect("released");
        check("releasing the heir");
    }

    /// A session opened under an owner is inserted only while the owner can still own sessions.
    /// Whichever of the two lands first, a binding of the owner to a project session or the new
    /// session's insert, orchestration stays two levels deep: this is the insert's side of the
    /// race `set_binding`'s `childless` check covers from the other.
    #[test]
    fn a_session_is_inserted_under_an_owner_only_while_the_owner_can_own() {
        let path = temp_db("insert-under-owner");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store
            .insert_session(&session(None))
            .expect("console session");
        store
            .insert_session(&project_session("owner", None))
            .expect("owner");
        store
            .insert_session(&project_session("peer", None))
            .expect("peer");

        // The owner binds itself to a project session after the new session's owner was resolved.
        store
            .set_binding("owner", None, "peer", Role::Worker, true, false)
            .expect("binding")
            .expect("the owner had no sessions of its own");
        assert!(!store
            .insert_session_under_owner(&project_session("child", Some("owner")), "owner")
            .expect("insert"));
        assert!(store.get_session("child").unwrap().is_none());

        // One with no sessions of its own and no project session to answer to can own sessions.
        store
            .insert_session(&project_session("free", None))
            .expect("free");
        assert!(store
            .insert_session_under_owner(&project_session("child", Some("free")), "free")
            .expect("insert"));

        // The other order: with the child in place, the owner can no longer be bound to a project
        // session.
        assert!(store
            .set_binding("free", None, "peer", Role::Worker, true, false)
            .expect("binding")
            .is_none());

        // An owner that is gone owns nothing.
        assert!(!store
            .insert_session_under_owner(&project_session("orphan", Some("nobody")), "nobody")
            .expect("insert"));
    }

    /// Locking is refused only for an archived session, which stays unlocked; a live session is
    /// locked, and an unknown id is not refused.
    #[test]
    fn a_session_is_locked_unless_archived() {
        let path = temp_db("lock-unless-archived");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store
            .insert_session(&project_session("live", None))
            .expect("live");
        let mut archived = project_session("archived", None);
        archived.status = SessionStatus::Archived;
        store.insert_session(&archived).expect("archived");

        assert!(store.set_session_locked_unless_archived("live").unwrap());
        assert!(store.get_session("live").unwrap().unwrap().locked);

        assert!(!store
            .set_session_locked_unless_archived("archived")
            .unwrap());
        assert!(!store.get_session("archived").unwrap().unwrap().locked);

        assert!(store.set_session_locked_unless_archived("nobody").unwrap());
        assert!(store.get_session("nobody").unwrap().is_none());
    }

    /// The owner notice count is marked, read and cleared by session, a clear naming the count it
    /// saw.
    #[test]
    fn the_owner_notice_flag_follows_the_session() {
        let path = temp_db("owner-notice");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_session(&session(None)).expect("session");
        assert_eq!(store.owner_notice_pending("session-1").unwrap(), 0);
        assert_eq!(store.owner_notice_note("session-1").unwrap(), None);
        store.mark_owner_notice("session-1").unwrap();
        let seen = store.owner_notice_pending("session-1").unwrap();
        assert_eq!(seen, 1);
        // A change marked after the launch read the count is not cleared by that launch.
        store.mark_owner_notice("session-1").unwrap();
        store.clear_owner_notice("session-1", seen).unwrap();
        assert_eq!(store.owner_notice_pending("session-1").unwrap(), 2);
        store.clear_owner_notice("session-1", 2).unwrap();
        assert_eq!(store.owner_notice_pending("session-1").unwrap(), 0);
        assert_eq!(store.owner_notice_pending("nobody").unwrap(), 0);
    }

    /// A binding is cleared only while the session is still bound to the owner the caller read,
    /// and the sessions bound to it keep it.
    #[test]
    fn a_binding_is_cleared_only_while_it_is_still_the_one_read() {
        let path = temp_db("clear-binding");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store
            .insert_session(&session(None))
            .expect("console session");
        store
            .insert_session(&project_session("coordinator", None))
            .expect("coordinator");
        store
            .insert_session(&project_session("child", Some("coordinator")))
            .expect("child");
        store
            .set_binding(
                "coordinator",
                None,
                "session-1",
                Role::Coordinator,
                false,
                false,
            )
            .expect("binding")
            .expect("coordinator bound");

        assert!(store
            .clear_binding("coordinator", "elsewhere", false, None)
            .unwrap()
            .is_none());
        assert!(store
            .clear_binding("child", "elsewhere", false, None)
            .unwrap()
            .is_none());
        let cleared = store
            .clear_binding("coordinator", "session-1", true, Some("good luck"))
            .unwrap()
            .expect("cleared");
        assert_eq!((cleared.bound_to, cleared.role), (None, Role::Coordinator));
        // The mark lands with the change, and only with a change that landed.
        assert_eq!(store.owner_notice_pending("coordinator").unwrap(), 1);
        assert_eq!(store.owner_notice_pending("child").unwrap(), 0);
        // The parting note is kept with the mark, for the release that is still the latest change.
        assert_eq!(
            store.owner_notice_note("coordinator").unwrap().as_deref(),
            Some("good luck")
        );
        store
            .set_binding("coordinator", None, "session-1", Role::Worker, false, true)
            .unwrap()
            .expect("bound again");
        assert_eq!(store.owner_notice_pending("coordinator").unwrap(), 2);
        assert_eq!(store.owner_notice_note("coordinator").unwrap(), None);
        // A release with nobody to be told keeps no note.
        assert!(store
            .clear_binding("coordinator", "session-1", false, Some("dropped"))
            .unwrap()
            .is_some());
        assert_eq!(store.owner_notice_note("coordinator").unwrap(), None);
        assert!(store
            .clear_binding("coordinator", "session-1", true, Some("good luck"))
            .unwrap()
            .is_none());
        assert_eq!(store.owner_notice_pending("coordinator").unwrap(), 2);
        // The note goes with the count that delivered it, and stays with a newer mark.
        store
            .set_binding("coordinator", None, "session-1", Role::Worker, false, false)
            .unwrap()
            .expect("bound once more");
        store
            .clear_binding("coordinator", "session-1", true, Some("good luck"))
            .unwrap()
            .expect("released again");
        store.clear_owner_notice("coordinator", 2).unwrap();
        assert_eq!(store.owner_notice_pending("coordinator").unwrap(), 3);
        assert!(store.owner_notice_note("coordinator").unwrap().is_some());
        store.clear_owner_notice("coordinator", 3).unwrap();
        assert_eq!(store.owner_notice_note("coordinator").unwrap(), None);
        // A failed write to a running session marks the notice with the note, while unbound only.
        store
            .mark_owner_notice_with_note("coordinator", "kept")
            .unwrap();
        assert_eq!(
            store.owner_notice_note("coordinator").unwrap().as_deref(),
            Some("kept")
        );
        store
            .set_binding("coordinator", None, "session-1", Role::Worker, false, false)
            .unwrap()
            .expect("bound again");
        store
            .mark_owner_notice_with_note("coordinator", "stale")
            .unwrap();
        assert_eq!(store.owner_notice_note("coordinator").unwrap(), None);
        let child = store.get_session("child").unwrap().unwrap();
        assert_eq!(child.bound_to.as_deref(), Some("coordinator"));
        // A console session is never bound, so nothing to clear.
        assert!(store
            .clear_binding("session-1", "coordinator", false, None)
            .unwrap()
            .is_none());
    }
}
