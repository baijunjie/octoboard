//! What Claude Code's accounts may share with the default account's configuration directory, and
//! the plugin registries that record where a plugin was installed. The measurements are in
//! `docs/agent-cli-reference.md`'s "Sharing a configuration directory between accounts" section.

use crate::accounts::shared_config::{
    config_dir, config_file, history_dir, history_lines, SharedItem,
};
use crate::protocol::AgentId;
use crate::providers::claude::{
    CLAUDE_CHECKPOINTS_DIR, CLAUDE_KEY_BINDINGS, CLAUDE_PROMPT_HISTORY, CLAUDE_RECORDS_DIR,
    CLAUDE_SETTINGS_FILE,
};

// Left out on purpose: `plans` (plan mode's writes lose their automatic permission through a
// link) and `sessions` (a registry of running processes, not history).
pub(super) const ITEMS: &[SharedItem] = &[
    config_file(CLAUDE_SETTINGS_FILE),
    config_file(AgentId::CLAUDE.descriptor().instruction_file),
    config_file(CLAUDE_KEY_BINDINGS),
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
    history_dir(CLAUDE_RECORDS_DIR),
    history_dir(CLAUDE_CHECKPOINTS_DIR),
    history_dir("paste-cache"),
    history_dir("image-cache"),
    history_dir("session-env"),
    history_dir("todos"),
    history_dir("tasks"),
    history_lines(CLAUDE_PROMPT_HISTORY),
];

pub(super) const PLUGIN_REGISTRIES: &[(&str, &str)] = &[
    ("plugins", "installed_plugins.json"),
    ("plugins", "known_marketplaces.json"),
];
