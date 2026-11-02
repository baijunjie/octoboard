//! The `AppState` a test runs over, the host tools and environment it runs in, and the records it
//! seeds.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::{isolated_git_env, ScratchDir, SYSTEM_GIT};
use crate::protocol::{Account, Agent, Console, Project, ProjectSource};
use crate::state::AppState;
use crate::store::Store;

/// The key [`app_state`]'s daemon requires of every connection.
pub(crate) const TEST_KEY: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// An `AppState` over a fresh store in a scratch directory labelled `label`, with no console in
/// it. The caller keeps the directory alive for as long as the state is used. A console added to
/// the state needs a workdir of its own rather than this directory, because deleting a console
/// removes its workdir. The directory is removed again once the state and its process ledger are
/// gone, since the state's exit watchers and blocking tasks can outlive the test and save a
/// session's output or the ledger into it.
pub(crate) fn app_state(label: &str) -> (Arc<AppState>, ScratchDir) {
    let mut dir = ScratchDir::new(label);
    let store = Store::open(&dir.join("octoboard.db")).expect("store");
    let state = Arc::new(AppState::new(
        store,
        1234,
        TEST_KEY.to_string(),
        "/opt/octoboardd".to_string(),
        dir.join("output"),
        dir.join("consoles"),
        dir.join("handoff"),
        crate::processes::ledger::ProcessLedger::new(&dir),
    ));
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
/// Trash that deletes, and a plain `bash` over [`shell_env`] for a shell terminal, so no test runs
/// the user's login shell or fills their Trash.
pub(crate) fn host_tools() -> crate::host_tools::Tools {
    crate::host_tools::Tools {
        git: || {
            Ok(crate::git::run::UserGit::new(
                SYSTEM_GIT,
                isolated_git_env(),
            ))
        },
        trash: |path| {
            if path.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            }
            .map_err(|err| crate::hostfs::trash::TrashFailure::new(path, err))
        },
        shell: || crate::terminal::shell::ShellCommand {
            program: "/bin/bash".to_string(),
            args: ["--noprofile", "--norc", "-i"].map(str::to_string).to_vec(),
        },
        shell_env: || Ok(shell_env()),
    }
}

/// The environment a test's shell terminal starts in: nothing of the developer's, a prompt that
/// is plain text, and no history written anywhere.
pub(crate) fn shell_env() -> HashMap<String, String> {
    [
        ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin"),
        ("PS1", "$ "),
        ("HISTFILE", "/dev/null"),
        // macOS's bash otherwise opens every interactive shell with a notice about zsh.
        ("BASH_SILENCE_DEPRECATION_WARNING", "1"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect()
}

/// A console whose working directory is below `dir`, for a store that needs one to hold projects
/// and sessions.
pub(crate) fn console(dir: &Path) -> crate::protocol::Console {
    crate::protocol::Console {
        id: "console-1".to_string(),
        name: "Console".to_string(),
        workdir: dir.join("workdir").to_string_lossy().into_owned(),
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
pub(crate) fn offer_models(state: &AppState, agent: Agent, models: &[(&str, &[&str])]) {
    let models = models
        .iter()
        .map(|(id, efforts)| crate::protocol::AgentModel {
            id: id.to_string(),
            name: id.to_string(),
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
        })
        .collect();
    state
        .agent_models
        .get_or_load(agent, None, || crate::agents::models::Listing {
            models,
            unavailable: None,
        });
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
        })
        .unwrap();
    for (id, agent) in [
        ("claude-a", Agent::Claude),
        ("codex-a", Agent::Codex),
        ("grok-a", Agent::Grok),
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
