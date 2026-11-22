//! Moving the agent's own records of deleted sessions to the Trash: the one place the daemon's Trash
//! is taken for them, and the move itself. Whose records may move is the caller's to settle
//! (`RecordsConsent` in the parent module); this is the registered exception of P9 for the records
//! of a session that never had a conversation, and for a deletion whose option the user ticked
//! (`docs/product/deleting-sessions.md`, "Moving the agent's own records to the Trash").

use std::path::{Path, PathBuf};

use crate::hostfs::trash::TrashFailure;
use crate::protocol::Session;
use crate::records;
use crate::state::AppState;

/// The system Trash, as the records of deleted sessions are moved through it outside a test.
pub(in crate::coordinator) fn host_trash(
    state: &AppState,
) -> fn(&Path) -> Result<(), TrashFailure> {
    state.host_tools.get().trash
}

/// Moves the agent's own records of each of `deleted` that deleting it may remove (see
/// `records::removable_record_paths`) through `trash`. The sessions are gone from the store, so
/// they no longer count as holding the conversation with another session. Whose records may move
/// at all is settled before this (`trash_agent_records_with`). A session is left alone when another
/// remaining session refers to the same agent session id (a resumed copy, say), and a path that is
/// missing is skipped. The records' loss costs the session its resumption in the agent only, so a
/// failure to find or move one is logged, never raised. The accounts' records are listed once for
/// the whole call, through `locator`.
///
/// A process that was stopped by the deletion may still rewrite its transcript after the move,
/// leaving a small file behind in the agent's directory; that is accepted.
pub(super) fn trash_agent_records_of(
    state: &AppState,
    deleted: &[Session],
    mut locator: records::Locator,
    trash: impl Fn(&Path) -> Result<(), TrashFailure>,
) {
    let (remaining, accounts) = match (state.store.list_sessions(), state.store.list_accounts()) {
        (Ok(sessions), Ok(accounts)) => (sessions, accounts),
        (Err(err), _) | (_, Err(err)) => {
            tracing::warn!(%err, "reading the sessions before trashing the agent's records failed");
            return;
        }
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for session in deleted {
        let Some(agent_id) = &session.agent_session_id else {
            continue;
        };
        let shared = remaining.iter().any(|other| {
            other.agent == session.agent
                && other.agent_session_id.as_ref() == Some(agent_id)
                && !deleted.iter().any(|gone| gone.id == other.id)
        });
        if shared {
            continue;
        }
        for path in
            records::removable_record_paths(&state.record_hints, &mut locator, &accounts, session)
        {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    for path in paths {
        if path.symlink_metadata().is_err() {
            continue;
        }
        if let Err(err) = trash(&path) {
            tracing::warn!(path = %path.display(), %err, "moving the agent's session record to the Trash failed");
        }
    }
}
