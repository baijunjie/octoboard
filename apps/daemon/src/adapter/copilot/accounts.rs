//! What GitHub Copilot CLI's accounts can be asked: whether one is signed in and as whom, and
//! signing in and reading the plan quota. The measurements are in `docs/agent-cli-reference.md`,
//! "GitHub Copilot CLI".
//!
//! A configuration directory's `config.json` names the GitHub logins it was signed in as
//! (`loggedInUsers`, `lastLoggedInUser`). The token itself is in the system credential store,
//! under the host and the login and not under the directory, and is never read here: a directory
//! that names a login is signed in exactly while the store holds that login's item, which is
//! asked for by its presence alone (`crate::accounts::credential_presence::item_present`), or
//! while the file itself holds the login's token, as it does where the user had it kept in plain
//! text. Such a token is read only to be handed to a run of Copilot through its environment
//! ([`token_in_file`]); Octoboard uses the credential but never writes it anywhere. A directory
//! with the empty login list left by `/logout` has no saved Copilot sign-in. A new directory
//! with no record of a login gives no local reading. A CLI run can also sign in through the
//! environment or GitHub CLI.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::login::CopilotLogin;
use crate::accounts::credential_presence::item_present;
use crate::accounts::login::flows::LoginFlow;
use crate::accounts::quotas::email::tidy;
use crate::accounts::quotas::sign_in::SignInReading;
use crate::accounts::AccountOps;
use crate::adapter::account_dir;
use crate::adapter::user_json::{self, Takes};
use crate::hostfs::read_regular_text;
use crate::platform::credential_store::Presence;
use crate::protocol::AgentId;
use crate::providers::copilot::COPILOT_STATE_FILE;

/// How much of the state file is read: it holds a handful of short entries.
const STATE_FILE_LIMIT: usize = 1024 * 1024;

/// The service Copilot keeps its tokens under in the system credential store.
const TOKEN_SERVICE: &str = "copilot-cli";

pub(super) struct CopilotAccounts;

impl AccountOps for CopilotAccounts {
    fn shared_items(&self) -> &'static [crate::accounts::shared_config::SharedItem] {
        super::shared_config::ITEMS
    }

    fn file_registries(&self) -> &'static [crate::accounts::shared_config::FileRegistry] {
        super::shared_config::REGISTRIES
    }

    fn plugin_registries(&self) -> Option<&'static [(&'static str, &'static str)]> {
        Some(&[])
    }

    fn run_home(
        &self,
        run_homes: &std::path::Path,
        run: crate::adapter::account_cli::BackgroundRun,
        account: &std::path::Path,
        shell_env: &std::collections::HashMap<String, String>,
        _: &crate::adapter::account_cli::RunWait<'_>,
    ) -> anyhow::Result<Option<crate::adapter::account_cli::RunHome>> {
        if run != crate::adapter::account_cli::BackgroundRun::QuotaReading {
            return Ok(None);
        }
        let state = state_of(account);
        let mut home = super::models::private_query_home(
            &run_homes.join(AgentId::COPILOT.wire_name()),
            state.as_ref(),
        )?;
        home.work_in_home = true;
        if let Some(token) = super::models::token_to_hand(state.as_ref(), shell_env) {
            home.env.push(("COPILOT_GITHUB_TOKEN".to_owned(), token));
        }
        Ok(Some(home))
    }

    fn quota_probe(&self) -> Option<&'static dyn crate::accounts::quotas::probe::QuotaProbe> {
        Some(&super::quota::QUOTA)
    }

    fn sign_in_after_quota_failure(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> SignInReading {
        self.sign_in_state(config_dir, shell_env)
    }

    fn mcp_inventory(
        &self,
    ) -> Option<&'static dyn crate::accounts::mcp_inventory::McpInventoryReader> {
        Some(&super::mcp_inventory::CopilotMcpInventory)
    }

    fn login(&self) -> Option<&'static dyn LoginFlow> {
        Some(&CopilotLogin)
    }

    /// The GitHub login the directory names: Copilot records no email.
    fn email(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> Option<String> {
        signed_in_login(&account_dir(AgentId::COPILOT, config_dir, shell_env))
    }

    /// A directory that names a login is signed in while the credential store holds that
    /// login's token item, and signed out while it does not. Copilot also records a cleared
    /// login list in the directory where `/logout` ran.
    /// **Blocks** on the store's answer, for a few seconds at most.
    fn sign_in_state(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> SignInReading {
        sign_in_of(
            &account_dir(AgentId::COPILOT, config_dir, shell_env),
            |service, account| item_present(service, account, &token_target(service, account)),
        )
    }
}

/// The sign-in of the configuration directory `dir`, `present` saying whether the credential
/// store holds the item of a service and an account. A directory whose state file holds its
/// login's token is signed in without asking; one that names a login and no host cannot name
/// the item, and cannot be told. The store is asked for presence only, never for the token.
fn sign_in_of(dir: &Path, present: impl Fn(&str, &str) -> Presence) -> SignInReading {
    let Some(state) = state_of(dir) else {
        return SignInReading::CannotTell;
    };
    let Some(user) = user_of(&state) else {
        // /logout removes lastLoggedInUser and keeps the now empty list in the invoking home.
        return if state.get("lastLoggedInUser").is_none()
            && state["loggedInUsers"].as_array().is_some_and(Vec::is_empty)
        {
            SignInReading::SignedOut
        } else {
            SignInReading::CannotTell
        };
    };
    let signed_in = || SignInReading::SignedIn {
        email: Some(user.login.clone()),
    };
    if user.token_in_file {
        return signed_in();
    }
    let Some(host) = &user.host else {
        return SignInReading::CannotTell;
    };
    match present(TOKEN_SERVICE, &token_account(host, &user.login)) {
        Presence::Present => signed_in(),
        Presence::Absent => SignInReading::SignedOut,
        Presence::CannotTell => SignInReading::CannotTell,
    }
}

/// The account Copilot keeps the token of `login` at `host` under: `<host>:<login>:github`, the
/// host as the state file names it (`https://github.com`).
fn token_account(host: &str, login: &str) -> String {
    format!("{host}:{login}:github")
}

/// Copilot names its Windows generic credential by account followed by service.
fn token_target(service: &str, account: &str) -> String {
    format!("{account}.{service}")
}

/// The login a state file names, with what finding its token takes.
struct User {
    login: String,
    /// The host the login is of, as the file names it; `None` where it names none.
    host: Option<String>,
    /// Whether the file itself holds a token for the login (`authTokens`), which Copilot keeps
    /// there in plain text only on the user's yes.
    token_in_file: bool,
}

/// The login the state file of the configuration directory `dir` names, if it names one.
pub(super) fn signed_in_login(dir: &Path) -> Option<String> {
    state_of(dir).as_ref().and_then(named_login)
}

/// The keys of a state file that say which login a run of Copilot is signed in as, and hold no
/// credential: `authTokens`, where the user keeps the token in the file, is never copied.
const SIGN_IN_KEYS: &[&str] = &["loggedInUsers", "lastLoggedInUser"];

/// The state file of `dir`, parsed as Copilot reads it; `None` where it cannot be read. Read once
/// for all a caller takes from it ([`sign_in_keys`], [`token_in_file`], [`named_login`]).
pub(super) fn state_of(dir: &Path) -> Option<Value> {
    let text = read_regular_text(&dir.join(COPILOT_STATE_FILE), STATE_FILE_LIMIT).ok()?;
    user_json::parse(&text, STATE_FILE_TAKES).ok()
}

/// The keys of a parsed state file that say which login Copilot is signed in as there
/// ([`SIGN_IN_KEYS`]), copied as they are, for a state file of Octoboard's own that is to be
/// signed in as the same login. Empty where the file names none.
pub(super) fn sign_in_keys(state: &Value) -> serde_json::Map<String, Value> {
    let Value::Object(state) = state else {
        return serde_json::Map::new();
    };
    state
        .iter()
        .filter(|(key, _)| SIGN_IN_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// The token a parsed state file holds in plain text for the login it names (`authTokens`, under
/// `<host>:<login>`), for a run of Copilot to be handed through its environment in place of a
/// copy of the file's. Never written, logged or shown.
pub(super) fn token_in_file(state: &Value) -> Option<String> {
    let (login, host) = named_user(state)?;
    file_token(state, host.as_deref()?, &login).map(str::to_string)
}

/// The login a parsed state file names ([`named_user`]).
pub(super) fn named_login(state: &Value) -> Option<String> {
    named_user(state).map(|(login, _)| login)
}

/// The token a parsed state file holds for `login` at `host`, where it holds a non-empty one.
fn file_token<'a>(state: &'a Value, host: &str, login: &str) -> Option<&'a str> {
    state
        .get("authTokens")?
        .get(format!("{host}:{login}"))?
        .as_str()
        .filter(|token| !token.is_empty())
}

/// What Copilot takes beyond JSON in its state file, each measured: a `//` comment wherever it
/// stands (the lines it writes above the JSON itself, a line of its own inside the JSON, after a
/// value), a `/* */` comment, a comma before the closing brace, and a byte order mark. The header
/// lines are comments like the others, so they need no leniency of their own.
pub(super) const STATE_FILE_TAKES: Takes = Takes {
    byte_order_mark: true,
    comments: true,
    trailing_commas: true,
    leading_comment_lines: false,
};

/// The login a parsed state file names, its host, and whether the file holds a token under them.
fn user_of(state: &Value) -> Option<User> {
    let (login, host) = named_user(state)?;
    let token_in_file = host
        .as_deref()
        .is_some_and(|host| file_token(state, host, &login).is_some());
    Some(User {
        login,
        host,
        token_in_file,
    })
}

/// The login a parsed state file names, the one it was last signed in as, else the first of the
/// ones it lists, with its host where it names one.
fn named_user(state: &Value) -> Option<(String, Option<String>)> {
    let user_of = |user: &Value| {
        Some((
            tidy(user.get("login")?.as_str()?)?,
            user.get("host").and_then(Value::as_str).and_then(tidy),
        ))
    };
    state.get("lastLoggedInUser").and_then(user_of).or_else(|| {
        state
            .get("loggedInUsers")?
            .as_array()?
            .iter()
            .find_map(user_of)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    /// The login a state file's text names: the one it was last signed in as, else the first of
    /// the ones it lists.
    fn login_in(text: &str) -> Option<String> {
        named_login(&user_json::parse(text, STATE_FILE_TAKES).ok()?)
    }

    /// The state file of a home signed in as `octocat`, shaped as the reference describes it: two
    /// `//` comment lines, then JSON holding `loggedInUsers` and `lastLoggedInUser`.
    const SIGNED_IN: &str = "// Copilot keeps its state in this file.\n// Do not edit it by hand.\n{\n  \"trustedFolders\": [\"/tmp/probe/work\"],\n  \"loggedInUsers\": [\n    { \"host\": \"https://github.com\", \"login\": \"octocat\", \"kind\": \"githubDotCom\" }\n  ],\n  \"lastLoggedInUser\": { \"host\": \"https://github.com\", \"login\": \"octocat\" }\n}\n";

    /// A home whose keychain was unavailable and whose answer was `y`: the token sits in the file
    /// as `authTokens`, beside the same two login keys. The token is made up.
    const PLAINTEXT: &str = "// Copilot keeps its state in this file.\n// Do not edit it by hand.\n{\n  \"authTokens\": {\n    \"https://github.com:octocat\": \"FAKE-TOKEN-NOT-A-REAL-ONE\",\n    \"https://github.com:octocat:github\": \"FAKE-TOKEN-NOT-A-REAL-ONE\"\n  },\n  \"loggedInUsers\": [\n    { \"host\": \"https://github.com\", \"login\": \"octocat\", \"kind\": \"githubDotCom\" }\n  ],\n  \"lastLoggedInUser\": { \"host\": \"https://github.com\", \"login\": \"octocat\" }\n}\n";

    #[test]
    fn the_login_is_read_below_the_comment_lines() {
        assert_eq!(login_in(SIGNED_IN).as_deref(), Some("octocat"));
        // Indented comment lines go too, and the host's `//` inside a value is not a comment.
        let indented = format!(
            "   // a comment\n\t// another\n{}",
            SIGNED_IN.lines().skip(2).collect::<Vec<_>>().join("\n")
        );
        assert_eq!(login_in(&indented).as_deref(), Some("octocat"));
    }

    /// What Copilot was measured to take in the file beyond JSON is read as it reads it: a comment
    /// line inside the JSON, a comment after a value, a block comment, a comma before the closing
    /// brace and a byte order mark. A file it does not take names no login.
    #[test]
    fn the_file_is_read_as_copilot_reads_it() {
        let body =
            "\"loggedInUsers\": [{\"host\": \"https://github.com\", \"login\": \"octocat\"}]";
        for text in [
            format!("// header\n{{\n  // a comment line in the middle\n  {body}\n}}\n"),
            format!("// header\n{{\n  \"firstLaunchAt\": \"2026-11-22\", // after a value\n  {body}\n}}\n"),
            format!("/* block */\n{{\n  {body},\n}}\n"),
            format!("{}{{\n  {body}\n}}\n", '\u{FEFF}'),
        ] {
            assert_eq!(login_in(&text).as_deref(), Some("octocat"), "{text}");
        }
        let broken = format!("{{\n  {body} oops\n}}\n");
        assert_eq!(login_in(&broken), None);
    }

    /// The login the home was last signed in as wins over the ones it lists, whatever their order.
    #[test]
    fn the_last_login_is_read_before_the_listed_ones() {
        let text = "{\"loggedInUsers\":[{\"host\":\"h\",\"login\":\"first\"},{\"host\":\"h\",\"login\":\"second\"}],\"lastLoggedInUser\":{\"host\":\"h\",\"login\":\"second\"}}";
        assert_eq!(login_in(text).as_deref(), Some("second"));
        let only_listed = "{\"loggedInUsers\":[{\"host\":\"h\",\"login\":\"first\"},{\"host\":\"h\",\"login\":\"second\"}]}";
        assert_eq!(login_in(only_listed).as_deref(), Some("first"));
        let only_last = "{\"lastLoggedInUser\":{\"host\":\"h\",\"login\":\"last\"}}";
        assert_eq!(login_in(only_last).as_deref(), Some("last"));
    }

    /// A last login that names nobody falls back to the listed ones; a listed entry that names
    /// nobody is passed over.
    #[test]
    fn an_entry_that_names_no_login_is_passed_over() {
        let text = "{\"lastLoggedInUser\":{\"host\":\"h\",\"login\":\"  \"},\"loggedInUsers\":[{\"host\":\"h\"},{\"host\":\"h\",\"login\":7},{\"host\":\"h\",\"login\":\"third\"}]}";
        assert_eq!(login_in(text).as_deref(), Some("third"));
    }

    #[test]
    fn a_file_that_names_no_login_names_none() {
        for text in [
            "// only comments\n// and nothing else\n",
            "",
            "{}",
            "{\"trustedFolders\":[\"/tmp/probe/work\"]}",
            "{\"loggedInUsers\":[],\"lastLoggedInUser\":null}",
            "{\"loggedInUsers\":\"octocat\",\"lastLoggedInUser\":\"octocat\"}",
            "[{\"login\":\"octocat\"}]",
            // Not JSON, or JSON cut short.
            "{\"loggedInUsers\":[{\"login\":\"octocat\"",
            "not json at all",
        ] {
            assert_eq!(login_in(text), None, "{text}");
        }
    }

    /// The keys copied into a state file of Octoboard's own name the login and never hold its
    /// token; the token the file keeps for the login it names is read only to be handed to a run,
    /// and a file that keeps none, or keeps one for another login, gives none.
    #[test]
    fn the_sign_in_keys_never_carry_the_token_kept_in_the_file() {
        let home = ScratchDir::new("copilot-sign-in-keys");
        std::fs::write(home.join("config.json"), PLAINTEXT).expect("state file");
        let state = |home: &Path| state_of(home).expect("a readable state file");
        let keys = sign_in_keys(&state(&home));
        let mut names: Vec<&str> = keys.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["lastLoggedInUser", "loggedInUsers"]);
        assert!(!Value::Object(keys).to_string().contains("FAKE-TOKEN"));
        assert_eq!(
            token_in_file(&state(&home)).as_deref(),
            Some("FAKE-TOKEN-NOT-A-REAL-ONE")
        );
        std::fs::write(
            home.join("config.json"),
            PLAINTEXT.replace("\"login\": \"octocat\" }\n}", "\"login\": \"hubot\" }\n}"),
        )
        .expect("state file");
        assert_eq!(token_in_file(&state(&home)), None);
        std::fs::write(
            home.join("config.json"),
            "{\"lastLoggedInUser\":{\"host\":\"https://github.com\",\"login\":\"octocat\"}}",
        )
        .expect("state file");
        assert_eq!(token_in_file(&state(&home)), None);
    }

    /// The plaintext shape is read for its login alone: the token beside it is never returned.
    #[test]
    fn the_plaintext_shape_gives_its_login_and_never_its_token() {
        assert_eq!(login_in(PLAINTEXT).as_deref(), Some("octocat"));
        let home = ScratchDir::new("copilot-plaintext");
        std::fs::write(home.join("config.json"), PLAINTEXT).expect("state file");
        let reading = CopilotAccounts.sign_in_state(Some(&home), &HashMap::new());
        assert_eq!(
            reading,
            SignInReading::SignedIn {
                email: Some("octocat".to_string())
            }
        );
        assert!(!format!("{reading:?}").contains("FAKE-TOKEN"));
    }

    /// A stand-in for the credential store that holds the items `held` and records what it was
    /// asked for: no test asks the host's own store.
    fn store<'a>(
        held: &'a [(&'a str, &'a str)],
        asked: &'a std::cell::RefCell<Vec<(String, String)>>,
    ) -> impl Fn(&str, &str) -> Presence + 'a {
        move |service, account| {
            asked
                .borrow_mut()
                .push((service.to_string(), account.to_string()));
            if held.iter().any(|(held_service, held_account)| {
                *held_service == service && *held_account == account
            }) {
                Presence::Present
            } else {
                Presence::Absent
            }
        }
    }

    /// The item Copilot was measured to keep the token of `octocat` under.
    const OCTOCAT_ITEM: (&str, &str) = ("copilot-cli", "https://github.com:octocat:github");

    /// A home that names a login is signed in as it exactly while the credential store holds
    /// that login's item, asked for by the service and the account Copilot was measured to use;
    /// another login's item does not sign it in, and a store that cannot tell leaves it untold.
    #[test]
    fn a_home_that_names_a_login_follows_its_token_items_presence() {
        let home = ScratchDir::new("copilot-item-presence");
        std::fs::write(home.join("config.json"), SIGNED_IN).expect("state file");
        let asked = std::cell::RefCell::new(Vec::new());
        assert_eq!(
            sign_in_of(&home, store(&[OCTOCAT_ITEM], &asked)),
            SignInReading::SignedIn {
                email: Some("octocat".to_string())
            }
        );
        assert_eq!(
            asked.borrow().as_slice(),
            [(OCTOCAT_ITEM.0.to_string(), OCTOCAT_ITEM.1.to_string())]
        );
        assert_eq!(
            token_target(OCTOCAT_ITEM.0, OCTOCAT_ITEM.1),
            "https://github.com:octocat:github.copilot-cli"
        );
        let another = [("copilot-cli", "https://github.com:hubot:github")];
        assert_eq!(
            sign_in_of(&home, store(&another, &asked)),
            SignInReading::SignedOut
        );
        assert_eq!(
            sign_in_of(&home, |_, _| Presence::CannotTell),
            SignInReading::CannotTell
        );
    }

    /// A home whose file holds its login's token is signed in without the store being asked; a
    /// home that names a login and no host cannot name the item and is not told, nor asked for.
    #[test]
    fn the_store_is_asked_only_where_the_file_does_not_settle_it() {
        let asked = std::cell::RefCell::new(Vec::new());
        let plaintext = ScratchDir::new("copilot-item-plaintext");
        std::fs::write(plaintext.join("config.json"), PLAINTEXT).expect("state file");
        assert_eq!(
            sign_in_of(&plaintext, store(&[], &asked)),
            SignInReading::SignedIn {
                email: Some("octocat".to_string())
            }
        );
        let hostless = ScratchDir::new("copilot-item-hostless");
        std::fs::write(
            hostless.join("config.json"),
            "{\"lastLoggedInUser\":{\"login\":\"octocat\"}}",
        )
        .expect("state file");
        assert_eq!(
            sign_in_of(&hostless, store(&[OCTOCAT_ITEM], &asked)),
            SignInReading::CannotTell
        );
        // An empty token in the file is none: the store decides.
        let emptied = ScratchDir::new("copilot-item-emptied");
        std::fs::write(
            emptied.join("config.json"),
            PLAINTEXT.replace("FAKE-TOKEN-NOT-A-REAL-ONE", ""),
        )
        .expect("state file");
        assert!(asked.borrow().is_empty());
        assert_eq!(
            sign_in_of(&emptied, store(&[], &asked)),
            SignInReading::SignedOut
        );
        assert_eq!(asked.borrow().len(), 1);
    }

    /// A home that names a login gives it as the account's "email", whatever the store holds.
    #[test]
    fn a_home_that_names_a_login_gives_it_as_the_accounts_email() {
        let home = ScratchDir::new("copilot-signed-in");
        std::fs::write(home.join("config.json"), SIGNED_IN).expect("state file");
        assert_eq!(
            CopilotAccounts
                .email(Some(&home), &HashMap::new())
                .as_deref(),
            Some("octocat")
        );
        assert_eq!(signed_in_login(&home).as_deref(), Some("octocat"));
    }

    /// The default account is the home the shell names, read the same way.
    #[test]
    fn the_default_account_is_the_home_the_shell_names() {
        let home = ScratchDir::new("copilot-shell-home");
        std::fs::write(home.join("config.json"), PLAINTEXT).expect("state file");
        let shell = HashMap::from([(
            "COPILOT_HOME".to_string(),
            home.to_string_lossy().into_owned(),
        )]);
        assert_eq!(
            CopilotAccounts.email(None, &shell).as_deref(),
            Some("octocat")
        );
        // The file holds the login's token, so the reading asks no store.
        assert_eq!(
            CopilotAccounts.sign_in_state(None, &shell),
            SignInReading::SignedIn {
                email: Some("octocat".to_string())
            }
        );
    }

    /// A new home gives no reading about its saved Copilot login.
    #[test]
    fn a_home_with_no_record_of_a_login_cannot_be_told() {
        let missing = ScratchDir::new("copilot-no-file");
        let empty_state = ScratchDir::new("copilot-no-key");
        std::fs::write(
            empty_state.join("config.json"),
            "// c\n{\"trustedFolders\":[]}\n",
        )
        .expect("state file");
        let malformed = ScratchDir::new("copilot-malformed");
        std::fs::write(malformed.join("config.json"), "// c\n{\"loggedInUsers\":[")
            .expect("state file");
        let a_directory = ScratchDir::new("copilot-directory");
        std::fs::create_dir(a_directory.join("config.json")).expect("a directory");
        for home in [&missing, &empty_state, &malformed, &a_directory] {
            let reading = CopilotAccounts.sign_in_state(Some(home), &HashMap::new());
            assert_eq!(reading, SignInReading::CannotTell, "{}", home.display());
            assert_ne!(reading, SignInReading::SignedOut);
            assert_eq!(CopilotAccounts.email(Some(home), &HashMap::new()), None);
            assert_eq!(signed_in_login(home), None);
        }
    }

    #[test]
    fn the_empty_login_list_left_by_logout_reads_signed_out() {
        let home = ScratchDir::new("copilot-logout");
        std::fs::write(home.join("config.json"), r#"{"loggedInUsers":[]}"#).unwrap();
        assert_eq!(
            sign_in_of(&home, |_, _| panic!("logout left no item to look up")),
            SignInReading::SignedOut
        );
    }

    /// The login flow reads a finished login from this record: the directory that names a login is
    /// confirmed, one that does not is the case the flow reports as not confirmed.
    #[test]
    fn a_finished_login_is_confirmed_by_the_record_the_flow_reads() {
        let home = ScratchDir::new("copilot-confirm");
        assert_eq!(signed_in_login(&home), None);
        std::fs::write(home.join("config.json"), SIGNED_IN).expect("state file");
        assert_eq!(signed_in_login(&home).as_deref(), Some("octocat"));
    }

    #[test]
    fn it_has_a_login_flow_and_a_quota_reading() {
        assert!(CopilotAccounts.login().is_some());
        assert!(CopilotAccounts.quota_probe().is_some());
    }
}
