//! What Codex's accounts can be asked: its quota and the email it signs in with, the `email` claim
//! of the ID token in `auth.json` under `CODEX_HOME`. The token's signature is not checked: the
//! file is the user's own and the claim only labels the account.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::login::CodexLogin;
use super::quota::{kick_command, CodexQuota};
use super::reset_credit::CodexResetCredits;
use super::shared_config::ITEMS;
use super::threads::CodexThreads;
use crate::accounts::login::flows::LoginFlow;
use crate::accounts::quotas::email::read_json;
use crate::accounts::quotas::email::tidy;
use crate::accounts::quotas::kick::KickCommand;
use crate::accounts::quotas::probe::QuotaProbe;
use crate::accounts::quotas::reset_credit::ResetCredits;
use crate::accounts::shared_config::SharedItem;
use crate::accounts::thread_registry::ThreadRegistry;
use crate::accounts::AccountOps;
use crate::adapter::account_dir;
use crate::protocol::AgentId;
use crate::providers::codex::CODEX_SIGN_IN_FILE;

pub(crate) struct CodexAccounts;

impl AccountOps for CodexAccounts {
    fn quota_probe(&self) -> Option<&'static dyn QuotaProbe> {
        Some(&CodexQuota)
    }

    fn kick_command(&self) -> Option<KickCommand> {
        Some(kick_command())
    }

    fn reset_credits(&self) -> Option<&'static dyn ResetCredits> {
        Some(&CodexResetCredits)
    }

    fn login(&self) -> Option<&'static dyn LoginFlow> {
        Some(&CodexLogin)
    }

    fn shared_items(&self) -> &'static [SharedItem] {
        ITEMS
    }

    fn thread_registry(&self) -> Option<&'static dyn ThreadRegistry> {
        Some(&CodexThreads)
    }

    fn email(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> Option<String> {
        codex_email(&read_json(
            &account_dir(AgentId::CODEX, config_dir, shell_env).join(CODEX_SIGN_IN_FILE),
        )?)
    }
}

pub(crate) fn codex_email(file: &Value) -> Option<String> {
    let token = file.pointer("/tokens/id_token")?.as_str()?;
    let payload = token.split('.').nth(1)?;
    let claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?)
            .ok()?;
    let email = claims.get("email").and_then(Value::as_str).or_else(|| {
        claims
            .pointer("/https:~1~1api.openai.com~1profile/email")
            .and_then(Value::as_str)
    })?;
    tidy(email)
}
