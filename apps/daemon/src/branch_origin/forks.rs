//! The forks recorded for branches, for the revert gate and for the messages that name targets.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use super::{is_same_branch, repo_key, transfer};
use crate::git::repo_files::repository_of;
use crate::state::AppState;
use crate::store::BranchOrigin;

/// Where a branch came from, as its row records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fork {
    /// The branch it was created from, its target branch, as text; `None` when it was created from
    /// something that is no local branch.
    pub target_branch: Option<String>,
    /// The commit it was created at.
    pub fork_point: String,
}

fn fork_of(row: BranchOrigin) -> (String, Fork) {
    let fork = Fork {
        target_branch: row.target_branch.as_deref().map(transfer::branch_name),
        fork_point: row.birth_commit,
    };
    (row.branch, fork)
}

/// The [`Fork`] of each of `branches` (wire paths) that has a row in `repo`, by branch, each only
/// while it is still the branch the row was made for. Reads the store and the reflog files, and
/// starts no process. Blocking.
pub fn recorded(
    state: &AppState,
    repo: &Path,
    branches: &[String],
) -> Result<HashMap<String, Fork>> {
    forks_in(state, repo, branches, true)
}

/// The [`Fork`] of each of `branches` that has a row in `repo`, by branch, each checked against its
/// branch's reflog when `verify` is set.
fn forks_in(
    state: &AppState,
    repo: &Path,
    branches: &[String],
    verify: bool,
) -> Result<HashMap<String, Fork>> {
    Ok(state
        .store
        .branch_origins(&repo_key(repo))?
        .into_iter()
        .filter(|row| branches.contains(&row.branch))
        .filter(|row| !verify || is_same_branch(repo, row))
        .map(fork_of)
        .collect())
}

/// [`recorded`] for the repository the project at `project_path` is in, and nothing when it is in
/// none.
pub fn recorded_in_project(
    state: &AppState,
    project_path: &str,
    branches: &[String],
) -> Result<HashMap<String, Fork>> {
    match repository_of(Path::new(project_path)) {
        Some(repo) => recorded(state, &repo, branches),
        None => Ok(HashMap::new()),
    }
}

/// [`recorded_in_project`] for a caller that holds a listing the repository's rows have just been
/// reconciled with, which checked each row against its branch's reflog: the rows are not checked
/// again. Reads the store only. Blocking.
pub fn forks_in_project(
    state: &AppState,
    project_path: &str,
    branches: &[String],
) -> Result<HashMap<String, Fork>> {
    match repository_of(Path::new(project_path)) {
        Some(repo) => forks_in(state, &repo, branches, false),
        None => Ok(HashMap::new()),
    }
}

/// The target branch recorded for each of `branches` (wire paths) in `repo`, as text and by
/// branch, for a message that names them; a branch with none is left out. A failure to read is
/// logged and leaves them all out: the message is worth sending without. Blocking.
pub fn targets_in(state: &AppState, repo: &Path, branches: &[String]) -> HashMap<String, String> {
    match recorded(state, repo, branches) {
        Ok(found) => found
            .into_iter()
            .filter_map(|(branch, fork)| Some((branch, fork.target_branch?)))
            .collect(),
        Err(err) => {
            tracing::warn!(%err, "reading the target branches of branches transferred failed");
            HashMap::new()
        }
    }
}

/// [`targets_in`] for the repository the project at `project_path` is in, and nothing when it is
/// in none.
pub fn targets_in_project(
    state: &AppState,
    project_path: &str,
    branches: &[String],
) -> HashMap<String, String> {
    repository_of(Path::new(project_path))
        .map(|repo| targets_in(state, &repo, branches))
        .unwrap_or_default()
}
