//! The conversation a switch of a session's account carries from one account's directory to
//! another's: its record, and what the agent keeps beside it. Found as [`conversation_files_in`]
//! finds a conversation's files, less the rollouts `codex archive` moved aside, which a resume
//! does not read.

use std::path::{Path, PathBuf};

use crate::agents::layout::layout;
use crate::hostfs::modified;
use crate::protocol::Agent;
use crate::records::{conversation_files_in, rollout_head, Locator};

/// The session's record in the account whose directory is `root`, as a path relative to `root`;
/// `None` when the account holds no record.
///
/// The record is, for Codex, the head of the thread's chain of rollouts, otherwise the copy
/// written to last when the agent keeps more than one. What the agent keeps besides it is
/// [`companions`]'s. `locator` keeps what it listed for later lookups: one directory several
/// accounts share is listed once for all of them.
pub fn locate_record(
    locator: &mut Locator,
    agent: Agent,
    root: &Path,
    agent_session_id: &str,
) -> Option<PathBuf> {
    let files = conversation_of(locator, agent, root, agent_session_id);
    head_of(agent, root, &files)
}

/// What the agent keeps for the session besides its `record` (as [`locate_record`] answers it),
/// as paths relative to `root` that exist there. Claude Code: the directory of the record's own
/// name beside it (subagent transcripts, tool results, the custom title) and `file-history/<id>`
/// (rewind checkpoints). Codex: the other rollouts of the thread's chain, which with `record` (the
/// head) are the whole conversation: a target given the head alone is refused, and one given the
/// first rollout alone resumes a stale branch. A subagent's rollout is named by its own thread id
/// and stays behind; Grok keeps everything inside the session directory.
pub fn companions(
    agent: Agent,
    root: &Path,
    record: &Path,
    agent_session_id: &str,
) -> Vec<PathBuf> {
    let files = conversation_of(&mut Locator::default(), agent, root, agent_session_id);
    companions_among(agent, files, record)
}

/// The record among `files`, the conversation's files relative to `root`.
fn head_of(agent: Agent, root: &Path, files: &[PathBuf]) -> Option<PathBuf> {
    let records_dir = Path::new(layout(agent).records_dir);
    let records = files
        .iter()
        .filter(|relative| relative.starts_with(records_dir));
    if agent == Agent::Codex {
        let absolute: Vec<PathBuf> = records.map(|relative| root.join(relative)).collect();
        rollout_head(&absolute).and_then(|head| head.strip_prefix(root).ok().map(Path::to_path_buf))
    } else {
        // The record is a file for Claude Code (its sidecar directory sits beside it) and a
        // directory for Grok Build. More than one is not expected; if it happens the one written
        // to last is the live one.
        records
            .filter(|relative| {
                std::fs::symlink_metadata(root.join(relative)).is_ok_and(|metadata| {
                    if agent == Agent::Claude {
                        metadata.is_file()
                    } else {
                        metadata.is_dir()
                    }
                })
            })
            .max_by_key(|relative| modified(&root.join(relative)))
            .cloned()
    }
}

/// What of `files`, the conversation's files, travels with `record` besides it.
fn companions_among(agent: Agent, files: Vec<PathBuf>, record: &Path) -> Vec<PathBuf> {
    let records_dir = Path::new(layout(agent).records_dir);
    match agent {
        Agent::Claude => {
            let sidecar = record.with_extension("");
            files
                .into_iter()
                .filter(|relative| *relative == sidecar || !relative.starts_with(records_dir))
                .collect()
        }
        Agent::Codex => files
            .into_iter()
            .filter(|relative| relative.starts_with(records_dir) && relative != record)
            .collect(),
        Agent::Grok => Vec::new(),
    }
}

/// Every file the agent keeps of the conversation in `root` that a switch may carry, relative to
/// `root`. An id that is not a canonical UUID has none.
fn conversation_of(
    locator: &mut Locator,
    agent: Agent,
    root: &Path,
    agent_session_id: &str,
) -> Vec<PathBuf> {
    let archived = layout(agent).archived_records_dir.map(Path::new);
    conversation_files_in(locator, agent, root, agent_session_id)
        .into_iter()
        .filter_map(|path| path.strip_prefix(root).ok().map(Path::to_path_buf))
        .filter(|relative| !archived.is_some_and(|archived| relative.starts_with(archived)))
        .collect()
}
