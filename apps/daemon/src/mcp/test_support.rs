//! Fixtures the MCP tests of more than one module share.

use crate::mcp::dispatch::call;
use crate::protocol::{
    Account, AgentId, Console, Event, Project, ProjectSource, QuotaWindow, QuotaWindowKind, Role,
    Session, SessionKind, SessionStatus,
};
use crate::session_request;
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::bare_session;
use crate::test_support::{running, Launched, ScratchDir, StandIn};
use anyhow::Result;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(super) type Launches = Arc<Mutex<Vec<Launched>>>;
pub(super) type Started = Arc<Mutex<Vec<StandIn>>>;

/// The conversation file of the session `prepare_relaunch` gives one, relative to the default
/// account's directory.
pub(super) const CONVERSATION: &str = "projects/slug/a0000000-0000-4000-8000-000000000001.jsonl";

/// An account that is enabled and shares nothing of the agent's default configuration.
pub(super) fn account(
    id: impl Into<String>,
    agent: AgentId,
    name: impl Into<String>,
    config_dir: impl Into<String>,
) -> Account {
    Account {
        id: id.into(),
        agent,
        name: name.into(),
        config_dir: config_dir.into(),
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled: false,
    }
}

/// A weekly quota window with `used_percent` of it spent, which `resets_at` renews.
pub(super) fn weekly(used_percent: f64, resets_at: i64) -> QuotaWindow {
    QuotaWindow {
        kind: QuotaWindowKind::Weekly,
        label: None,
        used_percent,
        resets_at: Some(resets_at),
        window_minutes: Some(10_080),
    }
}

pub(super) fn console() -> Console {
    Console {
        id: "console-1".to_string(),
        name: "Console".to_string(),
        workdir: "/tmp/console-1".to_string(),
        default_agent: AgentId::CLAUDE,
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
        host_id: LOCAL_HOST_ID.to_string(),
        title: "Hub".to_string(),
        ..bare_session(id, SessionKind::Console)
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
        kind: SessionKind::Project,
        console_id: console_id.to_string(),
        project_id: Some("project-1".to_string()),
        title: format!("Work {id}"),
        bound_to: bound_to.map(str::to_string),
        role: if bound_to.is_some() {
            Role::Worker
        } else {
            Role::Coordinator
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
                path: crate::test_support::host_path(&format!("/tmp/{id}")),
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

/// `shared_console_state` plus a project, `repo`, in a real repository, and sessions in it: `kid`
/// of the console session `a`, `coordinator` unbound, and `coordinator-kid` bound to `coordinator`.
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
            ("coordinator", None),
            ("coordinator-kid", Some("coordinator")),
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
            path: crate::test_support::host_path("/tmp/project-3"),
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

/// `peer_state` with `starter` bound to the console session `a` as a coordinator with a parent,
/// still owning `mine`.
pub(super) fn bound_coordinator_state(name: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = peer_state(name);
    state
        .store
        .set_binding("starter", None, "a", Role::Coordinator, false, false)
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
        .insert_account(&account(
            "acct-codex",
            AgentId::CODEX,
            "Work",
            "/secret/codex",
        ))
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

/// Makes the fixture's console and `projects` start in `dir`, which exists, since a relaunch is
/// refused at a working directory that does not, and puts a conversation of `session` in the
/// default account, whose directory the returned shell snapshot names.
pub(super) fn prepare_relaunch(
    state: &Arc<AppState>,
    dir: &ScratchDir,
    session: &str,
    projects: &[&str],
) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
    let mut console = state.store.get_console("console-1").unwrap().unwrap();
    console.workdir = dir.to_string_lossy().into_owned();
    state.store.update_console(&console).unwrap();
    for project in projects {
        if let Some(mut project) = state.store.get_project(project).unwrap() {
            project.path = dir.to_string_lossy().into_owned();
            state.store.update_project(&project).unwrap();
        }
    }
    let default = dir.join("default-claude");
    std::fs::create_dir_all(default.join(CONVERSATION).parent().unwrap()).unwrap();
    std::fs::write(default.join(CONVERSATION), "conversation").unwrap();
    let mut record = state.session_record(session).unwrap();
    record.has_conversation = true;
    record.agent_session_id = Some("a0000000-0000-4000-8000-000000000001".to_string());
    state.store.update_session(&record).unwrap();
    default_env(dir)
}

/// The shell environment `prepare_relaunch` makes the default account's directory come from, for a
/// relaunch that reads it again, so that no test reads the real one.
pub(super) fn default_env(
    dir: &ScratchDir,
) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
    let env = HashMap::from([(
        "CLAUDE_CONFIG_DIR".to_string(),
        dir.join("default-claude").to_string_lossy().into_owned(),
    )]);
    move || Ok(env)
}

/// A stand-in for the session's process whose end the daemon notices, as a relaunch needs.
pub(super) fn live_and_watched(state: &Arc<AppState>, id: &str) -> StandIn {
    let live = running(state, id);
    crate::coordinator::exit_watch::watch_exit(state, live.clone());
    live
}

/// Every session of the store as it is, for what a refusal must leave alone.
pub(super) fn sessions_as_stored(state: &Arc<AppState>) -> Value {
    let mut sessions = state.store.list_sessions().unwrap();
    sessions.sort_by(|a, b| a.id.cmp(&b.id));
    json!(sessions)
}
