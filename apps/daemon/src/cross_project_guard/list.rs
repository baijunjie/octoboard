//! The list a launch writes for the guard hook: which directories belong to other projects, which
//! belong to the session's own, and which pass whatever project holds them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::git::repo_files::repository_of;
use crate::hostfs::place::looked_within;
use crate::hostfs::replace::{replace_beside, Permissions};
use crate::protocol::{Project, Session, SessionKind};
use crate::session_tools::catalogue::qualified_tool_name;
use crate::state::AppState;

/// The file in the session's scratch directory that holds the list.
const LIST_FILE: &str = "cross-project-guard.json";

/// What the guard hook knows. Paths are as the daemon holds them, to be told apart by what they
/// lead to rather than by their text.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardList {
    /// The session's agent's own qualified name of `request_session`, which the refusal names.
    pub tool: String,
    /// The roots of the session's own project: where it wins over another project's root that
    /// holds it.
    pub own: Vec<String>,
    /// The canonical common directory of the repository the session's own project is in: a path
    /// in it is the session's own wherever its working tree is, a linked worktree made after the
    /// launch and the other projects of a monorepo included.
    #[serde(default)]
    pub repository: Option<String>,
    /// The roots of the other registered projects, whatever console they are in.
    pub projects: Vec<GuardedProject>,
    /// Directories that pass whatever project holds them: the agents' configuration and data
    /// directories, Octoboard's data directory (the session scratch directories included) and the
    /// system's temporary directories.
    pub shared: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardedProject {
    pub name: String,
    pub root: String,
}

/// How long the look at the registered projects' repositories may take. Past it a project on a
/// mount that does not answer stays listed among the others.
const LOOK_DEADLINE: Duration = Duration::from_secs(5);

/// What the list is made of, read from the store.
struct Inputs {
    project: Project,
    others: Vec<Project>,
    shared: Vec<String>,
}

/// The list for `session`'s launch, or `None` where no guard is injected: a console session, and
/// any session while the setting is off, which is read here and so applies to sessions launched
/// afterwards. The session's own project and the projects in its repository are never listed
/// among the others: a session works in the whole repository, its linked worktrees included.
/// The repositories are looked at on blocking threads, each within [`LOOK_DEADLINE`]; a project
/// whose look outran it is listed, as is one in no repository.
pub async fn for_session(state: &Arc<AppState>, session: &Session) -> Result<Option<GuardList>> {
    let inputs = {
        let (state, session) = (state.clone(), session.clone());
        tokio::task::spawn_blocking(move || read_inputs(&state, &session)).await??
    };
    let Some(Inputs {
        project,
        others,
        shared,
    }) = inputs
    else {
        return Ok(None);
    };
    let paths = std::iter::once(&project)
        .chain(&others)
        .map(|project| PathBuf::from(&project.path))
        .collect();
    let repositories =
        looked_within(&state.directory_looks, paths, LOOK_DEADLINE, repository_of).await;
    let repository = repositories.get(Path::new(&project.path));
    let projects = others
        .into_iter()
        .filter(|other| {
            repository.is_none() || repositories.get(Path::new(&other.path)) != repository
        })
        .map(|Project { name, path, .. }| GuardedProject { name, root: path })
        .collect();
    Ok(Some(GuardList {
        tool: qualified_tool_name(session.agent, "request_session"),
        own: vec![project.path],
        repository: repository.map(|repository| path_text(repository)),
        projects,
        shared,
    }))
}

/// The store's side of [`for_session`]: `None` where no guard is injected.
fn read_inputs(state: &AppState, session: &Session) -> Result<Option<Inputs>> {
    if session.kind != SessionKind::Project || !state.store.keep_sessions_in_own_project()? {
        return Ok(None);
    }
    let Some(project) = session
        .project_id
        .as_deref()
        .map(|id| state.store.require_project(id))
        .transpose()?
    else {
        return Ok(None);
    };
    let others = state
        .store
        .list_projects()?
        .into_iter()
        .filter(|other| other.id != project.id)
        .collect();
    let mut shared = vec![path_text(&crate::paths::data_dir())];
    // The lexical roots hold the canonical ones too.
    shared.extend(
        crate::path_links::temp_roots()
            .lexical
            .iter()
            .map(|dir| path_text(dir)),
    );
    for account in state.store.list_accounts()? {
        shared.push(account.config_dir);
    }
    Ok(Some(Inputs {
        project,
        others,
        shared,
    }))
}

impl GuardList {
    /// Adds what only the launch knows: the directory the session starts in, which can be a
    /// worktree of its project outside the project's directory, the temporary directories the
    /// session's shell names, and each agent's default configuration directory as the launch's
    /// shell resolves it, with the usual one under the home directory beside it.
    pub fn with_launch(mut self, cwd: &Path, shell_env: &HashMap<String, String>) -> Self {
        self.own.push(path_text(cwd));
        let home = crate::paths::known_home_dir();
        for name in ["TMPDIR", "TEMP", "TMP"] {
            // A temporary directory that is the root of a disk, or holds the home directory,
            // would make everything there pass.
            let Some(dir) = shell_env.get(name).map(Path::new).filter(|dir| {
                dir.is_absolute()
                    && dir.parent().is_some()
                    && !home.as_deref().is_some_and(|home| home.starts_with(dir))
            }) else {
                continue;
            };
            self.shared.push(path_text(dir));
        }
        for agent in crate::protocol::AgentId::ALL {
            let default = path_text(&crate::adapter::default_account_dir(agent, shell_env));
            let usual = path_text(
                &crate::adapter::snapshot_home(shell_env)
                    .join(crate::providers::layout::layout(agent).default_dir),
            );
            self.shared.push(default.clone());
            if usual != default {
                self.shared.push(usual);
            }
        }
        self
    }
}

/// Writes `list` into `scratch` and returns the file's path.
pub fn write(scratch: &Path, list: &GuardList) -> Result<PathBuf> {
    let file = scratch.join(LIST_FILE);
    let bytes = serde_json::to_vec(list).context("encoding the cross-project guard list")?;
    replace_beside(&file, &bytes, Permissions::Private)
        .context("writing the cross-project guard list")?;
    Ok(file)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests;
