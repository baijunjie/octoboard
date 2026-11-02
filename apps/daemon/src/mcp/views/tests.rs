use super::*;
use crate::mcp::test_support::{console, console_session, peer_state, project_session};
use crate::protocol::{Agent, Session, SessionStatus};
use serde_json::json;
use std::collections::HashMap;

/// What an idle session has left running is part of its description, and only while it is
/// idle: any other status describes none, whatever the daemon still remembers.
#[test]
fn a_description_lists_the_background_tasks_of_an_idle_session_only() {
    let (state, _dir) = crate::test_support::app_state("mcp-exec-describe-background");
    state.store.insert_console(&console()).unwrap();
    let caller = console_session("hub");
    let session = crate::test_support::project_session("s", Agent::Claude, SessionStatus::Idle);
    let describe = |session: &Session| {
        describe_session(&state, session, &caller, &HashMap::new(), &HashMap::new()).unwrap()
            ["background_tasks"]
            .clone()
    };

    assert_eq!(describe(&session), json!([]));

    state.apply_background_tasks_change(
        "s",
        crate::hooks::BackgroundTasksChange::Replace(vec![crate::protocol::BackgroundTask {
            id: "b1".to_string(),
            kind: Some("shell".to_string()),
            description: None,
        }]),
    );
    assert_eq!(describe(&session), json!([{"id": "b1", "type": "shell"}]));

    let working = Session {
        status: SessionStatus::Working,
        ..session
    };
    assert_eq!(describe(&working), json!([]));
}

/// An owner choosing a session for a task reads how full its context is from the description:
/// the figures the UI shows, or null before any was reported.
#[test]
fn a_description_carries_the_sessions_context() {
    let (state, _dir) = crate::test_support::app_state("mcp-exec-describe-context");
    state.store.insert_console(&console()).unwrap();
    let caller = console_session("hub");
    let session = crate::test_support::project_session("s", Agent::Claude, SessionStatus::Idle);
    assert!(
        describe_session(&state, &session, &caller, &HashMap::new(), &HashMap::new()).unwrap()
            ["context"]
            .is_null()
    );

    let used = Session {
        context: Some(crate::protocol::SessionContext {
            tokens: 120_000,
            window: Some(200_000),
            used_percent: Some(60),
        }),
        ..session
    };
    assert_eq!(
        describe_session(&state, &used, &caller, &HashMap::new(), &HashMap::new()).unwrap()
            ["context"],
        json!({"tokens": 120_000, "window": 200_000, "used_percent": 60})
    );
}

/// The role a session is stored with is the one its record and the wire carry, whatever its
/// parent and however many children it has.
#[test]
fn the_stored_role_is_what_the_record_and_the_wire_carry() {
    use crate::protocol::OrchestrationRole::{Coordinator, Worker};

    let (state, _dir) = peer_state("stored-session-role");
    let caller = state.session_record("a").unwrap();
    for (session, expected, name) in [
        (console_session("a"), Coordinator, "console"),
        (
            project_session("loose", "console-1", None),
            Coordinator,
            "independent",
        ),
        (
            project_session("of-a", "console-1", Some("a")),
            Worker,
            "worker",
        ),
        (
            Session {
                orchestration_role: Coordinator,
                ..project_session("empty-coordinator", "console-1", Some("a"))
            },
            Coordinator,
            "reporting coordinator without children",
        ),
    ] {
        assert_eq!(session.orchestration_role, expected, "{name}");
        assert_eq!(session.is_worker(), expected == Worker, "{name}");
        let wire = state.wire_session(session.clone());
        assert_eq!(wire.orchestration_role, expected, "{name}");
        let record =
            describe_session(&state, &session, &caller, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(record["orchestration_role"], json!(expected), "{name}");
        assert_eq!(record["owner"], json!(session.bound_to), "{name}");
        assert_eq!(record["owned_sessions"], 0);
    }
}
