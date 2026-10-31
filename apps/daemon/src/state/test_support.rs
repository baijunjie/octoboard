//! Fixtures the tests of the state's families share.

use crate::protocol::{
    Agent, BackgroundTask, OrchestrationRole, Origin, Role, Session, SessionStatus,
};
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;

pub(super) fn session_record(id: &str, status: SessionStatus) -> Session {
    Session {
        id: id.to_string(),
        agent: Agent::Claude,
        agent_session_id: None,
        console_id: "console-1".to_string(),
        project_id: None,
        host_id: LOCAL_HOST_ID.to_string(),
        role: Role::Project,
        origin: Origin::User,
        title: "Session".to_string(),
        status,
        terminal_title: None,
        record_titles: Default::default(),
        background_tasks: Vec::new(),
        switching_account: false,
        default_effort: None,
        orchestration_role: OrchestrationRole::Coordinator,
        context: None,
        observed_model: None,
        observed_effort: None,
        model: None,
        effort: None,
        has_conversation: false,
        bound_to: None,
        colour: None,
        ordinal: None,
        account_id: None,
        config_dir: None,
        pinned: false,
        locked: false,
        started_at: 0,
        ended_at: None,
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
