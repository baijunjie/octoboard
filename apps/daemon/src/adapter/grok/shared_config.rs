//! What Grok Build's accounts may share with the default account's configuration directory, and
//! the plugin registry that records where a plugin was installed. The measurements are in
//! `docs/agent-cli-reference.md`'s "Sharing a configuration directory between accounts" section.

use crate::accounts::shared_config::{config_dir, config_file, history_dir, SharedItem};
use crate::protocol::AgentId;
use crate::providers::grok::{GROK_CONFIG_FILE, GROK_RECORDS_DIR};

// Left out on purpose: `trusted_folders.toml` (Grok replaces a linked one with a plain file on
// save) and `hooks-paths` (Grok refuses to write it through a link).
pub(super) const ITEMS: &[SharedItem] = &[
    config_file(GROK_CONFIG_FILE),
    config_file(AgentId::GROK.descriptor().instruction_file),
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
    history_dir(GROK_RECORDS_DIR),
];

pub(super) const PLUGIN_REGISTRIES: &[(&str, &str)] = &[("installed-plugins", "registry.json")];
