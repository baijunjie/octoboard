//! Which paths deleting a session removes: its agent's records in every account that could hold
//! them, held to the one rule that nothing but the session's own records is ever named.

use std::path::{Path, PathBuf};

use crate::adapter::provider;
use crate::hostfs::{lexically_normalise, lexically_within};
use crate::protocol::{Account, AgentId, Session};
use crate::records::locate::{
    account_roots, in_real_directories, own_paths, plausible_id, record_paths, records_dir, Hints,
    Locator,
};

/// [`record_paths`] cut to what deleting `session` may remove: when `session` is on another
/// account than the default one, nothing in the default account's records directory, by its
/// spelling or with links resolved, such as the copy a switch left there; the default directory is
/// the user's own setup. The exception is an account whose directory shares the default
/// configuration with its last change complete: its sessions' records live in the default
/// directory, reached through its links, and are theirs. While the last change of that sharing is
/// incomplete, nothing in the default directory is removed: an orphan left there can still be
/// removed by hand, a conversation removed by mistake cannot.
pub fn removable_record_paths(
    hints: &Hints,
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> Vec<PathBuf> {
    let agent = session.agent;
    let paths = record_paths(hints, locator, accounts, session);
    let beside = removable_beside_records(locator, accounts, session);
    let keeps_default = match session.config_dir.as_deref() {
        None => true,
        Some(dir) => {
            let key = crate::hostfs::dir_key(dir);
            accounts.iter().any(|account| {
                account.agent == agent
                    && crate::hostfs::dir_key(&account.config_dir) == key
                    && account.share_default_config
                    && !account.share_default_config_incomplete
            })
        }
    };
    if keeps_default {
        return paths.into_iter().chain(beside).collect();
    }
    let default_root = locator
        .root(agent, None)
        .map(|root| lexically_normalise(&root));
    let default_records = default_root
        .as_deref()
        .and_then(|root| records_dir(agent, root));
    // Whether `path` lies in the directory `default`, by its spelling or with links resolved.
    let in_default = |path: &Path, default: Option<&Path>| {
        let Some(default) = default else {
            return false;
        };
        lexically_within(path, default)
            || crate::hostfs::canonical(default).is_ok_and(|default| {
                path.parent()
                    .and_then(|parent| crate::hostfs::canonical(parent).ok())
                    .is_some_and(|parent| parent.starts_with(default))
            })
    };
    let records = paths
        .into_iter()
        .filter(|path| !in_default(path, default_records.as_deref()));
    // What the agent keeps beside its records lies anywhere in the account's directory.
    let beside = beside
        .into_iter()
        .filter(|path| !in_default(path, default_root.as_deref()));
    records.chain(beside).collect()
}

/// What the agent keeps of `session`'s conversation outside its records directories, in every
/// account that could hold it, for a provider whose deletion removes it
/// (`RecordStore::removes_beside_records`). Held to the rule of [`record_paths`]: a canonical id,
/// and each path taken only when it lies lexically inside the account's directory, below
/// directories that are real up to that directory, so that a link never leads the removal
/// elsewhere. A path may not exist.
fn removable_beside_records(
    locator: &Locator,
    accounts: &[Account],
    session: &Session,
) -> Vec<PathBuf> {
    let store = provider(session.agent).records();
    let Some(agent_id) = session
        .agent_session_id
        .as_deref()
        .filter(|id| plausible_id(id) && store.removes_beside_records())
    else {
        return Vec::new();
    };
    let (roots, _) = account_roots(locator, accounts, session);
    let mut paths: Vec<PathBuf> = Vec::new();
    for root in &roots {
        for path in store.beside_records(root, agent_id) {
            let path = lexically_normalise(&path);
            if path != *root
                && lexically_within(&path, root)
                && in_real_directories(&path, root)
                && !paths.contains(&path)
            {
                paths.push(path);
            }
        }
    }
    paths
}

/// Every file the agent keeps of conversation `agent_id` in the account directory `root`, found
/// as [`record_paths`] finds a record (a canonical id, the agent's exact layout depth, no linked
/// directory below the records directory), with what the agent keeps beside it
/// (`RecordStore::beside_records`), such as Claude Code's rewind checkpoints (`file-history/<id>`)
/// and the rollouts `codex archive` moved into `archived_sessions/`. Each of those directories is
/// looked in where it leads ([`Resolved`]), as a records directory that is itself a link (to a
/// dotfiles copy, say) is still the account's, and every path answered is spelled through `root`.
/// Turning an account's sharing off moves these back into the account; `locator` lists each
/// directory once for all the conversations it moves.
pub fn conversation_files_in(
    locator: &mut Locator,
    agent: AgentId,
    root: &Path,
    agent_id: &str,
) -> Vec<PathBuf> {
    if !plausible_id(agent_id) {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = Vec::new();
    if let Some(records) = records_dir(agent, root).and_then(Resolved::of) {
        let found = locator
            .index_at(agent, records.real())
            .records
            .get(agent_id)
            .cloned()
            .unwrap_or_default();
        for record in found {
            if !in_real_directories(&record, records.real()) {
                continue;
            }
            for path in own_paths(agent, agent_id, &record, records.real()) {
                let Some(path) = records.spell(&path) else {
                    continue;
                };
                if !files.contains(&path) && std::fs::symlink_metadata(&path).is_ok() {
                    files.push(path);
                }
            }
        }
    }
    files.extend(provider(agent).records().beside_records(root, agent_id));
    files
}

/// A directory as an account spells it, and where it leads, for looking in it where it leads and
/// answering what is found there through the account's spelling.
pub(crate) struct Resolved {
    spelled: PathBuf,
    real: PathBuf,
}

impl Resolved {
    /// `dir` and where it leads; `None` when it cannot be resolved, as when it does not exist.
    pub(crate) fn of(dir: PathBuf) -> Option<Self> {
        let real = crate::hostfs::canonical(&dir).ok()?;
        Some(Self { spelled: dir, real })
    }

    /// Where the directory leads.
    pub(crate) fn real(&self) -> &Path {
        &self.real
    }

    /// `path`, found under where the directory leads, spelled through the directory instead.
    pub(crate) fn spell(&self, path: &Path) -> Option<PathBuf> {
        path.strip_prefix(&self.real)
            .ok()
            .map(|rest| self.spelled.join(rest))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::records::fixtures::*;

    /// What deleting a session removes: for a session on another account, nothing in the default
    /// account's directory, where a switch left a copy, unless that account shares the default
    /// configuration with nothing pending; for one on the default account, that copy; and never
    /// the default's record reached under another spelling.
    #[test]
    fn default_records_are_removed_only_for_the_default_or_a_sharing_account() {
        let dir = crate::test_support::ScratchDir::new("agent-records-removable");
        let (account, default) = (dir.join("account"), dir.join("default"));
        let transcript = |root: &Path| root.join("projects/one").join(format!("{CLAUDE_ID}.jsonl"));
        touch(&transcript(&account));
        touch(&transcript(&default));
        let mut locator = Locator::with_shell(HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_owned(),
            default.to_string_lossy().into_owned(),
        )]));
        let hints = Hints::default();
        let removable = |locator: &mut Locator, accounts: &[Account], on: Option<&Path>| {
            removable_record_paths(
                &hints,
                locator,
                accounts,
                &session(AgentId::CLAUDE, CLAUDE_ID, on),
            )
        };
        let on_account = removable(&mut locator, &[], Some(&account));
        assert!(on_account.contains(&transcript(&account)), "{on_account:?}");
        assert!(
            on_account.iter().all(|path| !path.starts_with(&default)),
            "{on_account:?}"
        );
        let on_default = removable(&mut locator, &[], None);
        assert!(on_default.contains(&transcript(&default)), "{on_default:?}");

        let mut sharing = Account {
            id: "a".to_owned(),
            agent: AgentId::CLAUDE,
            name: "Sharing".to_owned(),
            config_dir: account.to_string_lossy().into_owned(),
            share_default_config: true,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        };
        let on_sharing = removable(&mut locator, std::slice::from_ref(&sharing), Some(&account));
        assert!(on_sharing.contains(&transcript(&default)), "{on_sharing:?}");
        sharing.share_default_config_incomplete = true;
        let on_incomplete = removable(&mut locator, std::slice::from_ref(&sharing), Some(&account));
        assert!(
            on_incomplete.iter().all(|path| !path.starts_with(&default)),
            "{on_incomplete:?}"
        );

        // An account whose directory is a link to the default one reaches the default's record by
        // its own spelling; that is not its own to remove.
        let alias = dir.join("alias");
        crate::test_support::symlink(&default, &alias).unwrap();
        let through_alias = removable(&mut locator, &[], Some(&alias));
        assert!(through_alias.is_empty(), "{through_alias:?}");
    }

    /// For a provider that keeps part of a conversation outside the records directory and removes
    /// it with the session (the test-only provider's tree-shaped variant), deleting a session
    /// names that part too: in the session's own account and every other account that could hold
    /// a copy, never in the default account for a session on another, and never through a link.
    #[test]
    fn what_a_provider_keeps_beside_its_records_is_removed_only_where_it_says_so() {
        use crate::adapter::test_provider::TREE_STATE_DIR;
        let dir = crate::test_support::ScratchDir::new("agent-records-removable-beside");
        let (account, home) = (dir.join("account"), dir.join("home"));
        let default = home.join(crate::providers::layout::layout(AgentId::TEST).default_dir);
        let state = |root: &Path| root.join(TREE_STATE_DIR).join(format!("{CLAUDE_ID}.db"));
        let record = |root: &Path| root.join("sessions").join(format!("{CLAUDE_ID}.jsonl"));
        for root in [&account, &default] {
            touch(&record(root));
            touch(&state(root));
            touch(&root.join(TREE_STATE_DIR).join(format!("{CODEX_ID}.db")));
        }
        let shell = || HashMap::from([("HOME".to_owned(), home.to_string_lossy().into_owned())]);
        let removable = |on: Option<&Path>| {
            let mut paths = removable_record_paths(
                &Hints::default(),
                &mut Locator::with_shell(shell()),
                &[],
                &session(AgentId::TEST, CLAUDE_ID, on),
            );
            paths.sort();
            paths
        };

        // The plain provider says nothing is removed beside its records.
        assert_eq!(removable(Some(&account)), [record(&account)]);
        assert_eq!(removable(None), [record(&default)]);

        let _declared = crate::providers::test_provider::with_tree_records();
        // On its own account: the record and the state beside it, nothing of the default account.
        assert_eq!(
            removable(Some(&account)),
            [record(&account), state(&account)]
        );
        // On the default account: its own, and the copy a switch left in no other known account.
        assert_eq!(removable(None), [record(&default), state(&default)]);
        // An id that is no canonical UUID names nothing.
        assert!(removable_record_paths(
            &Hints::default(),
            &mut Locator::with_shell(shell()),
            &[],
            &session(AgentId::TEST, "..", Some(&account)),
        )
        .is_empty());

        // A state directory that is a link is not followed: what it leads to is not removed.
        let linked = dir.join("linked");
        touch(&record(&linked));
        std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
        touch(&dir.join("elsewhere").join(format!("{CLAUDE_ID}.db")));
        crate::test_support::symlink(dir.join("elsewhere"), linked.join(TREE_STATE_DIR)).unwrap();
        assert_eq!(removable(Some(&linked)), [record(&linked)]);

        // An account directory that is a link to the default one reaches nothing of the default's.
        let alias = dir.join("alias");
        crate::test_support::symlink(&default, &alias).unwrap();
        assert!(removable(Some(&alias)).is_empty());
    }

    /// What Claude Code keeps beside its transcripts (the rewind checkpoints) is not named for
    /// removal: its store does not say a deletion removes it.
    #[test]
    fn a_released_providers_files_beside_its_records_are_left() {
        let dir = crate::test_support::ScratchDir::new("agent-records-removable-claude");
        let account = dir.join("account");
        let transcript = account
            .join("projects/one")
            .join(format!("{CLAUDE_ID}.jsonl"));
        touch(&transcript);
        touch(&account.join("file-history").join(CLAUDE_ID).join("1"));
        let shell = HashMap::from([(
            "HOME".to_owned(),
            dir.join("home").to_string_lossy().into_owned(),
        )]);
        let paths = removable_record_paths(
            &Hints::default(),
            &mut Locator::with_shell(shell),
            &[],
            &session(AgentId::CLAUDE, CLAUDE_ID, Some(&account)),
        );
        assert_eq!(
            paths,
            [transcript, account.join("projects/one").join(CLAUDE_ID)]
        );
    }

    /// Every file of one conversation in an account: Claude Code's transcript, the subagent
    /// directory beside it and its rewind checkpoints; Codex's rollout under `sessions/` and the
    /// one `codex archive` moved under `archived_sessions/`; Grok Build's session directory.
    #[test]
    fn conversation_files_are_each_agents_records_and_what_it_keeps_beside_them() {
        let dir = crate::test_support::ScratchDir::new("agent-records-conversation-files");
        let root = dir.join("account");
        let mut locator = Locator::with_shell(HashMap::new());

        let transcript = root.join("projects/one").join(format!("{CLAUDE_ID}.jsonl"));
        touch(&transcript);
        touch(
            &root
                .join("projects/one")
                .join(CLAUDE_ID)
                .join("subagents/a.jsonl"),
        );
        touch(&root.join("file-history").join(CLAUDE_ID).join("1"));
        touch(&root.join("history.jsonl"));
        let mut claude = conversation_files_in(&mut locator, AgentId::CLAUDE, &root, CLAUDE_ID);
        claude.sort();
        assert_eq!(
            claude,
            [
                root.join("file-history").join(CLAUDE_ID),
                root.join("projects/one").join(CLAUDE_ID),
                transcript,
            ]
        );

        let rollout = root
            .join("sessions/2026/10/16")
            .join(format!("rollout-2026-10-16T00-00-00-{CODEX_ID}.jsonl"));
        let archived = root
            .join("archived_sessions")
            .join(format!("rollout-2026-10-15T00-00-00-{CODEX_ID}.jsonl.zst"));
        touch(&rollout);
        touch(&archived);
        assert_eq!(
            conversation_files_in(&mut locator, AgentId::CODEX, &root, CODEX_ID),
            [rollout, archived]
        );

        let grok = root.join("sessions/%2Fwork").join(CLAUDE_ID);
        touch(&grok.join("usage.json"));
        touch(&grok.join("chat_history.jsonl"));
        assert_eq!(
            conversation_files_in(&mut locator, AgentId::GROK, &root, CLAUDE_ID),
            [grok]
        );
    }
}
