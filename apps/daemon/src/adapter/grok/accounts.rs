//! What Grok Build's accounts can be asked: its quota and the email it signs in with, the `email`
//! of the entries of `auth.json` under `GROK_HOME`. Entries signed in as different people leave no
//! way to tell which one the CLI uses, so there is no email then.

use std::collections::BTreeSet;

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::login::GrokLogin;
use super::quota::{kick_command, GrokQuota};
use super::shared_config::{ITEMS, PLUGIN_REGISTRIES};
use crate::accounts::login::flows::LoginFlow;
use crate::accounts::quotas::email::read_json;
use crate::accounts::quotas::email::tidy;
use crate::accounts::quotas::kick::KickCommand;
use crate::accounts::quotas::probe::QuotaProbe;
use crate::accounts::shared_config::SharedItem;
use crate::accounts::AccountOps;
use crate::adapter::account_dir;
use crate::protocol::AgentId;
use crate::providers::grok::GROK_SIGN_IN_FILE;

pub(crate) struct GrokAccounts;

impl AccountOps for GrokAccounts {
    fn quota_probe(&self) -> Option<&'static dyn QuotaProbe> {
        Some(&GrokQuota)
    }

    fn kick_command(&self) -> Option<KickCommand> {
        Some(kick_command())
    }

    fn login(&self) -> Option<&'static dyn LoginFlow> {
        Some(&GrokLogin)
    }

    fn shared_items(&self) -> &'static [SharedItem] {
        ITEMS
    }

    fn plugin_registries(&self) -> &'static [(&'static str, &'static str)] {
        PLUGIN_REGISTRIES
    }

    fn email(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> Option<String> {
        grok_email(&read_json(
            &account_dir(AgentId::GROK, config_dir, shell_env).join(GROK_SIGN_IN_FILE),
        )?)
    }
}

pub(crate) fn grok_email(file: &Value) -> Option<String> {
    let emails: BTreeSet<String> = file
        .as_object()?
        .values()
        .filter_map(|entry| tidy(entry.get("email")?.as_str()?))
        .collect();
    let mut emails = emails.into_iter();
    let only = emails.next()?;
    emails.next().is_none().then_some(only)
}
