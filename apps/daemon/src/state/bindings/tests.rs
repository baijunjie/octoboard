use super::*;
use crate::protocol::{Agent, Role, SessionKind, SessionStatus};
use crate::test_support::{bare_session, console_session, project_session};

/// Owned sessions are counted as the walks of a team count them: in any status, and none for a
/// session with the role of a worker, whatever a row says is bound to it.
#[test]
fn owned_counts_leave_out_owners_with_the_bound_role() {
    let owned = |id: &str, owner: Option<&str>, coordinator: bool, status: SessionStatus| Session {
        bound_to: owner.map(str::to_string),
        role: if owner.is_none() || coordinator {
            Role::Coordinator
        } else {
            Role::Worker
        },
        ..project_session(id, Agent::Claude, status)
    };
    let sessions = [
        console_session("console", SessionStatus::Idle),
        owned("coordinator", Some("console"), true, SessionStatus::Idle),
        owned("bound", Some("console"), false, SessionStatus::Archived),
        owned(
            "under-coordinator",
            Some("coordinator"),
            false,
            SessionStatus::Interrupted,
        ),
        owned("under-bound", Some("bound"), false, SessionStatus::Idle),
    ];

    let counts = owned_counts(&sessions);

    assert_eq!(counts.get("console"), Some(&2));
    assert_eq!(counts.get("coordinator"), Some(&1));
    assert_eq!(counts.get("bound"), None);
    assert_eq!(counts.len(), 2);
}

/// The count of a single session is the one the count of every session gives, without
/// listing them: the same owners, the same numbers, and none for an owner with the role of a
/// worker.
#[test]
fn the_count_of_one_session_agrees_with_the_count_of_all() {
    let (state, dir) = crate::test_support::app_state("coordinator-owned-count-of");
    let owned = |id: &str, owner: Option<&str>, coordinator: bool, status: SessionStatus| Session {
        project_id: Some("p1".to_string()),
        bound_to: owner.map(str::to_string),
        role: if owner.is_none() || coordinator {
            Role::Coordinator
        } else {
            Role::Worker
        },
        ..project_session(id, Agent::Claude, status)
    };
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    crate::test_support::insert_project(&state, &dir);
    for session in [
        console_session("console", SessionStatus::Idle),
        owned("coordinator", Some("console"), true, SessionStatus::Idle),
        owned("bound", Some("console"), false, SessionStatus::Archived),
        owned(
            "under-coordinator",
            Some("coordinator"),
            false,
            SessionStatus::Interrupted,
        ),
        owned("under-bound", Some("bound"), false, SessionStatus::Idle),
        owned("alone", None, false, SessionStatus::Idle),
    ] {
        state.store.insert_session(&session).unwrap();
    }
    let all = owned_counts(&state.store.list_sessions().unwrap());

    for session in state.store.list_sessions().unwrap() {
        let one = owned_count_of(&state, &session).unwrap();
        assert_eq!(one.get(&session.id), all.get(&session.id), "{}", session.id);
        assert!(one.len() <= 1);
    }
}

#[test]
fn a_group_follows_the_bindings_two_levels_and_a_bound_role_owns_none() {
    let sessions = [
        Session {
            bound_to: Some("hub".to_string()),
            role: Role::Worker,
            ..bare_session("member", SessionKind::Project)
        },
        Session {
            bound_to: Some("member".to_string()),
            role: Role::Worker,
            ..bare_session("stray", SessionKind::Project)
        },
        Session {
            bound_to: Some("hub".to_string()),
            ..bare_session("coordinator", SessionKind::Project)
        },
        Session {
            bound_to: Some("coordinator".to_string()),
            role: Role::Worker,
            ..bare_session("led", SessionKind::Project)
        },
        bare_session("hub", SessionKind::Console),
    ];
    let ids = |group: Vec<&Session>| -> Vec<String> {
        group.iter().map(|session| session.id.clone()).collect()
    };
    assert_eq!(
        ids(group_in(&sessions, &sessions[4])),
        ["hub", "member", "coordinator", "led"]
    );
    assert_eq!(ids(group_in(&sessions, &sessions[0])), ["member"]);
    assert_eq!(
        ids(group_in(&sessions, &sessions[2])),
        ["coordinator", "led"]
    );
}
