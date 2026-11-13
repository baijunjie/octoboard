//! The `AppState` a test runs over, the host tools and environment it runs in, and the records it
//! seeds.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::{isolated_git_env, ScratchDir, SYSTEM_GIT};
use crate::protocol::{Account, AgentId, Console, Project, ProjectSource};
use crate::state::AppState;
use crate::store::Store;

/// The key [`app_state`]'s daemon requires of every connection.
pub(crate) const TEST_KEY: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The installation's entry point an [`app_state`] names, another path than its running image.
pub(crate) const INSTALL_EXE: &str = "/opt/octoboard/bin/octoboardd";

/// An `AppState` over a fresh store in a scratch directory labelled `label`, with no console in
/// it. The caller keeps the directory alive for as long as the state is used. A console added to
/// the state needs a workdir of its own rather than this directory, because deleting a console
/// removes its workdir. The directory is removed again once the state and its process ledger are
/// gone, since the state's exit watchers and blocking tasks can outlive the test and save a
/// session's output or the ledger into it.
pub(crate) fn app_state(label: &str) -> (Arc<AppState>, ScratchDir) {
    app_state_with_key(label, TEST_KEY)
}

/// [`app_state`] whose daemon requires `key` of every connection, such as the launch key of a
/// daemon started again over another one's data directory.
pub(crate) fn app_state_with_key(label: &str, key: &str) -> (Arc<AppState>, ScratchDir) {
    let mut dir = ScratchDir::new(label);
    let store = Store::open(&dir.join("octoboard.db")).expect("store");
    let state = Arc::new(AppState::new(
        store,
        1234,
        key.to_string(),
        "/opt/octoboardd".to_string(),
        INSTALL_EXE.to_string(),
        dir.join("output"),
        dir.join("consoles"),
        dir.join("handoff"),
        crate::processes::ledger::ProcessLedger::new(&dir),
    ));
    state.install_auto_switch(crate::coordinator::auto_switch::hooks());
    state.process_ledger.without_scanner();
    state.host_tools.replace(host_tools());
    // Stand-in sessions run no hooks, so nothing written into one is ever reported sent; a test of
    // that check sets a wait of its own.
    *state.send_wait.lock().unwrap() = None;
    dir.removed_again_after(&state);
    dir.removed_again_after(&state.process_ledger);
    (state, dir)
}

/// What requests reach the host through in a test: the system `git` over [`isolated_git_env`], a
/// Trash that deletes, and a plain shell over [`shell_env`] for a shell terminal ([`test_shell`]),
/// so no test runs the user's profile or fills their Trash.
pub(crate) fn host_tools() -> crate::host_tools::Tools {
    crate::host_tools::Tools {
        git: system_git,
        // A fresh snapshot finds the same `git`: no test takes the user's login shell.
        fresh_git: system_git,
        trash: |path| {
            if path.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            }
            .map_err(|err| crate::hostfs::trash::TrashFailure::new(path, err))
        },
        shell: test_shell,
        shell_env: || Ok(shell_env()),
        scheduled_shell_env: || Ok(shell_env()),
    }
}

/// The `git` a test's host tools hand out, from the cached snapshot or a fresh one alike:
/// [`SYSTEM_GIT`] over [`isolated_git_env`].
pub(crate) fn system_git() -> anyhow::Result<crate::git::run::UserGit> {
    Ok(crate::git::run::UserGit::new(
        SYSTEM_GIT,
        isolated_git_env(),
    ))
}

/// A stand-in for [`crate::host_tools::Tools::git`] or `fresh_git` on a machine without `git`: the
/// refusal `crate::env_shell::resolve_program` gives for one that is not on the `PATH`.
pub(crate) fn git_not_found() -> anyhow::Result<crate::git::run::UserGit> {
    Err(crate::protocol::CodedError::raised(
        crate::protocol::error_code::BINARY_NOT_FOUND,
        "`git` was not found on PATH in the snapshotted shell environment",
        &[("binary", "git")],
    ))
}

/// The shell a test's shell terminal runs: an interactive `bash` reading no profile on Unix; on
/// Windows the shell the daemon starts there (`platform::shell::login_shell`), PowerShell on every
/// host a test runs on, reading no profile and keeping no history.
fn test_shell() -> crate::platform::shell::ShellCommand {
    #[cfg(unix)]
    return posix_shell();
    #[cfg(windows)]
    return crate::platform::shell::ShellCommand {
        program: crate::platform::shell::login_shell().program,
        args: [
            "-NoLogo",
            "-NoProfile",
            "-NoExit",
            "-Command",
            "Set-PSReadLineOption -HistorySaveStyle SaveNothing",
        ]
        .map(Into::into)
        .to_vec(),
    };
}

/// An interactive `bash` reading no profile (Git Bash on Windows), for a test whose shell terminal
/// needs a POSIX shell on every host.
pub(crate) fn posix_shell() -> crate::platform::shell::ShellCommand {
    crate::platform::shell::ShellCommand {
        program: super::bash(),
        args: ["--noprofile", "--norc", "-i"].map(Into::into).to_vec(),
    }
}

/// What is typed into a test's shell terminal, Enter included: `bash` into the Unix one, or
/// `powershell` into the Windows one, PowerShell ([`test_shell`]).
pub(crate) fn typed(bash: &str, powershell: &str) -> Vec<u8> {
    format!("{}\r", if cfg!(windows) { powershell } else { bash }).into_bytes()
}

/// The environment a test's shell terminal starts in: nothing of the developer's, a prompt that
/// is plain text, and no history written anywhere. It is also the environment a launch and the
/// Claude Code model listing resolve its configuration in, so its home is a directory that does
/// not exist, which keeps a test from reading the developer's own `~/.claude`. That home reaches
/// a test's shell terminals as well; none relies on one. On Windows it also names the system's
/// directory, without which Windows PowerShell does not start.
pub(crate) fn shell_env() -> HashMap<String, String> {
    let system_root = cfg!(windows)
        .then(|| std::env::var("SystemRoot").ok())
        .flatten()
        .map(|root| ("SystemRoot".to_string(), root));
    [
        ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin"),
        ("PS1", "$ "),
        ("HISTFILE", "/dev/null"),
        ("HOME", "/nonexistent/octoboard-test-home"),
        // macOS's bash otherwise opens every interactive shell with a notice about zsh.
        ("BASH_SILENCE_DEPRECATION_WARNING", "1"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .chain(system_root)
    .collect()
}

/// A console whose working directory is below `dir`, for a store that needs one to hold projects
/// and sessions.
pub(crate) fn console(dir: &Path) -> crate::protocol::Console {
    crate::protocol::Console {
        id: "console-1".to_string(),
        name: "Console".to_string(),
        workdir: dir.join("workdir").to_string_lossy().into_owned(),
        default_agent: AgentId::CLAUDE,
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

/// The local project `p1` of [`console`]'s console, at `path`, inserted into `state`'s store.
pub(crate) fn insert_project(state: &AppState, path: &Path) -> crate::protocol::Project {
    let project = serde_json::from_value(serde_json::json!({
        "id": "p1", "console_id": "console-1", "host_id": crate::store::LOCAL_HOST_ID,
        "name": "Project", "path": path, "source": "local",
        "trust_consent": false, "pinned": false, "tags": [],
    }))
    .unwrap();
    state.store.insert_project(&project).unwrap();
    project
}

/// Seeds the models `agent` offers under its default account as `(id, efforts)`, so that a test
/// that gives a model or an effort is checked against this list and not against the agent
/// installed on the machine, which running its catalogue command would make it depend on.
pub(crate) fn offer_models(state: &AppState, agent: AgentId, models: &[(&str, &[&str])]) {
    let models = models
        .iter()
        .map(|(id, efforts)| crate::protocol::AgentModel {
            id: id.to_string(),
            name: id.to_string(),
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            version: None,
            available: None,
        })
        .collect();
    let listing = crate::agents::models::Listing {
        models,
        unavailable: None,
    };
    // The listing of a provider that names a default model is never cached, so it is put in place
    // another way.
    if crate::adapter::provider(agent)
        .catalogue()
        .names_default_model()
    {
        state.agent_models.seed(agent, None, listing);
    } else {
        state.agent_models.get_or_load(agent, None, || listing);
    }
}

/// A console and a project (whose directory does not exist, so a relaunch is refused at the
/// launch's first check before it writes anything), and one account of each agent whose
/// directory is under `dir`. `prefix` labels the scratch directory.
pub(crate) fn console_with_project_and_accounts(prefix: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = app_state(prefix);
    state
        .store
        .insert_console(&Console {
            workdir: dir.join("no-such-workdir").to_string_lossy().into_owned(),
            ..console(&dir)
        })
        .unwrap();
    state
        .store
        .insert_project(&Project {
            id: "project-1".to_string(),
            console_id: "console-1".to_string(),
            host_id: crate::store::LOCAL_HOST_ID.to_string(),
            name: "Project".to_string(),
            path: dir.join("no-such-project").to_string_lossy().into_owned(),
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
    for (id, agent) in [
        ("claude-a", AgentId::CLAUDE),
        ("codex-a", AgentId::CODEX),
        ("grok-a", AgentId::GROK),
    ] {
        state
            .store
            .insert_account(&Account {
                id: id.to_string(),
                agent,
                name: id.to_string(),
                config_dir: dir.join(id).to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();
    }
    (state, dir)
}

/// The sessions on record, sorted by id.
pub(crate) fn session_ids(state: &Arc<AppState>) -> Vec<String> {
    let mut ids: Vec<String> = state
        .store
        .list_sessions()
        .unwrap()
        .into_iter()
        .map(|session| session.id)
        .collect();
    ids.sort();
    ids
}
