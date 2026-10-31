//! A session's pages on the report panel, apart from the tools with which it pushes, lists and
//! deletes them: withdrawing every page of a session that has become a worker, the sweep at daemon
//! start of the pages left on workers, and telling the panel which pages are gone.

use std::sync::Arc;

use anyhow::Result;

use crate::protocol::Event;
use crate::state::AppState;

/// Withdraws every page `session_id` pushed, as `delete_pages` with `all` does, for a session that
/// has just lost the report panel's tools by becoming a worker: its pages would stay
/// on the panel, and be submittable, with no way left for it to list or delete them.
pub(crate) fn withdraw_pages(state: &Arc<AppState>, session_id: &str) -> Result<()> {
    // With `None` the store names no id, so its `Err(id)` arm, a named id that is not the
    // session's, cannot happen.
    if let Ok(deleted) = state.store.delete_pages(session_id, None)? {
        announce_deleted(state, session_id, &deleted);
    }
    Ok(())
}

/// Withdraws the pages of every session that has the role of a worker now: those a binding
/// change could not withdraw (a failed withdrawal, a page stored while the binding was being
/// written) and those pushed before pages were withdrawn on binding. Run once when the daemon
/// starts. One session's failure is logged and does not stop the others.
pub(crate) fn withdraw_stray_pages(state: &Arc<AppState>) -> Result<()> {
    for session_id in state.store.session_ids_with_pages_and_worker_role()? {
        if let Err(err) = withdraw_pages(state, &session_id) {
            tracing::warn!(
                session = %session_id, %err, "withdrawing the pages of a worker session failed"
            );
        }
    }
    Ok(())
}

/// Tells the panel which of a session's pages the store removed; nothing when none was.
pub(crate) fn announce_deleted(state: &Arc<AppState>, session_id: &str, deleted: &[String]) {
    if !deleted.is_empty() {
        state.broadcast(Event::PagesDeleted {
            session_id: session_id.to_string(),
            pages: deleted.to_vec(),
        });
    }
}
