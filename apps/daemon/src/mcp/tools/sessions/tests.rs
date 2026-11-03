use super::*;
use crate::coordinator;
use crate::mcp::dispatch::call;
use crate::mcp::test_support::{
    account, arguments, console, console_session, peer_state, project_session, refused,
    settings_state, shared_console_state, wait_for_request, weekly, with_status, Launches,
    RepoState, Started,
};
use crate::protocol::{
    AccountQuota, Agent, AgentAvailability, Availability, Event, Project, ProjectAccount,
    ProjectSource, QuotaWindow, QuotaWindowKind, Role, Session, SessionStatus,
};
use crate::session_request;
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{running, ScratchDir};
use crate::time::now_millis;
use anyhow::bail;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// The refusal `coordinator::open::open_session` raises for an unavailable agent is not only
/// the control socket's — a console session reaches it through its own `new_session` tool, and
/// comes back with a tool error carrying the reason in prose, as "The console session's tools"
/// in `docs/product/hub-orchestration.md` promises for any refusal.
#[tokio::test]
async fn an_unavailable_agent_refuses_the_console_sessions_own_new_session_tool() {
    let (state, _dir) = crate::test_support::app_state("mcp-exec-new-session-agent-unavailable");
    state.store.insert_console(&console()).unwrap();
    state
        .store
        .insert_project(&Project {
            id: "project-1".to_string(),
            console_id: "console-1".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: "Project".to_string(),
            path: "/tmp/project-1".to_string(),
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
    let owner = console_session("owner");
    state.store.insert_session(&owner).unwrap();
    state.set_agent_availability(vec![AgentAvailability {
        agent: Agent::Claude,
        availability: Availability::Unavailable,
        default_account_dir: Some("/home/user/.claude".to_string()),
    }]);

    let arguments = json!({
        "project": "project-1",
        "brief": { "goal": "do the thing" },
    })
    .as_object()
    .unwrap()
    .clone();

    let err = call(&state, &owner.id, "new_session", &arguments)
        .await
        .expect_err("refused");
    assert!(err.to_string().contains("Claude Code"), "{err}");
}

/// An agent that is not added is refused to an agent's own `new_session` and `request_session`
/// alike, the latter before the user is asked.
#[tokio::test]
async fn an_agent_that_is_not_added_is_refused_to_new_session_and_request_session() {
    let (state, _dir) = shared_console_state("new-session-agent-not-added");
    state
        .store
        .set_added_agents_if_undecided(&[Agent::Codex])
        .unwrap();
    let brief = json!({ "brief": { "goal": "do the thing" }, "agent": "claude" });
    let mut new = arguments(brief.clone());
    new.insert("project".to_string(), json!("project-1"));
    let mut request = arguments(brief);
    request.insert("kind".to_string(), json!("project"));
    request.insert("project".to_string(), json!("project-1"));

    for (tool, args) in [("new_session", new), ("request_session", request)] {
        let err = call(&state, "a", tool, &args).await.expect_err("refused");
        assert!(err.to_string().contains("is not added"), "{tool}: {err}");
        assert!(err.to_string().contains("Agent accounts"), "{tool}: {err}");
        assert!(!state.session_requests.is_waiting_for("a"));
    }
}

/// A session request is refused before the user is asked whenever the same call to
/// `new_session` would be, and so is one whose arguments do not say where the session would
/// be.
#[tokio::test]
async fn a_session_request_is_refused_before_the_user_is_asked() {
    let (state, _dir) = shared_console_state("request-refused-early");
    state.set_agent_availability(vec![AgentAvailability {
        agent: Agent::Claude,
        availability: Availability::Unavailable,
        default_account_dir: Some("/home/user/.claude".to_string()),
    }]);
    for (caller, extra, expected) in [
        ("loose", json!({ "kind": "project" }), "Claude Code"),
        ("loose", json!({ "kind": "nobody" }), "`kind` is `nobody`"),
        ("a", json!({ "kind": "project" }), "`project` is required"),
        (
            "loose",
            json!({ "kind": "project", "console": "console-1" }),
            "`console` is for",
        ),
        (
            "loose",
            json!({ "kind": "console", "console": "nowhere" }),
            "no console",
        ),
    ] {
        let mut args = arguments(json!({ "brief": { "goal": "do the thing" } }));
        args.extend(extra.as_object().unwrap().clone());
        let err = call(&state, caller, "request_session", &args)
            .await
            .expect_err("refused");
        assert!(err.to_string().contains(expected), "{err}");
        assert!(!state.session_requests.is_waiting_for(caller));
    }
}

/// Where the session asked for would be: the named project of whichever console, a console
/// session of the caller's console, or, for a project session naming none, its own project.
#[tokio::test]
async fn a_session_request_is_for_wherever_the_call_names() {
    let (state, _dir) = shared_console_state("request-where");
    for (caller, extra, (role, console, project)) in [
        (
            "a",
            json!({ "kind": "project", "project": "project-2" }),
            (Role::Project, "console-2", Some("project-2")),
        ),
        (
            "loose",
            json!({ "kind": "console" }),
            (Role::Console, "console-1", None),
        ),
        (
            "of-a",
            json!({ "kind": "project" }),
            (Role::Project, "console-1", Some("project-1")),
        ),
    ] {
        let mut args = arguments(json!({ "brief": { "goal": "do the thing" } }));
        args.extend(extra.as_object().unwrap().clone());
        let Event::SessionRequest {
            role: waiting_role,
            console: waiting_console,
            project: waiting_project,
            ..
        } = withdrawn_request(&state, caller, args).await
        else {
            unreachable!()
        };
        assert_eq!(
            (
                waiting_role,
                waiting_console.as_str(),
                waiting_project.as_deref()
            ),
            (role, console, project),
            "{caller}"
        );
    }
}

/// The branches a request hands over are the caller's to give in the project's own repository:
/// anything else is refused before the user is asked, and what is accepted reaches the dialog
/// once and by name.
#[tokio::test]
async fn a_session_request_hands_over_only_branches_the_caller_is_responsible_for() {
    let (state, _dir) = shared_console_state("request-branches");
    let repo = crate::test_support::repo_with("request-branches-repo", &[(b"a", b"a")]);
    state
        .store
        .insert_project(&Project {
            id: "repo".to_string(),
            name: "repo".to_string(),
            path: repo.to_string_lossy().into_owned(),
            ..state.store.get_project("project-1").unwrap().unwrap()
        })
        .unwrap();
    crate::test_support::branch_made_by(&state, &repo, "mine", "loose");
    crate::test_support::branch_made_by(&state, &repo, "theirs", "a");
    crate::test_support::git(&repo, &["branch", "unrecorded"]);
    let request = |extra: Value| {
        let mut args = arguments(json!({ "brief": { "goal": "do the thing" } }));
        args.extend(extra.as_object().unwrap().clone());
        args
    };

    for (extra, expected) in [
        (
            json!({ "kind": "project", "project": "repo", "branches": ["theirs"] }),
            "`theirs` is not a branch you are responsible for",
        ),
        (
            json!({ "kind": "project", "project": "repo", "branches": ["mine", "unrecorded"] }),
            "`unrecorded` is not a branch you are responsible for",
        ),
        (
            json!({ "kind": "project", "project": "repo", "branches": ["mine", "absent"] }),
            "`absent` is not a branch you are responsible for",
        ),
        (
            json!({ "kind": "project", "project": "repo", "branches": ["mine", " "] }),
            "blank name",
        ),
        (
            json!({ "kind": "project", "project": "repo", "branches": "mine" }),
            "array of strings",
        ),
        (
            json!({ "kind": "project", "project": "project-2", "branches": ["mine"] }),
            "not in a git repository",
        ),
        (
            json!({ "kind": "console", "branches": ["mine"] }),
            "`branches` is for `kind: project`",
        ),
    ] {
        let err = refused_promptly(&state, &request(extra)).await;
        assert!(err.to_string().contains(expected), "{err}");
        assert!(!state.session_requests.is_waiting_for("loose"));
    }

    // An empty list is no branches at all, for either kind, and a repeated name is one branch.
    for (extra, expected) in [
        (
            json!({ "kind": "console", "branches": [] }),
            Vec::<&str>::new(),
        ),
        (
            json!({ "kind": "project", "project": "repo", "branches": [] }),
            Vec::new(),
        ),
        (
            json!({ "kind": "project", "project": "repo", "branches": ["mine", "mine"] }),
            vec!["mine"],
        ),
    ] {
        let event = withdrawn_request(&state, "loose", request(extra)).await;
        assert!(
            matches!(&event, Event::SessionRequest { branches, .. } if branches == &expected),
            "{event:?}"
        );
    }

    // A branch deleted and made again is another branch, which the caller never was
    // responsible for, though a branch of that name is there.
    crate::test_support::branch_made_by(&state, &repo, "recreated", "loose");
    crate::test_support::git(&repo, &["branch", "-D", "recreated"]);
    crate::test_support::git(&repo, &["commit", "-q", "--allow-empty", "-m", "later"]);
    crate::test_support::git(&repo, &["branch", "recreated"]);
    let err = refused_promptly(
        &state,
        &request(json!({ "kind": "project", "project": "repo", "branches": ["recreated"] })),
    )
    .await;
    assert!(
        err.to_string()
            .contains("`recreated` is not a branch you are responsible for"),
        "{err}"
    );
}

/// `new_session`'s `branches` are the caller's to give as `request_session`'s are: anything
/// else is refused before a session is started, and what is accepted moves to the new session
/// once it is running, which the result says.
#[tokio::test]
async fn new_session_hands_the_callers_branches_to_the_session_it_starts() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;

    let (state, _dir) = shared_console_state("new-session-branches");
    let repo = crate::test_support::repo_with("new-session-branches-repo", &[(b"a", b"a")]);
    state
        .store
        .insert_project(&Project {
            id: "repo".to_string(),
            name: "repo".to_string(),
            path: repo.to_string_lossy().into_owned(),
            ..state.store.get_project("project-1").unwrap().unwrap()
        })
        .unwrap();
    state
        .store
        .insert_session(&Session {
            project_id: Some("repo".to_string()),
            ..project_session("dev", "console-1", None)
        })
        .unwrap();
    crate::test_support::branch_made_from(&state, &repo, "mine", "dev", Some("main"));
    crate::test_support::branch_made_by(&state, &repo, "also", "dev");
    crate::test_support::branch_made_by(&state, &repo, "theirs", "a");
    let _caller = running(&state, "dev");
    let caller = state.session_record("dev").unwrap();
    let responsible = |branch: &str| {
        let key =
            crate::branch_origin::repo_key(&crate::git::repo_files::repository_of(&repo).unwrap());
        state
            .store
            .branch_origins(&key)
            .unwrap()
            .into_iter()
            .find(|row| row.branch == branch)
            .unwrap()
            .responsible_session
    };
    let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
    let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
    let before = state.store.list_sessions().unwrap().len();

    let input = arguments(json!({ "brief": { "goal": "x" }, "branches": ["theirs"] }));
    let err = new_session(
        &state,
        &caller,
        &input,
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .expect_err("refused");
    assert!(
        err.to_string()
            .contains("`theirs` is not a branch you are responsible for"),
        "{err}"
    );
    assert!(launches.lock().unwrap().is_empty());
    assert_eq!(state.store.list_sessions().unwrap().len(), before);
    assert_eq!(responsible("mine"), "dev");

    let input = arguments(json!({
        "brief": { "goal": "x" },
        "branches": ["mine", "also", "mine"],
    }));
    let answer = new_session(
        &state,
        &caller,
        &input,
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .unwrap();
    let id = answer["session"].as_str().unwrap();
    assert_eq!(answer["branches"], json!(["mine", "also"]));
    let opening = launches.lock().unwrap()[0].2.clone().unwrap();
    assert!(
        opening.contains("handed the branches `mine` (target branch `main`), `also` over to you")
            && opening.contains("you are now the session responsible for them"),
        "{opening}"
    );
    assert!(answer["note"]
        .as_str()
        .unwrap()
        .contains("Handed over to it"));
    assert_eq!(responsible("mine"), id);
    assert_eq!(responsible("also"), id);
    assert_eq!(responsible("theirs"), "a");
    assert_eq!(
        state
            .store
            .get_session(id)
            .unwrap()
            .unwrap()
            .bound_to
            .as_deref(),
        Some("dev")
    );

    // A console session is responsible for no branch and is told so, not that it is not
    // responsible for this one.
    let console = state.session_record("a").unwrap();
    let input = arguments(json!({
        "project": "repo",
        "brief": { "goal": "x" },
        "branches": ["mine"],
    }));
    let err = new_session(
        &state,
        &console,
        &input,
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .expect_err("refused");
    assert!(
        err.to_string()
            .contains("`branches` is for a project session"),
        "{err}"
    );

    // Without `branches` the answer is what it was.
    let input = arguments(json!({ "brief": { "goal": "x" } }));
    let answer = new_session(
        &state,
        &caller,
        &input,
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .unwrap();
    assert!(answer.get("branches").is_none());
}

/// An approval starts the session with a brief that says who wrote it, and, when the request
/// handed branches over, that they are its to work on.
#[tokio::test]
async fn the_started_sessions_brief_says_which_branches_it_was_handed() {
    let (state, _dir) = shared_console_state("request-brief");
    let repo = crate::test_support::repo_with("request-brief-repo", &[(b"a", b"a")]);
    state
        .store
        .insert_project(&Project {
            id: "repo".to_string(),
            name: "repo".to_string(),
            path: repo.to_string_lossy().into_owned(),
            ..state.store.get_project("project-1").unwrap().unwrap()
        })
        .unwrap();
    crate::test_support::branch_made_by(&state, &repo, "one", "loose");
    crate::test_support::branch_made_from(&state, &repo, "two", "loose", Some("main"));
    crate::test_support::branch_made_by(&state, &repo, "three", "loose");
    let _caller = running(&state, "loose");

    for (branches, between) in [
        (
            json!(["one"]),
            " It also handed the branch `one` over to you, with the same approval: you are now \
             the session responsible for it, so carry on the work on it. This is the brief it \
             wrote:",
        ),
        (
            json!(["two", "three"]),
            " It also handed the branches `two` (target branch `main`), `three` over to you, \
             with the same approval: you are now the session responsible for them, so carry \
             on the work on them. This is the brief it wrote:",
        ),
        (json!([]), " This is the brief it wrote:"),
    ] {
        let args = arguments(json!({
            "kind": "project",
            "project": "repo",
            "branches": branches,
            "brief": { "goal": "do the thing" },
        }));
        let task = approved_task(&state, "loose", args).await;
        assert!(
            task.contains(&format!("share information with each other.{between}")),
            "{task}"
        );
    }
}

/// The refusal of the request `args` as `loose`, which is made before the user is asked: a call
/// that is not refused would wait for an answer, so it is cut short.
async fn refused_promptly(state: &Arc<AppState>, args: &Map<String, Value>) -> anyhow::Error {
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        call(state, "loose", "request_session", args),
    )
    .await
    .expect("the call is refused without waiting for the user")
    .expect_err("refused")
}

/// Makes the request `args` as `caller`, withdraws it once it waits, and returns what it asked.
async fn withdrawn_request(
    state: &Arc<AppState>,
    caller: &'static str,
    args: Map<String, Value>,
) -> Event {
    let call = {
        let state = state.clone();
        tokio::spawn(async move { call(&state, caller, "request_session", &args).await })
    };
    let event = wait_for_request(state, caller).await;
    session_request::withdraw_for_session(state, caller);
    call.await.unwrap().expect_err("withdrawn");
    event
}

/// Makes the request `args` as `caller` and approves it with a launch that starts nothing but
/// records a session; returns the brief the session was started with.
async fn approved_task(
    state: &Arc<AppState>,
    caller: &'static str,
    args: Map<String, Value>,
) -> String {
    let call = {
        let state = state.clone();
        tokio::spawn(async move { call(&state, caller, "request_session", &args).await })
    };
    let Event::SessionRequest { request_id, .. } = wait_for_request(state, caller).await else {
        unreachable!()
    };
    let task = Arc::new(std::sync::Mutex::new(None));
    let launch = |open: coordinator::open::OpenRequest| {
        let (state, task) = (state.clone(), task.clone());
        async move {
            *task.lock().unwrap() = open.task.clone();
            let started = Session {
                project_id: open.project_id.clone(),
                ..project_session(&Uuid::new_v4().to_string(), &open.console_id, None)
            };
            state.store.insert_session(&started)?;
            running(&state, &started.id);
            Ok(started)
        }
    };
    session_request::answer_with_resume(state, &request_id, true, launch, |_| async {
        bail!("nothing is resumed here")
    })
    .await
    .unwrap()
    .unwrap();
    call.await.unwrap().expect("approved");
    let task = task
        .lock()
        .unwrap()
        .take()
        .expect("the session was started");
    task
}

/// `interrupt_session` on a running session of the caller's answers with the function's
/// result: here one at its prompt, with nothing to interrupt.
#[tokio::test]
async fn interrupt_session_answers_for_a_running_session_of_the_caller_s() {
    let (state, _dir) = shared_console_state("tool-interrupt-idle");
    with_status(&state, "of-a", SessionStatus::Idle);
    let _live = running(&state, "of-a");

    let answer = call(
        &state,
        "a",
        "interrupt_session",
        &arguments(json!({ "session": "of-a" })),
    )
    .await
    .unwrap();

    assert_eq!(answer["interrupted"], false);
    assert_eq!(answer["status"], "idle");
    assert!(
        answer["note"]
            .as_str()
            .is_some_and(|note| note.starts_with("Nothing to interrupt")),
        "{answer}"
    );
}

/// An unbound project session starts sessions only in its own project: another project of the
/// same console is refused, by id or by name, before anything is launched.
#[tokio::test]
async fn an_unbound_project_session_cannot_start_a_session_in_another_project() {
    let (state, _dir) = peer_state("peer-start-scope");
    let before = state.store.list_sessions().unwrap().len();
    for project in ["project-3", "project-2"] {
        let err = call(
            &state,
            "starter",
            "new_session",
            &arguments(json!({ "project": project, "brief": { "goal": "x" } })),
        )
        .await
        .expect_err("refused");
        assert!(
            err.to_string().contains("only in your own project"),
            "{project}: {err}"
        );
    }
    assert_eq!(state.store.list_sessions().unwrap().len(), before);
}

/// Without a `project`, or naming the one the caller is in, the call gets as far as the launch
/// the console session's tool uses: an unavailable agent is refused there, in prose.
#[tokio::test]
async fn an_unbound_project_sessions_new_session_reaches_the_launch_in_its_own_project() {
    let (state, _dir) = peer_state("peer-start-launch");
    state.set_agent_availability(vec![AgentAvailability {
        agent: Agent::Claude,
        availability: Availability::Unavailable,
        default_account_dir: Some("/home/user/.claude".to_string()),
    }]);
    for project in [None, Some("project-1")] {
        let mut input = arguments(json!({ "brief": { "goal": "x" }, "agent": "claude" }));
        if let Some(project) = project {
            input.insert("project".to_string(), json!(project));
        }
        let err = call(&state, "starter", "new_session", &input)
            .await
            .expect_err("agent unavailable");
        assert!(
            err.to_string().contains("Claude Code"),
            "{project:?}: {err}"
        );
    }
}

/// A session is retitled and pinned only by the session it reports to: not one bound to
/// another session, not an unbound one, not the caller itself, and a bound project session has
/// no such tools at all. A blank title is refused.
#[tokio::test]
async fn only_the_owner_renames_or_pins_a_session() {
    let (state, _dir) = settings_state("rename-pin");
    let renamed = call(
        &state,
        "a",
        "rename_session",
        &arguments(json!({ "session": "of-a", "title": " New title " })),
    )
    .await
    .unwrap();
    assert_eq!(renamed["title"], "New title");
    let pinned = call(
        &state,
        "a",
        "set_session_pinned",
        &arguments(json!({ "session": "of-a", "pinned": true })),
    )
    .await
    .unwrap();
    assert_eq!(pinned["pinned"], true);
    assert!(state.store.get_session("of-a").unwrap().unwrap().pinned);

    let message = refused(
        &state,
        "a",
        "rename_session",
        json!({ "session": "of-a", "title": " " }),
    )
    .await;
    assert!(message.contains("`title` is required"), "{message}");
    for target in ["of-b", "loose", "a"] {
        for (tool, args) in [
            ("rename_session", json!({ "session": target, "title": "x" })),
            (
                "set_session_pinned",
                json!({ "session": target, "pinned": true }),
            ),
        ] {
            let message = refused(&state, "a", tool, args).await;
            assert!(
                message.contains("send_message") || message.contains("your own"),
                "{message}"
            );
        }
    }
    assert_eq!(
        state.store.get_session("of-b").unwrap().unwrap().title,
        "Work of-b"
    );
    let message = refused(
        &state,
        "of-a",
        "rename_session",
        json!({ "session": "loose", "title": "x" }),
    )
    .await;
    assert!(message.contains("not a tool"), "{message}");
}

/// `new_session`'s account must belong to the session's agent, and its model must be one the
/// agent offers; both are refused before anything is launched.
#[tokio::test]
async fn new_session_refuses_another_agents_account_and_a_model_not_offered() {
    let (state, _dir) = settings_state("new-session-overrides");
    for (extra, expected) in [
        (json!({ "account": "acct-codex" }), "unknown account"),
        (json!({ "model": "gpt-9" }), "is not a model"),
    ] {
        let mut args = json!({ "project": "project-1", "brief": { "goal": "do it" } });
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let message = refused(&state, "a", "new_session", args).await;
        assert!(message.contains(expected), "{message}");
    }
    assert_eq!(state.store.list_sessions().unwrap().len(), 8);
}

/// A Codex project session `a` on the account `codex-work`, in a console whose sessions
/// otherwise default to Claude Code on the default account.
fn codex_caller_state(name: &str) -> (Arc<AppState>, ScratchDir, Session) {
    let (state, dir) = shared_console_state(name);
    state
        .store
        .insert_account(&account(
            "codex-work",
            Agent::Codex,
            "Work",
            "/tmp/codex-work",
        ))
        .unwrap();
    let caller = Session {
        agent: Agent::Codex,
        role: Role::Project,
        project_id: Some("project-1".to_string()),
        account_id: Some("codex-work".to_string()),
        config_dir: Some("/tmp/codex-work".to_string()),
        ..state.session_record("a").unwrap()
    };
    (state, dir, caller)
}

/// The agent and account `session_in` reads for `caller` from `args`, starting a session in
/// `project` of `console`.
fn agent_and_account(
    state: &Arc<AppState>,
    caller: &Session,
    console: &str,
    project: &str,
    args: Value,
) -> (Option<Agent>, Option<Option<String>>) {
    let console = state.store.get_console(console).unwrap().unwrap();
    let project = state.store.get_project(project).unwrap();
    let mut args = arguments(args);
    args.insert("brief".to_string(), json!({ "goal": "Fix the build" }));
    let open = session_in(state, caller, &console, project, &args).unwrap();
    (open.agent, open.account)
}

/// A session a session starts runs on the caller's own agent and account when the call names
/// neither, whatever the console and project set; an agent or account the call names wins.
#[test]
fn a_started_session_defaults_to_the_callers_agent_and_account() {
    let (state, _dir, caller) = codex_caller_state("defaults-to-caller");
    let codex_work = Some(Some("codex-work".to_string()));

    let omitted = agent_and_account(&state, &caller, "console-1", "project-1", json!({}));
    assert_eq!(omitted, (Some(Agent::Codex), codex_work.clone()));
    // Naming the caller's own agent keeps the caller's account too.
    let same_agent = json!({ "agent": "codex" });
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", same_agent),
        (Some(Agent::Codex), codex_work)
    );
    // Another agent takes what the project and console set for it.
    let other_agent = json!({ "agent": "claude" });
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", other_agent),
        (Some(Agent::Claude), None)
    );
    let named = json!({ "account": "default" });
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", named),
        (Some(Agent::Codex), Some(None))
    );
}

/// A console session does not carry its own agent and account into a project: a session it
/// starts there runs on the project's default agent unless the call names one, and on the
/// caller's account only when that is the caller's own agent too.
#[test]
fn a_console_session_starts_sessions_on_the_projects_default_agent() {
    let (state, _dir, project_caller) = codex_caller_state("console-caller-default-agent");
    let caller = Session {
        role: Role::Console,
        project_id: None,
        ..project_caller.clone()
    };
    let project = state.store.get_project("project-1").unwrap().unwrap();
    let console = state.store.get_console("console-1").unwrap().unwrap();
    let expected = choice::session_agent(None, Some(&project), &console);
    assert_ne!(expected, Agent::Codex);

    // An omitted agent resolves later, from the project: the request names none.
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
        (None, None)
    );
    // An explicit agent wins, and the caller's account goes with its own agent only.
    let codex = json!({ "agent": "codex" });
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", codex),
        (Some(Agent::Codex), Some(Some("codex-work".to_string())))
    );
    let claude = json!({ "agent": "claude" });
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", claude),
        (Some(Agent::Claude), None)
    );
    // A project session keeps following its own agent.
    assert_eq!(
        agent_and_account(&state, &project_caller, "console-1", "project-1", json!({})).0,
        Some(Agent::Codex)
    );
    // A project whose default agent is the caller's own brings the caller's account along.
    state
        .store
        .update_project(&Project {
            default_agent: Some(Agent::Codex),
            ..project
        })
        .unwrap();
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
        (None, Some(Some("codex-work".to_string())))
    );
}

/// The caller's account is left for what the project and console set when it could not run
/// the session: turned off, its quota used up, or the account removed. The caller's agent is
/// left for theirs when it could not either.
#[test]
fn the_callers_account_or_agent_is_passed_over_when_it_could_not_open_a_session() {
    use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};

    let (state, _dir, caller) = codex_caller_state("defaults-passed-over");
    state.account_quotas.hold_in_dir_for_test(
        &AccountQuota {
            reset_credits: None,
            agent: Agent::Codex,
            account: Some("codex-work".to_string()),
            fetched_at: 1,
            plan: None,
            email: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent: 100.0,
                resets_at: None,
                window_minutes: None,
            }],
            unavailable: None,
        },
        "/tmp/codex-work",
    );
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
        (Some(Agent::Codex), None)
    );

    let (state, _dir, caller) = codex_caller_state("defaults-account-disabled");
    state
        .store
        .set_account_disabled("codex-work", true)
        .unwrap();
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
        (Some(Agent::Codex), None)
    );
    state
        .store
        .set_account_disabled("codex-work", false)
        .unwrap();
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
        (Some(Agent::Codex), Some(Some("codex-work".to_string())))
    );

    let (state, _dir, caller) = codex_caller_state("defaults-account-removed");
    state.store.delete_account("codex-work").unwrap();
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
        (Some(Agent::Codex), None)
    );

    let (state, _dir, caller) = codex_caller_state("defaults-agent-not-added");
    state
        .store
        .set_added_agents_if_undecided(&[Agent::Claude, Agent::Grok])
        .unwrap();
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
        (None, None)
    );
}

/// A session asked for in another console runs on what that console and its project set: its
/// accounts may be another job's.
#[test]
fn a_session_in_another_console_does_not_follow_the_caller() {
    let (state, _dir, caller) = codex_caller_state("defaults-other-console");
    assert_eq!(
        agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
        (None, None)
    );
}

fn insert_codex(state: &Arc<AppState>, id: &str, dir: &str) {
    state
        .store
        .insert_account(&account(id, Agent::Codex, id, dir))
        .unwrap();
}

fn own_console_account(
    state: &Arc<AppState>,
    caller: &Session,
    args: Value,
) -> Option<Option<String>> {
    agent_and_account(state, caller, "console-1", "project-1", args).1
}

#[test]
fn auto_select_account_skips_a_sharing_caller_only_when_enabled() {
    let (state, _dir, caller) = codex_caller_state("auto-select-sharing-caller-scope");
    let account = state.store.get_account("codex-work").unwrap().unwrap();
    let (_, _lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("codex-work".to_string())),
    );
    state.store.set_auto_select_account(true).unwrap();
    assert_eq!(own_console_account(&state, &caller, json!({})), Some(None));
}

#[test]
fn auto_select_account_uses_the_project_pin_when_the_callers_sharing_change_runs() {
    let (state, _dir, caller) = codex_caller_state("auto-select-sharing-caller-project-pin");
    insert_codex(&state, "project-pin", "/tmp/codex-project-pin");
    let mut project = state.store.get_project("project-1").unwrap().unwrap();
    project.accounts.codex = ProjectAccount::Account {
        id: "project-pin".to_string(),
    };
    state.store.update_project(&project).unwrap();
    let account = state.store.get_account("codex-work").unwrap().unwrap();
    let (_, _lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("codex-work".to_string())),
    );
    state.store.set_auto_select_account(true).unwrap();
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("project-pin".to_string())),
    );
}

#[test]
fn auto_select_account_falls_back_across_agents_only_for_unnamed_choices() {
    let (state, _dir, caller) = codex_caller_state("auto-select-agent-fallback");
    state.store.set_auto_select_account(true).unwrap();
    state
        .store
        .set_added_agents_if_undecided(&[Agent::Codex, Agent::Grok, Agent::Claude])
        .unwrap();
    state
        .store
        .set_default_account_disabled(Agent::Codex, true)
        .unwrap();
    state
        .store
        .set_account_disabled("codex-work", true)
        .unwrap();
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
        (Some(Agent::Grok), Some(None)),
    );
    assert_eq!(
        agent_and_account(
            &state,
            &caller,
            "console-1",
            "project-1",
            json!({"agent": "codex"})
        )
        .0,
        Some(Agent::Codex),
    );
    assert_eq!(
        agent_and_account(
            &state,
            &caller,
            "console-1",
            "project-1",
            json!({"account": "Work"})
        ),
        (Some(Agent::Codex), Some(Some("codex-work".to_string()))),
    );
    // The shared builder also resolves requests made in another console.
    state
        .store
        .set_default_account_disabled(Agent::Claude, true)
        .unwrap();
    assert_eq!(
        agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
        (Some(Agent::Grok), Some(None)),
    );
}

#[test]
fn auto_select_account_fallback_to_callers_agent_keeps_its_usable_account() {
    let (state, _dir, mut caller) = codex_caller_state("auto-select-caller-fallback");
    caller.role = Role::Console;
    caller.project_id = None;
    state.store.set_auto_select_account(true).unwrap();
    state
        .store
        .set_added_agents_if_undecided(&[Agent::Claude, Agent::Codex])
        .unwrap();
    state
        .store
        .set_default_account_disabled(Agent::Claude, true)
        .unwrap();
    assert_eq!(
        agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
        (Some(Agent::Codex), Some(Some("codex-work".to_string()))),
    );
    assert_eq!(
        agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
        (Some(Agent::Codex), Some(None)),
    );
}

/// With the setting off, a left-out account stays the caller's even when another account's
/// weekly quota resets sooner.
#[test]
fn auto_select_account_off_keeps_the_callers_account() {
    let (state, _dir, caller) = codex_caller_state("auto-select-off");
    let soon = now_millis() + 60_000;
    insert_codex(&state, "sooner", "/tmp/codex-sooner");
    hold(
        &state,
        Agent::Codex,
        Some("sooner"),
        Some("/tmp/codex-sooner"),
        vec![weekly(80.0, soon)],
    );
    hold(
        &state,
        Agent::Codex,
        Some("codex-work"),
        Some("/tmp/codex-work"),
        vec![weekly(10.0, soon + 3_600_000)],
    );
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("codex-work".to_string()))
    );
}

/// With the setting on, the account whose weekly quota resets soonest is chosen, including
/// when that account is the default.
#[test]
fn auto_select_account_on_chooses_the_soonest_weekly_reset_including_the_default() {
    let soon = now_millis() + 60_000;
    let later = soon + 3_600_000;

    let (state, _dir, caller) = codex_caller_state("auto-select-soonest-stored");
    state.store.set_auto_select_account(true).unwrap();
    insert_codex(&state, "sooner", "/tmp/codex-sooner");
    hold(&state, Agent::Codex, None, None, vec![weekly(10.0, later)]);
    hold(
        &state,
        Agent::Codex,
        Some("sooner"),
        Some("/tmp/codex-sooner"),
        vec![weekly(80.0, soon)],
    );
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("sooner".to_string()))
    );

    let (state, _dir, caller) = codex_caller_state("auto-select-soonest-default");
    state.store.set_auto_select_account(true).unwrap();
    hold(&state, Agent::Codex, None, None, vec![weekly(80.0, soon)]);
    hold(
        &state,
        Agent::Codex,
        Some("codex-work"),
        Some("/tmp/codex-work"),
        vec![weekly(10.0, later)],
    );
    assert_eq!(own_console_account(&state, &caller, json!({})), Some(None));
}

/// A named account is the one the call asked for, whatever resets sooner.
#[test]
fn auto_select_account_on_a_named_account_still_wins() {
    let (state, _dir, caller) = codex_caller_state("auto-select-named");
    state.store.set_auto_select_account(true).unwrap();
    let soon = now_millis() + 60_000;
    insert_codex(&state, "sooner", "/tmp/codex-sooner");
    hold(
        &state,
        Agent::Codex,
        Some("sooner"),
        Some("/tmp/codex-sooner"),
        vec![weekly(10.0, soon)],
    );
    assert_eq!(
        own_console_account(&state, &caller, json!({ "account": "Work" })),
        Some(Some("codex-work".to_string()))
    );
}

/// Nothing rankable leaves today's resolution: the caller's account, in the caller's own
/// console.
#[test]
fn auto_select_account_on_without_a_rankable_weekly_window_keeps_the_callers_account() {
    let soon = now_millis() + 60_000;
    let cases: &[(&str, Option<Vec<QuotaWindow>>)] = &[
        ("unread", None),
        (
            "session",
            Some(vec![QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 10.0,
                resets_at: Some(soon),
                window_minutes: Some(5 * 60),
            }]),
        ),
    ];
    for (label, windows) in cases {
        let (state, _dir, caller) = codex_caller_state(&format!("auto-select-none-{label}"));
        state.store.set_auto_select_account(true).unwrap();
        insert_codex(&state, "other", "/tmp/codex-other");
        if let Some(windows) = windows {
            hold(
                &state,
                Agent::Codex,
                Some("other"),
                Some("/tmp/codex-other"),
                windows.clone(),
            );
        }
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("codex-work".to_string())),
            "{label}"
        );
    }
}

/// Nothing rankable still inherits when the session is not the caller's agent, and when the
/// caller's own account can no longer open a session. It does not keep the caller's account.
#[test]
fn auto_select_account_on_without_a_rankable_window_inherits_otherwise() {
    let (state, _dir, caller) = codex_caller_state("auto-select-other-agent");
    state.store.set_auto_select_account(true).unwrap();
    assert_eq!(
        own_console_account(&state, &caller, json!({ "agent": "claude" })),
        Some(None)
    );

    let (state, _dir, caller) = codex_caller_state("auto-select-caller-used-up");
    state.store.set_auto_select_account(true).unwrap();
    hold(
        &state,
        Agent::Codex,
        Some("codex-work"),
        Some("/tmp/codex-work"),
        vec![QuotaWindow {
            kind: QuotaWindowKind::Session,
            label: None,
            used_percent: 100.0,
            resets_at: Some(now_millis() + 60_000),
            window_minutes: Some(5 * 60),
        }],
    );
    assert_eq!(own_console_account(&state, &caller, json!({})), Some(None));
}

/// Another console still takes a rankable account, and leaves the account unset when none
/// ranks — not the caller's.
#[test]
fn auto_select_account_on_in_another_console_picks_or_inherits_the_account() {
    let soon = now_millis() + 60_000;
    let (state, _dir, caller) = codex_caller_state("auto-select-other-console");
    state.store.set_auto_select_account(true).unwrap();
    state
        .store
        .insert_account(&account(
            "claude-soon",
            Agent::Claude,
            "Soon",
            "/tmp/claude-soon",
        ))
        .unwrap();
    hold(
        &state,
        Agent::Claude,
        Some("claude-soon"),
        Some("/tmp/claude-soon"),
        vec![weekly(20.0, soon)],
    );
    assert_eq!(
        agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
        (Some(Agent::Claude), Some(Some("claude-soon".to_string())))
    );

    let (state, _dir, caller) = codex_caller_state("auto-select-other-console-none");
    state.store.set_auto_select_account(true).unwrap();
    hold(
        &state,
        Agent::Codex,
        Some("codex-work"),
        Some("/tmp/codex-work"),
        vec![weekly(10.0, soon)],
    );
    assert_eq!(
        agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
        (Some(Agent::Claude), Some(None))
    );
}

/// A reading stored under a directory the account no longer points at does not choose it.
#[test]
fn auto_select_account_ignores_a_reading_from_another_config_dir() {
    let (state, _dir, caller) = codex_caller_state("auto-select-other-dir");
    state.store.set_auto_select_account(true).unwrap();
    let soon = now_millis() + 60_000;
    insert_codex(&state, "sooner", "/tmp/codex-sooner");
    insert_codex(&state, "later", "/tmp/codex-later");
    hold(
        &state,
        Agent::Codex,
        Some("sooner"),
        Some("/tmp/codex-elsewhere"),
        vec![weekly(10.0, soon)],
    );
    hold(
        &state,
        Agent::Codex,
        Some("later"),
        Some("/tmp/codex-later"),
        vec![weekly(80.0, soon + 3_600_000)],
    );
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("later".to_string()))
    );
}

/// A used-up 5-hour window keeps the account out, however soon its weekly window resets.
#[test]
fn auto_select_account_skips_an_account_whose_five_hour_window_is_used_up() {
    let (state, _dir, caller) = codex_caller_state("auto-select-five-hour");
    state.store.set_auto_select_account(true).unwrap();
    let soon = now_millis() + 60_000;
    insert_codex(&state, "sooner", "/tmp/codex-sooner");
    insert_codex(&state, "later", "/tmp/codex-later");
    hold(
        &state,
        Agent::Codex,
        Some("sooner"),
        Some("/tmp/codex-sooner"),
        vec![
            QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 100.0,
                resets_at: Some(soon + 3_600_000),
                window_minutes: None,
            },
            weekly(10.0, soon),
        ],
    );
    hold(
        &state,
        Agent::Codex,
        Some("later"),
        Some("/tmp/codex-later"),
        vec![weekly(80.0, soon + 3_600_000)],
    );
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("later".to_string()))
    );
}

/// An account whose sharing change is running would refuse the launch, so the pick passes
/// over it for the next one.
#[test]
fn auto_select_account_skips_an_account_whose_sharing_change_is_running() {
    let (state, _dir, caller) = codex_caller_state("auto-select-sharing-locked");
    state.store.set_auto_select_account(true).unwrap();
    let soon = now_millis() + 60_000;
    insert_codex(&state, "sooner", "/tmp/codex-sooner");
    hold(
        &state,
        Agent::Codex,
        Some("sooner"),
        Some("/tmp/codex-sooner"),
        vec![weekly(10.0, soon)],
    );
    hold(
        &state,
        Agent::Codex,
        Some("codex-work"),
        Some("/tmp/codex-work"),
        vec![weekly(80.0, soon + 3_600_000)],
    );
    let account = state.store.get_account("sooner").unwrap().unwrap();
    let (_, lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("codex-work".to_string()))
    );
    drop(lock);
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("sooner".to_string()))
    );
}

/// The same reset time prefers more remaining, and a tie keeps the earlier candidate.
#[test]
fn auto_select_account_breaks_a_reset_tie_by_remaining_then_by_candidate_order() {
    let reset = now_millis() + 60_000;

    let (state, _dir, caller) = codex_caller_state("auto-select-more-remaining");
    state.store.set_auto_select_account(true).unwrap();
    hold(&state, Agent::Codex, None, None, vec![weekly(80.0, reset)]);
    hold(
        &state,
        Agent::Codex,
        Some("codex-work"),
        Some("/tmp/codex-work"),
        vec![weekly(10.0, reset)],
    );
    assert_eq!(
        own_console_account(&state, &caller, json!({})),
        Some(Some("codex-work".to_string()))
    );

    let (state, _dir, caller) = codex_caller_state("auto-select-equal-remaining");
    state.store.set_auto_select_account(true).unwrap();
    hold(&state, Agent::Codex, None, None, vec![weekly(40.0, reset)]);
    hold(
        &state,
        Agent::Codex,
        Some("codex-work"),
        Some("/tmp/codex-work"),
        vec![weekly(40.0, reset)],
    );
    assert_eq!(own_console_account(&state, &caller, json!({})), Some(None));
}

/// The caller names the session; without a name it is named from the goal's first line.
#[test]
fn a_started_session_takes_the_title_given_else_its_goals() {
    let (state, _dir) = shared_console_state("started-session-title");
    let caller = state.session_record("a").unwrap();
    let console = state.store.get_console("console-1").unwrap().unwrap();
    let title = |args: Value| {
        session_in(&state, &caller, &console, None, &arguments(args))
            .unwrap()
            .title
    };
    let brief = json!({ "goal": "Fix the build\nIt fails on CI." });
    assert_eq!(
        title(json!({ "brief": brief, "title": "  修复构建  " })).as_deref(),
        Some("修复构建")
    );
    assert_eq!(
        title(json!({ "brief": brief, "title": " " })).as_deref(),
        Some("Fix the build")
    );
    assert_eq!(
        title(json!({ "brief": brief })).as_deref(),
        Some("Fix the build")
    );
}

/// `new_session` says which account the session opened on.
#[tokio::test]
async fn new_session_returns_the_account_it_opened_on() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;

    let (state, _dir) = shared_console_state("new-session-account");
    state
        .store
        .insert_account(&account(
            "claude-b",
            Agent::Claude,
            "Second",
            "/tmp/octoboard-new-session-account",
        ))
        .unwrap();
    let caller = state.session_record("a").unwrap();
    let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
    let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
    for (input, account, name) in [
        (json!({}), "default", crate::protocol::DEFAULT_ACCOUNT_NAME),
        (json!({ "account": "Second" }), "claude-b", "Second"),
    ] {
        let mut input = input;
        input["project"] = json!("project-1");
        input["brief"] = json!({ "goal": "x" });
        let answer = new_session(
            &state,
            &caller,
            &arguments(input),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();
        assert_eq!(answer["account"], account);
        assert_eq!(answer["account_name"], name);
    }
}

/// A brief part that is not text is refused rather than left out of the brief.
#[tokio::test]
async fn a_new_session_refuses_a_brief_part_that_is_not_text() {
    let (state, _dir) = shared_console_state("new-session-brief-context");
    let err = call(
        &state,
        "a",
        "new_session",
        &arguments(json!({ "project": "project-1", "brief": { "goal": "x", "context": 5 } })),
    )
    .await
    .expect_err("not text");
    assert_eq!(err.to_string(), "`context` must be a string");
}

/// A `model` or `effort` that is not text is refused before anything is reopened.
#[tokio::test]
async fn a_reopen_refuses_a_model_that_is_not_text() {
    let (state, _dir) = shared_console_state("tool-surface-reopen-model");
    for key in ["model", "effort"] {
        let err = call(
            &state,
            "a",
            "reopen_session",
            &arguments(json!({ "session": "archived-of-a", key: 5 })),
        )
        .await
        .expect_err("not text");
        assert_eq!(err.to_string(), format!("`{key}` must be a string or null"));
    }
    let after = state.store.get_session("archived-of-a").unwrap().unwrap();
    assert_eq!(after.status, SessionStatus::Archived);
}

/// `reopen_session` answers with the account, model and effort the relaunch used.
#[tokio::test]
async fn reopen_session_returns_the_account_model_and_effort_it_launched_with() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;

    let fixture = RepoState::new("reopen-returns-launch");
    let state = &fixture.state;
    let mut kid = state.store.get_session("kid").unwrap().unwrap();
    kid.status = SessionStatus::Archived;
    state.store.update_session(&kid).unwrap();
    state
        .set_session_launch_overrides("kid", &Some(Some("sonnet".to_string())), &Some(None))
        .unwrap();
    let caller = state.session_record("a").unwrap();
    let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
    let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));

    let answer = reopen_session(
        state,
        &caller,
        &arguments(json!({ "session": "kid" })),
        || Ok(HashMap::new()),
        {
            let (started, launches) = (started.clone(), launches.clone());
            move |request| stand_in_launch(&started, &launches, || {})(request)
        },
    )
    .await
    .unwrap();

    assert_eq!(launches.lock().unwrap().len(), 1);
    assert_eq!(answer["session"], "kid");
    assert_eq!(answer["account"], "default");
    assert_eq!(
        answer["account_name"],
        crate::protocol::DEFAULT_ACCOUNT_NAME
    );
    assert_eq!(answer["model"], "sonnet");
    assert_eq!(answer["effort"], Value::Null);
}

/// A blank `text` reopens the session with no instruction: no prompt is announced while it
/// launches and none is handed to the agent. A `text` that is not a string is refused.
#[tokio::test]
async fn a_reopen_with_blank_text_carries_no_instruction() {
    use crate::test_support::stand_in_launch;
    use std::sync::Mutex;

    let fixture = RepoState::new("reopen-blank-text");
    let state = &fixture.state;
    let mut kid = state.store.get_session("kid").unwrap().unwrap();
    kid.status = SessionStatus::Archived;
    state.store.update_session(&kid).unwrap();
    let caller = state.session_record("a").unwrap();

    let refused = reopen_session(
        state,
        &caller,
        &arguments(json!({ "session": "kid", "text": 5 })),
        || Ok(HashMap::new()),
        |_| unreachable!("a refused reopen launches nothing"),
    )
    .await
    .expect_err("not text");
    assert_eq!(refused.to_string(), "`text` must be a string");

    let launches = Launches::default();
    let started = Started::default();
    let prompted = Arc::new(Mutex::new(None::<bool>));
    reopen_session(
        state,
        &caller,
        &arguments(json!({ "session": "kid", "text": "  \n " })),
        || Ok(HashMap::new()),
        {
            let (started, launches, prompted) =
                (started.clone(), launches.clone(), prompted.clone());
            let watched = state.clone();
            move |request| {
                let (prompted, watched) = (prompted.clone(), watched.clone());
                stand_in_launch(&started, &launches, move || {
                    *prompted.lock().unwrap() = Some(watched.is_prompted_on_launch("kid"));
                })(request)
            }
        },
    )
    .await
    .unwrap();

    assert_eq!(*prompted.lock().unwrap(), Some(false));
    let launches = launches.lock().unwrap();
    assert_eq!(launches.len(), 1);
    assert_eq!(launches[0].2, None);
}

/// While accounts are selected automatically, `new_session` names the account that was
/// picked, not the caller's, and the model and effort it launched with.
#[tokio::test]
async fn new_session_returns_the_account_automatic_selection_picked() {
    use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;

    let (state, dir) = shared_console_state("new-session-picked-account");
    let config_dir = dir.join("claude-b").to_string_lossy().into_owned();
    state
        .store
        .insert_account(&account(
            "claude-b",
            Agent::Claude,
            "Second",
            config_dir.clone(),
        ))
        .unwrap();
    state.store.set_auto_select_account(true).unwrap();
    let mut project = state.store.get_project("project-1").unwrap().unwrap();
    project.launch.claude.model = Some("sonnet".to_string());
    project.launch.claude.effort = Some("high".to_string());
    state.store.update_project(&project).unwrap();
    let hold = |account: Option<&str>, used_percent: f64| {
        let quota = AccountQuota {
            reset_credits: None,
            agent: Agent::Claude,
            account: account.map(str::to_string),
            fetched_at: 1,
            plan: None,
            email: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent,
                resets_at: Some(crate::time::now_millis() + 86_400_000),
                window_minutes: None,
            }],
            unavailable: None,
        };
        match account {
            None => state.account_quotas.hold_for_test(&quota),
            Some(_) => state
                .account_quotas
                .hold_in_dir_for_test(&quota, &config_dir),
        }
    };
    // The caller's own account, the default one, is used up.
    hold(None, 100.0);
    hold(Some("claude-b"), 20.0);
    let caller = state.session_record("a").unwrap();
    let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
    let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));

    let answer = new_session(
        &state,
        &caller,
        &arguments(json!({ "project": "project-1", "brief": { "goal": "x" } })),
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .unwrap();

    assert_eq!(answer["account"], "claude-b");
    assert_eq!(answer["account_name"], "Second");
    assert_eq!(answer["model"], "sonnet");
    assert_eq!(answer["effort"], "high");
}

/// With automatic selection on and no usable account, `new_session` is refused naming the
/// account that becomes usable soonest, not the caller's own.
#[tokio::test]
async fn new_session_with_no_usable_account_is_refused_on_the_soonest_one() {
    use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;

    let (state, _dir) = shared_console_state("new-session-soonest-available");
    state.store.set_auto_select_account(true).unwrap();
    state
        .store
        .set_added_agents_if_undecided(&[Agent::Claude])
        .unwrap();
    let hold = |account: Option<&str>, resets_in: i64| {
        let quota = AccountQuota {
            reset_credits: None,
            agent: Agent::Claude,
            account: account.map(str::to_string),
            fetched_at: 1,
            plan: None,
            email: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 100.0,
                resets_at: Some(crate::time::now_millis() + resets_in),
                window_minutes: None,
            }],
            unavailable: None,
        };
        match account {
            None => state.account_quotas.hold_for_test(&quota),
            Some(id) => state
                .account_quotas
                .hold_in_dir_for_test(&quota, &format!("/tmp/{id}")),
        }
    };
    state
        .store
        .insert_account(&account(
            "claude-b",
            Agent::Claude,
            "Second",
            "/tmp/claude-b",
        ))
        .unwrap();
    hold(None, 7_200_000);
    hold(Some("claude-b"), 3_600_000);
    let caller = state.session_record("a").unwrap();
    let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
    let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));

    let err = new_session(
        &state,
        &caller,
        &arguments(json!({ "project": "project-1", "brief": { "goal": "x" } })),
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .expect_err("refused");

    assert!(err.to_string().starts_with("Second "), "{err}");
    assert!(launches.lock().unwrap().is_empty());
}

#[tokio::test]
async fn new_session_automatic_agent_fallback_inherits_the_final_agents_launch_settings() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;

    let (state, _dir) = shared_console_state("new-session-agent-fallback-launch");
    state.store.set_auto_select_account(true).unwrap();
    state
        .store
        .set_default_account_disabled(Agent::Claude, true)
        .unwrap();
    crate::test_support::offer_models(&state, Agent::Codex, &[("fallback-model", &["high"])]);
    let mut project = state.store.get_project("project-1").unwrap().unwrap();
    project.launch.claude.model = Some("sonnet".to_string());
    project.launch.codex.model = Some("fallback-model".to_string());
    project.launch.codex.effort = Some("high".to_string());
    state.store.update_project(&project).unwrap();
    let caller = state.session_record("a").unwrap();
    let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
    let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
    let answer = new_session(
        &state,
        &caller,
        &arguments(json!({"project": "project-1", "brief": {"goal": "x"}})),
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .unwrap();
    assert_eq!(answer["agent"], "codex");
    assert_eq!(answer["account"], "default");
    assert_eq!(answer["model"], "fallback-model");
    assert_eq!(answer["effort"], "high");
    let session = state
        .session_record(answer["session"].as_str().unwrap())
        .unwrap();
    assert_eq!(session.agent, Agent::Codex);
}

fn quota(agent: Agent, account: Option<&str>, windows: Vec<QuotaWindow>) -> AccountQuota {
    AccountQuota {
        reset_credits: None,
        agent,
        account: account.map(str::to_string),
        fetched_at: 1,
        plan: None,
        email: None,
        windows,
        unavailable: None,
    }
}

fn hold(
    state: &Arc<AppState>,
    agent: Agent,
    account: Option<&str>,
    config_dir: Option<&str>,
    windows: Vec<QuotaWindow>,
) {
    let quota = quota(agent, account, windows);
    match config_dir {
        Some(dir) => state.account_quotas.hold_in_dir_for_test(&quota, dir),
        None => state.account_quotas.hold_for_test(&quota),
    }
}
