//! What Antigravity CLI's account can be asked: whether it is signed in, on a host whose
//! credential store can be asked. Its login is one item of the system's credential store, shared
//! with Google's other Antigravity programs, and no file records whether it is there or whose it
//! is (1.5.14), so no email is read. A session whose launch finds no login says so itself, through
//! the CLI's own status report.
//!
//! Which item it is is the CLI's own fact ([`LOGIN_ITEM_SERVICE`], [`LOGIN_ITEM_ACCOUNT`], [`LOGIN_ITEM_TARGET`]); how a
//! host's store is asked whether it holds an item, by its presence alone and never its content, is
//! the generic `crate::accounts::credential_presence`. On a host whose store has no such question
//! the state cannot be told.

use std::collections::HashMap;
use std::path::Path;

use crate::accounts::credential_presence;
use crate::accounts::login::flows::LoginFlow;
use crate::accounts::mcp_inventory::McpInventoryReader;
use crate::accounts::quotas::probe::QuotaProbe;
use crate::accounts::quotas::sign_in::SignInReading;
use crate::accounts::AccountOps;
use crate::adapter::account_cli::{BackgroundRun, RunHome, RunWait};
use crate::platform::credential_store::Presence;

pub(super) struct AntigravityAccounts;

/// The login item on macOS and Linux (the same pair in the Linux 1.5.16 binary).
const LOGIN_ITEM_SERVICE: &str = "gemini";
const LOGIN_ITEM_ACCOUNT: &str = "antigravity";
/// The generic credential's target, measured on Windows with CLI 1.5.16.
const LOGIN_ITEM_TARGET: &str = "gemini:antigravity";

/// The variable that holds the key of the CLI's other way in, a Gemini API key named in its
/// settings, which needs no keychain item: with it set, a missing item does not say signed out.
const API_KEY_VAR: &str = "GEMINI_API_KEY";

/// The sign-in state the login item's `presence` gives. An item that is there is a login, with no
/// email since nothing records one. A missing one is none, unless the environment holds an API
/// key, which the CLI can run on with no item (whether it does is a setting of its own, and that
/// route's sign-in was not measured). A store that did not answer says nothing of the account.
fn reading_of(presence: Presence, shell_env: &HashMap<String, String>) -> SignInReading {
    match presence {
        Presence::Present => SignInReading::SignedIn { email: None },
        Presence::Absent if shell_env.get(API_KEY_VAR).is_none_or(String::is_empty) => {
            SignInReading::SignedOut
        }
        Presence::Absent | Presence::CannotTell => SignInReading::CannotTell,
    }
}

impl AccountOps for AntigravityAccounts {
    fn login(&self) -> Option<&'static dyn LoginFlow> {
        Some(&super::login::AntigravityLogin)
    }

    fn quota_probe(&self) -> Option<&'static dyn QuotaProbe> {
        Some(&super::quota::AntigravityQuota)
    }

    fn sign_in_after_quota_failure(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> SignInReading {
        self.sign_in_state(config_dir, shell_env)
    }

    fn mcp_inventory(&self) -> Option<&'static dyn McpInventoryReader> {
        Some(&super::mcp_inventory::AntigravityInventory)
    }

    fn plugin_registries(&self) -> Option<&'static [(&'static str, &'static str)]> {
        Some(&[])
    }

    fn run_home(
        &self,
        run_homes: &Path,
        run: BackgroundRun,
        _: &Path,
        shell_env: &HashMap<String, String>,
        wait: &RunWait<'_>,
    ) -> anyhow::Result<Option<RunHome>> {
        match run {
            BackgroundRun::QuotaReading => {
                super::home::quota_home(run_homes, shell_env, wait).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// The default account's state is the item's presence. An account with a directory of its own
    /// is one the CLI does not have (`Capabilities::owned_accounts`): the item is the default
    /// account's whatever directory is named, so nothing is asked for it and nothing can be told.
    fn sign_in_state(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> SignInReading {
        if config_dir.is_some() {
            return SignInReading::CannotTell;
        }
        reading_of(
            credential_presence::item_present(
                LOGIN_ITEM_SERVICE,
                LOGIN_ITEM_ACCOUNT,
                LOGIN_ITEM_TARGET,
            ),
            shell_env,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::credential_store::{presence_query_on, PresenceQuery};
    use crate::platform::HostOs;
    use crate::protocol::AgentId;
    use crate::test_support::{bash, ScratchDir};

    /// The item asked about is the CLI's login, by its service and its account name: what the
    /// macOS query names, where the item was measured.
    #[test]
    fn the_item_asked_about_is_the_clis_login() {
        let Some(PresenceQuery::Command(query)) = presence_query_on(
            HostOs::Macos,
            LOGIN_ITEM_SERVICE,
            LOGIN_ITEM_ACCOUNT,
            LOGIN_ITEM_TARGET,
        ) else {
            panic!("macOS has a command query");
        };
        let args: Vec<String> = query
            .command()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            ["find-generic-password", "-s", "gemini", "-a", "antigravity"]
        );
    }

    /// What a stand-in store whose query ends with `status` says of the item, asked through the
    /// generic bounded query; 44 is its status for an item that is not there, as the measured
    /// store's is.
    fn asked_of_a_store_ending_with(status: i32) -> Presence {
        let script = format!("exit {status}");
        credential_presence::ask(Some(PresenceQuery::stand_in(
            bash(),
            &["-c", script.as_str()],
            44,
        )))
    }

    /// The item that is there reads signed in, with no email since nothing records one; the item
    /// that is missing reads signed out, unless the environment holds an API key; any other end
    /// of the store's query, and a host with no query, cannot tell.
    #[test]
    fn the_items_presence_is_the_sign_in_state() {
        let env = HashMap::new();
        for (status, reading) in [
            (0, SignInReading::SignedIn { email: None }),
            (44, SignInReading::SignedOut),
            (1, SignInReading::CannotTell),
            (51, SignInReading::CannotTell),
        ] {
            assert_eq!(
                reading_of(asked_of_a_store_ending_with(status), &env),
                reading,
                "status {status}"
            );
        }
        assert_eq!(
            reading_of(credential_presence::ask(None), &env),
            SignInReading::CannotTell
        );

        // With an API key in the environment a missing item no longer says signed out; an empty
        // one counts as unset, and the item that is there still says signed in.
        let keyed = HashMap::from([("GEMINI_API_KEY".to_string(), "a-key".to_string())]);
        let empty = HashMap::from([("GEMINI_API_KEY".to_string(), String::new())]);
        assert_eq!(
            reading_of(asked_of_a_store_ending_with(44), &keyed),
            SignInReading::CannotTell
        );
        assert_eq!(
            reading_of(asked_of_a_store_ending_with(44), &empty),
            SignInReading::SignedOut
        );
        assert_eq!(
            reading_of(asked_of_a_store_ending_with(0), &keyed),
            SignInReading::SignedIn { email: None }
        );
        assert_eq!(
            reading_of(Presence::CannotTell, &keyed),
            SignInReading::CannotTell
        );
    }

    /// An account with a directory of its own is one the CLI does not have: nothing is asked for
    /// it and its state cannot be told, whatever the directory holds. No file gives an email, and
    /// quota probing also refuses that account.
    #[test]
    fn an_account_with_a_directory_cannot_be_told_and_no_email_is_read() {
        let accounts = crate::adapter::provider(AgentId::ANTIGRAVITY)
            .accounts()
            .expect("the provider answers account questions");

        let home = ScratchDir::new("antigravity-accounts");
        let account = home.join(".gemini");
        let data = account.join("antigravity-cli");
        std::fs::create_dir_all(&data).expect("data directory");
        std::fs::write(
            data.join("settings.json"),
            r#"{"modelProvider":"gemini","email":"someone@example.com"}"#,
        )
        .expect("settings");
        let env = HashMap::from([("HOME".to_string(), home.to_string_lossy().into_owned())]);

        assert_eq!(
            accounts.sign_in_state(Some(&account), &env),
            SignInReading::CannotTell
        );
        assert_eq!(accounts.email(Some(&account), &env), None);
        assert_eq!(accounts.email(None, &env), None);
        assert!(accounts.quota_probe().is_some());
    }
}
