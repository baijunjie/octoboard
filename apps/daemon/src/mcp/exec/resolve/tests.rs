use super::*;
use crate::mcp::exec::call;
use crate::mcp::test_support::{
    arguments, console, console_session, lead_state, peer_state, project_session, refused,
    shared_console_state,
};
use crate::protocol::{error_code, Agent, CodedError, Session, SessionStatus};
use serde_json::json;
use std::collections::HashMap;

/// The one ownership check, exercised through each write-side tool: a session that is unbound,
/// bound to the other console session, a console session, or from another console is refused
/// with a reason. For the two tools whose success would change the session, the session is also
/// checked to be untouched; `instruct_session` has no such visible effect to compare.
#[tokio::test]
async fn writes_are_refused_for_sessions_the_caller_does_not_own() {
    let (state, _dir) = shared_console_state("tool-surface-writes");
    let cases = [
        (
            "instruct_session",
            "of-b",
            "bound to another console session, `b`",
        ),
        ("instruct_session", "loose", "not bound to any session"),
        ("instruct_session", "b", "is a console session;"),
        ("instruct_session", "foreign", "not bound to any session"),
        (
            "interrupt_session",
            "of-b",
            "bound to another console session, `b`",
        ),
        ("interrupt_session", "loose", "not bound to any session"),
        ("interrupt_session", "b", "is a console session;"),
        (
            "archive_session",
            "of-b",
            "bound to another console session, `b`",
        ),
        ("archive_session", "loose", "not bound to any session"),
        ("archive_session", "a", "your own session"),
        (
            "reopen_session",
            "archived-of-b",
            "bound to another console session, `b`",
        ),
    ];
    for (tool, target, reason) in cases {
        let before = state.store.get_session(target).unwrap().unwrap();
        let err = call(
            &state,
            "a",
            tool,
            &arguments(json!({ "session": target, "text": "hello" })),
        )
        .await
        .expect_err("refused");
        assert!(err.to_string().contains(reason), "{tool} {target}: {err}");
        let after = state.store.get_session(target).unwrap().unwrap();
        if tool != "instruct_session" {
            assert_eq!(before.status, after.status, "{tool} {target}");
            assert_eq!(before.ended_at, after.ended_at, "{tool} {target}");
        }
    }

    // The caller's own is not refused by the ownership check: `interrupt_session` gets as
    // far as the session's own state, here no process to send a key to.
    let err = call(
        &state,
        "a",
        "interrupt_session",
        &arguments(json!({ "session": "of-a" })),
    )
    .await
    .expect_err("refused");
    assert_eq!(
        err.downcast_ref::<CodedError>().map(|err| err.code),
        Some(error_code::SESSION_NOT_RUNNING),
        "{err}"
    );
    call(
        &state,
        "a",
        "archive_session",
        &arguments(json!({ "session": "of-a" })),
    )
    .await
    .unwrap();
    let archived = state.store.get_session("of-a").unwrap().unwrap();
    assert_eq!(archived.status, SessionStatus::Archived);
}

/// The console session reads a lead session as bound to a console session and yours, and the
/// lead session's own sessions as bound to a project session and not yours; acting on one of
/// those is refused, naming the lead session as its owner.
#[tokio::test]
async fn the_console_session_reads_a_lead_sessions_sessions_but_cannot_drive_them() {
    let (state, _dir) = lead_state("lead-reads");
    for (target, owner, kind, yours) in [
        ("starter", "a", "console", true),
        ("mine", "starter", "project", false),
    ] {
        let read = call(
            &state,
            "a",
            "get_session",
            &arguments(json!({ "session": target })),
        )
        .await
        .unwrap();
        assert_eq!(read["owner"], owner, "{target}");
        assert_eq!(read["owner_kind"], kind, "{target}");
        assert_eq!(read["yours"], yours, "{target}");
    }
    for tool in ["instruct_session", "archive_session"] {
        let err = call(
            &state,
            "a",
            tool,
            &arguments(json!({ "session": "mine", "text": "hello" })),
        )
        .await
        .expect_err("not the console session's");
        assert!(
            err.to_string()
                .contains("bound to a project session, `starter`"),
            "{tool}: {err}"
        );
    }
    assert_eq!(
        state.store.get_session("mine").unwrap().unwrap().status,
        SessionStatus::Idle
    );
}

/// The ownership rule is the same one for a project session as owner: only what is bound to
/// the caller may be acted on, and everything else is refused with who owns it.
#[tokio::test]
async fn an_unbound_project_session_acts_only_on_the_sessions_bound_to_it() {
    let (state, _dir) = peer_state("peer-writes");
    let cases = [
        (
            "instruct_session",
            "of-a",
            "bound to a console session, `a`",
        ),
        ("instruct_session", "loose", "not bound to any session"),
        ("instruct_session", "starter", "your own session"),
        (
            "instruct_session",
            "other-project",
            "not bound to any session",
        ),
        ("instruct_session", "a", "is a console session"),
        (
            "interrupt_session",
            "of-a",
            "bound to a console session, `a`",
        ),
        ("interrupt_session", "starter", "your own session"),
        ("archive_session", "of-b", "bound to a console session, `b`"),
        ("archive_session", "loose", "not bound to any session"),
        (
            "reopen_session",
            "archived-of-a",
            "bound to a console session, `a`",
        ),
    ];
    for (tool, target, reason) in cases {
        let before = state.store.get_session(target).unwrap().unwrap();
        let err = call(
            &state,
            "starter",
            tool,
            &arguments(json!({ "session": target, "text": "hello" })),
        )
        .await
        .expect_err("refused");
        assert!(err.to_string().contains(reason), "{tool} {target}: {err}");
        let after = state.store.get_session(target).unwrap().unwrap();
        assert_eq!(before.status, after.status, "{tool} {target}");
    }

    // A project session owns nobody else's: `mine` is `starter`'s and not `loose`'s.
    let err = call(
        &state,
        "loose",
        "archive_session",
        &arguments(json!({ "session": "mine" })),
    )
    .await
    .expect_err("another owner's session");
    assert!(
        err.to_string()
            .contains("bound to another project session, `starter`"),
        "{err}"
    );

    call(
        &state,
        "starter",
        "archive_session",
        &arguments(json!({ "session": "mine" })),
    )
    .await
    .unwrap();
    let archived = state.store.get_session("mine").unwrap().unwrap();
    assert_eq!(archived.status, SessionStatus::Archived);
}

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

/// `send_message` is offered as the way to tell a session that is not the caller's only while
/// it can take one: not for an archived session, which it refuses.
#[test]
fn the_refusal_to_act_on_a_session_offers_send_message_only_while_it_can_take_it() {
    let (state, _dir) = shared_console_state("refusal-hint");
    let caller = state.session_record("a").unwrap();
    let refusal = |id: &str| {
        resolve_owned_session(&state, &caller, id)
            .expect_err("not the caller's")
            .to_string()
    };
    let mut archived_loose = state.session_record("loose").unwrap();
    archived_loose.id = "archived-loose".to_string();
    archived_loose.status = SessionStatus::Archived;
    state.store.insert_session(&archived_loose).unwrap();

    for (id, offered) in [
        ("of-b", true),
        ("archived-of-b", false),
        ("loose", true),
        ("archived-loose", false),
    ] {
        let message = refusal(id);
        assert_eq!(message.contains("send_message"), offered, "{id}: {message}");
        assert!(message.contains("not yours to"), "{id}: {message}");
    }
}

/// A refusal to drive a session that is not the caller's says why and what the caller can still
/// do, naming `send_message` as the caller's agent qualifies it; a session of the caller's own
/// is told to say it to the user instead, since `send_message` is refused for it.
#[tokio::test]
async fn a_refusal_to_drive_a_session_that_is_not_the_callers_names_send_message() {
    let (state, _dir) = shared_console_state("tool-refusal-send-message");
    for target in ["of-b", "loose", "b"] {
        let message = refused(
            &state,
            "a",
            "instruct_session",
            json!({ "session": target, "text": "hi" }),
        )
        .await;
        assert!(
            message.contains("You can still tell it something with `mcp__octoboard__send_message`"),
            "{target}: {message}"
        );
    }
    let message = refused(
        &state,
        "a",
        "instruct_session",
        json!({ "session": "of-b", "text": "hi" }),
    )
    .await;
    assert!(
        message.contains("Only that session instructs or drives it")
            && message.contains("its owner is sent a copy"),
        "{message}"
    );

    let mut grok = state.session_record("a").unwrap();
    grok.agent = Agent::Grok;
    state.store.update_session(&grok).unwrap();
    let message = refused(
        &state,
        "a",
        "instruct_session",
        json!({ "session": "loose", "text": "hi" }),
    )
    .await;
    assert!(message.contains("`octoboard__send_message`"), "{message}");
    assert!(
        !message.contains("mcp__octoboard__send_message"),
        "{message}"
    );

    let message = refused(
        &state,
        "a",
        "instruct_session",
        json!({ "session": "a", "text": "hi" }),
    )
    .await;
    assert!(!message.contains("send_message"), "{message}");
}
