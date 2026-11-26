//! A running session's scratch directory, as the files Octoboard keeps there for its agent (pasted
//! images, attached files) reach it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::protocol::{error_code, CodedError};
use crate::state::AppState;

/// The running session's own scratch directory, which is gone once its process has ended.
/// Refused with `unknown_session` or `session_not_running`.
pub(crate) fn scratch_of(state: &AppState, session_id: &str) -> Result<PathBuf> {
    state.session_record(session_id)?;
    state
        .live_session(session_id)
        .and_then(|live| live.scratch_dir())
        .ok_or_else(|| not_running(session_id))
}

/// Where the files Octoboard keeps for the session `session_id`'s agent (pasted images, attached
/// files) are saved: its scratch directory while its process runs, and, for an interrupted
/// session, its composer directory (`AppState::composer_dir`), created if missing,
/// where the files of a message queued for it wait for it to be resumed. Refused with
/// `unknown_session`, or `session_not_running` for an archived session. **Blocks.**
pub(crate) fn files_home(state: &AppState, session_id: &str) -> Result<PathBuf> {
    let record = state.session_record(session_id)?;
    if let Some(scratch) = state
        .live_session(session_id)
        .and_then(|live| live.scratch_dir())
    {
        return Ok(scratch);
    }
    // Only a session that rests with no process: one that reads as running but has none
    // registered is starting or ending, and its own scratch directory is what it would use.
    if record.status != crate::protocol::SessionStatus::Interrupted {
        return Err(not_running(session_id));
    }
    let directory = state.composer_dir(session_id);
    crate::platform::fs::create_private_dir(&directory)
        .with_context(|| format!("creating {}", directory.display()))?;
    Ok(directory)
}

/// The refusal of a session with no process.
pub(crate) fn not_running(session_id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_NOT_RUNNING,
        "this session is not running",
        &[("session", session_id)],
    )
}

/// The directory `name` under the session `session_id`'s scratch directory `scratch`, created if
/// missing. Only this directory is created, never the scratch directory above it: the process may
/// end and have it removed after the session was checked, and making it again would leave it
/// orphaned; that is refused as `session_not_running`. **Blocks.**
pub(crate) fn subdirectory(session_id: &str, scratch: &Path, name: &str) -> Result<PathBuf> {
    let directory = scratch.join(name);
    match std::fs::create_dir(&directory) {
        Ok(()) => Ok(directory),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(directory),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(not_running(session_id)),
        Err(err) => Err(err).with_context(|| format!("creating {}", directory.display())),
    }
}

#[cfg(test)]
mod tests;
