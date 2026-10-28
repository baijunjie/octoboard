//! What the tools that look at a project's repository, `list_project_changes`, `read_project_diff`
//! and `list_project_branches`, share: which project a call may read, one browse request run to its
//! reply under the daemon-wide gate, and the words a browse failure is given for an agent.
//!
//! Every read goes through `browse::serve` with the request the window sends, so the path checks,
//! the environment `git` runs in, the budgets and the rule that nothing is written to the
//! repository are the browse layer's. What is added here is the reach (any role, the caller's own
//! console's projects only, as `list_projects` gives another console's projects without their git
//! status) and a read slot and a share of the retained bytes from the same daemon-wide gate the
//! window's reads wait on (all the tools' reads together held to one connection's share).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

use crate::browse::git::GitEnv;
use crate::browse::lane::Admission;
use crate::browse::wire_path;
use crate::browse::{self, budget};
use crate::protocol::{error_code, BrowseBody, CodedError, Event, GitSourceInfo, Project, Session};
use crate::state::AppState;

use super::args::{find_project, optional_string};

/// How the daemon's `git` environment is made: the user's shell in production, a fixture's own in
/// a test.
pub(super) type MakeGit = fn() -> Result<GitEnv>;

/// The project a call names, among the caller's own console's: its own when none is named and it
/// has one. Another console's project is refused as its `git_status` is not listed by
/// `list_projects`: what its repository holds is for that console to look at.
pub(super) fn readable_project(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Project> {
    let wanted = match optional_string(arguments, "project") {
        Some(wanted) => wanted,
        None => caller
            .project_id
            .clone()
            .ok_or_else(|| anyhow!("`project` is required: a console session has no project"))?,
    };
    let project = find_project(state, caller, &wanted)?;
    if project.console_id != caller.console_id {
        bail!(
            "`{wanted}` is a project of another console; a project's repository can be read \
             only for this console's own projects, as `list_projects` lists another console's \
             without its git status"
        );
    }
    Ok(project)
}

/// Runs one browse request to its reply on the blocking pool, after waiting for a place in the
/// tools' share, a read slot and `reservation` retained bytes from the daemon-wide gate, as a
/// window's request waits for its own. The admission comes back with the reply and is held until
/// the caller has built its answer from it; if the call is dropped meanwhile, the `git` is stopped
/// and the admission is given back when it has.
pub(super) async fn browse(
    state: &Arc<AppState>,
    make_git: MakeGit,
    body: BrowseBody,
    reservation: usize,
) -> Result<(Event, Admission)> {
    let cancel = Arc::new(AtomicBool::new(false));
    let _stop = StopOnDrop(cancel.clone());
    let admission = state.browse_gate.admit(reservation).await;
    let state = state.clone();
    let (outcome, admission) = tokio::task::spawn_blocking(move || {
        let outcome = browse::serve(&state, None, body, &make_git, &cancel);
        (outcome, admission)
    })
    .await?;
    Ok((outcome.map_err(readable_error)?, admission))
}

/// Cancels the read it was made for when it drops.
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// A browse failure worded for the agent: the daemon's message, with the cases an agent can act
/// on named.
fn readable_error(err: anyhow::Error) -> anyhow::Error {
    let Some(coded) = err.downcast_ref::<CodedError>() else {
        return err;
    };
    match coded.code {
        error_code::NOT_A_GIT_REPOSITORY => anyhow!("the project is not in a git repository"),
        error_code::LIMIT_EXCEEDED => anyhow!(
            "too large to read through this tool ({}); it is bounded at the daemon, not by an \
             argument",
            coded.message
        ),
        _ => anyhow!("{}", coded.message),
    }
}

/// What `project_source` says of the project's repository, or the reason there is none.
pub(super) async fn git_source(
    state: &Arc<AppState>,
    make_git: MakeGit,
    project: &Project,
) -> Result<GitSourceInfo> {
    let body = BrowseBody::GetProjectSource {
        project: project.id.clone(),
    };
    let (Event::ProjectSource { source, .. }, _admission) =
        browse(state, make_git, body, budget::READ_RESERVATION).await?
    else {
        bail!("the daemon answered something other than the project's source");
    };
    match source.git {
        Some(git) => Ok(git),
        None => match source.git_error {
            Some(reason) => bail!("git cannot read the project's repository: {reason}"),
            None => bail!("the project is not in a git repository"),
        },
    }
}

/// A name the daemon spells as a wire path, as the agent knows it: plain text, as
/// `get_session_branches` gives paths and branch names.
pub(super) fn plain(wire: &str) -> String {
    wire_path::decode(wire).map_or_else(
        || wire.to_string(),
        |bytes| String::from_utf8_lossy(&bytes).into_owned(),
    )
}

/// The `git` environment of a fixture, as the tools' tests make it.
#[cfg(test)]
pub(super) fn test_git() -> Result<GitEnv> {
    use crate::test_support::{isolated_git_env, SYSTEM_GIT};
    Ok(GitEnv::new(SYSTEM_GIT.to_string(), isolated_git_env()))
}
