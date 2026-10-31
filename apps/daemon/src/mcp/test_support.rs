//! Fixtures the MCP tests of more than one module share.

use crate::mcp::exec::call;
use crate::protocol::{
    Agent, Console, Event, OrchestrationRole, Origin, Project, ProjectSource, Role, Session,
    SessionStatus,
};
use crate::session_request;
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::ScratchDir;
use serde_json::{Map, Value};
use std::sync::Arc;

pub(super) fn console() -> Console {
    Console {
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

pub(super) fn console_session(id: &str) -> Session {
    Session {
        id: id.to_string(),
        agent: Agent::Claude,
        agent_session_id: None,
        console_id: "console-1".to_string(),
        project_id: None,
        host_id: LOCAL_HOST_ID.to_string(),
        role: Role::Console,
        origin: Origin::User,
        title: "Hub".to_string(),
        status: SessionStatus::Idle,
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

/// The waiting request of `caller`, once it is there.
pub(super) async fn wait_for_request(state: &Arc<AppState>, caller: &str) -> Event {
    let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
    loop {
        let found = session_request::pending_events(state).into_iter().find(
            |event| matches!(event, Event::SessionRequest { session, .. } if session == caller),
        );
        if let Some(event) = found {
            return event;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the request never waited"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

pub(super) fn project_session(id: &str, console_id: &str, bound_to: Option<&str>) -> Session {
    Session {
        role: Role::Project,
        console_id: console_id.to_string(),
        project_id: Some("project-1".to_string()),
        title: format!("Work {id}"),
        bound_to: bound_to.map(str::to_string),
        orchestration_role: if bound_to.is_some() {
            OrchestrationRole::Worker
        } else {
            OrchestrationRole::Coordinator
        },
        ..console_session(id)
    }
}

/// Two console sessions of one console, one project, and a session each: bound to `a`, bound to
/// `b`, unbound, one archived under each owner, and one in another console.
pub(super) fn shared_console_state(name: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = crate::test_support::app_state(&format!("mcp-exec-{name}"));
    state.store.insert_console(&console()).unwrap();
    state
        .store
        .insert_console(&Console {
            id: "console-2".to_string(),
            ..console()
        })
        .unwrap();
    for (id, console_id) in [("project-1", "console-1"), ("project-2", "console-2")] {
        state
            .store
            .insert_project(&Project {
                id: id.to_string(),
                console_id: console_id.to_string(),
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
    let mut sessions = vec![
        console_session("a"),
        console_session("b"),
        project_session("of-a", "console-1", Some("a")),
        project_session("of-b", "console-1", Some("b")),
        project_session("loose", "console-1", None),
        Session {
            project_id: Some("project-2".to_string()),
            ..project_session("foreign", "console-2", None)
        },
    ];
    for (id, owner) in [("archived-of-a", "a"), ("archived-of-b", "b")] {
        let mut archived = project_session(id, "console-1", Some(owner));
        archived.status = SessionStatus::Archived;
        sessions.push(archived);
    }
    for session in &sessions {
        state.store.insert_session(session).unwrap();
    }
    (state, dir)
}

pub(super) fn arguments(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

/// `shared_console_state` plus a project, `repo`, in a real repository, and sessions in it:
/// `kid` of the console session `a`, `lead` unbound, and `lead-kid` bound to `lead`.
pub(super) struct RepoState {
    pub state: Arc<AppState>,
    pub repo: ScratchDir,
    _dir: ScratchDir,
}

impl RepoState {
    pub fn new(name: &str) -> Self {
        let (state, dir) = shared_console_state(name);
        let repo =
            crate::test_support::repo_with(&format!("mcp-exec-{name}-repo"), &[(b"a", b"a")]);
        state
            .store
            .insert_project(&Project {
                id: "repo".to_string(),
                name: "repo".to_string(),
                path: repo.to_string_lossy().into_owned(),
                ..state.store.get_project("project-1").unwrap().unwrap()
            })
            .unwrap();
        for (id, owner) in [
            ("kid", Some("a")),
            ("lead", None),
            ("lead-kid", Some("lead")),
        ] {
            state
                .store
                .insert_session(&Session {
                    project_id: Some("repo".to_string()),
                    ..project_session(id, "console-1", owner)
                })
                .unwrap();
        }
        Self {
            state,
            repo,
            _dir: dir,
        }
    }

    /// Makes `branch`, recorded as `session`'s.
    pub fn branch(&self, branch: &str, session: &str) {
        crate::test_support::branch_made_by(&self.state, &self.repo, branch, session);
    }
}

/// `shared_console_state` plus an unbound project session, `starter`, in `project-1` with one
/// session bound to it, `mine`, and a second project of the same console with a session of its
/// own.
pub(super) fn peer_state(name: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = shared_console_state(name);
    state
        .store
        .insert_project(&Project {
            id: "project-3".to_string(),
            console_id: "console-1".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: "project-3".to_string(),
            path: "/tmp/project-3".to_string(),
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
    for session in [
        project_session("starter", "console-1", None),
        project_session("mine", "console-1", Some("starter")),
        Session {
            project_id: Some("project-3".to_string()),
            ..project_session("other-project", "console-1", None)
        },
    ] {
        state.store.insert_session(&session).unwrap();
    }
    (state, dir)
}

/// `peer_state` with `starter` bound to the console session `a` as its lead session, still
/// owning `mine`.
pub(super) fn lead_state(name: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = peer_state(name);
    state
        .store
        .set_binding(
            "starter",
            None,
            "a",
            OrchestrationRole::Coordinator,
            false,
            false,
        )
        .unwrap()
        .unwrap();
    (state, dir)
}

/// The status of a session, set directly, bypassing the lifecycle that would reach it.
pub(super) fn with_status(state: &Arc<AppState>, id: &str, status: SessionStatus) {
    let mut session = state.store.get_session(id).unwrap().unwrap();
    session.status = status;
    state.store.update_session(&session).unwrap();
}

/// `shared_console_state`, with the console's working directory somewhere a console's save
/// may write its instruction files, a codex account, and a remote URL carrying a token on the
/// console's project.
pub(super) fn settings_state(name: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = shared_console_state(name);
    let workdir = dir.join("workdir");
    std::fs::create_dir(&workdir).unwrap();
    state
        .store
        .update_console(&Console {
            workdir: workdir.to_string_lossy().into_owned(),
            ..console()
        })
        .unwrap();
    state
        .store
        .insert_account(&crate::protocol::Account {
            id: "acct-codex".to_string(),
            agent: Agent::Codex,
            name: "Work".to_string(),
            config_dir: "/secret/codex".to_string(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        })
        .unwrap();
    crate::project_git::remote::record_remote_url(
        &state,
        "project-1",
        Some("https://user:token@example.com/r.git".to_string()),
    );
    (state, dir)
}

pub(super) async fn refused(
    state: &Arc<AppState>,
    caller: &str,
    tool: &str,
    args: Value,
) -> String {
    call(state, caller, tool, &arguments(args))
        .await
        .expect_err("refused")
        .to_string()
}
