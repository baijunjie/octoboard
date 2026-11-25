//! Records that are rows: a provider whose agent keeps its conversations in a database of its
//! own, with no file per conversation to find by name, read as it grows or move to the Trash.
//! Such a provider's store hands out a [`RowRecords`] (`RecordStore::rows`), which lists, reads
//! and removes conversations itself, by the agent's own means; Octoboard never writes the agent's
//! database.
//!
//! A conversation's rows come back as lines, in order, and are read by the same per-line
//! answers a record file's lines are (`RecordStore::entries`, `fold_line`, `branching`, the feed
//! reader), so the conversation, the history, the context size and the live feed are one reading
//! whichever shape the record has. What differs is here: a conversation is looked for in the
//! session's own account alone, by an id of the shape the provider says its agent gives
//! ([`RowRecords::names_conversation`]), and is read whole each time, when an event of the session
//! says it changed (`Hints::stirred`), never on a timer.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

use serde_json::Value;

use crate::adapter::provider;
use crate::protocol::{AgentId, Session};
use crate::records::locate::Locator;

/// The account a row store is asked about: its pinned directory, `None` for the default account,
/// and the environment its sessions launch in.
#[cfg_attr(not(test), allow(dead_code))]
pub struct RowAccount<'a> {
    pub config_dir: Option<&'a Path>,
    pub shell_env: &'a HashMap<String, String>,
}

/// How a provider whose records are rows lists, reads and removes them (`RecordStore::rows`).
/// Every call may run the agent's own command, so it is blocking and bounded by the provider.
/// Each default is the answer of a store that can tell nothing and removes nothing.
pub trait RowRecords: Send + Sync {
    /// Whether `id` has the exact shape of the ids the agent gives its conversations. Only such
    /// an id is handed to the other methods: it comes from a hook payload nothing authenticates,
    /// and a provider may put it on a command line.
    fn names_conversation(&self, _id: &str) -> bool {
        false
    }

    /// The id of every conversation the account holds, or `None` when that cannot be told (the
    /// database is not there or cannot be read, the agent's command failed). A conversation
    /// missing from a listing is not there.
    fn conversations(&self, _account: &RowAccount<'_>) -> Option<HashSet<String>> {
        None
    }

    /// The rows of conversation `agent_id` as it stands, in order, each as one line the store's
    /// per-line answers read. `ErrorKind::NotFound` for a conversation the account does not hold.
    fn lines(&self, _account: &RowAccount<'_>, _agent_id: &str) -> io::Result<Vec<Value>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this agent's records cannot be read",
        ))
    }

    /// Removes conversation `agent_id` from the account by the agent's own means, for a session
    /// being deleted, and says whether it did: `Ok(false)` for an agent that has no such means.
    /// What it removes is gone, not in the Trash.
    fn remove(&self, _account: &RowAccount<'_>, _agent_id: &str) -> anyhow::Result<bool> {
        Ok(false)
    }
}

/// The row store of `agent`'s provider, when its records are rows.
pub(crate) fn shaped(agent: AgentId) -> Option<&'static dyn RowRecords> {
    provider(agent).records().rows()
}

/// The row store `session`'s conversation is kept in, with the conversation's id, when the
/// session names one of the shape its agent gives.
pub(crate) fn of(session: &Session) -> Option<(&'static dyn RowRecords, &str)> {
    let rows = shaped(session.agent)?;
    let agent_id = session
        .agent_session_id
        .as_deref()
        .filter(|id| rows.names_conversation(id))?;
    Some((rows, agent_id))
}

/// The lines of `session`'s conversation, read from its own account: `None` for a session whose
/// provider's records are not rows. A session that names no conversation, or an account whose
/// environment is unknown, is an error like a conversation that cannot be read.
pub(crate) fn read(locator: &Locator, session: &Session) -> Option<io::Result<Vec<Value>>> {
    shaped(session.agent)?;
    let Some((rows, agent_id)) = of(session) else {
        return Some(Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the session names no conversation of its agent's",
        )));
    };
    let Some(shell_env) = locator.shell_env() else {
        return Some(Err(io::Error::other(
            "the environment the account launches in is unknown",
        )));
    };
    let account = RowAccount {
        config_dir: session.config_dir.as_deref().map(Path::new),
        shell_env,
    };
    Some(rows.lines(&account, agent_id))
}

/// Removes `session`'s conversation from its own account (`RowRecords::remove`), for a session
/// that was deleted: `None` for a session whose provider's records are not rows, `Ok(false)` when
/// nothing was removed (the session names no conversation, the account does not hold it or
/// cannot be asked, the agent has no means). Only a conversation the account's listing shows is
/// asked to be removed.
pub(crate) fn remove(locator: &Locator, session: &Session) -> Option<anyhow::Result<bool>> {
    shaped(session.agent)?;
    let Some((rows, agent_id)) = of(session) else {
        return Some(Ok(false));
    };
    if locator.holds_conversation(session) != Some(true) {
        return Some(Ok(false));
    }
    let Some(shell_env) = locator.shell_env() else {
        return Some(Ok(false));
    };
    let account = RowAccount {
        config_dir: session.config_dir.as_deref().map(Path::new),
        shell_env,
    };
    Some(rows.remove(&account, agent_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::test_provider::row_reads;
    use crate::records::fixtures::{row_database, session, ROW_ID, ROW_TURNS};

    fn shell() -> HashMap<String, String> {
        HashMap::new()
    }

    /// A provider that hands out no row store is not row-shaped, whatever its session names:
    /// nothing is read, removed or listed for it here.
    #[test]
    fn a_provider_whose_records_are_files_is_not_asked() {
        let dir = crate::test_support::ScratchDir::new("agent-records-rows-files");
        let own = session(AgentId::TEST, ROW_ID, Some(&dir.join("account")));
        let locator = Locator::with_shell(shell());
        for agent in AgentId::ALL {
            let mut other = own.clone();
            other.agent = agent;
            if provider(agent).records().rows().is_none() {
                assert!(read(&locator, &other).is_none());
                assert!(remove(&locator, &other).is_none());
                assert_eq!(locator.holds_conversation(&other), None);
            }
        }
    }

    /// The rows of a conversation are read from the session's own account, by an id of the
    /// agent's shape alone: an id of another shape never reaches the provider.
    #[test]
    fn a_conversation_is_read_by_an_id_of_the_agents_shape() {
        let _declared = crate::providers::test_provider::with_row_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-rows-read");
        let account = dir.join("account");
        row_database(&account, &[(ROW_ID, ROW_TURNS)]);
        let locator = Locator::with_shell(shell());

        let own = session(AgentId::TEST, ROW_ID, Some(&account));
        let lines = read(&locator, &own).expect("row-shaped").unwrap();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0]["user"], "first");

        let before = row_reads();
        for id in [
            crate::records::fixtures::CLAUDE_ID,
            "row_../other",
            "--help",
        ] {
            let stranger = session(AgentId::TEST, id, Some(&account));
            let refused = read(&locator, &stranger).expect("row-shaped").unwrap_err();
            assert_eq!(refused.kind(), io::ErrorKind::NotFound, "{id}");
            assert!(!remove(&locator, &stranger).unwrap().unwrap(), "{id}");
        }
        assert_eq!(row_reads(), before, "the provider was not asked");

        let gone = session(AgentId::TEST, "row_gone", Some(&account));
        let missing = read(&locator, &gone).expect("row-shaped").unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    }

    /// Deleting a session removes its conversation by the provider's own means, from its own
    /// account, and leaves every other conversation; one the account does not hold is not asked
    /// for.
    #[test]
    fn a_conversation_is_removed_through_the_provider() {
        let _declared = crate::providers::test_provider::with_row_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-rows-remove");
        let account = dir.join("account");
        row_database(&account, &[(ROW_ID, ROW_TURNS), ("row_other", ROW_TURNS)]);
        let own = session(AgentId::TEST, ROW_ID, Some(&account));

        let locator = Locator::with_shell(shell());
        assert!(remove(&locator, &own).unwrap().unwrap());

        let after = Locator::with_shell(shell());
        assert_eq!(after.holds_conversation(&own), Some(false));
        let other = session(AgentId::TEST, "row_other", Some(&account));
        assert_eq!(after.holds_conversation(&other), Some(true));
        assert!(!remove(&after, &own).unwrap().unwrap());
    }
}
