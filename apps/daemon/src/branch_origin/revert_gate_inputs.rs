//! What the daemon tells the revert gate each time a project session's git moves a branch or
//! pushes: whether it applies at all, and the branches it holds commits against.
//!
//! The hook (`octoboardd gate-hook`) asks at run time rather than being started with answers,
//! because the answers change while a session runs: the switch is the user's to turn off in the
//! middle of a session, and a branch the session creates is a target to hold later work against
//! from the next transaction on. So the setting is read from the store at every request, and the
//! request is answered from the store, one reflog line per recorded branch and the remotes' `HEAD`
//! files, never a long `git`; only a reftable repository, which has no such files, runs one.
//!
//! The gate applies only to the repository the session's own project is in, as a report of a
//! created branch does (see [`super::ref_created`]): a session's git working in a clone elsewhere
//! is not its project's to guard. Every refusal to answer is the same `None`, which the hook takes
//! as "skip": a session that is gone, a console session, a switch that is off, another repository,
//! and any failure on the way.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::browse::{compare, wire_path};
use crate::git_workflow;
use crate::revert_gate::GateInputs;
use crate::state::AppState;

use super::{git_files, is_same_branch, repo_key, reported_repository, repository_of};

/// Where git ran a hook, as a report of a created branch says it ([`super::RefReport`]); paths
/// are wire paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateRequest {
    pub cwd: String,
    pub git_dir: Option<String>,
}

/// The inputs of the gate for the session's project, or `None` for a hook that is to skip it.
/// Blocking: it reads the store, one reflog line per row and the remotes' `HEAD` files, and runs
/// `git` for the default branch only in a reftable repository.
pub fn inputs(state: &AppState, session_id: &str, request: &GateRequest) -> Option<GateInputs> {
    match inputs_of(state, session_id, request) {
        Ok(inputs) => inputs,
        Err(err) => {
            tracing::debug!(session = %session_id, %err, "revert gate: no inputs for the hook");
            None
        }
    }
}

fn inputs_of(
    state: &AppState,
    session_id: &str,
    request: &GateRequest,
) -> Result<Option<GateInputs>> {
    let Some(session) = state.store.get_session(session_id)? else {
        return Ok(None);
    };
    let Some(project_id) = session.project_id else {
        return Ok(None);
    };
    let Some(project) = state.store.get_project(&project_id)? else {
        return Ok(None);
    };
    if !git_workflow::effective(&state.store, &project)?.revert_gate {
        return Ok(None);
    }
    let Some(repo) = repository_of(Path::new(&project.path)) else {
        return Ok(None);
    };
    if reported_repository(&request.cwd, request.git_dir.as_deref()).as_deref() != Some(&repo) {
        return Ok(None);
    }

    let mut inputs = GateInputs {
        guard: default_branch(state, &repo).into_iter().collect(),
        ..GateInputs::default()
    };
    for row in state.store.branch_origins(&repo_key(&repo))? {
        // A branch deleted and made again outside the session is not the one the row describes.
        if !is_same_branch(&repo, &row) {
            continue;
        }
        let Some(branch) = branch_name(&row.branch) else {
            continue;
        };
        if let Some(target) = row.target_branch.as_deref().and_then(branch_name) {
            inputs.targets.push((branch.clone(), target));
        }
        inputs.fork_points.push((branch, row.birth_commit));
    }
    Ok(Some(inputs))
}

/// The repository's default branch as a local branch name: what a remote's `HEAD` names, less the
/// remote. A repository with none, or whose lookup fails, has no resident guarded branch.
///
/// This departs from the Git mode's lookup (`compare::remote_default_branch`), which asks `git` for
/// the configured remotes: the hook waits on the answer and a `git` costs tens of milliseconds, so
/// where the references are files the remotes' `HEAD` files are read directly, `origin` preferred
/// and else the first in byte order, as that lookup prefers. Only a reftable repository, which has
/// no such files, asks `git`.
fn default_branch(state: &AppState, repo: &Path) -> Option<String> {
    if !git_files::uses_reftable(repo) {
        return git_files::remote_default_branch_in_files(repo)
            .and_then(|branch| String::from_utf8(branch).ok());
    }
    let found = (state.host_tools.get().git)().and_then(|git| {
        compare::remote_default_branch(&git.browse_env(), repo, &AtomicBool::new(false))
    });
    match found {
        // `<remote>/<branch>`; a remote whose own name holds a slash is not told apart.
        Ok(Some(upstream)) => upstream
            .name
            .split_once('/')
            .and_then(|(_, branch)| branch_name(branch)),
        Ok(None) => None,
        Err(err) => {
            tracing::debug!(repository = %repo.display(), %err, "revert gate: no default branch");
            None
        }
    }
}

/// A branch name given as a wire path, as text; a name that is not valid UTF-8 is left out, as
/// the gate reads the hook's lines as text.
fn branch_name(wire: &str) -> Option<String> {
    String::from_utf8(wire_path::decode(wire)?).ok()
}
