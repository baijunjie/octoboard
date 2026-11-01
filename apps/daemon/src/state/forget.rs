//! What is dropped when a session, a project or a record is removed: each registry's entry
//! for it, the saved output and the handoff package.

use super::AppState;
use crate::protocol::Event;

impl AppState {
    /// What follows a session's record being deleted: dropping what the daemon holds for it and
    /// telling every client.
    pub(super) fn forget_deleted(&self, id: &str) {
        self.forget_held_for_session(id);
        self.broadcast(Event::SessionDeleted {
            session: id.to_string(),
        });
    }

    /// What follows sessions' records going with their console or project, which the store
    /// cascades: dropping what the daemon holds for each. No event is sent for them, since every
    /// client drops a console's or project's sessions on its own deletion event.
    pub fn forget_cascaded_sessions(&self, ids: &[impl AsRef<str>]) {
        for id in ids {
            self.forget_held_for_session(id.as_ref());
        }
    }

    /// What follows projects' records being deleted, on their own or with their console: dropping
    /// what the daemon holds for each and stopping the watch on its repository.
    pub fn forget_deleted_projects(&self, ids: &[impl AsRef<str>]) {
        for id in ids {
            let id = id.as_ref();
            self.remove_git_status(id);
            self.unresolved_removals.forget(id);
            self.path_link_roots.forget(id);
            crate::project_git::watch::remove(self, id);
        }
    }

    /// What the daemon holds in memory for a session whose record is gone, whichever way it went.
    fn forget_held_for_session(&self, id: &str) {
        self.forget_saved_output(id);
        self.forget_left_for_limit(id);
        self.forget_sharing_activity(id);
        self.forget_handoff_package(id);
        self.forget_request_hand(id);
    }

    pub(super) fn forget_session_bookkeeping(&self, id: &str) {
        self.quota_recovery.cancel(id);
        self.revoke_mcp_tokens(id);
        self.turns.lock().expect("turn lock poisoned").remove(id);
        self.forget_request_hand(id);
        self.background_tasks
            .lock()
            .expect("background tasks lock poisoned")
            .remove(id);
        self.record_watch_generations
            .lock()
            .expect("record watch lock poisoned")
            .remove(id);
        self.record_offsets
            .lock()
            .expect("record offset lock poisoned")
            .remove(id);
    }

    /// Removes the handoff package written for a session, which is named by the session's id. Called
    /// wherever a record is deleted; a session that was never a receiving one has none. A failure
    /// is logged: the record is gone either way.
    pub fn forget_handoff_package(&self, id: &str) {
        if let Err(err) = crate::handoff::package_file::remove_in(self.handoff_dir(), id) {
            tracing::warn!(session = %id, %err, "removing a handoff package failed");
        }
    }
}
