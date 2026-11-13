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
//! created branch does (see [`crate::branch_origin::ref_created`]): a session's git working in a
//! clone elsewhere is not its project's to guard. Every refusal to answer is the same `None`, which
//! the hook takes as "skip": a session that is gone, a console session, a switch that is off,
//! another repository, and any failure on the way.
//!
//! Beside the gate's inputs the answer can carry a verdict on the repository itself, which the
//! hook acts on whether or not the gate applies: with the setting that keeps a session to its own
//! project on, a repository that belongs to another registered project and not to the session's
//! is refused ([`crate::cross_project_guard::foreign_repository`]). It is decided first, so
//! neither the revert gate's own switch nor the hook's `REVERT_GATE_SKIP` turns it off; and it is
//! sent without the gate's inputs, so that a hook older than the verdict, which cannot read it,
//! fails to read the answer and skips, rather than run the gate on inputs the daemon meant to
//! refuse the update over.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::branch_origin::{is_same_branch, repo_key, reported_repository};
use crate::cross_project_guard::{foreign_repository, ForeignRepository};
use crate::git::isolated::IsolatedGit;
use crate::git::remote_branch::split_remote_branch;
use crate::git::remote_branches::{remote_default_branch, UPSTREAM_REMOTES_STDOUT};
use crate::git::repo_files::{remote_default_branch_in_files, repository_of, uses_reftable};
use crate::git_workflow;
use crate::protocol::{Project, Session};
use crate::revert_gate::GateInputs;
use crate::state::AppState;
use crate::wire_path;

/// Where git ran a hook, as a report of a created branch says it
/// ([`crate::branch_origin::RefReport`]); paths are wire paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateRequest {
    pub cwd: String,
    pub git_dir: Option<String>,
}

/// What the daemon answers a hook with: the gate's inputs, flattened so that a hook that knows
/// nothing of the verdict still reads them from a daemon that does not give one, and the verdict.
/// A verdict comes with no inputs: an older hook then finds none of the fields it requires, and
/// skips. A newer hook reads an older daemon's answer as inputs alone.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateAnswer {
    #[serde(flatten)]
    pub inputs: Option<GateInputs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign_repository: Option<ForeignRepository>,
}

/// The answer for the session's hook, or `None` for a hook that is to skip everything: the
/// verdict on the repository first, then the gate's inputs. The session, its project and their
/// repository are read once for both. Blocking: it reads the store, one reflog line per row and
/// the remotes' `HEAD` files, resolves projects' directories until the verdict is settled, and
/// runs `git` for the default branch only in a reftable repository.
pub fn answer(state: &AppState, session_id: &str, request: &GateRequest) -> Option<GateAnswer> {
    let (session, project) = match session_and_project(state, session_id) {
        Ok(Some(found)) => found,
        Ok(None) => return None,
        Err(err) => {
            tracing::debug!(session = %session_id, %err, "revert gate: no answer for the hook");
            return None;
        }
    };
    let own = repository_of(Path::new(&project.path));
    let reported = reported_repository(&request.cwd, request.git_dir.as_deref());
    match foreign_repository(state, &session, own.as_deref(), reported.as_deref()) {
        Ok(Some(foreign)) => {
            return Some(GateAnswer {
                inputs: None,
                foreign_repository: Some(foreign),
            })
        }
        Ok(None) => {}
        // A verdict that cannot be reached is no verdict: the gate is still asked.
        Err(err) => {
            tracing::debug!(session = %session_id, %err, "cross-project guard: no verdict for the hook");
        }
    }
    match inputs_of(state, &project, own.as_deref(), reported.as_deref()) {
        Ok(inputs) => inputs.map(|inputs| GateAnswer {
            inputs: Some(inputs),
            foreign_repository: None,
        }),
        Err(err) => {
            tracing::debug!(session = %session_id, %err, "revert gate: no inputs for the hook");
            None
        }
    }
}

/// The session and the project it belongs to; `None` for a session that is gone and for a console
/// session.
fn session_and_project(state: &AppState, session_id: &str) -> Result<Option<(Session, Project)>> {
    let Some(session) = state.store.get_session(session_id)? else {
        return Ok(None);
    };
    let Some(project_id) = &session.project_id else {
        return Ok(None);
    };
    let Some(project) = state.store.get_project(project_id)? else {
        return Ok(None);
    };
    Ok(Some((session, project)))
}

/// The inputs of the gate for the session's project, in the repository `own`, or `None` for a hook
/// that is to skip it: `reported` is the repository the hook ran in.
fn inputs_of(
    state: &AppState,
    project: &Project,
    own: Option<&Path>,
    reported: Option<&Path>,
) -> Result<Option<GateInputs>> {
    if !git_workflow::effective(&state.store, project)?.revert_gate {
        return Ok(None);
    }
    let Some(repo) = own else {
        return Ok(None);
    };
    if reported != Some(repo) {
        return Ok(None);
    }

    let mut inputs = GateInputs {
        guard: default_branch(state, repo).into_iter().collect(),
        ..GateInputs::default()
    };
    for row in state.store.branch_origins(&repo_key(repo))? {
        // A branch deleted and made again outside the session is not the one the row describes.
        if !is_same_branch(repo, &row) {
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
/// This departs from the Git mode's lookup (`remote_default_branch`), which asks `git` for
/// the configured remotes: the hook waits on the answer and a `git` costs tens of milliseconds, so
/// where the references are files the remotes' `HEAD` files are read directly, `origin` preferred
/// and else the first in byte order, as that lookup prefers. Only a reftable repository, which has
/// no such files, asks `git`.
fn default_branch(state: &AppState, repo: &Path) -> Option<String> {
    if !uses_reftable(repo) {
        return remote_default_branch_in_files(repo)
            .and_then(|branch| String::from_utf8(branch).ok());
    }
    let found = (state.host_tools.get().git)().and_then(|git| {
        let (isolated, cancel) = (IsolatedGit::of(git), AtomicBool::new(false));
        let Some(upstream) = remote_default_branch(&isolated, repo, &cancel)? else {
            return Ok(None);
        };
        // `<remote>/<branch>` as a wire path: decoded before it is split by the remotes the
        // repository has, which `git` prints as they are. A remote's own name may hold a slash.
        let Some(upstream) = branch_name(&upstream.name) else {
            return Ok(None);
        };
        let listing = isolated.run(repo, "remote", &[], UPSTREAM_REMOTES_STDOUT, &cancel)?;
        let listing = String::from_utf8_lossy(&listing);
        Ok(
            split_remote_branch(&upstream, listing.lines().filter(|name| !name.is_empty()))
                .map(|(_, branch)| branch.to_string()),
        )
    });
    match found {
        Ok(branch) => branch,
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
