//! The entries of an agent's configuration directory that an account may share with the default
//! account's, each marked as a file or a directory and as configuration or conversation history
//! (the two are brought under the shared directory differently). Anything not listed here is
//! never shared: credentials, the agent's own caches and process registries, and files the agent
//! replaces rather than writes through when a link stands in for them. The measurements are in
//! `docs/agent-cli-reference.md`'s "Sharing a configuration directory between accounts" section.

use crate::protocol::Agent;

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
/// history, a thread index, Codex's state database — cannot simply be moved over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemClass {
    Configuration,
    ConversationHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedItem {
    /// The entry's name directly inside the configuration directory.
    pub name: &'static str,
    pub kind: ItemKind,
    pub class: ItemClass,
}

const fn config_file(name: &'static str) -> SharedItem {
    SharedItem {
        name,
        kind: ItemKind::File,
        class: ItemClass::Configuration,
    }
}

const fn config_dir(name: &'static str) -> SharedItem {
    SharedItem {
        name,
        kind: ItemKind::Directory,
        class: ItemClass::Configuration,
    }
}

const fn history_file(name: &'static str) -> SharedItem {
    SharedItem {
        name,
        kind: ItemKind::File,
        class: ItemClass::ConversationHistory,
    }
}

const fn history_dir(name: &'static str) -> SharedItem {
    SharedItem {
        name,
        kind: ItemKind::Directory,
        class: ItemClass::ConversationHistory,
    }
}

// Left out on purpose: `plans` (plan mode's writes lose their automatic permission through a
// link) and `sessions` (a registry of running processes, not history).
const CLAUDE: &[SharedItem] = &[
    config_file("settings.json"),
    config_file("CLAUDE.md"),
    config_file("keybindings.json"),
    config_file("loop.md"),
    config_dir("agents"),
    config_dir("commands"),
    config_dir("skills"),
    config_dir("output-styles"),
    config_dir("rules"),
    config_dir("plugins"),
    config_dir("workflows"),
    config_dir("routines"),
    config_dir("themes"),
    config_dir("hooks"),
    config_dir("agent-memory"),
    history_dir("projects"),
    history_dir("file-history"),
    history_dir("paste-cache"),
    history_dir("image-cache"),
    history_dir("session-env"),
    history_dir("todos"),
    history_dir("tasks"),
    history_file("history.jsonl"),
];

// The thread state database is a file item: SQLite opened through a link keeps its `-wal` and
// `-shm` beside the target, so those are not items. Codex versions it by name (0.162.1 uses
// `state_5.sqlite`), and a new schema generation is a new file beside the old one.
const CODEX: &[SharedItem] = &[
    config_file("config.toml"),
    config_file("AGENTS.md"),
    config_file("AGENTS.override.md"),
    config_file("hooks.json"),
    config_dir("rules"),
    config_dir("skills"),
    config_dir("prompts"),
    config_dir("plugins"),
    config_dir("themes"),
    config_dir("agents"),
    history_dir("sessions"),
    history_dir("archived_sessions"),
    history_file("session_index.jsonl"),
    history_file("history.jsonl"),
    history_file("state_5.sqlite"),
];

// Left out on purpose: `trusted_folders.toml` (Grok replaces a linked one with a plain file on
// save) and `hooks-paths` (Grok refuses to write it through a link).
const GROK: &[SharedItem] = &[
    config_file("config.toml"),
    config_file("AGENTS.md"),
    config_file("pager.toml"),
    config_file("lsp.json"),
    config_file("settings.json"),
    config_file("sandbox.toml"),
    config_file("statusline.sh"),
    config_file("disabled-hooks"),
    config_dir("rules"),
    config_dir("skills"),
    config_dir("commands"),
    config_dir("agents"),
    config_dir("personas"),
    config_dir("workflows"),
    config_dir("plugins"),
    config_dir("hooks"),
    config_dir("installed-plugins"),
    history_dir("sessions"),
];

/// The items `agent` shares between accounts.
pub fn shared_items(agent: Agent) -> &'static [SharedItem] {
    match agent {
        Agent::Claude => CLAUDE,
        Agent::Codex => CODEX,
        Agent::Grok => GROK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ItemClass::{Configuration as C, ConversationHistory as H};
    use ItemKind::{Directory as D, File as F};

    fn listed(agent: Agent) -> Vec<(&'static str, ItemKind, ItemClass)> {
        shared_items(agent)
            .iter()
            .map(|item| (item.name, item.kind, item.class))
            .collect()
    }

    fn assert_absent(agent: Agent, never_shared: &[&str]) {
        for name in never_shared {
            assert!(
                shared_items(agent).iter().all(|item| item.name != *name),
                "{agent:?} must not share {name}"
            );
        }
    }

    #[test]
    fn claude_catalog() {
        assert_eq!(
            listed(Agent::Claude),
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
                ("history.jsonl", F, H),
            ]
        );
    }

    #[test]
    fn codex_catalog() {
        assert_eq!(
            listed(Agent::Codex),
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
                ("session_index.jsonl", F, H),
                ("history.jsonl", F, H),
                ("state_5.sqlite", F, H),
            ]
        );
    }

    #[test]
    fn grok_catalog() {
        assert_eq!(
            listed(Agent::Grok),
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
            Agent::Claude,
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
            Agent::Codex,
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
            Agent::Grok,
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
