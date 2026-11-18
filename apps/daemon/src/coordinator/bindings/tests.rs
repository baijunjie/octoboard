use super::*;
use crate::coordinator::handle;
use crate::coordinator::test_support::{
    binding_fixture, bound_coordinator, bound_session, console, no_process_with_conversation,
    relaunch_stand_in, relaunch_stand_in_with, resolve_bound_to,
};
use crate::mcp;
use crate::messaging::owner_notices;
use crate::protocol::{
    error_code, AgentId, CodedError, Project, ProjectSource, RequestBody, Role, Session,
    SessionKind, SessionStatus,
};
use crate::session_tools;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    bare_session, code_of, console_session, console_with_project_and_accounts, project_session,
    running, within, ScratchDir, StandIn, PATIENCE,
};
use std::path::Path;

/// A console session is never bound, whatever the request named — `resolve_owner` does not
/// even look at `requested` for one.
#[test]
fn a_console_session_is_never_bound() {
    let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-console");
    let bound = resolve_bound_to(
        &state,
        "console-1",
        None,
        SessionKind::Console,
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
    let bound =
        resolve_bound_to(&state, "console-1", None, SessionKind::Project, None).expect("resolved");
    assert_eq!(bound, None);
}

/// Naming a real console session of the same console is honoured.
#[test]
fn a_project_session_binds_to_the_console_session_it_names() {
    let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-valid");
    state
        .store
        .insert_console(&console(AgentId::CLAUDE, Path::new("/tmp/unused")))
        .unwrap();
    state
        .store
        .insert_session(&bare_session("console-session-1", SessionKind::Console))
        .unwrap();

    let bound = resolve_bound_to(
        &state,
        "console-1",
        None,
        SessionKind::Project,
        Some("console-session-1".to_string()),
    )
    .expect("resolved");
    assert_eq!(bound, Some("console-session-1".to_string()));
}

/// A binding changes afterwards only at the session's or its owner's request, so naming one
/// that turns out not to exist, to belong to another console, to be archived, or to be a project
/// session outside the new session's own project must be refused outright rather than silently
/// dropped or discovered later.
#[test]
fn binding_to_an_invalid_target_is_refused() {
    let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-invalid");
    state
        .store
        .insert_console(&console(AgentId::CLAUDE, Path::new("/tmp/unused")))
        .unwrap();
    let mut other_console = console(AgentId::CLAUDE, Path::new("/tmp/unused"));
    other_console.id = "console-2".to_string();
    state.store.insert_console(&other_console).unwrap();

    let mut elsewhere = bare_session("console-session-elsewhere", SessionKind::Console);
    elsewhere.console_id = "console-2".to_string();
    state.store.insert_session(&elsewhere).unwrap();
    state
        .store
        .insert_session(&bare_session("project-session-1", SessionKind::Project))
        .unwrap();
    let mut archived = bare_session("console-session-archived", SessionKind::Console);
    archived.status = SessionStatus::Archived;
    state.store.insert_session(&archived).unwrap();

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
        ("console-session-archived", "an archived console session"),
    ];
    for (target, why) in cases {
        let err = resolve_bound_to(
            &state,
            "console-1",
            None,
            SessionKind::Project,
            Some(target.to_string()),
        )
        .expect_err(why);
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::UNKNOWN_SESSION, "{why}");
    }
}

/// An unbound project session or a bound coordinator of the new session's own project may own it.
/// One of another project, or one with the role of a worker, may not — among them a
/// session bound to a bound coordinator, which would make a third level.
#[test]
fn a_project_session_binds_only_to_an_unbound_or_bound_coordinator_of_its_own_project() {
    let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-project");
    state
        .store
        .insert_console(&console(AgentId::CLAUDE, Path::new("/tmp/unused")))
        .unwrap();
    state
        .store
        .insert_session(&bare_session("console-session-1", SessionKind::Console))
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
                versions: Default::default(),
            })
            .unwrap();
    }
    for (id, project, owner, coordinator) in [
        ("starter", "project-1", None, false),
        ("elsewhere", "project-2", None, false),
        (
            "bound-starter",
            "project-1",
            Some("console-session-1"),
            false,
        ),
        ("coordinator", "project-1", Some("console-session-1"), true),
        ("of-coordinator", "project-1", Some("coordinator"), false),
    ] {
        let mut session = bare_session(id, SessionKind::Project);
        session.project_id = Some(project.to_string());
        session.bound_to = owner.map(str::to_string);
        if owner.is_some() && !coordinator {
            session.role = Role::Worker;
        }
        state.store.insert_session(&session).unwrap();
    }
    let resolve = |target: &str| {
        resolve_bound_to(
            &state,
            "console-1",
            Some("project-1"),
            SessionKind::Project,
            Some(target.to_string()),
        )
    };

    assert_eq!(resolve("starter").unwrap(), Some("starter".to_string()));
    assert_eq!(
        resolve("coordinator").unwrap(),
        Some("coordinator".to_string())
    );
    for target in ["elsewhere", "bound-starter", "of-coordinator"] {
        let err = resolve(target).expect_err(target);
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::UNKNOWN_SESSION, "{target}");
    }
}

/// An unbound project session binds itself to a running console session of its console and
/// becomes a bound coordinator keeping its own sessions, or, with none of its own, to a running
/// project session of its project that can own sessions and becomes an ordinary bound one.
/// Each refusal names its reason, carries its own code and the session (the owner too, for an owner
/// that cannot own it), and changes nothing.
#[tokio::test]
async fn a_session_binds_itself_only_where_orchestration_stays_two_levels_deep() {
    use SessionStatus::{Archived, Idle, Interrupted, Starting};
    let (state, _dir, _live) = binding_fixture(
        "bind",
        vec![
            console_session("hub", Idle),
            console_session("sleeping-hub", Interrupted),
            console_session("starting-hub", Starting),
            // Reads running, but has no process behind it.
            console_session("ghost-hub", Idle),
            console_session("archived-hub", Archived),
            Session {
                console_id: "console-2".to_string(),
                ..console_session("foreign-hub", Idle)
            },
            project_session("starter", AgentId::CLAUDE, Idle),
            project_session("other-starter", AgentId::CLAUDE, Idle),
            project_session("loner", AgentId::CLAUDE, Idle),
            project_session("archived-loner", AgentId::CLAUDE, Archived),
            // Its only session of its own is archived, which still counts as one.
            project_session("with-archived-child", AgentId::CLAUDE, Idle),
            bound_session("archived-child", Archived, "with-archived-child"),
            Session {
                project_id: Some("project-2".to_string()),
                ..project_session("elsewhere", AgentId::CLAUDE, Idle)
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

    // (session, owner, the reason it names, the code, whether the refusal names the owner)
    let owner_cannot_own = error_code::OWNER_CANNOT_OWN;
    for (session, owner, reason, code, names_owner) in [
        (
            "bound",
            "hub",
            "already bound",
            error_code::SESSION_ALREADY_BOUND,
            false,
        ),
        (
            "hub",
            "starter",
            "console session cannot be bound",
            error_code::CONSOLE_SESSION_NEVER_BOUND,
            false,
        ),
        (
            "archived-loner",
            "hub",
            "is archived",
            error_code::SESSION_ARCHIVED,
            false,
        ),
        (
            "starter",
            "other-starter",
            "has sessions of its own (`mine`)",
            owner_cannot_own,
            true,
        ),
        (
            "with-archived-child",
            "other-starter",
            "has sessions of its own (`archived-child`)",
            owner_cannot_own,
            true,
        ),
        (
            "loner",
            "mine",
            "cannot own sessions",
            owner_cannot_own,
            true,
        ),
        (
            "loner",
            "elsewhere",
            "another project",
            owner_cannot_own,
            true,
        ),
        (
            "loner",
            "foreign-hub",
            "another console",
            owner_cannot_own,
            true,
        ),
        (
            "loner",
            "sleeping-hub",
            "not running",
            owner_cannot_own,
            true,
        ),
        (
            "loner",
            "starting-hub",
            "not running",
            owner_cannot_own,
            true,
        ),
        ("loner", "ghost-hub", "not running", owner_cannot_own, true),
        (
            "loner",
            "archived-hub",
            "not running",
            owner_cannot_own,
            true,
        ),
        ("loner", "loner", "its own owner", owner_cannot_own, true),
    ] {
        let before = state.store.get_session(session).unwrap().unwrap();
        let err = bind_session(&state, session, owner).expect_err(reason);
        assert!(err.to_string().contains(reason), "{session} {owner}: {err}");
        assert_eq!(code_of(&err), code, "{session} {owner}: {err}");
        let coded = err.downcast_ref::<CodedError>().expect("a coded refusal");
        assert_eq!(
            coded.params.get("session").map(String::as_str),
            Some(session),
            "{session} {owner}"
        );
        assert_eq!(
            coded.params.get("owner").map(String::as_str),
            names_owner.then_some(owner),
            "{session} {owner}"
        );
        let after = state.store.get_session(session).unwrap().unwrap();
        assert_eq!(
            (before.bound_to, before.role),
            (after.bound_to, after.role),
            "{session} {owner}"
        );
    }
    let loner = state.store.get_session("loner").unwrap().unwrap();
    assert_eq!((loner.bound_to, loner.role), (None, Role::Coordinator));

    let (coordinator, _, _) = bind_session(&state, "starter", "hub").unwrap();
    assert_eq!(
        (coordinator.bound_to.as_deref(), coordinator.role),
        (Some("hub"), Role::Coordinator)
    );
    assert!(!coordinator.is_worker());
    let mine = state.store.get_session("mine").unwrap().unwrap();
    assert_eq!(mine.bound_to.as_deref(), Some("starter"));

    let (joined, _, _) = bind_session(&state, "loner", "other-starter").unwrap();
    assert_eq!(
        (joined.bound_to.as_deref(), joined.role),
        (Some("other-starter"), Role::Worker)
    );
    assert!(joined.is_worker());
    let again = bind_session(&state, "loner", "hub").expect_err("bound already");
    assert!(again.to_string().contains("already bound"), "{again}");
}

/// An owner hands a running session of its own to another running owner under the same rules: a
/// bound coordinator keeps being one under another console session, goes to a project session only
/// once it has no sessions of its own, and then can own none. Each refusal names its reason and
/// changes nothing.
#[tokio::test]
async fn an_owner_transfers_a_session_only_where_orchestration_stays_two_levels_deep() {
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
            bound_coordinator("coordinator", Idle, "hub"),
            bound_session("child", Idle, "coordinator"),
            bound_coordinator("spare-coordinator", Idle, "hub"),
            bound_session("plain", Idle, "hub"),
            bound_session("no-process", Interrupted, "hub"),
            bound_session("starting", Starting, "hub"),
            project_session("peer", AgentId::CLAUDE, Idle),
        ],
        &[
            "hub",
            "hub-2",
            "foreign-hub",
            "coordinator",
            "child",
            "spare-coordinator",
            "plain",
            "peer",
        ],
    );

    // A session being launched holds the launch claim, which refuses a binding change.
    let _claim = state.begin_launch("starting").unwrap();
    for (session, from, to, reason) in [
        (
            "coordinator",
            "hub",
            "peer",
            "has sessions of its own (`child`)",
        ),
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
        assert_eq!((before.bound_to, before.role), (after.bound_to, after.role));
    }

    // A session that becomes a bound one loses the report panel's tools, so its pages are
    // withdrawn; a bound coordinator keeps them.
    for (page, session) in [("l-1", "coordinator"), ("sl-1", "spare-coordinator")] {
        state
            .store
            .insert_page(&crate::test_support::test_page(page, session, 1))
            .unwrap();
    }
    let (coordinator, ..) = transfer_session(&state, "coordinator", "hub", "hub-2").unwrap();
    assert_eq!(
        (coordinator.bound_to.as_deref(), coordinator.role),
        (Some("hub-2"), Role::Coordinator)
    );
    assert_eq!(state.store.list_pages("coordinator").unwrap().len(), 1);
    let child = state.store.get_session("child").unwrap().unwrap();
    assert_eq!(child.bound_to.as_deref(), Some("coordinator"));

    let (plain, ..) = transfer_session(&state, "plain", "hub", "peer").unwrap();
    assert_eq!(
        (plain.bound_to.as_deref(), plain.role),
        (Some("peer"), Role::Worker)
    );
    let (spare, owner, _) = transfer_session(&state, "spare-coordinator", "hub", "peer").unwrap();
    assert_eq!(
        (spare.bound_to.as_deref(), spare.role),
        (Some("peer"), Role::Worker)
    );
    assert!(spare.is_worker());
    assert_eq!(owner.id, "peer");
    assert!(state
        .store
        .list_pages("spare-coordinator")
        .unwrap()
        .is_empty());
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
            project_session("loner", AgentId::CLAUDE, Idle),
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

    let (loner, _, _) = bind_session(&state, "loner", "hub").unwrap();
    assert_eq!(loner.bound_to.as_deref(), Some("hub"));
    let (plain, ..) = transfer_session(&state, "plain", "hub", "hub-2").unwrap();
    assert_eq!(plain.bound_to.as_deref(), Some("hub-2"));
}

/// A bound session, running or not, is released to an ordinary unbound project session: a
/// bound coordinator keeps the sessions bound to it, and one the user releases needs no owner
/// named. Each refusal names its reason and changes nothing.
#[tokio::test]
async fn a_session_is_released_into_an_unbound_one_that_keeps_its_own_sessions() {
    use SessionStatus::{Idle, Interrupted};
    let (state, _dir, _live) = binding_fixture(
        "release",
        vec![
            console_session("hub", Idle),
            bound_coordinator("coordinator", Idle, "hub"),
            bound_session("child", Idle, "coordinator"),
            bound_session("plain", Idle, "hub"),
            bound_session("no-process", Interrupted, "hub"),
            bound_session("by-user", Idle, "hub"),
            project_session("loner", AgentId::CLAUDE, Idle),
        ],
        &["hub", "coordinator", "child", "plain", "by-user"],
    );

    for (session, owner, code) in [
        ("hub", Some("hub"), error_code::CONSOLE_SESSION_NEVER_BOUND),
        ("loner", Some("hub"), error_code::SESSION_NOT_BOUND),
        ("loner", None, error_code::SESSION_NOT_BOUND),
        (
            "plain",
            Some("coordinator"),
            error_code::SESSION_NOT_BOUND_TO_OWNER,
        ),
        ("child", Some("hub"), error_code::SESSION_NOT_BOUND_TO_OWNER),
    ] {
        let before = state.store.get_session(session).unwrap().unwrap();
        let err = release_session(&state, session, owner, None).expect_err(code.as_str());
        assert_eq!(code_of(&err), code, "{session}: {err}");
        let after = state.store.get_session(session).unwrap().unwrap();
        assert_eq!((before.bound_to, before.role), (after.bound_to, after.role));
    }

    let coordinator = release_session(&state, "coordinator", Some("hub"), None)
        .unwrap()
        .session;
    assert!(!coordinator.is_worker());
    assert_eq!(
        (coordinator.bound_to, coordinator.role),
        (None, Role::Coordinator)
    );
    let child = state.store.get_session("child").unwrap().unwrap();
    assert_eq!(child.bound_to.as_deref(), Some("coordinator"));

    let plain = release_session(&state, "plain", Some("hub"), None)
        .unwrap()
        .session;
    assert_eq!((plain.bound_to, plain.role), (None, Role::Coordinator));
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
    let (rebound, _, _) = bind_session(&state, "plain", "hub").unwrap();
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
            no_process_with_conversation("released", AgentId::CLAUDE, "hub"),
            no_process_with_conversation("handed", AgentId::CLAUDE, "hub"),
            no_process_with_conversation("replaced", AgentId::CLAUDE, "hub"),
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
        owner_notices::tell_released(&state, &released, "hub", running, None)
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
        .set_binding("replaced", None, "hub-2", Role::Coordinator, false, false)
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

/// A failed launch keeps the notice owed. A successful launch sends it in the resume prompt
/// if the provider takes an instruction there, or queues it for delivery once it takes input.
#[tokio::test]
async fn a_failed_launch_keeps_the_notice_and_a_resumed_launch_delivers_it() {
    use SessionStatus::Idle;
    let takes_prompt = |agent: AgentId| {
        agent
            .descriptor()
            .capabilities
            .delivery
            .resume_instruction_as_launch_prompt
    };
    let with_prompt = AgentId::ALL
        .into_iter()
        .find(|agent| takes_prompt(*agent))
        .unwrap();
    let (state, _dir, _live) = binding_fixture(
        "owner-notice-launch",
        std::iter::once(console_session("hub", Idle))
            .chain(
                AgentId::ALL
                    .into_iter()
                    .map(|agent| no_process_with_conversation(agent.wire_name(), agent, "hub")),
            )
            .collect(),
        &["hub"],
    );
    for agent in AgentId::ALL {
        release_session(&state, agent.wire_name(), None, None).unwrap();
    }

    let id = with_prompt.wire_name();
    relaunch_stand_in(&state, id, true)
        .await
        .expect_err("the launch fails");
    assert_eq!(state.store.owner_notice_pending(id).unwrap(), 1);
    assert!(state.take_queued(id).is_empty());
    assert_eq!(
        state.store.get_session(id).unwrap().unwrap().status,
        SessionStatus::Interrupted
    );

    for agent in AgentId::ALL {
        let id = agent.wire_name();
        let (prompt, queued) = relaunch_stand_in(&state, id, false).await.unwrap();
        if takes_prompt(agent) {
            assert!(
                prompt.unwrap().contains("you have no parent session now"),
                "{agent:?}"
            );
        } else {
            assert!(prompt.is_none(), "{agent:?}: {prompt:?}");
            assert_eq!(queued.len(), 1, "{agent:?}: {queued:?}");
            assert!(
                queued[0].contains("you have no parent session now"),
                "{agent:?}: {queued:?}"
            );
        }
        assert_eq!(
            state.store.owner_notice_pending(id).unwrap(),
            0,
            "{agent:?}"
        );
    }
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
            no_process_with_conversation("noted", AgentId::CLAUDE, "hub"),
            no_process_with_conversation("blank", AgentId::CLAUDE, "hub"),
            no_process_with_conversation("moved", AgentId::CLAUDE, "hub"),
            no_process_with_conversation("rebound", AgentId::CLAUDE, "hub"),
            no_process_with_conversation("ghosted", AgentId::CLAUDE, "hub"),
        ],
        &["hub", "hub-2"],
    );

    let Released {
        session: noted,
        running,
        ..
    } = release_session(&state, "noted", Some("hub"), Some("Pick up at step 3.")).unwrap();
    assert!(!running);
    assert!(owner_notices::tell_released(
        &state,
        &noted,
        "hub",
        running,
        Some("Pick up at step 3.")
    )
    .await
    .unwrap()
    .is_none());
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
    // The former parent session is named, and the note follows as a quote, word for word.
    assert!(
        prompt.contains(
            "Message from Octoboard: you have no parent session now. Your role is coordinator \
             and your reporting relationship is independent. You were released from the console \
             session `hub` (\"hub\"), the session you reported to, and you are the user's own \
             session: take work only from the user typing in this terminal, and do not call \
             `mcp__octoboard__report`, which is refused for a session with no parent session. \
             This replaces what you were told of your parent session when you were started. "
        ),
        "{prompt}"
    );
    assert!(
        prompt.contains(
            "Your former parent session left this note:\n\n> Pick up at step 3.\n\
             — end of the note —"
        ),
        "{prompt}"
    );
    assert_eq!(state.store.owner_notice_pending("noted").unwrap(), 0);
    assert_eq!(state.store.owner_notice_note("noted").unwrap(), None);

    // A former owner whose record cannot be read is named by its id alone.
    release_session(&state, "ghosted", Some("hub"), None).unwrap();
    state.store.clear_owner_notice("ghosted", 1).unwrap();
    state
        .store
        .mark_released_notice("ghosted", "ghost", None)
        .unwrap();
    let (prompt, _) = relaunch_stand_in(&state, "ghosted", false).await.unwrap();
    let prompt = prompt.unwrap();
    assert!(
        prompt.contains(
            "You were released from the session `ghost`, the session you reported to, and you \
             are the user's own session: "
        ),
        "{prompt}"
    );
    assert!(!prompt.contains("former parent session left"), "{prompt}");

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
    let (moved, _, _) = bind_session(&state, "moved", "hub-2").unwrap();
    assert_eq!(moved.bound_to.as_deref(), Some("hub-2"));
    assert_eq!(state.store.owner_notice_note("moved").unwrap(), None);
    release_session(&state, "rebound", Some("hub"), Some("Pick up at step 3.")).unwrap();
    state
        .store
        .set_binding("rebound", None, "hub-2", Role::Coordinator, false, true)
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
            no_process_with_conversation("gone", AgentId::CLAUDE, "hub"),
        ],
        &["hub"],
    );

    // Judged running, as at the release, but no process is there to write to.
    let released = release_session(&state, "gone", Some("hub"), Some("Mind step 3."))
        .unwrap()
        .session;
    state.store.clear_owner_notice("gone", 1).unwrap();
    assert!(
        owner_notices::tell_released(&state, &released, "hub", true, Some("Mind step 3."))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(state.store.owner_notice_pending("gone").unwrap(), 1);
    let (prompt, _) = relaunch_stand_in(&state, "gone", false).await.unwrap();
    let prompt = prompt.unwrap();
    assert!(prompt.contains("> Mind step 3."), "{prompt}");
    assert!(
        prompt.contains("released from the console session `hub` (\"hub\"), the session"),
        "{prompt}"
    );
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
    let plain = running(&state, "plain");
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
    // The released session is told which session it was released from.
    let heard = output_containing(&plain, "the session you reported to");
    assert!(
        heard.contains("You were released from the console session `hub` ("),
        "{heard}"
    );
    let told = output_containing(&hub, "released");
    assert!(told.contains("`plain`"), "{told}");
    assert!(told.contains("Retry audit"), "{told}");
    assert!(
        told.contains("no longer instruct, drive or archive it"),
        "{told}"
    );
    assert!(told.contains("left with the user"), "{told}");
    let inform_session =
        session_tools::catalogue::qualified_tool_name(AgentId::CLAUDE, "inform_session");
    assert!(told.contains(&format!("`{inform_session}`")), "{told}");
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
/// who it reports to through `owner_of`: a coordinator is told its console session, which makes
/// its role description name it, while a session bound to it is told the bound coordinator owns it.
#[test]
fn a_bound_coordinator_is_told_its_console_session_at_launch() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-bound-launch");
    let coordinator = bound_coordinator("coordinator", SessionStatus::Interrupted, "hub");
    let of_coordinator = bound_session("of-coordinator", SessionStatus::Interrupted, "coordinator");
    for session in [
        console_session("hub", SessionStatus::Idle),
        coordinator.clone(),
        of_coordinator.clone(),
    ] {
        state.store.insert_session(&session).unwrap();
    }

    assert_eq!(
        owner_of(&state, &coordinator).unwrap(),
        Some(session_tools::Owner::ConsoleOfCoordinator)
    );
    assert_eq!(
        owner_of(&state, &console_session("hub", SessionStatus::Idle)).unwrap(),
        None
    );
    assert_eq!(
        owner_of(&state, &of_coordinator).unwrap(),
        Some(session_tools::Owner::Project {
            session_id: "coordinator".to_string()
        })
    );
}

/// A relaunch that owes the session a notice and carries an owner's instruction hands both over
/// as one message: the notice, then the instruction whole, with its own header.
#[tokio::test]
async fn an_instruction_after_an_owed_notice_keeps_who_sent_it() {
    use crate::messaging::texts;
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "owner-notice-instruction",
        vec![
            console_session("hub", Idle),
            no_process_with_conversation("owned", AgentId::CLAUDE, "hub"),
        ],
        &["hub"],
    );
    release_session(&state, "owned", None, None).unwrap();
    let hub = state.session_record("hub").unwrap();
    let instruction = texts::render_instruction(&hub, "Run the tests.");

    let (prompt, _) = relaunch_stand_in_with(&state, "owned", false, Some(&instruction))
        .await
        .unwrap();
    let prompt = prompt.unwrap();
    let notice = prompt.strip_suffix(&format!("\n\n{instruction}")).unwrap();
    assert!(notice.starts_with(texts::NOTICE_PREFIX), "{prompt}");
}

/// Bound to a console session a session stays the coordinator it is, with its report-panel pages;
/// bound to a project coordinator it becomes a worker, and its pages are withdrawn.
#[tokio::test]
async fn a_bind_sets_the_role_the_owner_makes_it_and_withdraws_a_new_workers_pages() {
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "bind-roles",
        vec![
            console_session("hub", Idle),
            project_session("under-console", AgentId::CLAUDE, Idle),
            project_session("under-project", AgentId::CLAUDE, Idle),
            project_session("coordinator", AgentId::CLAUDE, Idle),
        ],
        &["hub", "coordinator", "under-console", "under-project"],
    );
    for (page, session) in [("c-1", "under-console"), ("p-1", "under-project")] {
        state
            .store
            .insert_page(&crate::test_support::test_page(page, session, 1))
            .unwrap();
    }

    let (kept, owner, _) = bind_session(&state, "under-console", "hub").unwrap();
    assert_eq!(owner.id, "hub");
    assert_eq!(
        (kept.bound_to.as_deref(), kept.role),
        (Some("hub"), Role::Coordinator)
    );
    assert_eq!(state.store.list_pages("under-console").unwrap().len(), 1);

    let (worker, owner, _) = bind_session(&state, "under-project", "coordinator").unwrap();
    assert_eq!(owner.id, "coordinator");
    assert_eq!(
        (worker.bound_to.as_deref(), worker.role),
        (Some("coordinator"), Role::Worker)
    );
    assert!(state.store.list_pages("under-project").unwrap().is_empty());
    let stored = state.store.get_session("under-project").unwrap().unwrap();
    assert_eq!(
        (stored.bound_to.as_deref(), stored.role),
        (Some("coordinator"), Role::Worker)
    );
}

/// A bind says whether the session has a process to be told in. A running one is owed nothing; one
/// with none is left owed the notice for its next launch, which the binding write itself records,
/// and the launch delivers it, naming the new owner.
#[tokio::test]
async fn a_bind_leaves_a_session_with_no_process_owed_its_notice() {
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "bind-running",
        vec![
            console_session("hub", Idle),
            project_session("live", AgentId::CLAUDE, Idle),
            Session {
                bound_to: None,
                role: Role::Coordinator,
                ..no_process_with_conversation("asleep", AgentId::CLAUDE, "hub")
            },
        ],
        &["hub", "live"],
    );

    let (_, _, running) = bind_session(&state, "live", "hub").unwrap();
    assert!(running);
    assert_eq!(state.store.owner_notice_pending("live").unwrap(), 0);

    let (asleep, _, running) = bind_session(&state, "asleep", "hub").unwrap();
    assert!(!running);
    assert_eq!(asleep.bound_to.as_deref(), Some("hub"));
    assert_eq!(state.store.owner_notice_pending("asleep").unwrap(), 1);
    let (prompt, _) = relaunch_stand_in(&state, "asleep", false).await.unwrap();
    let prompt = prompt.unwrap();
    assert!(prompt.contains("`hub`"), "{prompt}");
    assert!(prompt.contains("parent session"), "{prompt}");
    // It had no parent when it was launched, so it is told what ending a round does under one.
    assert!(
        prompt.contains(
            "`done` with no open items archives you, and is refused while a session you started \
             is not archived; `needs_decision` or open items leave you running. Items that are \
             your parent session's to carry out rather than yours go in `owner_items`, not \
             `open_items`."
        ),
        "{prompt}"
    );
    assert_eq!(state.store.owner_notice_pending("asleep").unwrap(), 0);
}

/// A bind by the user tells both sessions: the one bound that the user set its parent, and the new
/// owner that the session is its own from now on. A session with no process is told at its next
/// launch instead, and the owner is told all the same. A refused bind tells nobody. The notices
/// follow the answer, from a task of their own, so each is waited for on the runtime: a wait that
/// blocks the test's thread would keep that task from running.
#[tokio::test]
async fn a_bind_by_the_user_tells_the_session_and_its_owner() {
    use crate::test_support::owner_team::{hub_heard, hub_heard_of};
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "bind-by-user",
        vec![
            console_session("hub", Idle),
            Session {
                title: "Retry audit".to_string(),
                ..project_session("loner", AgentId::CLAUDE, Idle)
            },
            Session {
                title: "Slow migration".to_string(),
                bound_to: None,
                role: Role::Coordinator,
                ..no_process_with_conversation("asleep", AgentId::CLAUDE, "hub")
            },
            bound_session("bound", Idle, "hub"),
        ],
        &[],
    );
    let hub = running(&state, "hub");
    let loner = running(&state, "loner");
    let bind = |session: &str| {
        handle(
            &state,
            None,
            RequestBody::BindSession {
                session: session.to_string(),
                owner: "hub".to_string(),
            },
        )
    };

    bind("loner").await.unwrap();
    let stored = state.store.get_session("loner").unwrap().unwrap();
    assert_eq!(stored.bound_to.as_deref(), Some("hub"));
    let told = hub_heard_of(&loner, "the user set").await;
    assert!(told.contains("your role is coordinator"), "{told}");
    assert!(told.contains("as your parent session"), "{told}");
    assert!(told.contains("`hub`"), "{told}");
    let told_owner = hub_heard_of(&hub, "the user bound").await;
    assert!(told_owner.contains("`loner`"), "{told_owner}");
    assert!(told_owner.contains("Retry audit"), "{told_owner}");
    assert!(
        told_owner.contains("It is yours from now on"),
        "{told_owner}"
    );
    assert!(
        told_owner.contains("It had no parent session before. It is being told so."),
        "{told_owner}"
    );
    assert_eq!(state.store.owner_notice_pending("loner").unwrap(), 0);

    // No process: the session is told at its next launch, the owner now.
    bind("asleep").await.unwrap();
    assert_eq!(
        state
            .store
            .get_session("asleep")
            .unwrap()
            .unwrap()
            .bound_to
            .as_deref(),
        Some("hub")
    );
    assert_eq!(state.store.owner_notice_pending("asleep").unwrap(), 1);
    let told_owner = hub_heard_of(&hub, "Slow migration").await;
    assert!(told_owner.contains("the user bound"), "{told_owner}");
    assert!(
        told_owner.contains("It is not running, and will be told so when it next starts."),
        "{told_owner}"
    );
    let (prompt, _) = relaunch_stand_in(&state, "asleep", false).await.unwrap();
    assert!(prompt.unwrap().contains("`hub`"));
    assert_eq!(state.store.owner_notice_pending("asleep").unwrap(), 0);

    // A refused bind changes nothing and tells nobody.
    let err = bind("bound").await.expect_err("already bound");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_BOUND);
    assert!(!hub_heard(&state, &hub).await.contains("`bound`"));
}

/// The sessions a bind by the user is about to tell: the console session `hub` and the project
/// session `loner`, both idle with a process of their own that hears what it is written. Another
/// console session, `hub2`, is on record without a process.
fn hub_and_loner(name: &str) -> (Arc<AppState>, ScratchDir, [StandIn; 2]) {
    use SessionStatus::Idle;
    let (state, dir, _live) = binding_fixture(
        name,
        vec![
            Session {
                title: "Retry audit".to_string(),
                ..project_session("loner", AgentId::CLAUDE, Idle)
            },
            console_session("hub", Idle),
            console_session("hub2", Idle),
        ],
        &[],
    );
    let hub = running(&state, "hub");
    let loner = running(&state, "loner");
    (state, dir, [hub, loner])
}

/// The user's request to bind `loner` to `hub`, answered.
async fn bind_loner_to_hub(state: &Arc<AppState>) {
    handle(
        state,
        None,
        RequestBody::BindSession {
            session: "loner".to_string(),
            owner: "hub".to_string(),
        },
    )
    .await
    .unwrap();
}

/// The request to bind `loner` to `hub` is answered, and then `hub`'s process is taken down under
/// a claim, as an account switch or a handoff of `hub` does: from then on the owner cannot be
/// written to until the claim ends and a process is back, which is what the owner's notice waits
/// for.
///
/// The notices run in a task spawned by the request. A test on a current-thread runtime (the
/// tests that use this, and the one after it, say so) lets that task run only once the test awaits
/// something, and the request path awaits nothing after the spawn: so nothing is awaited between
/// the answer and the claim, and the claim is there before the first notice is written, whatever
/// the machine's speed. On a multi-thread runtime the task could already be running.
async fn bind_while_the_owner_is_taken_down(
    state: &Arc<AppState>,
    hub: &StandIn,
) -> crate::state::LaunchClaim {
    bind_loner_to_hub(state).await;
    let claim = state.begin_switch("hub").unwrap();
    assert!(state.remove_live(hub), "hub's process was the live one");
    claim
}

/// The user's request is answered as soon as the binding is written, and the owner is told after
/// the answer: a process the owner gets only once the request has been answered, under the claim
/// that ended it, hears the notice, which a request that told the owner before answering could not
/// have left for it.
#[tokio::test(flavor = "current_thread")]
async fn a_bind_by_the_user_is_answered_before_the_owner_is_told() {
    use crate::test_support::owner_team::hub_heard_of;
    let (state, _dir, [hub, _loner]) = hub_and_loner("bind-answered-first");

    let claim = bind_while_the_owner_is_taken_down(&state, &hub).await;

    // Answered, and bound, with the owner not yet told.
    let stored = state.store.get_session("loner").unwrap().unwrap();
    assert_eq!(stored.bound_to.as_deref(), Some("hub"));

    // The claim ends with a process back, as a resume or a switch ends.
    let relaunched = running(&state, "hub");
    drop(claim);
    let told_owner = hub_heard_of(&relaunched, "the user bound").await;
    assert!(told_owner.contains("`loner`"), "{told_owner}");
    assert!(told_owner.contains("Retry audit"), "{told_owner}");
    assert!(
        told_owner.contains("It is yours from now on"),
        "{told_owner}"
    );
}

/// The session is told before its owner: while the owner's notice is held up on a claim, the
/// session's own notice arrives, and the owner is told only afterwards. An order the other way
/// round would leave the session untold for as long as the owner's claim lasts, which is as long
/// as the owner's write waits for it ([`PATIENCE`] under test), so the wait for the session's
/// notice ends at half of that and a reversed order fails here, not by a race of equal waits.
#[tokio::test(flavor = "current_thread")]
async fn a_bind_by_the_user_tells_the_session_before_its_owner() {
    use crate::test_support::owner_team::hub_heard_of;
    let (state, _dir, [hub, loner]) = hub_and_loner("bind-session-first");

    let claim = bind_while_the_owner_is_taken_down(&state, &hub).await;

    // The owner has no process, and its claim is held until this test ends it: the session's
    // notice arrives with the owner's still to be written.
    let watched = (*loner).clone();
    assert!(
        within(PATIENCE / 2, move || {
            String::from_utf8_lossy(&watched.recent_output(8 * 1024)).contains("the user set")
        })
        .await,
        "the session was not told while its owner's notice waited on the claim"
    );
    let told = hub_heard_of(&loner, "the user set").await;
    assert!(told.contains("as your parent session"), "{told}");
    assert!(told.contains("`hub`"), "{told}");
    // Told now, so nothing is owed it.
    assert_eq!(state.store.owner_notice_pending("loner").unwrap(), 0);

    let relaunched = running(&state, "hub");
    drop(claim);
    let told_owner = hub_heard_of(&relaunched, "the user bound").await;
    assert!(told_owner.contains("`loner`"), "{told_owner}");
}

/// How many tasks the test's runtime has alive, for [`notices_have_ended`].
fn alive_tasks() -> usize {
    tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks()
}

/// Waits until the task a request spawned to tell the sessions has ended, `before` being
/// [`alive_tasks`] read before the request: what a test needs before it asserts that an owner was
/// not told, since a marker written to the owner orders only against writes already made.
///
/// It relies on the request path spawning exactly one tokio task, the one that tells the sessions,
/// which [`one_task_spawned`] asserts right after the request: a count that fell back to `before`
/// because some other task ended would end the wait early and make the absence asserted after it
/// vacuous.
async fn notices_have_ended(before: usize) {
    let runtime = tokio::runtime::Handle::current();
    assert!(
        within(PATIENCE, move || {
            runtime.metrics().num_alive_tasks() <= before
        })
        .await,
        "the task telling the sessions of the bind ended"
    );
}

/// Asserts that the request made since `before` ([`alive_tasks`]) spawned exactly one task, which
/// has not run yet on a current-thread runtime that has not been awaited on since.
fn one_task_spawned(before: usize) {
    assert_eq!(
        alive_tasks(),
        before + 1,
        "the bind request spawned exactly the one task that tells the sessions"
    );
}

/// A session the user released from its new owner before the owner's notice is written is not
/// told to that owner: it would say the session is the owner's when it is not. The owner is not
/// refused the binding, only the notice, and it is not written once its process is back either.
#[tokio::test(flavor = "current_thread")]
async fn a_bind_by_the_user_is_not_told_to_an_owner_the_session_was_released_from() {
    use crate::test_support::owner_team::hub_heard;
    let (state, _dir, [hub, _loner]) = hub_and_loner("bind-released-before-owner-told");
    let before = alive_tasks();

    bind_loner_to_hub(&state).await;
    one_task_spawned(before);
    // Before the task that tells them has run (see `bind_while_the_owner_is_taken_down`).
    release_session(&state, "loner", None, None).unwrap();

    notices_have_ended(before).await;
    let heard = hub_heard(&state, &hub).await;
    assert!(!heard.contains("the user bound"), "{heard}");
    let stored = state.store.get_session("loner").unwrap().unwrap();
    assert_eq!(stored.bound_to, None);
}

/// The same for a session handed to another owner meanwhile: its first owner is not told it is
/// theirs.
#[tokio::test(flavor = "current_thread")]
async fn a_bind_by_the_user_is_not_told_to_an_owner_the_session_was_transferred_from() {
    use crate::test_support::owner_team::hub_heard;
    let (state, _dir, [hub, _loner]) = hub_and_loner("bind-transferred-before-owner-told");
    let _hub2 = running(&state, "hub2");
    let before = alive_tasks();

    bind_loner_to_hub(&state).await;
    one_task_spawned(before);
    transfer_session(&state, "loner", "hub", "hub2").unwrap();

    notices_have_ended(before).await;
    let heard = hub_heard(&state, &hub).await;
    assert!(!heard.contains("the user bound"), "{heard}");
    let stored = state.store.get_session("loner").unwrap().unwrap();
    assert_eq!(stored.bound_to.as_deref(), Some("hub2"));
}

/// A session that leaves the owner while the owner's notice waits out a claim on it is not told to
/// the owner once its process is back. Nothing but a change that does not look at the owner's
/// claim can release a session then (a release is refused for a claimed owner), so the binding is
/// cleared in the store, as the release does.
#[tokio::test(flavor = "current_thread")]
async fn a_bind_by_the_user_is_not_told_to_an_owner_after_a_claim_on_it_when_the_session_left() {
    use crate::test_support::owner_team::{hub_heard, hub_heard_of};
    let (state, _dir, [hub, loner]) = hub_and_loner("bind-left-during-claim");
    let before = alive_tasks();

    let claim = bind_while_the_owner_is_taken_down(&state, &hub).await;
    one_task_spawned(before);
    // The session's notice is out, so the owner's is next: it waits on the claim, or is refused as
    // it starts, whichever the machine's speed has it reach first.
    let told = hub_heard_of(&loner, "the user set").await;
    assert!(told.contains("as your parent session"), "{told}");
    state
        .store
        .clear_binding("loner", "hub", false, None)
        .unwrap()
        .expect("released");

    let relaunched = running(&state, "hub");
    drop(claim);
    notices_have_ended(before).await;
    let heard = hub_heard(&state, &relaunched).await;
    assert!(!heard.contains("the user bound"), "{heard}");
}

/// A running session whose notice cannot be written (its process is gone by the time it is told)
/// is left owed the notice for its next launch, the binding stands, and the owner is still told.
#[tokio::test(flavor = "current_thread")]
async fn a_bind_by_the_user_leaves_a_session_owed_the_notice_it_could_not_be_told() {
    use crate::test_support::owner_team::hub_heard_of;
    let (state, _dir, [hub, loner]) = hub_and_loner("bind-write-fails");

    bind_loner_to_hub(&state).await;
    // Bound while it ran, so nothing is owed yet; its process ends before the notice, which runs in
    // a task that has not started (see `bind_while_the_owner_is_taken_down`).
    assert_eq!(state.store.owner_notice_pending("loner").unwrap(), 0);
    assert!(
        state.remove_live(&loner),
        "loner's process was the live one"
    );

    // The owner is told after the session's turn, so once it is, the session's failure is recorded.
    let told_owner = hub_heard_of(&hub, "the user bound").await;
    assert!(told_owner.contains("`loner`"), "{told_owner}");
    // The owner is not told the session is being told: it could not be, and is told at its next
    // launch.
    assert!(
        told_owner.contains("It could not be told now, and will be told so when it next starts."),
        "{told_owner}"
    );
    assert!(!told_owner.contains("It is being told so."), "{told_owner}");
    let stored = state.store.get_session("loner").unwrap().unwrap();
    assert_eq!(stored.bound_to.as_deref(), Some("hub"));
    assert_eq!(state.store.owner_notice_pending("loner").unwrap(), 1);
}

/// An owner that stops being able to own sessions between the check and the write (here, one whose
/// record is no coordinator, which the check does not look at for a console session) refuses the
/// bind as an owner that cannot own it, naming the session and the owner, and changes nothing; the
/// session's own binding cannot have changed under its claim, so it is never reported as already
/// bound.
#[tokio::test]
async fn an_owner_that_cannot_own_by_the_time_of_the_write_is_refused_as_such() {
    use SessionStatus::Idle;
    let (state, _dir, _live) = binding_fixture(
        "bind-owner-changed",
        vec![
            Session {
                role: Role::Worker,
                ..console_session("hub", Idle)
            },
            project_session("loner", AgentId::CLAUDE, Idle),
        ],
        &["hub", "loner"],
    );

    let err = bind_session(&state, "loner", "hub").expect_err("owner cannot own");
    assert_eq!(code_of(&err), error_code::OWNER_CANNOT_OWN, "{err}");
    let coded = err.downcast_ref::<CodedError>().expect("a coded refusal");
    assert_eq!(
        coded.params.get("session").map(String::as_str),
        Some("loner")
    );
    assert_eq!(coded.params.get("owner").map(String::as_str), Some("hub"));
    assert_eq!(
        state.store.get_session("loner").unwrap().unwrap().bound_to,
        None
    );
    assert_eq!(state.store.owner_notice_pending("loner").unwrap(), 0);
}
