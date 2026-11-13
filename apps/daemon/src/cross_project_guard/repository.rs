//! The git layer of the guard: whether the repository a session's git is about to update belongs
//! to another registered project than the session's.
//!
//! The session's git hook asks the daemon at every ref update, and the daemon answers from here
//! ([`crate::session_git_hooks::gate_inputs::answer`]), so the setting's change reaches running
//! sessions at once.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::project_git::first_project_in_repository;
use crate::protocol::Session;
use crate::session_tools::catalogue::qualified_tool_name;
use crate::state::AppState;

/// The project a refused repository belongs to, and what the session calls to hand its work over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignRepository {
    pub project: String,
    /// The session's agent's own qualified name of `request_session`.
    pub tool: String,
}

/// The verdict on `reported`, the repository the hook ran in, for `session`, whose own project is
/// in the repository `own` (as [`crate::git::repo_files::repository_of`] answers for it). Only a
/// repository that belongs to a registered project and to none of the session's own is foreign:
/// another project in the same repository, a linked worktree of it included, is the session's own
/// when the session's project is in the repository too. A repository no project is in (a test
/// suite's temporary one) and one the daemon cannot trace are not foreign, and neither is any
/// repository while the setting is off, which is read here at every update. Blocking: it reads
/// the store and resolves projects' directories until one is found in the repository.
pub fn foreign_repository(
    state: &AppState,
    session: &Session,
    own: Option<&Path>,
    reported: Option<&Path>,
) -> Result<Option<ForeignRepository>> {
    if !state.store.keep_sessions_in_own_project()? {
        return Ok(None);
    }
    let Some(reported) = reported else {
        return Ok(None);
    };
    if own == Some(reported) {
        return Ok(None);
    }
    Ok(
        first_project_in_repository(state, reported)?.map(|owner| ForeignRepository {
            project: owner.name,
            tool: qualified_tool_name(session.agent, "request_session"),
        }),
    )
}
