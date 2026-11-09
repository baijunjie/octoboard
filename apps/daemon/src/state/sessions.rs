//! The session record's writes through `AppState`, each of which writes only its own columns, and
//! the removal of a record with what is held in memory for it.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;

use super::AppState;
use crate::protocol::{CodedError, Event, Session};
use crate::session::LiveSession;
use crate::store::Store;
use crate::time::now_millis;

/// What a delete of a session came to.
pub enum Deletion {
    Refused,
    /// The record is gone. `process` is the session's process, read under the lock that keeps a
    /// resume from registering a new one, for the caller to end. `session` is the row returned by
    /// the delete statement, including native identity changes made since an earlier read.
    Deleted {
        process: Option<Arc<LiveSession>>,
        session: Box<Session>,
    },
}

impl AppState {
    /// Reads a session record, failing with a message the UI can show when it is gone.
    pub fn session_record(&self, id: &str) -> Result<Session> {
        Ok(self.store.require_session(id)?)
    }

    pub fn publish_session(&self, session: &Session) {
        self.publish_session_by_id(&session.id);
    }

    /// Publishes the session the store holds under `id`, as [`Self::publish_session`] does.
    pub fn publish_session_by_id(&self, id: &str) {
        {
            let _publication = self
                .session_publication
                .lock()
                .expect("session publication lock poisoned");
            // Partial store writes preserve columns owned by other paths. Always publish the
            // stored row so a previously read record cannot blank a terminal title learned in the
            // meantime.
            match self.store.get_session(id) {
                Ok(Some(session)) => self.broadcast_locked(Event::SessionUpserted {
                    session: self.wire_session(session),
                }),
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(session = %id, %err, "reading the session for publication failed")
                }
            }
        }
        if let Ok(Some(session)) = self.store.get_session(id) {
            self.note_sharing_activity(&session);
        }
    }

    /// Writes a whole session record and tells every client about it, for tests to put a session
    /// in a state; see [`Store::update_session`]. A record deleted in the meantime is not written
    /// and not announced: publishing it would leave a ghost row in every client.
    #[cfg(test)]
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let live = session
            .status
            .has_no_process()
            .then(|| self.live_session(&session.id))
            .flatten();
        // Titles and new control snapshots can publish the row independently, so freeze before
        // the database exposes the missing process, and undo that freeze if the write fails.
        let update = || -> Result<_> { Ok(self.store.update_session(session)?) };
        let updated = match live {
            Some(live) => live.freeze_output_for(update)?,
            None => update()?,
        };
        if updated {
            self.publish_session(session);
        }
        Ok(())
    }

    /// Runs `write`, a store write that leaves the session without a process, and tells every
    /// client about the record it returns. A record deleted in the meantime is not announced:
    /// publishing it would leave a ghost row in every client.
    fn write_no_process(
        &self,
        id: &str,
        write: impl FnOnce() -> Result<Option<Session>>,
    ) -> Result<()> {
        // Titles and new control snapshots can publish the row independently, so freeze before
        // the database exposes the missing process, and undo that freeze if the write fails.
        let written = match self.live_session(id) {
            Some(live) => {
                let mut written = None;
                live.freeze_output_for(|| {
                    written = write()?;
                    Ok(written.is_some())
                })?;
                written
            }
            None => write()?,
        };
        if let Some(session) = written {
            self.publish_session(&session);
        }
        Ok(())
    }

    /// Archives the session as of now and tells every client; see [`Store::archive_session`]. A
    /// turn cut short is judged on the agent's own status, and remembered for its late end
    /// ([`Self::end_process_record`]).
    pub fn archive_session_record(&self, id: &str) -> Result<()> {
        self.write_no_process(id, || {
            let ended = self
                .end_process_record(id, |own| self.store.archive_session(id, now_millis(), own))?;
            Ok(ended.map(|(_, session)| session))
        })
    }

    /// Interrupts the session as of now unless it is archived, and tells every client; see
    /// [`Store::interrupt_session`]. A turn cut short is judged and remembered as in
    /// [`Self::archive_session_record`].
    pub fn interrupt_session_record(&self, id: &str) -> Result<()> {
        self.write_no_process(id, || {
            let ended = self.end_process_record(id, |own| {
                self.store.interrupt_session(id, now_millis(), own)
            })?;
            Ok(ended.map(|(_, session)| session))
        })
    }

    /// Writes the session's title and tells every client. Returns the record as it now stands.
    pub fn set_session_title(&self, id: &str, title: &str) -> Result<Session> {
        let session = self
            .store
            .set_session_title(id, title)?
            .ok_or_else(|| CodedError::unknown_session(id))?;
        self.publish_session(&session);
        Ok(session)
    }

    /// Records the account a session runs under and its directory, and tells every client. A
    /// record deleted in the meantime is not written and not announced.
    pub fn set_session_account(
        &self,
        id: &str,
        account_id: Option<&str>,
        config_dir: Option<&str>,
    ) -> Result<()> {
        self.cancel_recoveries(id);
        if let Some(session) = self.store.set_session_account(id, account_id, config_dir)? {
            self.publish_session(&session);
            self.publish_account_sharing();
        }
        Ok(())
    }

    /// Replaces the session's model and effort overrides and tells every client; see
    /// [`Store::set_session_launch_overrides`].
    #[cfg(test)]
    pub fn set_session_launch_overrides(
        &self,
        id: &str,
        model: &Option<Option<String>>,
        effort: &Option<Option<String>>,
    ) -> Result<()> {
        self.cancel_recoveries(id);
        self.store.set_session_launch_overrides(id, model, effort)?;
        self.publish_session_by_id(id);
        Ok(())
    }

    /// Replaces all held launch values in one write and tells every client once.
    pub(crate) fn set_session_launch(
        &self,
        id: &str,
        model: &Option<Option<String>>,
        effort: &Option<Option<String>>,
        compatibility: &BTreeMap<String, bool>,
    ) -> Result<()> {
        self.cancel_recoveries(id);
        self.store
            .set_session_launch(id, model, effort, compatibility)?;
        self.publish_session_by_id(id);
        Ok(())
    }

    /// Deletes the session's record and its saved output if it is archived, and broadcasts
    /// `session_deleted` when it was. `false` means it was not archived, is gone already, or is
    /// being launched right now (a resume in flight). Checked and deleted under the live-sessions
    /// lock (`live`), so a resume cannot claim the session between the two. An archived session
    /// whose process is still exiting is deleted: its exit watcher finds no record and skips.
    pub fn delete_if_archived(&self, id: &str) -> Result<bool> {
        let deletion = self.delete_where(id, |store| Ok(store.delete_session_if_archived(id)?))?;
        Ok(matches!(deletion, Deletion::Deleted { .. }))
    }

    /// Deletes the session's record and its saved output if it is unused (see
    /// `Store::delete_session_if_unused`), and broadcasts `session_deleted` when it was. It is
    /// refused when the session is in use, is gone already, or is being launched right now. The
    /// returned session carries the native identity captured atomically with the deletion.
    pub fn delete_if_unused(&self, id: &str) -> Result<Deletion> {
        self.delete_where(id, |store| Ok(store.delete_session_if_unused(id)?))
    }

    fn delete_where(
        &self,
        id: &str,
        delete: impl FnOnce(&Store) -> Result<Option<Session>>,
    ) -> Result<Deletion> {
        let live = self.live.read().expect("live sessions lock poisoned");
        if live.launching.contains(id) {
            return Ok(Deletion::Refused);
        }
        let Some(session) = delete(&self.store)? else {
            return Ok(Deletion::Refused);
        };
        let process = live.sessions.get(id).cloned();
        drop(live);
        self.forget_deleted(id);
        Ok(Deletion::Deleted {
            process,
            session: Box::new(session),
        })
    }

    /// Deletes the record of a session whose launch failed and tells every client it is gone. The
    /// launch claim is the caller's to hold until this has returned, so nothing else acts on the
    /// session meanwhile. The record goes whatever is bound to it, which is why a session still
    /// starting is never accepted as an owner (`coordinator::bindings::resolve_owner`): nothing
    /// of it ran, so a conversation that binding meant to keep never existed.
    pub fn discard_session(&self, id: &str) -> Result<()> {
        self.store.delete_session(id)?;
        self.forget_deleted(id);
        Ok(())
    }

    /// Records that the session has had a turn, so a later resume reopens the stored conversation
    /// instead of failing on an id the agent never wrote.
    pub fn mark_conversation_started(&self, id: &str) -> Result<()> {
        match self.store.mark_conversation_started(id)? {
            Some(session) => self.publish_session(&session),
            // Already recorded; a session that is gone fails as it always has.
            None => {
                self.session_record(id)?;
            }
        }
        Ok(())
    }

    /// Forgets the conversation of a session whose agent no longer has it, so that its next launch
    /// opens a fresh one, as for a session that has had no turn. The new conversation's id is
    /// recorded by its first hook, as for any session.
    pub fn forget_conversation(&self, id: &str) -> Result<()> {
        let session = self
            .store
            .forget_conversation(id)?
            .ok_or_else(|| CodedError::unknown_session(id))?;
        self.publish_session(&session);
        Ok(())
    }

    pub fn set_agent_session_id(&self, id: &str, agent_session_id: &str) -> Result<()> {
        match self.store.set_agent_session_id(id, agent_session_id)? {
            Some(session) => self.publish_session(&session),
            // Unchanged; a session that is gone fails as it always has.
            None => {
                self.session_record(id)?;
            }
        }
        Ok(())
    }
}
