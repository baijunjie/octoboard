//! The conversation a switch of a session's account carries from one account's directory to
//! another's: its record, and what the agent keeps beside it. Found as [`conversation_files_in`]
//! finds a conversation's files, less the rollouts `codex archive` moved aside, which a resume
//! does not read.

use std::path::{Path, PathBuf};

use crate::adapter::provider;
use crate::protocol::AgentId;
use crate::providers::layout::layout;
use crate::records::{conversation_files_in, Locator};

/// The session's record in the account whose directory is `root`, as a path relative to `root`;
/// `None` when the account holds no record.
///
/// The record is the one the provider's store picks among the conversation's files under the
/// records directory (`RecordStore::carried_head`): for Codex the head of the thread's chain of
/// rollouts, otherwise the copy written to last when the agent keeps more than one. What the agent
/// keeps besides it is [`companions`]'s. `locator` keeps what it listed for later lookups: one
/// directory several accounts share is listed once for all of them.
pub fn locate_record(
    locator: &mut Locator,
    agent: AgentId,
    root: &Path,
    agent_session_id: &str,
) -> Option<PathBuf> {
    let files = conversation_of(locator, agent, root, agent_session_id);
    head_of(agent, root, &files)
}

/// What the agent keeps for the session besides its `record` (as [`locate_record`] answers it), as
/// paths relative to `root` that exist there (`RecordStore::carried_companions`). Claude Code: the
/// directory of the record's own name beside it (subagent transcripts, tool results, the custom
/// title) and `file-history/<id>` (rewind checkpoints). Codex: the other rollouts of the thread's
/// chain, which with `record` (the head) are the whole conversation: a target given the head alone
/// is refused, and one given the first rollout alone resumes a stale branch. A subagent's rollout
/// is named by its own thread id and stays behind; Grok keeps everything inside the session
/// directory.
pub fn companions(
    agent: AgentId,
    root: &Path,
    record: &Path,
    agent_session_id: &str,
) -> Vec<PathBuf> {
    let files = conversation_of(&mut Locator::default(), agent, root, agent_session_id);
    companions_among(agent, files, record)
}

/// The record among `files`, the conversation's files relative to `root`.
fn head_of(agent: AgentId, root: &Path, files: &[PathBuf]) -> Option<PathBuf> {
    let records_dir = layout(agent).records();
    let records: Vec<PathBuf> = files
        .iter()
        .filter(|relative| records_dir.is_some_and(|dir| relative.starts_with(dir)))
        .cloned()
        .collect();
    provider(agent).records().carried_head(root, &records)
}

/// What of `files`, the conversation's files, travels with `record` besides it.
fn companions_among(agent: AgentId, files: Vec<PathBuf>, record: &Path) -> Vec<PathBuf> {
    provider(agent).records().carried_companions(files, record)
}

/// Every file the agent keeps of the conversation in `root` that a switch may carry, relative to
/// `root`. An id that is not a canonical UUID has none.
fn conversation_of(
    locator: &mut Locator,
    agent: AgentId,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::fixtures::*;

    /// A conversation kept partly outside the records directory is carried whole: the record is
    /// the one under the records directory, and what lies beside it travels as its companion,
    /// another conversation's left where it is.
    #[test]
    fn what_lies_outside_the_records_directory_travels_with_the_record() {
        use crate::adapter::test_provider::TREE_STATE_DIR;
        let _declared = crate::providers::test_provider::with_tree_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-carried-beside");
        let root = dir.join("account");
        test_record(&root, CLAUDE_ID, TREE_TRUNK);
        touch(&root.join(TREE_STATE_DIR).join(format!("{CLAUDE_ID}.db")));
        touch(&root.join(TREE_STATE_DIR).join(format!("{CODEX_ID}.db")));

        let record = locate_record(&mut Locator::default(), AgentId::TEST, &root, CLAUDE_ID)
            .expect("the record");
        assert_eq!(
            record,
            PathBuf::from("sessions").join(format!("{CLAUDE_ID}.jsonl"))
        );
        assert_eq!(
            companions(AgentId::TEST, &root, &record, CLAUDE_ID),
            [PathBuf::from(TREE_STATE_DIR).join(format!("{CLAUDE_ID}.db"))]
        );
    }

    /// An agent that keeps no record files carries no record: with an empty records directory the
    /// conversation's files that sit outside it (what the agent keeps beside its records, and the
    /// stray files the account directory holds) are not the records directory's, and none of them
    /// is taken for the record, though the test-only provider's variant would take any file for
    /// one.
    #[test]
    fn a_layout_with_no_records_directory_carries_no_record() {
        let _declared = crate::providers::test_provider::without_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-carried-none");
        let root = dir.join("account");
        touch(&root.join(format!("{CLAUDE_ID}.jsonl")));
        touch(&root.join(CLAUDE_ID).join("usage.json"));
        touch(&root.join(format!("{CLAUDE_ID}.sidecar")));
        let mut locator = Locator::default();

        // What the agent keeps beside its records is still the conversation's, ...
        assert_eq!(
            conversation_of(&mut locator, AgentId::TEST, &root, CLAUDE_ID),
            [PathBuf::from(format!("{CLAUDE_ID}.sidecar"))]
        );
        // ... but none of it is a record.
        assert_eq!(
            locate_record(&mut locator, AgentId::TEST, &root, CLAUDE_ID),
            None
        );
    }
}
