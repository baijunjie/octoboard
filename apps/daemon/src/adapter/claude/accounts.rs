//! What Claude Code's accounts can be asked: its quota and the email it signs in with, the
//! `oauthAccount.emailAddress` of `.claude.json`. With `CLAUDE_CONFIG_DIR` set the file is in that
//! directory; without it, it is `~/.claude.json`, beside the default directory and not inside it.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::login::ClaudeLogin;
use super::quota::{kick_command, ClaudeQuota};
use super::settings::global_config_file;
use super::shared_config::{ITEMS, PLUGIN_REGISTRIES};
use crate::accounts::login::flows::LoginFlow;
use crate::accounts::quotas::email::read_json;
use crate::accounts::quotas::email::tidy;
use crate::accounts::quotas::kick::KickCommand;
use crate::accounts::quotas::probe::QuotaProbe;
use crate::accounts::shared_config::SharedItem;
use crate::accounts::AccountOps;
use crate::providers::claude::CLAUDE_RECORDS_DIR;

pub(crate) struct ClaudeAccounts;

impl AccountOps for ClaudeAccounts {
    fn quota_probe(&self) -> Option<&'static dyn QuotaProbe> {
        Some(&ClaudeQuota)
    }

    fn kick_command(&self) -> Option<KickCommand> {
        Some(kick_command())
    }

    fn login(&self) -> Option<&'static dyn LoginFlow> {
        Some(&ClaudeLogin)
    }

    fn shared_items(&self) -> &'static [SharedItem] {
        ITEMS
    }

    fn plugin_registries(&self) -> &'static [(&'static str, &'static str)] {
        PLUGIN_REGISTRIES
    }

    /// Where Claude Code keeps auto memory, which the shared directory is given for.
    fn shared_memory_item(&self) -> Option<&'static str> {
        Some(CLAUDE_RECORDS_DIR)
    }

    fn email(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> Option<String> {
        claude_email(&read_json(&global_config_file(shell_env, config_dir))?)
    }
}

pub(crate) fn claude_email(file: &Value) -> Option<String> {
    tidy(file.pointer("/oauthAccount/emailAddress")?.as_str()?)
}
