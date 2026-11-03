//! Fixtures the tests of the state's families share.

use crate::protocol::{BackgroundTask, Role, Session, SessionStatus};
use crate::state::AppState;
use crate::test_support::bare_session;

pub(crate) fn session_record(id: &str, status: SessionStatus) -> Session {
    Session {
        status,
        ..bare_session(id, Role::Project)
    }
}

/// Stores a session record, with the console it belongs to.
pub(crate) fn insert_record(
    state: &AppState,
    dir: &std::path::Path,
    id: &str,
    status: SessionStatus,
) {
    state
        .store
        .insert_console(&crate::test_support::console(dir))
        .unwrap();
    state
        .store
        .insert_session(&session_record(id, status))
        .unwrap();
}

pub(super) fn subagent(id: &str) -> BackgroundTask {
    BackgroundTask {
        id: id.to_string(),
        kind: Some("subagent".to_string()),
        description: None,
    }
}
