//! What Qwen Code's accounts can be asked: whether one is signed in, read from its own files. It
//! has no plan quota Octoboard reads. The key sign-in uses its Web Shell (`super::login`).
//!
//! An account chooses its sign-in method in `settings.json` (`security.auth.selectedType`). A
//! method that signs in with a key takes it from an environment variable: the one the chosen
//! model's entry under `modelProviders` names (`envKey`), else the method's own. `/auth` writes
//! the key into the settings' `env` object under a name it makes up, and writes that name into
//! the entries it adds. Qwen Code also loads the variable from `.env` files, and those of the
//! account's directory and the home directory are read here as it reads them
//! ([`key_in_env_files`]). This much is measured against an account signed in with an Anthropic key
//! on 0.28.3; the Qwen subscription, whose tokens are in `subscription_credentials.json`, is read
//! from Qwen Code's code only (`docs/agent-cli-reference.md`, section "Qwen Code").

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::settings::{system_defaults_path, system_settings_path, SYSTEM_FILE_LIMIT};
use super::settings_file::{self, SettingsFile};
use crate::accounts::login::flows::LoginFlow;
use crate::accounts::mcp_inventory::McpInventoryReader;
use crate::accounts::quotas::sign_in::SignInReading;
use crate::accounts::AccountOps;
use crate::adapter::files::{LARGE_FILE_LIMIT, SMALL_FILE_LIMIT};
use crate::adapter::{account_dir, snapshot_home};
use crate::protocol::AgentId;
use crate::providers::qwen::{QWEN_SETTINGS_FILE, QWEN_SUBSCRIPTION_FILE};

/// The sign-in method of a Qwen subscription.
const SUBSCRIPTION: &str = "qwen-subscription";

pub(super) struct QwenAccounts;

impl AccountOps for QwenAccounts {
    fn login(&self) -> Option<&'static dyn LoginFlow> {
        Some(&super::login::QwenLogin)
    }

    fn mcp_inventory(&self) -> Option<&'static dyn McpInventoryReader> {
        Some(&super::mcp_inventory::QwenMcpInventory)
    }

    fn shared_items(&self) -> &'static [crate::accounts::shared_config::SharedItem] {
        super::shared_config::ITEMS
    }

    fn plugin_registries(&self) -> Option<&'static [(&'static str, &'static str)]> {
        Some(&[])
    }

    fn run_home(
        &self,
        _: &std::path::Path,
        _: crate::adapter::account_cli::BackgroundRun,
        _: &std::path::Path,
        _: &std::collections::HashMap<String, String>,
        _: &crate::adapter::account_cli::RunWait<'_>,
    ) -> anyhow::Result<Option<crate::adapter::account_cli::RunHome>> {
        Ok(None)
    }

    fn sign_in_state(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> SignInReading {
        let dir = account_dir(AgentId::QWEN, config_dir, shell_env);
        let read = |file: &Path| settings_file::read(file, LARGE_FILE_LIMIT);
        let settings = match read(&dir.join(QWEN_SETTINGS_FILE)) {
            // Qwen Code reads the file or reports it; which, and what it holds, is not known here.
            SettingsFile::Unusable(_) => return SignInReading::CannotTell,
            settings => settings.settings(),
        };
        let chosen = text(settings.as_ref(), &["security", "auth", "selectedType"]);
        let credentials = match read(&dir.join(QWEN_SUBSCRIPTION_FILE)) {
            // Tokens that are there and cannot be read are not tokens that are missing.
            SettingsFile::Unusable(_) if chosen == Some(SUBSCRIPTION) => {
                return SignInReading::CannotTell
            }
            credentials => credentials.settings(),
        };
        let reading = sign_in_reading(
            settings.as_ref(),
            credentials.as_ref(),
            shell_env,
            &|variable| key_in_env_files(variable, &dir, shell_env),
        );
        // The account's own file is one layer of four: the system settings file is laid over it
        // and the system defaults file under it, and either may choose the method and hold the
        // key (an administrator's set-up). Those layers are not merged here, so an account that
        // chose no method itself is only called signed out where neither says anything of it.
        let chosen_elsewhere = || {
            [
                system_settings_path(shell_env),
                system_defaults_path(shell_env),
            ]
            .iter()
            // Read within the size the launch reads them in (`super::settings`).
            .any(|file| match settings_file::read(file, SYSTEM_FILE_LIMIT) {
                SettingsFile::Settings(layer) => layer.contains_key("security"),
                SettingsFile::Unusable(_) => true,
                SettingsFile::Missing => false,
            })
        };
        if chosen.is_none() && chosen_elsewhere() {
            return SignInReading::CannotTell;
        }
        reading
    }
}

/// The name of the file Qwen Code loads variables from.
const ENV_FILE: &str = ".env";

/// Whether a `.env` file of the account sets `variable`, the one its key method reads the key
/// from, to something.
///
/// Qwen Code loads `.env` files into its environment before it looks for the key (0.28.3, each of
/// the four places run against a stand-in): the first of `<directory>/.qwen/.env` and
/// `<directory>/.env` found walking up from the working directory, then `<Qwen home>/.env`,
/// `~/.qwen/.env` where the Qwen home is another directory, and `~/.env`. A file's value is taken
/// only for a variable the environment does not have or has empty, so any of the files setting it
/// to something signs the account in. The three that belong to the account, whatever the project,
/// are read here; a project's own is not, an account's reading having no project, so an account
/// whose key is only there still cannot be told.
fn key_in_env_files(variable: &str, account: &Path, shell_env: &HashMap<String, String>) -> bool {
    let home = snapshot_home(shell_env);
    [
        account.join(ENV_FILE),
        home.join(crate::providers::qwen::LAYOUT.default_dir)
            .join(ENV_FILE),
        home.join(ENV_FILE),
    ]
    .iter()
    .any(|file| {
        crate::hostfs::read_regular_text(file, SMALL_FILE_LIMIT)
            .is_ok_and(|content| env_file_sets(&content, variable))
    })
}

/// Whether `content`, a `.env` file, leaves `variable` set to something: the last line that sets
/// it decides, as in the `dotenv` parser Qwen Code reads the file with ([`env_file_value`]).
fn env_file_sets(content: &str, variable: &str) -> bool {
    env_file_value(content, variable).is_some_and(|value| !value.is_empty())
}

/// The value `content`, a `.env` file, gives `variable`, read as `dotenv` 16 reads it, whose line
/// pattern Qwen Code 0.28.3 carries: a line is `NAME=value` or `NAME: value` after an optional
/// `export`, a later line for the same name replacing an earlier one.
///
/// - `:` needs white space after it, `=` does not.
/// - A value in single, double or back quotes runs to its closing quote, over line ends and over a
///   quote behind a backslash (which closes it only where no other quote can), and may start on
///   the line after the `=`; only a comment may follow it. The quotes are taken off.
/// - Any other value runs to the first `#` or the end of the line, and is trimmed.
///
/// Escapes inside double quotes are not expanded, which changes no value from empty to set.
fn env_file_value(content: &str, variable: &str) -> Option<String> {
    let text = content.replace("\r\n", "\n").replace('\r', "\n");
    let mut value = None;
    let mut at = 0;
    while at < text.len() {
        let line_end = |from: usize| text[from..].find('\n').map_or(text.len(), |end| from + end);
        match env_assignment(&text[at..]) {
            Some((name, assigned, length)) => {
                if name == variable {
                    value = Some(assigned);
                }
                at = line_end(at + length) + 1;
            }
            None => at = line_end(at) + 1,
        }
    }
    value
}

/// The assignment `text` starts with, `text` beginning at the start of a line: the name, the
/// value and the length read, which ends on the line the value ends on.
fn env_assignment(text: &str) -> Option<(&str, String, usize)> {
    let blank = |c: char| c == ' ' || c == '\t';
    let line = text.trim_start_matches(blank);
    let line = line
        .strip_prefix("export")
        .filter(|rest| rest.starts_with(blank))
        .map_or(line, |rest| rest.trim_start_matches(blank));
    let name_length = line
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')))
        .unwrap_or(line.len());
    let (name, rest) = line.split_at(name_length);
    if name.is_empty() {
        return None;
    }
    let rest = match rest.trim_start().strip_prefix('=') {
        Some(rest) => rest,
        None => rest
            .strip_prefix(':')
            .and_then(|rest| rest.strip_prefix(char::is_whitespace))?,
    };
    let read = |rest: &str| text.len() - rest.len();
    // A quoted value, where only a comment follows its closing quote.
    let quoted = rest.trim_start();
    if let Some(quote) = quoted
        .chars()
        .next()
        .filter(|c| ['"', '\'', '`'].contains(c))
    {
        let inner = &quoted[1..];
        // The quotes that may close it: the first with no backslash before it, and, should what
        // follows that one be no comment, each one before it from the last back.
        let mut closers = Vec::new();
        let mut previous = None;
        for (at, c) in inner.char_indices() {
            if c == quote {
                closers.push(at);
                if previous != Some('\\') {
                    break;
                }
            }
            previous = Some(c);
        }
        for close in closers.into_iter().rev() {
            let after = inner[close + 1..].trim_start_matches(blank);
            if after.is_empty() || after.starts_with(['#', '\n']) {
                return Some((name, inner[..close].to_string(), read(after)));
            }
        }
    }
    let end = rest.find(['#', '\n']).unwrap_or(rest.len());
    let bare = rest[..end].trim();
    // A value that is quoted whole loses its quotes, wherever the quoted reading stopped.
    let unquoted = ['"', '\'', '`'].iter().find_map(|quote| {
        bare.strip_prefix(*quote)
            .filter(|_| bare.len() > 1)
            .and_then(|inner| inner.strip_suffix(*quote))
    });
    Some((
        name,
        unquoted.unwrap_or(bare).to_string(),
        read(&rest[end..]),
    ))
}

/// The sign-in state an account's `settings` and subscription `credentials` say, each `None` when
/// there is no such file ([`AccountOps::sign_in_state`] answers for a file that is there and
/// cannot be read as settings before it gets here). `shell_env` is the environment a session
/// starts in, and `env_files` whether a `.env` file of the account sets a variable to something
/// ([`key_in_env_files`]).
///
/// - No method chosen: signed out, since Qwen Code asks for one before anything else.
/// - `qwen-subscription`: signed in exactly when the credentials hold an access token.
/// - `qwen-oauth`: signed out, the method having been discontinued.
/// - Any other method signs in with a key: signed in when the variable that holds it
///   ([`model_key_variable`], else [`default_key_variable`]) is set in the settings' `env`, in
///   the environment or in a `.env` file of the account, or, where no entry names the variable,
///   when the settings hold a key themselves (`security.auth.apiKey`). With none of these it is
///   unknown, not signed out: Qwen Code also reads the variable from a project's `.env` files.
fn sign_in_reading(
    settings: Option<&Value>,
    credentials: Option<&Value>,
    shell_env: &HashMap<String, String>,
    env_files: &dyn Fn(&str) -> bool,
) -> SignInReading {
    let auth = |key| text(settings, &["security", "auth", key]);
    let Some(method) = auth("selectedType") else {
        return SignInReading::SignedOut;
    };
    match method {
        "qwen-oauth" => SignInReading::SignedOut,
        SUBSCRIPTION => {
            if text(credentials, &["tokens", "access_token"]).is_some() {
                SignInReading::SignedIn { email: None }
            } else {
                SignInReading::SignedOut
            }
        }
        _ => {
            let named = model_key_variable(settings);
            let variable = named.or_else(|| default_key_variable(method));
            let set = variable.is_some_and(|variable| {
                text(settings, &["env", variable]).is_some_and(|key| !key.trim().is_empty())
                    || shell_env
                        .get(variable)
                        .is_some_and(|key| !key.trim().is_empty())
                    || env_files(variable)
            });
            // Qwen Code looks at the settings' own key only when no entry names a variable.
            if set || (named.is_none() && auth("apiKey").is_some()) {
                SignInReading::SignedIn { email: None }
            } else {
                SignInReading::CannotTell
            }
        }
    }
}

/// The non-empty string at `path` in `value`.
fn text<'a>(value: Option<&'a Value>, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(value?, |at, key| at.get(key))?
        .as_str()
        .filter(|text| !text.is_empty())
}

/// The variable the chosen model's entry under `modelProviders` names for its key: the entry with
/// the model's id (`model.name`), the one with its base URL too where several have the id. Every
/// provider's entries are searched, since a provider may be listed under a name of the user's
/// choosing.
fn model_key_variable(settings: Option<&Value>) -> Option<&str> {
    let model = text(settings, &["model", "name"])?;
    let base_url = text(settings, &["model", "baseUrl"]);
    let entries: Vec<&Value> = settings?
        .get("modelProviders")?
        .as_object()?
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .filter(|entry| text(Some(entry), &["id"]) == Some(model))
        .collect();
    let entry = entries
        .iter()
        .find(|entry| base_url.is_some() && text(Some(entry), &["baseUrl"]) == base_url)
        .or(entries.first())?;
    text(Some(entry), &["envKey"])
}

/// The variable a sign-in method reads its key from when no model entry names one.
fn default_key_variable(method: &str) -> Option<&'static str> {
    match method {
        "openai" | "openai-responses" => Some("OPENAI_API_KEY"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "gemini" => Some("GEMINI_API_KEY"),
        "vertex-ai" => Some("GOOGLE_API_KEY"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::adapter::provider;
    use crate::test_support::ScratchDir;

    fn settings(auth: Value) -> Value {
        json!({ "security": { "auth": auth }, "ui": { "theme": "dark" } })
    }

    /// [`sign_in_reading`] in a shell that holds no variable.
    fn reading(settings: Option<&Value>, credentials: Option<&Value>) -> SignInReading {
        sign_in_reading(settings, credentials, &HashMap::new(), &|_| false)
    }

    fn signed_in() -> SignInReading {
        SignInReading::SignedIn { email: None }
    }

    /// The environment of a session whose system settings and system defaults files are in
    /// `system`, a scratch directory of its own, so that no reading looks at the machine's.
    fn shell_in(system: &Path) -> HashMap<String, String> {
        HashMap::from([
            (
                "QWEN_CODE_SYSTEM_SETTINGS_PATH".to_string(),
                system
                    .join("system-settings.json")
                    .to_string_lossy()
                    .into_owned(),
            ),
            (
                "QWEN_CODE_SYSTEM_DEFAULTS_PATH".to_string(),
                system
                    .join("system-defaults.json")
                    .to_string_lossy()
                    .into_owned(),
            ),
        ])
    }

    /// An account's directory holding `files` (their text as it is on disk) and the system layers
    /// `system` holds (`system-settings.json` and `system-defaults.json`, by that name), read
    /// through the provider as a session reads them.
    fn state_of(files: &[(&str, &str)], system: &[(&str, &str)]) -> SignInReading {
        let dir = ScratchDir::new("qwen-account");
        let layers = ScratchDir::new("qwen-system");
        for (name, content) in files {
            std::fs::write(dir.join(name), content).expect("account file");
        }
        for (name, content) in system {
            std::fs::write(layers.join(name), content).expect("system file");
        }
        provider(AgentId::QWEN)
            .accounts()
            .expect("Qwen Code has accounts")
            .sign_in_state(Some(&dir), &shell_in(&layers))
    }

    /// An account's directory holding `files`, with no system layer.
    fn reading_of(files: &[(&str, Value)]) -> SignInReading {
        let texts: Vec<(&str, String)> = files
            .iter()
            .map(|(name, content)| (*name, content.to_string()))
            .collect();
        let files: Vec<(&str, &str)> = texts
            .iter()
            .map(|(name, content)| (*name, content.as_str()))
            .collect();
        state_of(&files, &[])
    }

    /// A key method whose key is in a `.env` file of the account's directory, of `~/.qwen` or of
    /// the home directory reads signed in, by the variable the chosen model's entry names or the
    /// method's own; with the variable in none of them it still cannot be told.
    #[test]
    fn a_key_in_an_env_file_of_the_account_or_the_home_signs_it_in() {
        let named = json!({
            "security": { "auth": { "selectedType": "anthropic" } },
            "model": { "name": "m1", "baseUrl": "https://example.test" },
            "modelProviders": { "anthropic": [
                { "id": "m1", "baseUrl": "https://example.test", "envKey": "QWEN_CUSTOM_KEY_1" }
            ] },
        });
        let default = json!({ "security": { "auth": { "selectedType": "openai" } } });
        for (settings, variable) in [(&named, "QWEN_CUSTOM_KEY_1"), (&default, "OPENAI_API_KEY")] {
            // 0: the account's directory, 1: `~/.qwen`, 2: the home directory.
            for place in 0..3 {
                let account = ScratchDir::new("qwen-env-account");
                let home = ScratchDir::new("qwen-env-home");
                let layers = ScratchDir::new("qwen-env-system");
                std::fs::write(account.join("settings.json"), settings.to_string())
                    .expect("settings");
                let mut env = shell_in(&layers);
                env.insert("HOME".to_string(), home.to_string_lossy().into_owned());
                let state = || {
                    provider(AgentId::QWEN)
                        .accounts()
                        .expect("accounts")
                        .sign_in_state(Some(&account), &env)
                };
                assert_eq!(state(), SignInReading::CannotTell, "{variable} before");

                let file = match place {
                    0 => account.join(".env"),
                    1 => home.join(".qwen").join(".env"),
                    _ => home.join(".env"),
                };
                std::fs::create_dir_all(file.parent().expect("a parent")).expect("directory");
                // Another variable signs nothing in.
                std::fs::write(&file, "OTHER_KEY=sk-fake\n").expect("file");
                assert_eq!(state(), SignInReading::CannotTell, "{variable} other");
                std::fs::write(&file, format!("# keys\nOTHER=1\n{variable}=sk-fake\n"))
                    .expect("file");
                assert_eq!(state(), signed_in(), "{variable} in place {place}");
            }
        }
    }

    /// A `.env` file signs nothing in for an account that chose no method, a subscription or the
    /// discontinued method.
    #[test]
    fn an_env_file_does_not_sign_in_an_account_that_reads_no_key() {
        for auth in [
            json!({}),
            json!({ "selectedType": "qwen-subscription" }),
            json!({ "selectedType": "qwen-oauth" }),
        ] {
            let account = ScratchDir::new("qwen-env-no-key");
            let layers = ScratchDir::new("qwen-env-no-key-system");
            std::fs::write(
                account.join("settings.json"),
                settings(auth.clone()).to_string(),
            )
            .expect("settings");
            std::fs::write(
                account.join(".env"),
                "OPENAI_API_KEY=sk-fake\nANTHROPIC_API_KEY=sk-fake\n",
            )
            .expect("file");
            let reading = provider(AgentId::QWEN)
                .accounts()
                .expect("accounts")
                .sign_in_state(Some(&account), &shell_in(&layers));
            assert_eq!(reading, SignInReading::SignedOut, "{auth}");
        }
    }

    /// A line of a `.env` file sets the variable as the `dotenv` parser reads it.
    #[test]
    fn an_env_file_line_sets_a_variable_as_dotenv_reads_it() {
        for line in [
            "KEY=value",
            "  KEY = value",
            "export KEY=value",
            "KEY: value",
            "KEY=\"quoted value\"",
            "KEY='quoted'",
            "KEY=`quoted`",
            "KEY=value # a comment",
            "KEY=\"# not a comment\"",
            // A quote behind a backslash does not close the value, and a quoted value may start
            // on the line after the `=` and run over line ends.
            "KEY='it\\'s'",
            "KEY=\n\"value\"",
            "KEY=\"line one\nline two\" # a comment",
            // Text after the closing quote makes the whole a bare value.
            "KEY='a' and more",
            // The last line that sets it decides.
            "KEY=\nKEY=value",
        ] {
            assert!(env_file_sets(line, "KEY"), "{line:?}");
            assert!(
                env_file_sets(&format!("A=1\r\n{line}\r\nB=2"), "KEY"),
                "{line:?}"
            );
        }
        for line in [
            "",
            "KEY",
            "KEY=",
            "KEY=   ",
            "KEY=\"\"",
            "KEY=''",
            "KEY= # only a comment",
            "KEY=#comment",
            "# KEY=value",
            "KEYS=value",
            "MY_KEY=value",
            "exportKEY=value",
            // `:` needs white space after it.
            "KEY:value",
            // A bare value stops at any `#`.
            "KEY=#value",
            "KEY= ## value",
            // The last line that sets it decides.
            "KEY=value\nKEY=",
            "KEY=value\nKEY=''",
            // A line inside another variable's quoted value sets nothing.
            "OTHER=\"first\nKEY=value\"",
        ] {
            assert!(!env_file_sets(line, "KEY"), "{line:?}");
        }
    }

    /// Nothing on disk: no method was ever chosen, so the account is signed out.
    #[test]
    fn an_account_with_no_files_is_signed_out() {
        assert_eq!(reading_of(&[]), SignInReading::SignedOut);
        assert_eq!(reading(None, None), SignInReading::SignedOut);
    }

    #[test]
    fn settings_that_choose_no_method_leave_the_account_signed_out() {
        for settings in [
            json!({}),
            json!({ "security": {} }),
            json!({ "security": { "auth": {} } }),
            json!([]),
        ] {
            assert_eq!(
                reading(Some(&settings), None),
                SignInReading::SignedOut,
                "{settings}"
            );
        }
        // An empty or non-text method is none.
        for chosen in [json!(""), json!(7), json!(null)] {
            assert_eq!(
                reading(Some(&settings(json!({ "selectedType": chosen }))), None),
                SignInReading::SignedOut,
                "{chosen}"
            );
        }
    }

    /// A Qwen subscription is signed in exactly when its credentials hold an access token.
    #[test]
    fn a_subscription_is_signed_in_by_the_access_token_in_its_credentials() {
        let chosen = settings(json!({ "selectedType": "qwen-subscription" }));
        let tokens =
            json!({ "tokens": { "access_token": "t0ken" }, "baseUrl": "https://example.test" });
        assert_eq!(reading(Some(&chosen), Some(&tokens)), signed_in());
        assert_eq!(
            reading_of(&[
                ("settings.json", chosen.clone()),
                ("subscription_credentials.json", tokens)
            ]),
            signed_in()
        );

        for credentials in [
            None,
            Some(json!({})),
            Some(json!({ "tokens": {} })),
            Some(json!({ "tokens": { "access_token": "" } })),
            Some(json!({ "tokens": { "access_token": 3 } })),
            Some(json!({ "access_token": "t0ken" })),
        ] {
            assert_eq!(
                reading(Some(&chosen), credentials.as_ref()),
                SignInReading::SignedOut,
                "{credentials:?}"
            );
        }
        // A token on disk without the method chosen is no sign-in.
        assert_eq!(
            reading_of(&[(
                "subscription_credentials.json",
                json!({ "tokens": { "access_token": "t0ken" } })
            )]),
            SignInReading::SignedOut
        );
    }

    /// `qwen-oauth` is discontinued and refused by Qwen Code, tokens or not.
    #[test]
    fn the_discontinued_oauth_method_is_signed_out() {
        let chosen = settings(json!({ "selectedType": "qwen-oauth", "apiKey": "key" }));
        let tokens = json!({ "tokens": { "access_token": "t0ken" } });
        assert_eq!(
            reading(Some(&chosen), Some(&tokens)),
            SignInReading::SignedOut
        );
    }

    /// A key method is signed in when the settings hold the key; the key may come from the
    /// environment instead, which the account's files do not show, so otherwise it cannot tell.
    #[test]
    fn a_key_method_is_signed_in_only_when_the_settings_hold_the_key() {
        for method in ["openai", "anthropic", "gemini"] {
            let with_key = settings(json!({ "selectedType": method, "apiKey": "sk-test" }));
            assert_eq!(reading(Some(&with_key), None), signed_in(), "{method}");
            assert_eq!(
                reading_of(&[("settings.json", with_key)]),
                signed_in(),
                "{method}"
            );

            let without = settings(json!({ "selectedType": method }));
            assert_eq!(
                reading(Some(&without), None),
                SignInReading::CannotTell,
                "{method}"
            );
            assert_eq!(
                reading_of(&[("settings.json", without)]),
                SignInReading::CannotTell,
                "{method}"
            );

            let empty = settings(json!({ "selectedType": method, "apiKey": "" }));
            assert_eq!(
                reading(Some(&empty), None),
                SignInReading::CannotTell,
                "{method}"
            );
        }
        // Subscription tokens do not stand in for a key.
        let tokens = json!({ "tokens": { "access_token": "t0ken" } });
        assert_eq!(
            reading(
                Some(&settings(json!({ "selectedType": "openai" }))),
                Some(&tokens)
            ),
            SignInReading::CannotTell
        );
    }

    /// An account's settings file that is there and cannot be used is not "no method chosen": Qwen
    /// Code reads it or reports it, which is not known here, so it cannot be told. One with
    /// comments or a byte order mark is read as Qwen Code reads it.
    #[test]
    fn a_settings_file_that_cannot_be_used_cannot_be_told_and_one_with_comments_is_read() {
        for content in ["{ not json", "[1, 2]", "", "\"text\""] {
            assert_eq!(
                state_of(&[("settings.json", content)], &[]),
                SignInReading::CannotTell,
                "{content:?}"
            );
        }
        let commented = "\u{FEFF}{\n // chosen in /auth\n \"security\": { \"auth\": { \"selectedType\": \"openai\", \"apiKey\": \"sk-fake\" } }\n}";
        assert_eq!(state_of(&[("settings.json", commented)], &[]), signed_in());
        // A file that is not there is no method chosen, as before.
        assert_eq!(state_of(&[], &[]), SignInReading::SignedOut);
    }

    /// The default account is the directory the shell's `QWEN_HOME` names.
    #[test]
    fn the_default_account_is_the_one_the_shell_names() {
        let dir = ScratchDir::new("qwen-default-account");
        let layers = ScratchDir::new("qwen-default-system");
        std::fs::write(
            dir.join("settings.json"),
            settings(json!({ "selectedType": "openai", "apiKey": "sk-test" })).to_string(),
        )
        .expect("settings");
        let mut env = shell_in(&layers);
        env.insert("QWEN_HOME".to_string(), dir.to_string_lossy().into_owned());
        let reading = provider(AgentId::QWEN)
            .accounts()
            .expect("accounts")
            .sign_in_state(None, &env);
        assert_eq!(reading, signed_in());
    }

    /// The account's file is one layer of four. An account that chose no method itself is signed
    /// out only where neither the system settings file nor the system defaults file says anything
    /// of security (an administrator may choose the method and hold the key there); where either
    /// does, or cannot be used, it cannot be told.
    #[test]
    fn an_account_that_chose_no_method_cannot_be_told_where_a_system_layer_may_have() {
        let nothing_chosen = json!({ "ui": { "theme": "dark" } }).to_string();
        let admin = json!({ "security": { "auth": { "selectedType": "openai" } } }).to_string();
        let unrelated = json!({ "ui": { "theme": "light" } }).to_string();
        for (system, expected) in [
            (vec![], SignInReading::SignedOut),
            (
                vec![("system-settings.json", unrelated.as_str())],
                SignInReading::SignedOut,
            ),
            (
                vec![("system-defaults.json", unrelated.as_str())],
                SignInReading::SignedOut,
            ),
            (
                vec![("system-settings.json", admin.as_str())],
                SignInReading::CannotTell,
            ),
            (
                vec![("system-defaults.json", admin.as_str())],
                SignInReading::CannotTell,
            ),
            (
                vec![("system-settings.json", "{\"security\": {}}")],
                SignInReading::CannotTell,
            ),
            (
                vec![("system-settings.json", "{ not json")],
                SignInReading::CannotTell,
            ),
            (
                vec![("system-defaults.json", "[1]")],
                SignInReading::CannotTell,
            ),
            (
                vec![(
                    "system-settings.json",
                    "// set by the administrator\n{ \"security\": { \"auth\": {} } }",
                )],
                SignInReading::CannotTell,
            ),
        ] {
            let layers: Vec<&str> = system.iter().map(|(name, _)| *name).collect();
            assert_eq!(
                state_of(&[("settings.json", nothing_chosen.as_str())], &system),
                expected,
                "{layers:?}"
            );
            // An account with no settings file at all is the same.
            assert_eq!(state_of(&[], &system), expected, "no file, {layers:?}");
        }
    }

    /// Where the account chose a method itself the system layers change nothing of its reading.
    #[test]
    fn an_account_that_chose_a_method_is_read_as_it_is_whatever_the_system_layers_hold() {
        let signed = settings(json!({ "selectedType": "openai", "apiKey": "sk-fake" })).to_string();
        let unsigned = settings(json!({ "selectedType": "openai" })).to_string();
        let admin = json!({ "security": { "auth": {} } }).to_string();
        for system in [
            vec![],
            vec![("system-settings.json", admin.as_str())],
            vec![("system-defaults.json", "{ not json")],
        ] {
            assert_eq!(
                state_of(&[("settings.json", signed.as_str())], &system),
                signed_in()
            );
            assert_eq!(
                state_of(&[("settings.json", unsigned.as_str())], &system),
                SignInReading::CannotTell
            );
        }
    }

    const FAKE_KEY: &str = "sk-fake-not-a-real-key";
    const CUSTOM_VAR: &str = "QWEN_CUSTOM_API_KEY_HTTPS___EXAMPLE_TEST_0123abcd";
    const BASE_URL: &str = "https://example.test/v1";

    /// An account of the shape `/auth` records for a custom Anthropic provider on 0.28.3: the method
    /// is `anthropic`, there is no `security.auth.apiKey`, the key is in the settings' `env` under a
    /// name `/auth` made up, the model's entry names that variable and `model` is the chosen one.
    fn custom_provider(env: Value) -> Value {
        json!({
            "security": { "auth": { "selectedType": "anthropic" } },
            "env": env,
            "modelProviders": { "anthropic": [{
                "id": "fake-model",
                "name": "Fake Model",
                "baseUrl": BASE_URL,
                "envKey": CUSTOM_VAR,
            }] },
            "model": { "name": "fake-model", "baseUrl": BASE_URL },
        })
    }

    fn with_shell(settings: &Value, variable: &str, value: &str) -> SignInReading {
        sign_in_reading(
            Some(settings),
            None,
            &HashMap::from([(variable.to_string(), value.to_string())]),
            &|_| false,
        )
    }

    #[test]
    fn a_key_the_models_entry_names_in_the_settings_env_is_signed_in() {
        let account = custom_provider(json!({ CUSTOM_VAR: FAKE_KEY }));
        assert_eq!(reading(Some(&account), None), signed_in());
        assert_eq!(reading_of(&[("settings.json", account)]), signed_in());
    }

    /// With the entry naming a variable that is set nowhere the key may still be in a `.env` file,
    /// so it cannot tell, even when the settings hold a key of their own: Qwen Code looks at that
    /// one only when no entry names a variable.
    #[test]
    fn a_variable_set_nowhere_cannot_be_told_even_with_a_key_of_the_settings() {
        for env in [
            json!({}),
            json!({ CUSTOM_VAR: "" }),
            json!({ CUSTOM_VAR: "  \t" }),
            json!({ CUSTOM_VAR: 7 }),
        ] {
            let account = custom_provider(env.clone());
            assert_eq!(
                reading(Some(&account), None),
                SignInReading::CannotTell,
                "{env}"
            );
            assert_eq!(
                reading_of(&[("settings.json", account.clone())]),
                SignInReading::CannotTell,
                "{env}"
            );

            let mut with_key = account;
            with_key["security"]["auth"]["apiKey"] = json!(FAKE_KEY);
            assert_eq!(
                reading(Some(&with_key), None),
                SignInReading::CannotTell,
                "{env}"
            );
        }
    }

    #[test]
    fn a_key_in_the_settings_env_is_trimmed_and_one_in_the_shell_counts() {
        let padded = custom_provider(json!({ CUSTOM_VAR: format!("  {FAKE_KEY}\n") }));
        assert_eq!(reading(Some(&padded), None), signed_in());

        let none_in_settings = custom_provider(json!({}));
        assert_eq!(
            with_shell(&none_in_settings, CUSTOM_VAR, FAKE_KEY),
            signed_in()
        );
        // A value that is empty, or only blanks, is not a key.
        for blank in ["", " \t "] {
            assert_eq!(
                with_shell(&none_in_settings, CUSTOM_VAR, blank),
                SignInReading::CannotTell,
                "{blank:?}"
            );
        }
        // Another variable of the shell's is no help.
        assert_eq!(
            with_shell(&none_in_settings, "ANTHROPIC_API_KEY", FAKE_KEY),
            SignInReading::CannotTell
        );
    }

    /// The entry is looked for under every provider's list, whatever the user called it.
    #[test]
    fn the_models_entry_is_found_under_any_providers_list() {
        let mut account = custom_provider(json!({ CUSTOM_VAR: FAKE_KEY }));
        let entries = account["modelProviders"]["anthropic"].take();
        account["modelProviders"] = json!({ "openai": [], "my-own-provider": entries });
        assert_eq!(reading(Some(&account), None), signed_in());
    }

    /// Where several entries have the model's id the one with its base URL names the variable,
    /// else the first.
    #[test]
    fn the_entry_with_the_models_base_url_wins_among_those_with_its_id() {
        let entry = |base_url: &str, key: &str| json!({ "id": "fake-model", "baseUrl": base_url, "envKey": key });
        let account = |model_base_url: Option<&str>| {
            let mut model = json!({ "name": "fake-model" });
            if let Some(url) = model_base_url {
                model["baseUrl"] = json!(url);
            }
            json!({
                "security": { "auth": { "selectedType": "openai" } },
                "modelProviders": { "openai": [entry("https://a.test", "KEY_A"), entry("https://b.test", "KEY_B")] },
                "model": model,
            })
        };
        let set = |variable: &str| json!({ "env": { variable: FAKE_KEY } });
        let with_env = |mut account: Value, variable: &str| {
            account["env"] = set(variable)["env"].clone();
            account
        };
        // Its base URL picks the second entry, though the first has the id too.
        assert_eq!(
            reading(
                Some(&with_env(account(Some("https://b.test")), "KEY_B")),
                None
            ),
            signed_in()
        );
        assert_eq!(
            reading(
                Some(&with_env(account(Some("https://b.test")), "KEY_A")),
                None
            ),
            SignInReading::CannotTell
        );
        // Without a base URL, or one no entry has, it is the first.
        for url in [None, Some("https://c.test")] {
            assert_eq!(
                reading(Some(&with_env(account(url), "KEY_A")), None),
                signed_in(),
                "{url:?}"
            );
            assert_eq!(
                reading(Some(&with_env(account(url), "KEY_B")), None),
                SignInReading::CannotTell,
                "{url:?}"
            );
        }
    }

    /// An entry of another model names a variable that is none of this model's.
    #[test]
    fn another_models_entry_is_not_this_models() {
        let mut account = custom_provider(json!({ CUSTOM_VAR: FAKE_KEY }));
        account["model"]["name"] = json!("some-other-model");
        // No entry has the id: the method's own variable is the one, and it is not set.
        assert_eq!(reading(Some(&account), None), SignInReading::CannotTell);
        account["env"] = json!({ "ANTHROPIC_API_KEY": FAKE_KEY });
        assert_eq!(reading(Some(&account), None), signed_in());
        // No model chosen at all: the same.
        account["model"] = json!({});
        assert_eq!(reading(Some(&account), None), signed_in());
    }

    /// With no entry naming a variable, the method's own is read, from the settings' `env` or from
    /// the shell, and only the one of that method.
    #[test]
    fn with_no_entry_the_methods_own_variable_is_read() {
        for (method, variable) in [
            ("openai", "OPENAI_API_KEY"),
            ("openai-responses", "OPENAI_API_KEY"),
            ("anthropic", "ANTHROPIC_API_KEY"),
            ("gemini", "GEMINI_API_KEY"),
            ("vertex-ai", "GOOGLE_API_KEY"),
        ] {
            let bare = settings(json!({ "selectedType": method }));
            assert_eq!(
                reading(Some(&bare), None),
                SignInReading::CannotTell,
                "{method}"
            );
            assert_eq!(
                with_shell(&bare, variable, FAKE_KEY),
                signed_in(),
                "{method}"
            );

            let mut in_env = bare.clone();
            in_env["env"] = json!({ variable: FAKE_KEY });
            assert_eq!(reading(Some(&in_env), None), signed_in(), "{method}");
            assert_eq!(
                reading_of(&[("settings.json", in_env)]),
                signed_in(),
                "{method}"
            );

            // A blank value is none, and another method's variable is not this one's.
            let mut blank = bare.clone();
            blank["env"] = json!({ variable: "  " });
            assert_eq!(
                reading(Some(&blank), None),
                SignInReading::CannotTell,
                "{method}"
            );
            let other = if variable == "OPENAI_API_KEY" {
                "GEMINI_API_KEY"
            } else {
                "OPENAI_API_KEY"
            };
            assert_eq!(
                with_shell(&bare, other, FAKE_KEY),
                SignInReading::CannotTell,
                "{method}"
            );
        }
    }

    /// A method that names no variable of its own signs in by the settings' key alone, and cannot
    /// be told otherwise.
    #[test]
    fn an_unknown_method_has_no_variable_and_signs_in_by_the_settings_key() {
        let mut account = settings(json!({ "selectedType": "custom-method" }));
        account["env"] = json!({ "OPENAI_API_KEY": FAKE_KEY, "ANTHROPIC_API_KEY": FAKE_KEY });
        assert_eq!(
            with_shell(&account, "GEMINI_API_KEY", FAKE_KEY),
            SignInReading::CannotTell
        );
        account["security"]["auth"]["apiKey"] = json!(FAKE_KEY);
        assert_eq!(reading(Some(&account), None), signed_in());
    }

    /// The settings' key still signs in a method that has no entry naming a variable, whatever the
    /// method's own variable holds.
    #[test]
    fn the_settings_key_signs_in_when_no_entry_names_a_variable() {
        let mut account = settings(json!({ "selectedType": "anthropic", "apiKey": FAKE_KEY }));
        assert_eq!(reading(Some(&account), None), signed_in());
        // Entries of other models name variables, but not this model's.
        account["modelProviders"] =
            json!({ "anthropic": [{ "id": "other", "envKey": CUSTOM_VAR }] });
        account["model"] = json!({ "name": "fake-model" });
        assert_eq!(reading(Some(&account), None), signed_in());
    }

    /// A subscription's tokens that are there and cannot be read are not tokens that are missing:
    /// the account cannot be told. Missing ones are signed out, as before, and readable ones say
    /// what they hold.
    #[test]
    fn a_subscription_whose_credentials_cannot_be_read_cannot_be_told() {
        let chosen = settings(json!({ "selectedType": "qwen-subscription" })).to_string();
        for credentials in ["{ not json", "[1, 2]", "", "\"text\""] {
            assert_eq!(
                state_of(
                    &[
                        ("settings.json", chosen.as_str()),
                        ("subscription_credentials.json", credentials)
                    ],
                    &[]
                ),
                SignInReading::CannotTell,
                "{credentials:?}"
            );
        }
        assert_eq!(
            state_of(&[("settings.json", chosen.as_str())], &[]),
            SignInReading::SignedOut,
            "missing"
        );
        assert_eq!(
            state_of(
                &[
                    ("settings.json", chosen.as_str()),
                    (
                        "subscription_credentials.json",
                        "// refreshed\n{\"tokens\": {\"access_token\": \"t0ken\"}}"
                    ),
                ],
                &[]
            ),
            signed_in(),
            "readable, with a comment"
        );
        assert_eq!(
            state_of(
                &[
                    ("settings.json", chosen.as_str()),
                    ("subscription_credentials.json", "{\"tokens\": {}}"),
                ],
                &[]
            ),
            SignInReading::SignedOut,
            "readable, no token"
        );
    }

    /// Under any other method, or none, tokens that cannot be read change nothing.
    #[test]
    fn credentials_that_cannot_be_read_change_nothing_for_another_method() {
        let signed = settings(json!({ "selectedType": "openai", "apiKey": "sk-fake" })).to_string();
        let unsigned = settings(json!({ "selectedType": "openai" })).to_string();
        let oauth = settings(json!({ "selectedType": "qwen-oauth" })).to_string();
        let none = json!({ "ui": {} }).to_string();
        for (account, expected) in [
            (signed, signed_in()),
            (unsigned, SignInReading::CannotTell),
            (oauth, SignInReading::SignedOut),
            (none, SignInReading::SignedOut),
        ] {
            assert_eq!(
                state_of(
                    &[
                        ("settings.json", account.as_str()),
                        ("subscription_credentials.json", "{ not json")
                    ],
                    &[]
                ),
                expected,
                "{account}"
            );
        }
    }

    /// The system layers are read within the size the launch reads them in, so a system file the
    /// launch would refuse as unusable (here, over that size) is one this cannot tell either, where
    /// it would be read and found to say nothing of security if the limit were larger.
    #[test]
    fn a_system_file_over_the_launchs_size_is_unusable_here_too() {
        let padding = "x".repeat(SYSTEM_FILE_LIMIT + 1024);
        let big = json!({ "ui": { "theme": "dark" }, "padding": padding }).to_string();
        assert!(big.len() > SYSTEM_FILE_LIMIT);
        let nothing_chosen = json!({ "ui": {} }).to_string();
        for name in ["system-settings.json", "system-defaults.json"] {
            assert_eq!(
                state_of(
                    &[("settings.json", nothing_chosen.as_str())],
                    &[(name, big.as_str())]
                ),
                SignInReading::CannotTell,
                "{name}"
            );
        }
    }
}
