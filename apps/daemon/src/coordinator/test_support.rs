//! Fixtures the coordinator tests of more than one submodule share.

use crate::coordinator::account_switch::switch_session_account_with;
use crate::coordinator::bindings::resolve_owner;
use crate::coordinator::handle;
use crate::coordinator::launch::relaunch_session_with;
use crate::coordinator::launch::LaunchEdit;
use crate::coordinator::open::OpenRequest;
use crate::protocol::{
    error_code, Account, AgentId, Availability, Console, Origin, Project, ProjectAccount,
    ProjectSource, RequestBody, Role, Session, SessionKind, SessionStatus,
};
use crate::session::term;
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    code_of, console_session, console_with_project_and_accounts, idle_stand_in, project_session,
    ScratchDir,
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) fn console(default_agent: AgentId, workdir: &Path) -> Console {
    Console {
        id: "console-1".to_string(),
        name: "Console".to_string(),
        workdir: workdir.to_string_lossy().into_owned(),
        default_agent,
        project_session_agent: None,
        accounts: Default::default(),
        launch: Default::default(),
        project_session_launch: Default::default(),
        icon: None,
        console_session_prompt: None,
        created_at: 0,
        versions: Default::default(),
    }
}

/// A console workdir inside a store's scratch directory, kept apart from the database, which
/// deleting the console would otherwise wipe.
pub(super) fn console_workdir_in(dir: &Path) -> PathBuf {
    let workdir = dir.join("workdir");
    std::fs::create_dir(&workdir).expect("workdir");
    workdir
}

pub(super) fn temp_dir(name: &str) -> ScratchDir {
    ScratchDir::new(&format!("coordinator-{name}"))
}

/// A project with nothing of its own set for the session's agent, scratch-built for the tests
/// that read its settings.
pub(super) fn project_of(console_id: &str, id: &str) -> Project {
    Project {
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
        versions: Default::default(),
    }
}

/// A project's setting naming the account `id`.
pub(super) fn pins(id: &str) -> ProjectAccount {
    ProjectAccount::Account { id: id.to_string() }
}

pub(super) fn create_account_request(agent: AgentId, name: &str, config_dir: &str) -> RequestBody {
    RequestBody::CreateAccount {
        agent,
        name: name.to_string(),
        config_dir: config_dir.to_string(),
    }
}

pub(super) fn set_availability(state: &Arc<AppState>, agent: AgentId, availability: Availability) {
    state.set_agent_availability(vec![crate::protocol::AgentAvailability {
        agent,
        availability,
        default_account_dir: None,
    }]);
}

pub(super) fn only_account(state: &Arc<AppState>) -> Account {
    let accounts = state.store.list_accounts().unwrap();
    assert_eq!(accounts.len(), 1, "{accounts:?}");
    accounts.into_iter().next().unwrap()
}

pub(super) fn shell_env(
    dir: &Path,
) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
    let env = HashMap::from([(
        "CLAUDE_CONFIG_DIR".to_string(),
        dir.join("default-claude").to_string_lossy().into_owned(),
    )]);
    move || Ok(env)
}

/// Hold the real relaunch path inside its blocking launcher while the user archives the
/// owner. Both a late process and a late refusal must leave that archive intact.
pub(super) async fn archive_during_relaunch(switching: bool, succeeds: bool) {
    use SessionStatus::{Archived, Idle, Interrupted};
    let (state, dir) = console_with_project_and_accounts("coordinator-archive-during-relaunch");
    std::fs::create_dir_all(dir.join("claude-a")).unwrap();
    state
        .store
        .insert_session(&console_session("hub", Idle))
        .unwrap();
    state
        .store
        .insert_session(&bound_session("worker", Interrupted, "hub"))
        .unwrap();
    let (release, released) = std::sync::mpsc::channel();
    let (started_tx, started) = tokio::sync::oneshot::channel();
    let stand_in = succeeds.then(|| idle_stand_in("worker"));
    let process = stand_in.as_ref().map(|live| (*live).clone());
    let launch = move |_: term::LaunchRequest| {
        started_tx.send(()).unwrap();
        released
            .recv_timeout(crate::test_support::PATIENCE)
            .expect("release launch");
        match process {
            Some(session) => Ok(term::Launch {
                session,
                agent_session_id: None,
                notice: None,
            }),
            None => Err(anyhow::anyhow!("delayed launch failure")),
        }
    };
    let running = tokio::spawn({
        let state = state.clone();
        async move {
            if switching {
                switch_session_account_with(
                    &state,
                    "worker",
                    Some(Some("claude-a".to_string())),
                    None,
                    None,
                    LaunchEdit::default(),
                    || Ok(HashMap::new()),
                    launch,
                )
                .await
                .map(drop)
            } else {
                let claim = state.begin_launch("worker")?;
                relaunch_session_with(
                    &state,
                    &claim,
                    "worker",
                    Some("obsolete instruction"),
                    true,
                    None,
                    None,
                    launch,
                )
                .await
            }
        }
    });
    tokio::time::timeout(crate::test_support::PATIENCE, started)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status_of(&state, "worker"), Some(Idle));
    assert!(state.live_session("worker").is_none());
    handle(
        &state,
        None,
        RequestBody::ArchiveSession {
            session: "hub".to_string(),
            force_remove: Vec::new(),
            keep: Vec::new(),
            keep_process_ids: Vec::new(),
        },
    )
    .await
    .unwrap();
    let ended_at = state.session_record("worker").unwrap().ended_at;
    release.send(()).unwrap();
    let err = running
        .await
        .unwrap()
        .expect_err("archive wins over the relaunch");
    if succeeds {
        assert_eq!(code_of(&err), error_code::SESSION_ARCHIVED);
    } else {
        assert!(err.to_string().contains("delayed launch failure"));
    }
    if let Some(live) = &stand_in {
        wait_until(|| live.poll_exit()).await;
        wait_until(|| state.live_session("worker").is_none()).await;
    }
    let after = state.session_record("worker").unwrap();
    assert_eq!(after.status, Archived);
    if !succeeds {
        assert_eq!(
            after.ended_at, ended_at,
            "rollback preserves the newer archive time"
        );
    }
    assert_eq!(
        after.account_id, None,
        "a failed switch restores the original account"
    );
    assert_eq!(after.config_dir, None);
    assert_eq!(status_of(&state, "hub"), Some(Archived));
}

/// A login-shell snapshot that runs no shell: only a `HOME`, below `dir`, for the default
/// accounts to be found under.
pub(super) fn stub_shell(
    dir: &Path,
) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
    let env = HashMap::from([(
        "HOME".to_string(),
        dir.join("home").to_string_lossy().into_owned(),
    )]);
    move || Ok(env)
}

/// A project session bound to the console session `owner`, titled by its id so a refusal that
/// names it can be read back.
pub(super) fn bound_session(id: &str, status: SessionStatus, owner: &str) -> Session {
    Session {
        title: id.to_string(),
        bound_to: Some(owner.to_string()),
        role: Role::Worker,
        ..project_session(id, AgentId::CLAUDE, status)
    }
}

pub(super) fn status_of(state: &Arc<AppState>, id: &str) -> Option<SessionStatus> {
    state.store.get_session(id).unwrap().map(|s| s.status)
}

/// A coordinator in `status` under the console session `owner`: bound to it after it was
/// opened.
pub(super) fn bound_coordinator(id: &str, status: SessionStatus, owner: &str) -> Session {
    Session {
        role: Role::Coordinator,
        ..bound_session(id, status, owner)
    }
}

/// `console_with_project_and_accounts` plus a second console and a second project of the
/// first, with `sessions` recorded and the ones in `live` given a stand-in process.
pub(super) fn binding_fixture(
    name: &str,
    sessions: Vec<Session>,
    live: &[&str],
) -> (Arc<AppState>, ScratchDir, Vec<crate::test_support::StandIn>) {
    let (state, dir) = console_with_project_and_accounts(&format!("coordinator-{name}"));
    state
        .store
        .insert_console(&Console {
            id: "console-2".to_string(),
            ..console(AgentId::CLAUDE, &dir.join("no-such-workdir-2"))
        })
        .unwrap();
    let project_1 = state.store.get_project("project-1").unwrap().unwrap();
    state
        .store
        .insert_project(&Project {
            id: "project-2".to_string(),
            path: format!("{}-2", project_1.path),
            ..project_1
        })
        .unwrap();
    for session in &sessions {
        state.store.insert_session(session).unwrap();
    }
    let stand_ins = live
        .iter()
        .map(|id| {
            let stand_in = idle_stand_in(id);
            state.register_live(stand_in.clone());
            stand_in
        })
        .collect();
    (state, dir, stand_ins)
}

/// Relaunches the process-less session `id` with a stand-in launch, giving the launch prompt, the
/// messages queued for it and the flag's count afterwards, or the launch's failure.
pub(super) async fn relaunch_stand_in(
    state: &Arc<AppState>,
    id: &str,
    fail: bool,
) -> Result<(Option<String>, Vec<String>)> {
    relaunch_stand_in_with(state, id, fail, None).await
}

/// [`relaunch_stand_in`] with `instruction` delivered by the relaunch.
pub(super) async fn relaunch_stand_in_with(
    state: &Arc<AppState>,
    id: &str,
    fail: bool,
    instruction: Option<&str>,
) -> Result<(Option<String>, Vec<String>)> {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;
    let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
    let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
    let claim = state.begin_launch(id).unwrap();
    let launch = stand_in_launch(&started, &launches, || {});
    relaunch_session_with(
        state,
        &claim,
        id,
        instruction,
        true,
        None,
        None,
        move |request| {
            if fail {
                anyhow::bail!("the agent could not start");
            }
            launch(request)
        },
    )
    .await?;
    let prompt = launches.lock().unwrap()[0].2.clone();
    Ok((prompt, state.take_queued(id)))
}

pub(super) fn no_process_with_conversation(id: &str, agent: AgentId, owner: &str) -> Session {
    Session {
        agent,
        agent_session_id: Some(format!("conversation-{id}")),
        has_conversation: true,
        ..bound_session(id, SessionStatus::Interrupted, owner)
    }
}

/// What opens a console session of the console `state_for_opening` sets up.
pub(super) fn open_request() -> OpenRequest {
    OpenRequest {
        console_id: "console-1".to_string(),
        project_id: None,
        agent: None,
        account: None,
        model: None,
        effort: None,
        task: None,
        title: None,
        origin: Origin::User,
        bound_to: None,
        resume_agent_session_id: None,
        session_id: None,
        terminal_appearance: None,
    }
}

pub(super) fn state_for_opening(name: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = crate::test_support::app_state(name);
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(AgentId::CLAUDE, &workdir))
        .unwrap();
    (state, dir)
}

pub(super) async fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + crate::test_support::PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The id [`resolve_owner`] resolves a request for an owner to, for the tests of its rules.
pub(super) fn resolve_bound_to(
    state: &Arc<AppState>,
    console_id: &str,
    project_id: Option<&str>,
    kind: SessionKind,
    requested: Option<String>,
) -> Result<Option<String>> {
    Ok(resolve_owner(state, console_id, project_id, kind, requested)?.map(|owner| owner.id))
}

/// Starts the terminal `id` in a holder of `dir`, running `script` in bash at `size`, and leaves
/// it there as a daemon that is gone would: its terminal is leaked, since dropping one releases it
/// in the holder, and its link is parted from. Returns the program's pid.
#[cfg(unix)]
pub(super) fn left_running_script(
    dir: &ScratchDir,
    id: &str,
    meta: serde_json::Value,
    script: &str,
    size: crate::platform::term::Size,
) -> i32 {
    let previous = crate::holder::client::in_process(dir.join("holders")).expect("holders");
    let mut command = crate::platform::term::Command::new(crate::test_support::bash());
    command.args(["-c", script]);
    let held = previous
        .spawn(crate::holder::protocol::SpawnSpec {
            id: id.to_string(),
            command,
            size,
            containment: None,
            kill_session_leftovers: false,
            meta,
        })
        .expect("the program starts in the holder");
    let pid = held.pid().0 as i32;
    std::mem::forget(held);
    drop(previous);
    pid
}

/// The terminals a daemon over `holders` finds in them.
pub(super) fn found_in(
    holders: &crate::holder::client::Holders,
) -> Vec<crate::holder::client::FoundTerminal> {
    holders
        .adopt_existing(crate::holder::client::unresolved())
        .found
        .into_iter()
        .flat_map(|holder| holder.terminals)
        .collect()
}
