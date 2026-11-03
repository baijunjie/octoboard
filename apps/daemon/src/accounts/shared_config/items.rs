//! The entries of an agent's configuration directory that an account may share with the default
//! account's, each marked as a file or a directory and as configuration or one of the kinds of
//! conversation history (they are brought under the shared directory differently). Each provider
//! lists its own (`AccountOps::shared_items`); anything not listed is never shared: credentials,
//! the agent's own caches and process registries, and files the agent replaces rather than writes
//! through when a link stands in for them. The measurements are in `docs/agent-cli-reference.md`'s
//! "Sharing a configuration directory between accounts" section.

use crate::adapter::provider;
use crate::protocol::AgentId;

/// Whether an item is a single file or a directory. A missing directory item is created in the
/// default directory; a missing file item is skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    File,
    Directory,
}

/// What an item holds: configuration the account already has is backed up and then linked, while
/// conversation history is carried into the default directory before it is linked, so earlier
/// conversations stay resumable. A history file the default directory already holds — a prompt
/// history, a thread index, a thread database — cannot simply be moved over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemClass {
    Configuration,
    /// Conversation history carried into the default directory before it is linked.
    ConversationHistory,
    /// A history of one line per entry (prompt histories, a thread-name index), merged into the
    /// default directory's copy rather than set aside, so the recall and the names of the
    /// account's earlier conversations survive. A name that is lost can make a provider start a
    /// new session where a resume by that name was asked for.
    LineHistory,
    /// The provider's thread database, whose rows name where records are
    /// ([`ThreadRegistry`](crate::accounts::thread_registry::ThreadRegistry)).
    ThreadDatabase,
}

impl ItemClass {
    /// Whether the item holds conversation history of any kind, which is carried into the default
    /// directory rather than backed up.
    pub fn is_history(self) -> bool {
        self != Self::Configuration
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedItem {
    /// The entry's name directly inside the configuration directory.
    pub name: &'static str,
    pub kind: ItemKind,
    pub class: ItemClass,
}

pub(crate) const fn config_file(name: &'static str) -> SharedItem {
    SharedItem {
        name,
        kind: ItemKind::File,
        class: ItemClass::Configuration,
    }
}

pub(crate) const fn config_dir(name: &'static str) -> SharedItem {
    SharedItem {
        name,
        kind: ItemKind::Directory,
        class: ItemClass::Configuration,
    }
}

pub(crate) const fn history_dir(name: &'static str) -> SharedItem {
    SharedItem {
        name,
        kind: ItemKind::Directory,
        class: ItemClass::ConversationHistory,
    }
}

pub(crate) const fn history_lines(name: &'static str) -> SharedItem {
    SharedItem {
        name,
        kind: ItemKind::File,
        class: ItemClass::LineHistory,
    }
}

pub(crate) const fn thread_database(name: &'static str) -> SharedItem {
    SharedItem {
        name,
        kind: ItemKind::File,
        class: ItemClass::ThreadDatabase,
    }
}

/// The items `agent` shares between accounts.
pub fn shared_items(agent: AgentId) -> &'static [SharedItem] {
    provider(agent)
        .accounts()
        .map_or(&[], |ops| ops.shared_items())
}

#[cfg(test)]
mod tests {
    use super::*;

    use ItemClass::{
        Configuration as C, ConversationHistory as H, LineHistory as L, ThreadDatabase as T,
    };
    use ItemKind::{Directory as D, File as F};

    fn listed(agent: AgentId) -> Vec<(&'static str, ItemKind, ItemClass)> {
        shared_items(agent)
            .iter()
            .map(|item| (item.name, item.kind, item.class))
            .collect()
    }

    fn assert_absent(agent: AgentId, never_shared: &[&str]) {
        for name in never_shared {
            assert!(
                shared_items(agent).iter().all(|item| item.name != *name),
                "{agent:?} must not share {name}"
            );
        }
    }

    /// A thread database is catalogued only where a registry keeps its rows in step, and a
    /// provider with no account operations shares nothing and keeps no registry.
    #[test]
    fn a_thread_database_is_catalogued_only_with_a_registry_for_its_rows() {
        for agent in AgentId::ALL {
            let has_database = shared_items(agent)
                .iter()
                .any(|item| item.class == ItemClass::ThreadDatabase);
            if has_database {
                assert!(
                    crate::accounts::thread_registry::thread_registry(agent).is_some(),
                    "{agent:?}"
                );
            }
        }
        assert!(provider(AgentId::TEST).accounts().is_none());
        assert!(shared_items(AgentId::TEST).is_empty());
        assert!(crate::accounts::thread_registry::thread_registry(AgentId::TEST).is_none());
        assert!(super::super::registries::registries(AgentId::TEST).is_empty());
    }

    #[test]
    fn claude_catalog() {
        assert_eq!(
            listed(AgentId::CLAUDE),
            vec![
                ("settings.json", F, C),
                ("CLAUDE.md", F, C),
                ("keybindings.json", F, C),
                ("loop.md", F, C),
                ("agents", D, C),
                ("commands", D, C),
                ("skills", D, C),
                ("output-styles", D, C),
                ("rules", D, C),
                ("plugins", D, C),
                ("workflows", D, C),
                ("routines", D, C),
                ("themes", D, C),
                ("hooks", D, C),
                ("agent-memory", D, C),
                ("projects", D, H),
                ("file-history", D, H),
                ("paste-cache", D, H),
                ("image-cache", D, H),
                ("session-env", D, H),
                ("todos", D, H),
                ("tasks", D, H),
                ("history.jsonl", F, L),
            ]
        );
    }

    #[test]
    fn codex_catalog() {
        assert_eq!(
            listed(AgentId::CODEX),
            vec![
                ("config.toml", F, C),
                ("AGENTS.md", F, C),
                ("AGENTS.override.md", F, C),
                ("hooks.json", F, C),
                ("rules", D, C),
                ("skills", D, C),
                ("prompts", D, C),
                ("plugins", D, C),
                ("themes", D, C),
                ("agents", D, C),
                ("sessions", D, H),
                ("archived_sessions", D, H),
                ("session_index.jsonl", F, L),
                ("history.jsonl", F, L),
                ("state_5.sqlite", F, T),
            ]
        );
    }

    #[test]
    fn grok_catalog() {
        assert_eq!(
            listed(AgentId::GROK),
            vec![
                ("config.toml", F, C),
                ("AGENTS.md", F, C),
                ("pager.toml", F, C),
                ("lsp.json", F, C),
                ("settings.json", F, C),
                ("sandbox.toml", F, C),
                ("statusline.sh", F, C),
                ("disabled-hooks", F, C),
                ("rules", D, C),
                ("skills", D, C),
                ("commands", D, C),
                ("agents", D, C),
                ("personas", D, C),
                ("workflows", D, C),
                ("plugins", D, C),
                ("hooks", D, C),
                ("installed-plugins", D, C),
                ("sessions", D, H),
            ]
        );
    }

    #[test]
    fn claude_never_shares_credentials_or_per_machine_state() {
        assert_absent(
            AgentId::CLAUDE,
            &[
                ".credentials.json",
                "hfi-auth.json",
                ".session_ingress_token",
                "mcp-needs-auth-cache.json",
                ".claude.json",
                ".claude.json.backup",
                "backups",
                "plans",
                "sessions",
                "remote-settings.json",
                "policy-limits.json",
                ".last-cleanup",
                "shell-snapshots",
                "settings.local.json",
                "statsig",
                "cache",
                "debug",
                "ide",
                "telemetry",
            ],
        );
    }

    #[test]
    fn codex_never_shares_credentials_or_per_machine_state() {
        assert_absent(
            AgentId::CODEX,
            &[
                "auth.json",
                ".credentials.json",
                "models_cache.json",
                "cache",
                "installation_id",
                "version.json",
                "thread_history_1.sqlite",
                "logs_2.sqlite",
                "packages",
                "app-server-daemon",
                "app-server-control",
                "thread-writer-locks",
                ".tmp",
                "tmp",
                "shell_snapshots",
                "log",
                "state_5.sqlite-wal",
                "state_5.sqlite-shm",
            ],
        );
    }

    #[test]
    fn grok_never_shares_credentials_or_per_machine_state() {
        assert_absent(
            AgentId::GROK,
            &[
                "auth.json",
                "mcp_credentials.json",
                "trusted_folders.toml",
                "hooks-paths",
                "active_sessions.json",
                "agent_id",
                "models_cache.json",
                "settings_cache.json",
                "bin",
                "docs",
                "bundled",
                "bundles",
                "logs",
                "memory-v2",
                "managed_config.toml",
                "requirements.toml",
                "version.json",
            ],
        );
    }
}
