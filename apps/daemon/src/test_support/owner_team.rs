//! An owner and the sessions bound to it on record, for the tests of what is written into an
//! owner — reports, notices, kept messages: a console session `hub` with a stand-in process, or
//! one that is interrupted, with project sessions bound to it, and reads of what reached it.

use std::sync::Arc;

use crate::protocol::{Agent, OrchestrationRole, Origin, Role, Session, SessionStatus};
use crate::state::AppState;

pub(crate) fn project_session(title: &str) -> Session {
    Session {
        id: "session-7".to_string(),
        agent: Agent::Claude,
        agent_session_id: None,
        console_id: "console-1".to_string(),
        project_id: None,
        host_id: "local".to_string(),
        role: Role::Project,
        origin: Origin::Console,
        title: title.to_string(),
        status: SessionStatus::Idle,
        terminal_title: None,
        record_titles: Default::default(),
        background_tasks: Vec::new(),
        switching_account: false,
        paused: false,
        prompt_on_launch: false,
        default_effort: None,
        orchestration_role: OrchestrationRole::Worker,
        context: None,
        observed_model: None,
        observed_effort: None,
        model: None,
        effort: None,
        has_conversation: true,
        bound_to: Some("console-session-1".to_string()),
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

/// A session's `console_id` is a foreign key, so a test needs this on record before any
/// session.
pub(crate) fn console() -> crate::protocol::Console {
    crate::protocol::Console {
        id: "console-1".to_string(),
        name: "Console".to_string(),
        workdir: "/tmp/console-1".to_string(),
        default_agent: Agent::Claude,
        project_session_agent: None,
        accounts: Default::default(),
        launch: Default::default(),
        project_session_launch: Default::default(),
        icon: None,
        console_session_prompt: None,
        created_at: 0,
    }
}

/// A console session `hub` with a stand-in process, and the sessions given, each `(id, owner,
/// lead, status)`, on record. The stand-in's output is pumped into its ring buffer, so what
/// reaches it can be read back.
pub(crate) fn team_state(
    label: &str,
    sessions: &[(&str, &str, bool, SessionStatus)],
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
) {
    let (state, dir) = crate::test_support::app_state(label);
    state.store.insert_console(&console()).unwrap();
    let hub = Session {
        id: "hub".to_string(),
        role: Role::Console,
        bound_to: None,
        ..project_session("hub")
    };
    state.store.insert_session(&hub).unwrap();
    let live = crate::test_support::idle_stand_in("hub");
    state.register_live(live.clone());
    crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
    for (id, owner, lead, status) in sessions {
        let session = Session {
            id: id.to_string(),
            bound_to: Some(owner.to_string()),
            orchestration_role: if *lead {
                OrchestrationRole::Coordinator
            } else {
                OrchestrationRole::Worker
            },
            status: *status,
            ..project_session(id)
        };
        state.store.insert_session(&session).unwrap();
    }
    (state, dir, live)
}

/// What the hub has been written once a marker has reached it, so an absence asserted in it is
/// one of a reader that has caught up.
pub(crate) async fn hub_heard(state: &Arc<AppState>, hub: &crate::test_support::StandIn) -> String {
    let (state, hub) = (state.clone(), (**hub).clone());
    tokio::task::spawn_blocking(move || crate::test_support::heard_after_marker(&state, &hub))
        .await
        .unwrap()
}

/// What the hub hears until `needle` is in it.
pub(crate) async fn hub_heard_of(
    hub: &crate::test_support::StandIn,
    needle: &'static str,
) -> String {
    let hub = (**hub).clone();
    tokio::task::spawn_blocking(move || crate::test_support::output_containing(&hub, needle))
        .await
        .unwrap()
}

/// An owner `hub` that is interrupted with no process, and `worker`, bound to it and waiting
/// for the user.
pub(crate) fn interrupted_hub_state(
    label: &str,
) -> (Arc<AppState>, crate::test_support::ScratchDir) {
    let (state, dir) = crate::test_support::app_state(label);
    state.store.insert_console(&console()).unwrap();
    for (id, owner, status) in [
        ("hub", None, SessionStatus::Interrupted),
        ("worker", Some("hub"), SessionStatus::WaitingUser),
    ] {
        let session = Session {
            id: id.to_string(),
            role: if id == "hub" {
                Role::Console
            } else {
                Role::Project
            },
            bound_to: owner.map(str::to_string),
            orchestration_role: if owner.is_some() {
                OrchestrationRole::Worker
            } else {
                OrchestrationRole::Coordinator
            },
            status,
            ..project_session(id)
        };
        state.store.insert_session(&session).unwrap();
    }
    (state, dir)
}

pub(crate) fn kept_texts(state: &Arc<AppState>) -> Vec<String> {
    let kept = state.store.kept_messages("hub").unwrap();
    kept.into_iter().map(|(_, text)| text).collect()
}
