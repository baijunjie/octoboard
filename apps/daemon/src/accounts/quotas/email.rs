//! The email an account is signed in with, read from the agent's own sign-in record so a client
//! can tell accounts apart. Each record is a format of the CLI's own, which it may change or not
//! write at all (Codex can keep its credentials in the keyring, say), so any failure to find or
//! read one means no email, never a failure of the account. Nothing but the email leaves the
//! provider's reader (`AccountOps::email`, in `crate::adapter::<provider>::accounts`): the rest of
//! the record, tokens included, is parsed and dropped, and the email is never logged.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::adapter::files::LARGE_FILE_LIMIT;
use crate::adapter::provider;
use crate::hostfs::read_regular_text;
use crate::protocol::AgentId;

/// The longest email address there can be.
const MAX_EMAIL_CHARS: usize = 254;

/// The email `agent`'s account signs in with, `config_dir` being the account's directory and
/// `None` for the default account, whose directory follows `shell_env` as the CLI's would.
pub(super) fn account_email(
    agent: AgentId,
    config_dir: Option<&Path>,
    shell_env: &HashMap<String, String>,
) -> Option<String> {
    provider(agent).accounts()?.email(config_dir, shell_env)
}

pub(crate) fn read_json(file: &Path) -> Option<Value> {
    serde_json::from_str(&read_regular_text(file, LARGE_FILE_LIMIT).ok()?).ok()
}

/// `email` as an address worth showing: trimmed, one line, not empty or implausibly long.
pub(crate) fn tidy(email: &str) -> Option<String> {
    let email = email.trim();
    (!email.is_empty()
        && email.chars().count() <= MAX_EMAIL_CHARS
        && !email.chars().any(char::is_control))
    .then(|| email.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::claude::accounts::claude_email;
    use crate::adapter::codex::accounts::codex_email;
    use crate::adapter::grok::accounts::grok_email;
    use crate::test_support::ScratchDir;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use serde_json::json;

    fn jwt(claims: &Value) -> String {
        format!(
            "e30.{}.sig",
            URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes())
        )
    }

    fn write(dir: &Path, name: &str, content: &str) {
        std::fs::create_dir_all(dir).expect("dir");
        std::fs::write(dir.join(name), content).expect("file");
    }

    fn env(pairs: &[(&str, &Path)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string_lossy().into_owned()))
            .collect()
    }

    #[test]
    fn claude_reads_the_oauth_account_of_the_dot_file_in_its_directory() {
        let dir = ScratchDir::new("email-claude");
        let account = dir.join("account");
        write(
            &account,
            ".claude.json",
            r#"{"oauthAccount":{"emailAddress":" me@example.com "}}"#,
        );
        assert_eq!(
            account_email(AgentId::CLAUDE, Some(&account), &HashMap::new()).as_deref(),
            Some("me@example.com")
        );
    }

    #[test]
    fn claudes_default_account_reads_the_file_beside_the_default_directory_or_in_the_one_the_shell_names(
    ) {
        let dir = ScratchDir::new("email-claude-default");
        let home = dir.join("home");
        write(
            &home,
            ".claude.json",
            r#"{"oauthAccount":{"emailAddress":"home@example.com"}}"#,
        );
        assert_eq!(
            account_email(AgentId::CLAUDE, None, &env(&[("HOME", &home)])).as_deref(),
            Some("home@example.com")
        );
        let named = dir.join("named");
        write(
            &named,
            ".claude.json",
            r#"{"oauthAccount":{"emailAddress":"named@example.com"}}"#,
        );
        assert_eq!(
            account_email(
                AgentId::CLAUDE,
                None,
                &env(&[("HOME", &home), ("CLAUDE_CONFIG_DIR", &named)])
            )
            .as_deref(),
            Some("named@example.com")
        );
    }

    #[test]
    fn a_missing_malformed_or_signed_out_record_has_no_email() {
        let dir = ScratchDir::new("email-none");
        for agent in AgentId::ALL {
            // Nothing there.
            assert_eq!(
                account_email(agent, Some(&dir.join("absent")), &HashMap::new()),
                None
            );
        }
        let account = dir.join("account");
        write(&account, ".claude.json", "{not json");
        write(&account, "auth.json", "{not json");
        for agent in AgentId::ALL {
            assert_eq!(
                account_email(agent, Some(&account), &HashMap::new()),
                None,
                "{agent:?}"
            );
        }
        // Signed out: a record without the account, or with an email that is not text.
        write(&account, ".claude.json", r#"{"projects":{}}"#);
        write(
            &account,
            "auth.json",
            r#"{"OPENAI_API_KEY":null,"tokens":null}"#,
        );
        for agent in AgentId::ALL {
            assert_eq!(
                account_email(agent, Some(&account), &HashMap::new()),
                None,
                "{agent:?}"
            );
        }
        assert_eq!(
            claude_email(&json!({"oauthAccount": {"emailAddress": 3}})),
            None
        );
        assert_eq!(
            claude_email(&json!({"oauthAccount": {"emailAddress": "  "}})),
            None
        );
        assert_eq!(
            claude_email(&json!({"oauthAccount": {"emailAddress": "a\nb@example.com"}})),
            None
        );
    }

    #[test]
    fn codex_reads_the_email_claim_of_its_id_token() {
        let dir = ScratchDir::new("email-codex");
        let account = dir.join("account");
        let token = jwt(&json!({"email": "me@example.com", "sub": "x"}));
        write(
            &account,
            "auth.json",
            &json!({"tokens": {"id_token": token}}).to_string(),
        );
        assert_eq!(
            account_email(AgentId::CODEX, Some(&account), &HashMap::new()).as_deref(),
            Some("me@example.com")
        );
        // The default account follows CODEX_HOME.
        assert_eq!(
            account_email(AgentId::CODEX, None, &env(&[("CODEX_HOME", &account)])).as_deref(),
            Some("me@example.com")
        );
        let profile =
            jwt(&json!({"https://api.openai.com/profile": {"email": "profile@example.com"}}));
        assert_eq!(
            codex_email(&json!({"tokens": {"id_token": profile}})).as_deref(),
            Some("profile@example.com")
        );
        let null_email = jwt(&json!({"email": null,
            "https://api.openai.com/profile": {"email": "profile@example.com"}}));
        assert_eq!(
            codex_email(&json!({"tokens": {"id_token": null_email}})).as_deref(),
            Some("profile@example.com")
        );
        // Not a token, a token with no claim, or a payload that is not JSON.
        assert_eq!(codex_email(&json!({"tokens": {"id_token": "nope"}})), None);
        assert_eq!(
            codex_email(&json!({"tokens": {"id_token": jwt(&json!({"sub": "x"}))}})),
            None
        );
        assert_eq!(
            codex_email(&json!({"tokens": {"id_token": "a.!!!.c"}})),
            None
        );
    }

    #[test]
    fn grok_reads_the_one_email_its_entries_share_and_none_when_they_differ() {
        let dir = ScratchDir::new("email-grok");
        let account = dir.join("account");
        write(
            &account,
            "auth.json",
            r#"{"https://auth.x.ai::1":{"email":"me@example.com","auth_mode":"oidc"},
                "https://auth.x.ai::2":{"email":"me@example.com"}}"#,
        );
        assert_eq!(
            account_email(AgentId::GROK, Some(&account), &HashMap::new()).as_deref(),
            Some("me@example.com")
        );
        write(
            &account,
            "auth.json",
            r#"{"https://auth.x.ai::1":{"email":"me@example.com"},
                "https://auth.x.ai::2":{"email":"other@example.com"}}"#,
        );
        assert_eq!(
            account_email(AgentId::GROK, Some(&account), &HashMap::new()),
            None
        );
        assert_eq!(grok_email(&json!({})), None);
        assert_eq!(grok_email(&json!([{"email": "me@example.com"}])), None);
    }
}
