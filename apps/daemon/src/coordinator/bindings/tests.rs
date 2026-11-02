use super::*;
use crate::coordinator::handle;
use crate::coordinator::test_support::{
    binding_fixture, bound_session, code_of, console, lead_session, no_process_with_conversation,
    relaunch_stand_in,
};
use crate::mcp;
use crate::messaging::owner_notices;
use crate::protocol::{
    error_code, Agent, CodedError, OrchestrationRole, Project, ProjectSource, RequestBody, Role,
    Session, SessionStatus,
};
use crate::session_tools;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    bare_session, console_session, console_with_project_and_accounts, project_session,
};
use std::path::Path;

/// Owned sessions are counted as the walks of a team count them: in any status, and none for a
/// session with the role of a worker, whatever a row says is bound to it.
#[test]
fn owned_counts_leave_out_owners_with_the_bound_role() {
    let owned = |id: &str, owner: Option<&str>, lead: bool, status: SessionStatus| Session {
        bound_to: owner.map(str::to_string),
        orchestration_role: if owner.is_none() || lead {
            OrchestrationRole::Coordinator
        } else {
            OrchestrationRole::Worker
        },
        ..project_session(id, Agent::Claude, status)
    };
    let sessions = [
        console_session("console", SessionStatus::Idle),
        owned("lead", Some("console"), true, SessionStatus::Idle),
        owned("bound", Some("console"), false, SessionStatus::Archived),
        owned(
            "under-lead",
            Some("lead"),
            false,
            SessionStatus::Interrupted,
        ),
        owned("under-bound", Some("bound"), false, SessionStatus::Idle),
    ];

    let counts = owned_counts(&sessions);

    assert_eq!(counts.get("console"), Some(&2));
    assert_eq!(counts.get("lead"), Some(&1));
    assert_eq!(counts.get("bound"), None);
    assert_eq!(counts.len(), 2);
}

/// The count of a single session is the one the count of every session gives, without
/// listing them: the same owners, the same numbers, and none for an owner with the role of a
/// worker.
#[test]
fn the_count_of_one_session_agrees_with_the_count_of_all() {
    let (state, dir) = crate::test_support::app_state("coordinator-owned-count-of");
    let owned = |id: &str, owner: Option<&str>, lead: bool, status: SessionStatus| Session {
        project_id: Some("p1".to_string()),
        bound_to: owner.map(str::to_string),
        orchestration_role: if owner.is_none() || lead {
            OrchestrationRole::Coordinator
        } else {
            OrchestrationRole::Worker
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
        owned("lead", Some("console"), true, SessionStatus::Idle),
        owned("bound", Some("console"), false, SessionStatus::Archived),
        owned(
            "under-lead",
            Some("lead"),
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

/// A console session is never bound, whatever the request named — `resolve_bound_to` does not
/// even look at `requested` for one.
#[test]
fn a_console_session_is_never_bound() {
    let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-console");
    let bound = resolve_bound_to(
        &state,
        "console-1",
        None,
        Role::Console,
        Some("anything".into()),
    )
    .expect("resolved");
    assert_eq!(bound, None);
}

/// With nothing named, a project session opens unbound — the ordinary case for one the user
/// starts by hand without checking a box.
#[test]
fn a_project_session_named_nothing_to_bind_to_opens_unbound() {
    let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-none");
    let bound = resolve_bound_to(&state, "console-1", None, Role::Project, None).expect("resolved");
    assert_eq!(bound, None);
}

/// Naming a real console session of the same console is honoured.
#[test]
fn a_project_session_binds_to_the_console_session_it_names() {
    let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-valid");
    state
        .store
        .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
        .unwrap();
    state
        .store
        .insert_session(&bare_session("console-session-1", Role::Console))
        .unwrap();

    let bound = resolve_bound_to(
        &state,
        "console-1",
        None,
        Role::Project,
        Some("console-session-1".to_string()),
    )
    .expect("resolved");
    assert_eq!(bound, Some("console-session-1".to_string()));
}

/// A binding changes afterwards only at the session's or its owner's request, so naming one
/// that turns out not to exist, to belong to another console, or to be a project session
/// outside the new session's own project must be refused outright rather than silently dropped
/// or discovered later.
#[test]
fn binding_to_an_invalid_target_is_refused() {
    let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-invalid");
    state
        .store
        .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
        .unwrap();
    let mut other_console = console(Agent::Claude, Path::new("/tmp/unused"));
    other_console.id = "console-2".to_string();
    state.store.insert_console(&other_console).unwrap();

    let mut elsewhere = bare_session("console-session-elsewhere", Role::Console);
    elsewhere.console_id = "console-2".to_string();
    state.store.insert_session(&elsewhere).unwrap();
    state
        .store
        .insert_session(&bare_session("project-session-1", Role::Project))
        .unwrap();

    // (requested target, why it is not a valid binding)
    let cases = [
        ("does-not-exist", "no such session at all"),
        (
            "console-session-elsewhere",
            "a console session of another console",
        ),
        (
            "project-session-1",
            "a project session when the new session has no project",
        ),
    ];
    for (target, why) in cases {
        let err = resolve_bound_to(
            &state,
            "console-1",
            None,
            Role::Project,
            Some(target.to_string()),
        )
        .expect_err(why);
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::UNKNOWN_SESSION, "{why}");
    }
}

/// An unbound project session or a lead session of the new session's own project may own it.
/// One of another project, or one with the role of a worker, may not — among them a
/// session bound to a lead session, which would make a third level.
#[test]
fn a_project_session_binds_only_to_an_unbound_or_lead_session_of_its_own_project() {
    let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-project");
    state
        .store
        .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
        .unwrap();
    state
        .store
        .insert_session(&bare_session("console-session-1", Role::Console))
        .unwrap();
    for id in ["project-1", "project-2"] {
        state
            .store
            .insert_project(&Project {
                id: id.to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: id.to_string(),
                path: format!("/tmp/{id}"),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: None,
                accounts: Default::default(),
                launch: Default::default(),
                trust_consent: false,
                pinned: false,
                tags: Vec::new(),
                description: None,
                git_workflow: Default::default(),
            })
            .unwrap();
    }
    for (id, project, owner, lead) in [
        ("starter", "project-1", None, false),
        ("elsewhere", "project-2", None, false),
        (
            "bound-starter",
            "project-1",
            Some("console-session-1"),
            false,
        ),
        ("lead", "project-1", Some("console-session-1"), true),
        ("of-lead", "project-1", Some("lead"), false),
    ] {
        let mut session = bare_session(id, Role::Project);
        session.project_id = Some(project.to_string());
        session.bound_to = owner.map(str::to_string);
        if owner.is_some() && !lead {
            session.orchestration_role = OrchestrationRole::Worker;
        }
        state.store.insert_session(&session).unwrap();
    }
    let resolve = |target: &str| {
        resolve_bound_to(
            &state,
            "console-1",
            Some("project-1"),
            Role::Project,
            Some(target.to_string()),
        )
    };

    assert_eq!(resolve("starter").unwrap(), Some("starter".to_string()));
    assert_eq!(resolve("lead").unwrap(), Some("lead".to_string()));
    for target in ["elsewhere", "bound-starter", "of-lead"] {
        let err = resolve(target).expect_err(target);
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::UNKNOWN_SESSION, "{target}");
    }
}

/// An unbound project session binds itself to a running console session of its console and
/// becomes a lead session keeping its own sessions, or, with none of its own, to a running
/// project session of its project that can own sessions and becomes an ordinary bound one.
/// Each refusal names its reason and changes nothing.
#[tokio::test]
async fn a_session_binds_itself_only_where_orchestration_stays_two_levels_deep() {
    use SessionStatus::{Idle, Interrupted, Starting};
    let (state, _dir, _live) = binding_fixture(
        "bind",
        vec![
            console_session("hub", Idle),
            console_session("sleeping-hub", Interrupted),
            console_session("starting-hub", Starting),
            // Reads running, but has no process behind it.
            console_session("ghost-hub", Idle),
            Session {
                console_id: "console-2".to_string(),
                ..console_session("foreign-hub", Idle)
            },
            project_session("starter", Agent::Claude, Idle),
            project_session("other-starter", Agent::Claude, Idle),
            project_session("loner", Agent::Claude, Idle),
            Session {
                project_id: Some("project-2".to_string()),
                ..project_session("elsewhere", Agent::Claude, Idle)
            },
            bound_session("mine", Idle, "starter"),
            bound_session("bound", Idle, "hub"),
        ],
        &[
            "hub",
            "starting-hub",
            "foreign-hub",
            "other-starter",
            "elsewhere",
            "mine",
        ],
    );

    for (session, owner, reason) in [
        ("bound", "hub", "already bound"),
        ("hub", "starter", "console session cannot be bound"),
        (
            "starter",
            "other-starter",
            "has sessions of its own (`mine`)",
        ),
        ("loner", "mine", "cannot own sessions"),
        ("loner", "elsewhere", "another project"),
        ("loner", "foreign-hub", "another console"),
        ("loner", "sleeping-hub", "not running"),
        ("loner", "starting-hub", "not running"),
        ("loner", "ghost-hub", "not running"),
        ("loner", "loner", "its own owner"),
    ] {
        let err = bind_session(&state, session, owner).expect_err(reason);
        assert!(err.to_string().contains(reason), "{session} {owner}: {err}");
    }
    let loner = state.store.get_session("loner").unwrap().unwrap();
    assert_eq!(
        (loner.bound_to, loner.orchestration_role),
        (None, OrchestrationRole::Coordinator)
    );

    let (lead, _) = bind_session(&state, "starter", "hub").unwrap();
    assert_eq!(
        (lead.bound_to.as_deref(), lead.orchestration_role),
        (Some("hub"), OrchestrationRole::Coordinator)
    );
    assert!(!lead.is_worker());
    let mine = state.store.get_session("mine").unwrap().unwrap();
    assert_eq!(mine.bound_to.as_deref(), Some("starter"));

    let (joined, _) = bind_session(&state, "loner", "other-starter").unwrap();
    assert_eq!(
        (joined.bound_to.as_deref(), joined.orchestration_role),
        (Some("other-starter"), OrchestrationRole::Worker)
    );
    assert!(joined.is_worker());
    let again = bind_session(&state, "loner", "hub").expect_err("bound already");
    assert!(again.to_string().contains("already bound"), "{again}");
}

/// An owner hands a running session of its own to another running owner under the same rules:
/// a lead session keeps being one under another console session, goes to a project session only
/// once it has no sessions of its own, and then can own none. Each refusal names its reason
/// and changes nothing.
#[tokio::test]
async fn an_owner_hands_a_session_over_only_where_orchestration_stays_two_levels_deep() {
    use SessionStatus::{Idle, Interrupted, Starting};
    let (state, _dir, _live) = binding_fixture(
        "transfer",
        vec![
            console_session("hub", Idle),
            console_session("hub-2", Idle),
            console_session("sleeping-hub", Interrupted),
            console_session("starting-hub", Starting),
            Session {
                console_id: "console-2".to_string(),
                ..console_session("foreign-hub", Idle)
            },
            lead_session("lead", Idle, "hub"),
            bound_session("child", Idle, "lead"),
            lead_session("spare-lead", Idle, "hub"),
            bound_session("plain", Idle, "hub"),
            bound_session("no-process", Interrupted, "hub"),
            bound_session("starting", Starting, "hub"),
            project_session("peer", Agent::Claude, Idle),
        ],
        &[
            "hub",
            "hub-2",
            "foreign-hub",
            "lead",
            "child",
            "spare-lead",
            "plain",
            "peer",
        ],
    );

    // A session being launched holds the launch claim, which refuses a binding change.
    let _claim = state.begin_launch("starting").unwrap();
    for (session, from, to, reason) in [
        ("lead", "hub", "peer", "has sessions of its own (`child`)"),
        ("plain", "hub", "child", "cannot own sessions"),
        ("plain", "hub", "foreign-hub", "another console"),
        ("plain", "hub", "sleeping-hub", "not running"),
        ("plain", "hub", "starting-hub", "not running"),
        ("plain", "hub", "hub", "already bound"),
        ("plain", "hub", "plain", "its own owner"),
        ("starting", "hub", "hub-2", "being started"),
        ("child", "hub", "hub-2", "not bound to `hub`"),
    ] {
        let before = state.store.get_session(session).unwrap().unwrap();
        let err = transfer_session(&state, session, from, to).expect_err(reason);
        assert!(err.to_string().contains(reason), "{session} to {to}: {err}");
        let after = state.store.get_session(session).unwrap().unwrap();
        assert_eq!(
            (before.bound_to, before.orchestration_role),
            (after.bound_to, after.orchestration_role)
        );
    }

    // A session that becomes a bound one loses the report panel's tools, so its pages are
    // withdrawn; a lead session keeps them.
    for (page, session) in [("l-1", "lead"), ("sl-1", "spare-lead")] {
        state
            .store
            .insert_page(&crate::test_support::test_page(page, session, 1))
            .unwrap();
    }
    let (lead, ..) = transfer_session(&state, "lead", "hub", "hub-2").unwrap();
    assert_eq!(
        (lead.bound_to.as_deref(), lead.orchestration_role),
        (Some("hub-2"), OrchestrationRole::Coordinator)
    );
    assert_eq!(state.store.list_pages("lead").unwrap().len(), 1);
    let child = state.store.get_session("child").unwrap().unwrap();
    assert_eq!(child.bound_to.as_deref(), Some("lead"));

    let (plain, ..) = transfer_session(&state, "plain", "hub", "peer").unwrap();
    assert_eq!(
        (plain.bound_to.as_deref(), plain.orchestration_role),
        (Some("peer"), OrchestrationRole::Worker)
    );
    let (spare, owner, _) = transfer_session(&state, "spare-lead", "hub", "peer").unwrap();
    assert_eq!(
        (spare.bound_to.as_deref(), spare.orchestration_role),
        (Some("peer"), OrchestrationRole::Worker)
    );
    assert!(spare.is_worker());
    assert_eq!(owner.id, "peer");
    assert!(state.store.list_pages("spare-lead").unwrap().is_empty());
}

/// Asserts `err` is the coded refusal `code`, naming `session` (and `owner`, when given).
fn assert_busy_refusal(
    err: &anyhow::Error,
    code: crate::protocol::Code,
    session: &str,
    owner: Option<&str>,
) {
    assert_eq!(code_of(err), code);
    let coded = err.downcast_ref::<CodedError>().expect("a coded refusal");
    assert_eq!(
        coded.params.get("session").map(String::as_str),
        Some(session)
    );
    assert_eq!(coded.params.get("owner").map(String::as_str), owner);
}

/// A session, or an owner, under a handoff or switch claim is refused a binding change that
/// involves it, and nothing changes; once the claim is released the same change goes through.
#[tokio::test]
async fn a_claimed_session_or_owner_is_not_bound_to_or_from() {
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "binding-claimed",
        vec![
            console_session("hub", Idle),
            console_session("hub-2", Idle),
            project_session("loner", Agent::Claude, Idle),
            bound_session("plain", Idle, "hub"),
        ],
        &["hub", "hub-2", "loner", "plain"],
    );

    let claim = state.begin_switch("loner").unwrap();
    let err = bind_session(&state, "loner", "hub").expect_err("session claimed");
    assert_busy_refusal(&err, error_code::SESSION_CHANGING, "loner", None);
    drop(claim);

    let claim = state.begin_switch("hub").unwrap();
    let err = bind_session(&state, "loner", "hub").expect_err("owner claimed");
    assert_busy_refusal(
        &err,
        error_code::SESSION_OWNER_CHANGING,
        "loner",
        Some("hub"),
    );
    assert_eq!(
        state.store.get_session("loner").unwrap().unwrap().bound_to,
        None
    );
    let err = transfer_session(&state, "plain", "hub", "hub-2").expect_err("old owner claimed");
    assert_busy_refusal(
        &err,
        error_code::SESSION_OWNER_CHANGING,
        "plain",
        Some("hub"),
    );
    drop(claim);

    let claim = state.begin_switch("hub-2").unwrap();
    let err = transfer_session(&state, "plain", "hub", "hub-2").expect_err("new owner claimed");
    assert_busy_refusal(
        &err,
        error_code::SESSION_OWNER_CHANGING,
        "plain",
        Some("hub-2"),
    );
    assert_eq!(
        state
            .store
            .get_session("plain")
            .unwrap()
            .unwrap()
            .bound_to
            .as_deref(),
        Some("hub")
    );
    drop(claim);

    let (loner, _) = bind_session(&state, "loner", "hub").unwrap();
    assert_eq!(loner.bound_to.as_deref(), Some("hub"));
    let (plain, ..) = transfer_session(&state, "plain", "hub", "hub-2").unwrap();
    assert_eq!(plain.bound_to.as_deref(), Some("hub-2"));
}

/// A bound session, running or not, is released to an ordinary unbound project session: a
/// lead session keeps the sessions bound to it, and one the user releases needs no owner
/// named. Each refusal names its reason and changes nothing.
#[tokio::test]
async fn a_session_is_released_into_an_unbound_one_that_keeps_its_own_sessions() {
    use SessionStatus::{Idle, Interrupted};
    let (state, _dir, _live) = binding_fixture(
        "release",
        vec![
            console_session("hub", Idle),
            lead_session("lead", Idle, "hub"),
            bound_session("child", Idle, "lead"),
            bound_session("plain", Idle, "hub"),
            bound_session("no-process", Interrupted, "hub"),
            bound_session("by-user", Idle, "hub"),
            project_session("loner", Agent::Claude, Idle),
        ],
        &["hub", "lead", "child", "plain", "by-user"],
    );

    for (session, owner, code) in [
        ("hub", Some("hub"), error_code::CONSOLE_SESSION_NEVER_BOUND),
        ("loner", Some("hub"), error_code::SESSION_NOT_BOUND),
        ("loner", None, error_code::SESSION_NOT_BOUND),
        (
            "plain",
            Some("lead"),
            error_code::SESSION_NOT_BOUND_TO_OWNER,
        ),
        ("child", Some("hub"), error_code::SESSION_NOT_BOUND_TO_OWNER),
    ] {
        let before = state.store.get_session(session).unwrap().unwrap();
        let err = release_session(&state, session, owner, None).expect_err(code.as_str());
        assert_eq!(code_of(&err), code, "{session}: {err}");
        let after = state.store.get_session(session).unwrap().unwrap();
        assert_eq!(
            (before.bound_to, before.orchestration_role),
            (after.bound_to, after.orchestration_role)
        );
    }

    let lead = release_session(&state, "lead", Some("hub"), None)
        .unwrap()
        .session;
    assert!(!lead.is_worker());
    assert_eq!(
        (lead.bound_to, lead.orchestration_role),
        (None, OrchestrationRole::Coordinator)
    );
    let child = state.store.get_session("child").unwrap().unwrap();
    assert_eq!(child.bound_to.as_deref(), Some("lead"));

    let plain = release_session(&state, "plain", Some("hub"), None)
        .unwrap()
        .session;
    assert_eq!(
        (plain.bound_to, plain.orchestration_role),
        (None, OrchestrationRole::Coordinator)
    );
    let released = release_session(&state, "no-process", Some("hub"), None)
        .unwrap()
        .session;
    assert_eq!(released.bound_to, None);
    let by_user = release_session(&state, "by-user", None, None).unwrap();
    assert_eq!(by_user.former_owner, "hub");
    let by_user = by_user.session;
    assert_eq!(by_user.bound_to, None);
    let err = release_session(&state, "by-user", None, None).expect_err("already released");
    assert_eq!(code_of(&err), error_code::SESSION_NOT_BOUND, "{err}");

    // Unbound now, it can be bound again like any unbound project session.
    let (rebound, _) = bind_session(&state, "plain", "hub").unwrap();
    assert_eq!(rebound.bound_to.as_deref(), Some("hub"));
}

/// A session without a process whose owner changed is told when it next starts: the notice is built
/// at launch from the binding as it stands, ahead of the launch's own instruction. A release and a
/// transfer leave one, a later change replaces an earlier pending notice, and once the launch has
/// delivered it nothing is left.
#[tokio::test]
async fn a_no_process_session_is_told_of_its_new_owner_when_it_next_starts() {
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "owner-notice",
        vec![
            console_session("hub", Idle),
            console_session("hub-2", Idle),
            no_process_with_conversation("released", Agent::Claude, "hub"),
            no_process_with_conversation("handed", Agent::Claude, "hub"),
            no_process_with_conversation("replaced", Agent::Claude, "hub"),
        ],
        &["hub", "hub-2"],
    );

    // A release leaves the notice owed, decided with the binding change.
    let Released {
        session: released,
        running,
        ..
    } = release_session(&state, "released", Some("hub"), None).unwrap();
    assert!(!running);
    assert!(
        owner_notices::tell_released(&state, &released, running, None)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(state.store.owner_notice_pending("released").unwrap(), 1);
    let (prompt, _) = relaunch_stand_in(&state, "released", false).await.unwrap();
    assert!(prompt.unwrap().contains("you have no parent session now"));
    assert_eq!(state.store.owner_notice_pending("released").unwrap(), 0);

    // A transfer, which a session with no process takes.
    let args = serde_json::json!({ "session": "handed", "to": "hub-2", "note": "carry on" });
    let result = mcp::dispatch::call(&state, "hub", "transfer_session", args.as_object().unwrap())
        .await
        .unwrap();
    assert!(
        result["note"]
            .as_str()
            .unwrap()
            .contains("will be told when it next starts"),
        "{result}"
    );
    let (prompt, _) = relaunch_stand_in(&state, "handed", false).await.unwrap();
    let prompt = prompt.unwrap();
    assert!(
        prompt.contains("your parent session has changed"),
        "{prompt}"
    );
    assert!(prompt.contains("`hub-2`"), "{prompt}");
    assert_eq!(state.store.owner_notice_pending("handed").unwrap(), 0);

    // A later change replaces the pending notice: it states the owner as it then stands.
    release_session(&state, "replaced", Some("hub"), None).unwrap();
    state
        .store
        .set_binding(
            "replaced",
            None,
            "hub-2",
            OrchestrationRole::Coordinator,
            false,
            false,
        )
        .unwrap()
        .unwrap();
    owner_notices::running_or_leave(&state, &state.session_record("replaced").unwrap()).unwrap();
    let (prompt, _) = relaunch_stand_in(&state, "replaced", false).await.unwrap();
    let prompt = prompt.unwrap();
    assert!(prompt.contains("`hub-2`"), "{prompt}");
    assert!(
        !prompt.contains("you have no parent session now"),
        "{prompt}"
    );
    assert_eq!(state.store.owner_notice_pending("replaced").unwrap(), 0);
}

/// A launch that fails delivers nothing and keeps the notice owed, and a launch that is not a
/// Claude Code resume delivers it through the queue once the agent takes input.
#[tokio::test]
async fn a_failed_launch_keeps_the_notice_and_a_queued_launch_delivers_it() {
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "owner-notice-launch",
        vec![
            console_session("hub", Idle),
            no_process_with_conversation("claude", Agent::Claude, "hub"),
            no_process_with_conversation("codex", Agent::Codex, "hub"),
        ],
        &["hub"],
    );
    for id in ["claude", "codex"] {
        release_session(&state, id, None, None).unwrap();
    }

    relaunch_stand_in(&state, "claude", true)
        .await
        .expect_err("the launch fails");
    assert_eq!(state.store.owner_notice_pending("claude").unwrap(), 1);
    assert!(state.take_queued("claude").is_empty());
    assert_eq!(
        state.store.get_session("claude").unwrap().unwrap().status,
        SessionStatus::Interrupted
    );
    let (prompt, _) = relaunch_stand_in(&state, "claude", false).await.unwrap();
    assert!(prompt.unwrap().contains("you have no parent session now"));

    let (prompt, queued) = relaunch_stand_in(&state, "codex", false).await.unwrap();
    assert!(prompt.is_none());
    assert_eq!(queued.len(), 1);
    assert!(
        queued[0].contains("you have no parent session now"),
        "{queued:?}"
    );
    assert_eq!(state.store.owner_notice_pending("codex").unwrap(), 0);
}

/// The parting note of an owner that releases a session that is not running is kept with the
/// owed notice and told at the next launch, while the release is still the latest change of
/// owner: a later transfer or bind drops it, and a blank note is not kept.
#[tokio::test]
async fn a_no_process_release_keeps_the_parting_note_for_the_next_launch() {
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "release-note",
        vec![
            console_session("hub", Idle),
            console_session("hub-2", Idle),
            no_process_with_conversation("noted", Agent::Claude, "hub"),
            no_process_with_conversation("blank", Agent::Claude, "hub"),
            no_process_with_conversation("moved", Agent::Claude, "hub"),
            no_process_with_conversation("rebound", Agent::Claude, "hub"),
        ],
        &["hub", "hub-2"],
    );

    let Released {
        session: noted,
        running,
        ..
    } = release_session(&state, "noted", Some("hub"), Some("Pick up at step 3.")).unwrap();
    assert!(!running);
    assert!(
        owner_notices::tell_released(&state, &noted, running, Some("Pick up at step 3."))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        state.store.owner_notice_note("noted").unwrap().as_deref(),
        Some("Pick up at step 3.")
    );
    let (prompt, _) = relaunch_stand_in(&state, "noted", false).await.unwrap();
    let prompt = prompt.unwrap();
    assert!(
        prompt.contains("you have no parent session now"),
        "{prompt}"
    );
    assert!(
        prompt.contains("Your former parent session left this note:"),
        "{prompt}"
    );
    assert!(prompt.contains("> Pick up at step 3."), "{prompt}");
    assert_eq!(state.store.owner_notice_pending("noted").unwrap(), 0);
    assert_eq!(state.store.owner_notice_note("noted").unwrap(), None);

    // The tool drops a blank note before the release.
    let args = serde_json::json!({ "session": "blank", "note": "   " });
    mcp::dispatch::call(&state, "hub", "release_session", args.as_object().unwrap())
        .await
        .unwrap();
    assert_eq!(state.store.owner_notice_note("blank").unwrap(), None);
    let (prompt, _) = relaunch_stand_in(&state, "blank", false).await.unwrap();
    let prompt = prompt.unwrap();
    assert!(
        prompt.contains("you have no parent session now"),
        "{prompt}"
    );
    assert!(!prompt.contains("former parent session left"), "{prompt}");

    // A transfer or a bind after the release makes the note stale, so it is dropped.
    release_session(&state, "moved", Some("hub"), Some("Pick up at step 3.")).unwrap();
    let (moved, _) = bind_session(&state, "moved", "hub-2").unwrap();
    assert_eq!(moved.bound_to.as_deref(), Some("hub-2"));
    assert_eq!(state.store.owner_notice_note("moved").unwrap(), None);
    release_session(&state, "rebound", Some("hub"), Some("Pick up at step 3.")).unwrap();
    state
        .store
        .set_binding(
            "rebound",
            None,
            "hub-2",
            OrchestrationRole::Coordinator,
            false,
            true,
        )
        .unwrap()
        .unwrap();
    assert_eq!(state.store.owner_notice_note("rebound").unwrap(), None);
    for id in ["moved", "rebound"] {
        let (prompt, _) = relaunch_stand_in(&state, id, false).await.unwrap();
        let prompt = prompt.unwrap();
        assert!(prompt.contains("`hub-2`"), "{prompt}");
        assert!(!prompt.contains("Pick up at step 3."), "{prompt}");
    }
}

/// A release whose message cannot be written to a session that was running, its process gone
/// meanwhile, leaves the notice owed with the parting note, and the note still goes at launch.
#[tokio::test]
async fn a_failed_write_after_a_release_keeps_the_note_owed() {
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "release-note-failed-write",
        vec![
            console_session("hub", Idle),
            no_process_with_conversation("gone", Agent::Claude, "hub"),
        ],
        &["hub"],
    );

    // Judged running, as at the release, but no process is there to write to.
    let released = release_session(&state, "gone", Some("hub"), Some("Mind step 3."))
        .unwrap()
        .session;
    state.store.clear_owner_notice("gone", 1).unwrap();
    assert!(
        owner_notices::tell_released(&state, &released, true, Some("Mind step 3."))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(state.store.owner_notice_pending("gone").unwrap(), 1);
    let (prompt, _) = relaunch_stand_in(&state, "gone", false).await.unwrap();
    let prompt = prompt.unwrap();
    assert!(prompt.contains("> Mind step 3."), "{prompt}");
}

/// A release by the user tells the owner the session was released from, when the owner is running:
/// one without a process is not told, and a later attempt by it to drive the session is refused as
/// not its own. The released session is told as for any release.
#[tokio::test]
async fn a_release_by_the_user_tells_a_running_owner() {
    use crate::test_support::{heard_after_marker, output_containing, running};
    use SessionStatus::{Idle, Interrupted};
    let (state, _dir, _live) = binding_fixture(
        "release-by-user",
        vec![
            console_session("hub", Idle),
            console_session("hub-2", Interrupted),
            Session {
                title: "Retry audit".to_string(),
                ..bound_session("plain", Idle, "hub")
            },
            bound_session("orphan", Idle, "hub-2"),
        ],
        &[],
    );
    let hub = running(&state, "hub");
    let _plain = running(&state, "plain");
    let _orphan = running(&state, "orphan");
    let release = |session: &str| {
        handle(
            &state,
            None,
            RequestBody::ReleaseSession {
                session: session.to_string(),
            },
        )
    };

    release("plain").await.unwrap();
    let told = output_containing(&hub, "released");
    assert!(told.contains("`plain`"), "{told}");
    assert!(told.contains("Retry audit"), "{told}");
    assert!(
        told.contains("no longer instruct, drive or archive it"),
        "{told}"
    );
    assert!(told.contains("left with the user"), "{told}");
    let send_message = session_tools::catalogue::qualified_tool_name(Agent::Claude, "send_message");
    assert!(told.contains(&format!("`{send_message}`")), "{told}");
    assert_eq!(
        state.store.get_session("plain").unwrap().unwrap().bound_to,
        None
    );

    // Nothing is owed to an owner without a process, and the release goes through.
    release("orphan").await.unwrap();
    assert_eq!(
        state.store.get_session("orphan").unwrap().unwrap().bound_to,
        None
    );
    assert_eq!(state.store.owner_notice_pending("hub-2").unwrap(), 0);
    assert!(state.take_queued("hub-2").is_empty());
    assert!(!heard_after_marker(&state, &hub).contains("`orphan`"));
}

/// A session, or its owner, under a handoff or switch claim is not released, and nothing
/// changes; once the claim is released the same call goes through.
#[tokio::test]
async fn a_claimed_session_or_owner_is_not_released() {
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "release-claimed",
        vec![
            console_session("hub", Idle),
            bound_session("plain", Idle, "hub"),
        ],
        &["hub", "plain"],
    );

    let claim = state.begin_switch("plain").unwrap();
    let err = release_session(&state, "plain", Some("hub"), None).expect_err("session claimed");
    assert_busy_refusal(&err, error_code::SESSION_CHANGING, "plain", None);
    drop(claim);

    let claim = state.begin_switch("hub").unwrap();
    let err = release_session(&state, "plain", None, None).expect_err("owner claimed");
    assert_busy_refusal(
        &err,
        error_code::SESSION_OWNER_CHANGING,
        "plain",
        Some("hub"),
    );
    assert_eq!(
        state
            .store
            .get_session("plain")
            .unwrap()
            .unwrap()
            .bound_to
            .as_deref(),
        Some("hub")
    );
    drop(claim);

    let released = release_session(&state, "plain", None, None)
        .unwrap()
        .session;
    assert_eq!(released.bound_to, None);
}

/// Every launch of a session — opening, resuming, reopening, a switch of its account — tells it
/// who it reports to through `owner_of`, and a lead session is told nobody: it gets an unbound
/// project session's role description and tools, as at its first launch, while a session
/// bound to it is told the lead session owns it.
#[test]
fn a_lead_session_launches_as_an_unbound_project_session() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-lead-launch");
    let lead = lead_session("lead", SessionStatus::Interrupted, "hub");
    let of_lead = bound_session("of-lead", SessionStatus::Interrupted, "lead");
    for session in [
        console_session("hub", SessionStatus::Idle),
        lead.clone(),
        of_lead.clone(),
    ] {
        state.store.insert_session(&session).unwrap();
    }

    assert_eq!(owner_of(&state, &lead).unwrap(), None);
    assert_eq!(
        owner_of(&state, &of_lead).unwrap(),
        Some(session_tools::Owner::Project {
            session_id: "lead".to_string()
        })
    );
}

#[test]
fn a_group_follows_the_bindings_two_levels_and_a_bound_role_owns_none() {
    let sessions = [
        Session {
            bound_to: Some("hub".to_string()),
            orchestration_role: OrchestrationRole::Worker,
            ..bare_session("member", Role::Project)
        },
        Session {
            bound_to: Some("member".to_string()),
            orchestration_role: OrchestrationRole::Worker,
            ..bare_session("stray", Role::Project)
        },
        Session {
            bound_to: Some("hub".to_string()),
            ..bare_session("lead", Role::Project)
        },
        Session {
            bound_to: Some("lead".to_string()),
            orchestration_role: OrchestrationRole::Worker,
            ..bare_session("led", Role::Project)
        },
        bare_session("hub", Role::Console),
    ];
    let ids = |group: Vec<&Session>| -> Vec<String> {
        group.iter().map(|session| session.id.clone()).collect()
    };
    assert_eq!(
        ids(group_in(&sessions, &sessions[4])),
        ["hub", "member", "lead", "led"]
    );
    assert_eq!(ids(group_in(&sessions, &sessions[0])), ["member"]);
    assert_eq!(ids(group_in(&sessions, &sessions[2])), ["lead", "led"]);
}
