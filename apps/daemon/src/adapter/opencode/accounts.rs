//! What OpenCode's one account can be asked: whether it is signed in. One installation holds
//! credentials for many model providers at once, in `auth.json` under its data directory, and
//! also takes a provider's key from the environment; either is a sign-in. There is no login flow
//! (a key is entered at the CLI's own prompt, `opencode auth login`) and no plan quota.
//!
//! It is never read as signed out. OpenCode works with neither: on the vendor's free models, on a
//! provider whose key is written in its configuration file, on a local or compatible provider. A
//! reading of signed out would refuse every new session on the only account OpenCode has, so an
//! account with no credential found is one this cannot tell about.
//!
//! The file is read first, since it needs no process. Only an account with no credential there is
//! asked through `opencode auth list`, which also names the keys the environment supplies.
//! Measured on 1.19.6; the output is in `docs/agent-cli-reference.md`, section "OpenCode".

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::accounts::quotas::email::read_json;
use crate::accounts::quotas::sign_in::SignInReading;
use crate::accounts::AccountOps;
use crate::adapter::{account_cli, account_dir};
use crate::protocol::AgentId;
use crate::terminal::plain_text::strip_escapes;

/// The credential record in the data directory: an object with an entry per model provider.
const SIGN_IN_FILE: &str = "auth.json";
/// How long `opencode auth list` may take, and the most of its output that is read.
const AUTH_LIST_TIMEOUT: Duration = Duration::from_secs(20);
const AUTH_LIST_LIMIT: usize = 256 * 1024;

pub(super) struct OpenCodeAccounts;

impl AccountOps for OpenCodeAccounts {
    fn mcp_inventory(
        &self,
    ) -> Option<&'static dyn crate::accounts::mcp_inventory::McpInventoryReader> {
        Some(&super::mcp_inventory::OpenCodeMcpInventory)
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
        let Some(v2) = super::version::background_v2(shell_env) else {
            return SignInReading::CannotTell;
        };
        if !v2 {
            let dir = account_dir(AgentId::OPENCODE, config_dir, shell_env);
            let stored = read_json(&dir.join(SIGN_IN_FILE))
                .as_ref()
                .and_then(Value::as_object)
                .is_some_and(|credentials| !credentials.is_empty());
            if stored {
                return SignInReading::SignedIn { email: None };
            }
        }
        let args = if v2 {
            &["auth", "list", "--standalone", "--format", "json"][..]
        } else {
            &["auth", "list"][..]
        };
        match account_cli::query(
            AgentId::OPENCODE,
            shell_env,
            config_dir,
            args,
            AUTH_LIST_TIMEOUT,
            AUTH_LIST_LIMIT,
            None,
        ) {
            Ok(output) if output.status.success() && v2 => {
                let connected = serde_json::from_slice::<Vec<Value>>(&output.stdout).is_ok_and(
                    |integrations| {
                        integrations.iter().any(|entry| {
                            entry["connections"]
                                .as_array()
                                .is_some_and(|items| !items.is_empty())
                        })
                    },
                );
                if connected {
                    SignInReading::SignedIn { email: None }
                } else {
                    SignInReading::CannotTell
                }
            }
            Ok(output) if output.status.success() => {
                // Which stream carries the listing is not fixed, so both are read.
                let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
                text.push('\n');
                text.push_str(&String::from_utf8_lossy(&output.stderr));
                read_auth_list(&text)
            }
            _ => SignInReading::CannotTell,
        }
    }
}

/// What `opencode auth list` says: it ends its stored credentials with `N credentials` and, when
/// the environment supplies any key, lists those under `N environment variable(s)`. Signed in
/// with either above zero. With both at zero, or an output without the credentials line, it
/// cannot be told: OpenCode may still have a model to work with (see the module's note).
fn read_auth_list(output: &str) -> SignInReading {
    let text = strip_escapes(output);
    let stored = count_before(&text, "credential").unwrap_or(0);
    let from_environment = count_before(&text, "environment").unwrap_or(0);
    if stored + from_environment > 0 {
        SignInReading::SignedIn { email: None }
    } else {
        SignInReading::CannotTell
    }
}

/// The number written right before a word starting with `noun`, on the first line that has one.
fn count_before(text: &str, noun: &str) -> Option<u32> {
    text.lines().find_map(|line| {
        let words: Vec<&str> = line.split_whitespace().collect();
        words.windows(2).find_map(|pair| {
            pair[1]
                .starts_with(noun)
                .then(|| pair[0].parse().ok())
                .flatten()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, ScratchDir};

    /// What `opencode auth list` printed on 1.19.6 (`testdata/opencode-captures.md`), standard
    /// output, with its colour escapes as they came down a pipe.
    const SIGNED_OUT: &str =
        include_str!("../../../testdata/opencode_auth_list_signed_out_1.19.6.txt");
    const ENVIRONMENT: &str =
        include_str!("../../../testdata/opencode_auth_list_environment_1.19.6.txt");
    const STORED: &str = include_str!("../../../testdata/opencode_auth_list_stored_1.19.6.txt");
    const STORED_AND_ENVIRONMENT: &str =
        include_str!("../../../testdata/opencode_auth_list_stored_and_environment_1.19.6.txt");
    /// What it printed on standard error in every one of those runs: a lone reset escape.
    const STDERR: &str = "\u{1b}[0m\n";

    const SIGNED_IN: SignInReading = SignInReading::SignedIn { email: None };
    const CREDENTIAL: &str = r#"{"anthropic":{"type":"api","key":"sk-test"}}"#;

    #[test]
    fn the_captures_carry_the_colour_escapes_the_reading_has_to_look_past() {
        for capture in [SIGNED_OUT, ENVIRONMENT, STORED, STORED_AND_ENVIRONMENT] {
            assert!(capture.contains('\u{1b}'), "{capture:?}");
        }
    }

    /// Nothing stored and nothing in the environment is not signed out: OpenCode works on the
    /// vendor's free models, on a key written in its configuration file and on a local provider,
    /// so a reading of signed out would refuse every session. It cannot be told.
    #[test]
    fn what_the_cli_printed_with_no_credential_cannot_be_told() {
        assert_eq!(read_auth_list(SIGNED_OUT), SignInReading::CannotTell);
        assert_eq!(
            count_before(&strip_escapes(SIGNED_OUT), "credential"),
            Some(0)
        );
        assert_eq!(
            count_before(&strip_escapes(SIGNED_OUT), "environment"),
            None
        );
    }

    #[test]
    fn a_stored_credential_reads_signed_in() {
        assert_eq!(read_auth_list(STORED), SIGNED_IN);
        assert_eq!(count_before(&strip_escapes(STORED), "credential"), Some(1));
        assert_eq!(count_before(&strip_escapes(STORED), "environment"), None);
    }

    /// A key the environment supplies is a sign-in, with no credential stored.
    #[test]
    fn a_key_from_the_environment_reads_signed_in() {
        assert_eq!(read_auth_list(ENVIRONMENT), SIGNED_IN);
        let text = strip_escapes(ENVIRONMENT);
        assert_eq!(count_before(&text, "credential"), Some(0));
        assert_eq!(count_before(&text, "environment"), Some(1));
    }

    #[test]
    fn credentials_and_environment_keys_are_each_counted() {
        assert_eq!(read_auth_list(STORED_AND_ENVIRONMENT), SIGNED_IN);
        let text = strip_escapes(STORED_AND_ENVIRONMENT);
        assert_eq!(count_before(&text, "credential"), Some(1));
        assert_eq!(count_before(&text, "environment"), Some(2));
    }

    /// Whichever stream carries the listing, the other's lone escape changes nothing.
    #[test]
    fn the_other_streams_escape_is_no_part_of_the_reading() {
        let joined = |stdout: &str| format!("{stdout}\n{STDERR}");
        assert_eq!(
            read_auth_list(&joined(SIGNED_OUT)),
            SignInReading::CannotTell
        );
        assert_eq!(read_auth_list(&joined(STORED)), SIGNED_IN);
        assert_eq!(read_auth_list(&format!("{STDERR}\n{STORED}")), SIGNED_IN);
        assert_eq!(
            read_auth_list(&format!("{STDERR}\n{SIGNED_OUT}")),
            SignInReading::CannotTell
        );
    }

    /// An output without the credentials line is one this does not know, and says nothing of the
    /// account; only an environment count, which is itself a sign-in, is read from it.
    #[test]
    fn an_output_without_the_credentials_line_is_read_for_what_it_holds() {
        for output in ["", STDERR, "Error: something went wrong\n"] {
            assert_eq!(
                read_auth_list(output),
                SignInReading::CannotTell,
                "{output:?}"
            );
        }
        assert_eq!(read_auth_list("  1 environment variable\n"), SIGNED_IN);
        assert_eq!(
            read_auth_list("  0 environment variables\n"),
            SignInReading::CannotTell
        );
    }

    /// Whatever the output, it is never read as signed out.
    #[test]
    fn no_output_is_read_as_signed_out() {
        for output in [
            SIGNED_OUT,
            ENVIRONMENT,
            STORED,
            STORED_AND_ENVIRONMENT,
            "",
            STDERR,
            "0 credentials\n0 environment variables\n",
            "Error: boom\n",
        ] {
            assert_ne!(
                read_auth_list(output),
                SignInReading::SignedOut,
                "{output:?}"
            );
        }
    }

    #[test]
    fn a_count_is_the_number_right_before_the_word() {
        assert_eq!(count_before("└  12 credentials\n", "credential"), Some(12));
        assert_eq!(count_before("0 credentials", "credential"), Some(0));
        assert_eq!(
            count_before("a line\n  2 environment variables\n", "environment"),
            Some(2)
        );
        assert_eq!(count_before("credentials 3", "credential"), None);
        assert_eq!(count_before("many credentials", "credential"), None);
        assert_eq!(count_before("", "credential"), None);
    }

    fn sign_in(shell_env: &HashMap<String, String>, config_dir: Option<&Path>) -> SignInReading {
        OpenCodeAccounts.sign_in_state(config_dir, shell_env)
    }

    /// A shell environment whose default account's directory is `<root>/<dir>/opencode`, named
    /// through `XDG_DATA_HOME`, and which has no `opencode` on its `PATH`.
    fn shell_with_data_home(root: &Path, dir: &str) -> HashMap<String, String> {
        HashMap::from([
            ("HOME".to_string(), root.to_string_lossy().into_owned()),
            (
                "XDG_DATA_HOME".to_string(),
                root.join(dir).to_string_lossy().into_owned(),
            ),
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ])
    }

    fn store(data_home: &Path, content: &str) {
        let dir = data_home.join("opencode");
        std::fs::create_dir_all(&dir).expect("data directory");
        std::fs::write(dir.join("auth.json"), content).expect("auth.json");
    }

    /// A credential in `auth.json` is a sign-in, found without running the CLI (the shell has none),
    /// in the directory `XDG_DATA_HOME` names, or the usual one under the home directory.
    #[test]
    fn a_credential_record_is_read_from_the_default_accounts_directory() {
        let root = ScratchDir::new("opencode-sign-in-file");
        store(&root.join("data"), CREDENTIAL);
        assert_eq!(
            sign_in(&shell_with_data_home(&root, "data"), None),
            SIGNED_IN
        );

        let home = ScratchDir::new("opencode-sign-in-home");
        store(&home.join(".local/share"), CREDENTIAL);
        let shell = HashMap::from([
            ("HOME".to_string(), home.to_string_lossy().into_owned()),
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ]);
        assert_eq!(sign_in(&shell, None), SIGNED_IN);
    }

    /// A pinned directory is read in place of the shell's.
    #[test]
    fn a_pinned_directory_is_read_in_place_of_the_shells() {
        let root = ScratchDir::new("opencode-sign-in-pinned");
        store(&root.join("elsewhere"), CREDENTIAL);
        let pinned = root.join("elsewhere").join("opencode");
        assert_eq!(
            sign_in(&shell_with_data_home(&root, "empty"), Some(&pinned)),
            SIGNED_IN
        );
    }

    /// With no record that holds a credential the CLI is asked: signed in when it counts a
    /// credential or the environment supplies a key, and when it counts none nothing can be told
    /// (never signed out), for each of the real outputs.
    #[test]
    fn without_a_stored_credential_the_cli_is_asked() {
        for (record, output, expected) in [
            (None, SIGNED_OUT, SignInReading::CannotTell),
            (Some("{}"), SIGNED_OUT, SignInReading::CannotTell),
            (Some("not json"), SIGNED_OUT, SignInReading::CannotTell),
            (Some("[1]"), SIGNED_OUT, SignInReading::CannotTell),
            (None, ENVIRONMENT, SIGNED_IN),
            (None, STORED, SIGNED_IN),
            (None, STORED_AND_ENVIRONMENT, SIGNED_IN),
        ] {
            let root = ScratchDir::new("opencode-sign-in-cli");
            if let Some(record) = record {
                store(&root.join("data"), record);
            }
            let capture = root.join("auth-list.txt");
            std::fs::write(&capture, output).expect("capture");
            let mut shell = fake_agent(
                &root,
                "opencode",
                &format!(
                    "[ \"$1 $2\" = 'auth list' ] && cat '{}' && exit 0\nexit 3",
                    capture.display()
                ),
            );
            shell.insert(
                "XDG_DATA_HOME".to_string(),
                root.join("data").to_string_lossy().into_owned(),
            );
            assert_eq!(sign_in(&shell, None), expected, "{record:?} {output:?}");
        }
    }

    /// A CLI that fails, prints nothing it knows, or is not there says nothing about the account.
    #[test]
    fn a_query_that_fails_or_cannot_be_read_cannot_tell() {
        let root = ScratchDir::new("opencode-sign-in-failing");
        for body in ["echo 'boom' >&2\nexit 1", "echo 'something else'", "exit 0"] {
            let shell = fake_agent(&root, "opencode", body);
            assert_eq!(sign_in(&shell, None), SignInReading::CannotTell, "{body}");
        }
        let none = shell_with_data_home(&root, "data");
        assert_eq!(sign_in(&none, None), SignInReading::CannotTell);
    }
}
