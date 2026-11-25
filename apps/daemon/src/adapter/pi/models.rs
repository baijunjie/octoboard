//! Pi's model catalogue and the sign-in reading built on it. `pi --list-models` prints the models of
//! every model provider the account holds a usable credential for (a key or a sign-in in
//! `auth.json`, a key in the environment, a provider of its `models.json`), as a table, and a
//! notice instead of a table when it holds none. An account with stored OAuth uses Pi's
//! provider-specific auth check without refresh for the background sign-in reading.
//! The measurements are in `docs/agent-cli-reference.md`, section "Pi".

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::accounts::quotas::sign_in::SignInReading;
use crate::adapter::account_cli::account_dir;
use crate::adapter::files::{read_regular_text, SMALL_FILE_LIMIT};
use crate::agents::models::{
    failed, levels, run_catalogue_command, unreadable, Listing, ModelCatalogue, COMMAND_TIMEOUT,
};
use crate::protocol::{AgentId, AgentModel};

/// The arguments that list the account's models.
const LIST_MODELS: &[&str] = &["--list-models"];

/// What `--thinking` takes (1.5.2), offered for every model that thinks. Pi lowers a level the
/// model does not support to the highest it does (`max` and `xhigh` to `high` for a model that
/// maps neither), so every level is safe to pass.
const PI_EFFORTS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

/// The line Pi prints in place of the table when the account holds no usable credential.
const NO_MODELS: &str = "No models available.";

/// Pi's models, asked of its CLI.
pub(super) struct PiCatalogue;

impl ModelCatalogue for PiCatalogue {
    fn list(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _run_homes: &Path,
        timeout: Duration,
    ) -> Listing {
        run_catalogue_command(AgentId::PI, shell_env, config_dir, LIST_MODELS, timeout)
            .and_then(|stdout| match parse(&stdout) {
                Parsed::Models(models) => Ok(models),
                // The listing failed, with Pi's own notice as the detail, as the detail of any
                // other failed listing is the CLI's own words.
                Parsed::NoCredential(notice) => Err(failed(AgentId::PI, &notice)),
                Parsed::Unreadable => Err(unreadable(AgentId::PI)),
            })
            .map_or_else(Listing::unavailable, Listing::found)
    }
}

/// A readiness check for stored OAuth accounts avoids refreshing their credentials. For an
/// account with no stored OAuth, the model list also covers keys supplied by the environment
/// or a custom provider. Pi keeps no email for a credential.
pub(super) fn sign_in_state(
    config_dir: Option<&Path>,
    shell_env: &HashMap<String, String>,
) -> SignInReading {
    // The catalogue resolves OAuth with refresh enabled. A sign-in poll only checks whether the
    // stored providers are currently usable and leaves a credential Pi could refresh undecided.
    match stored_auth(config_dir, shell_env) {
        Ok(Some(providers)) if providers.values().any(|record| record.kind == "oauth") => {
            return checked_sign_in(config_dir, shell_env, &providers);
        }
        Err(_) => return SignInReading::CannotTell,
        _ => {}
    }
    let listed = run_catalogue_command(
        AgentId::PI,
        shell_env,
        config_dir,
        LIST_MODELS,
        COMMAND_TIMEOUT,
    );
    match listed.as_deref().map(parse) {
        Ok(Parsed::Models(_)) => SignInReading::SignedIn { email: None },
        Ok(Parsed::NoCredential(_)) => SignInReading::SignedOut,
        Ok(Parsed::Unreadable) | Err(_) => SignInReading::CannotTell,
    }
}

#[derive(Deserialize)]
struct StoredAuth {
    #[serde(rename = "type")]
    kind: String,
}

fn stored_auth(
    config_dir: Option<&Path>,
    shell_env: &HashMap<String, String>,
) -> anyhow::Result<Option<HashMap<String, StoredAuth>>> {
    let path = account_dir(AgentId::PI, config_dir, shell_env).join("auth.json");
    match read_regular_text(&path, SMALL_FILE_LIMIT) {
        Ok(text) => Ok(Some(serde_json::from_str(
            text.trim_start_matches('\u{feff}'),
        )?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn checked_sign_in(
    config_dir: Option<&Path>,
    shell_env: &HashMap<String, String>,
    providers: &HashMap<String, StoredAuth>,
) -> SignInReading {
    #[derive(Deserialize)]
    struct Check {
        status: String,
        provider: String,
    }

    let deadline = Instant::now() + COMMAND_TIMEOUT;
    for (provider, record) in providers {
        if !matches!(record.kind.as_str(), "api_key" | "oauth")
            || provider.is_empty()
            || !provider
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            continue;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let result = run_catalogue_command(
            AgentId::PI,
            shell_env,
            config_dir,
            &[
                "auth",
                "check",
                "--provider",
                provider,
                "--json",
                "--no-refresh",
            ],
            left,
        );
        if result
            .ok()
            .and_then(|stdout| serde_json::from_str::<Check>(&stdout).ok())
            .is_some_and(|check| check.status == "ready" && check.provider == provider.as_str())
        {
            return SignInReading::SignedIn { email: None };
        }
    }
    SignInReading::CannotTell
}

/// What `pi --list-models` printed.
#[derive(Debug, PartialEq)]
pub(super) enum Parsed {
    /// The models of its table, in the order printed (by provider, then by id).
    Models(Vec<AgentModel>),
    /// Its notice that no model is available, which it prints for an account with no usable
    /// credential: the notice's first line.
    NoCredential(String),
    /// Neither: no table with a row that reads, and no notice.
    Unreadable,
}

/// Reads `pi --list-models`' output: a header line naming the columns `provider`, `model` and
/// `thinking` among others, then a row a model, its columns separated by runs of spaces; or the
/// notice [`NO_MODELS`] in place of all of it. A model's id is `provider/model`, which `--model`
/// takes; its efforts are [`PI_EFFORTS`] where its `thinking` column says `yes`, none otherwise.
/// Lines before the header that are neither are passed over, whatever they say, and a row that
/// does not read is skipped.
pub(super) fn parse(stdout: &str) -> Parsed {
    let mut lines = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let mut header = None;
    for line in lines.by_ref() {
        if line.starts_with(NO_MODELS) {
            return Parsed::NoCredential(line.to_string());
        }
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.first() == Some(&"provider") && columns.get(1) == Some(&"model") {
            header = Some(columns);
            break;
        }
    }
    let Some(header) = header else {
        return Parsed::Unreadable;
    };
    let Some(thinking) = header.iter().position(|column| *column == "thinking") else {
        return Parsed::Unreadable;
    };
    let models: Vec<AgentModel> = lines
        .filter_map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            if columns.len() != header.len() {
                return None;
            }
            let id = format!("{}/{}", columns[0], columns[1]);
            let efforts = match columns[thinking] {
                "yes" => levels(&PI_EFFORTS),
                "no" => Vec::new(),
                _ => return None,
            };
            Some(AgentModel {
                name: id.clone(),
                id,
                efforts,
                version: None,
                available: Some(true),
            })
        })
        .collect();
    if models.is_empty() {
        Parsed::Unreadable
    } else {
        Parsed::Models(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::models::{list, COMMAND_TIMEOUT};
    use crate::protocol::models_unavailable_code as code;
    use crate::test_support::{fake_agent, ScratchDir};

    // What `pi --list-models` printed on Pi 1.5.2 (`testdata/pi-captures.md`): an account whose
    // `models.json` holds three models, one of which thinks, and an account with no credential.
    const TABLE: &str = include_str!("../../../testdata/pi_models.txt");
    const SIGNED_OUT: &str = include_str!("../../../testdata/pi_models_signed_out.txt");
    const NOTICE: &str =
        "No models available. Use /login to log into a provider via OAuth or API key. See:";

    /// Every value `--thinking` takes, in Pi's own order.
    const EVERY_LEVEL: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

    fn models(parsed: Parsed) -> Vec<AgentModel> {
        match parsed {
            Parsed::Models(models) => models,
            other => panic!("no models: {other:?}"),
        }
    }

    fn ids(models: &[AgentModel]) -> Vec<&str> {
        models.iter().map(|model| model.id.as_str()).collect()
    }

    fn header() -> &'static str {
        TABLE.lines().next().expect("the header")
    }

    /// A model is passed to `--model` as `provider/model`, so that is its id and its name, in the
    /// order printed; the levels are offered for a model whose `thinking` column says `yes` and
    /// none for the others.
    #[test]
    fn the_table_lists_each_model_by_provider_and_id_with_levels_where_it_thinks() {
        let models = models(parse(TABLE));
        assert_eq!(
            ids(&models),
            ["another/plain", "local/deep-think", "local/small-chat"]
        );
        for model in &models {
            assert_eq!(model.name, model.id);
            assert_eq!(model.available, Some(true));
            assert_eq!(model.version, None);
        }
        assert!(models[0].efforts.is_empty());
        assert_eq!(models[1].efforts, EVERY_LEVEL.map(String::from));
        assert!(models[2].efforts.is_empty());
    }

    /// With no credential Pi prints a notice in place of a table, and the first line of it is what
    /// is kept.
    #[test]
    fn the_no_models_notice_is_not_a_table() {
        assert_eq!(parse(SIGNED_OUT), Parsed::NoCredential(NOTICE.to_string()));
        assert_eq!(
            parse(&format!("\n  {NOTICE}\n  /docs/models.md\n")),
            Parsed::NoCredential(NOTICE.to_string())
        );
    }

    /// Lines before the header that are not recognised are passed over.
    #[test]
    fn unrecognised_lines_before_the_header_are_passed_over() {
        let led = format!("a line of something else\nand another\n{TABLE}");
        assert_eq!(parse(&led), parse(TABLE));
    }

    /// A row that does not read is skipped and the rest are kept: one short of columns, one with
    /// extra, and one whose `thinking` is neither `yes` nor `no`.
    #[test]
    fn a_row_that_does_not_read_is_skipped() {
        let text = format!(
            "{}\nshort  row\nlocal  ok  1K  1K  no  no\nlocal  extra  1K  1K  no  no  yes\n\
             local  odd  1K  1K  maybe  no\nlocal  think  1K  1K  yes  no\n",
            header()
        );
        assert_eq!(ids(&models(parse(&text))), ["local/ok", "local/think"]);
    }

    /// A header with no row that reads, no header, a header without the `thinking` column and no
    /// output at all are not a listing.
    #[test]
    fn output_with_no_readable_model_is_unreadable() {
        for text in [
            "",
            "\n\n",
            "garbage that is no table\n",
            header(),
            &format!("{}\nshort row\n", header()),
            "provider  model  context\nlocal  m  1K\n",
        ] {
            assert_eq!(parse(text), Parsed::Unreadable, "{text:?}");
        }
    }

    /// Runs the catalogue of an account under `dir` against a `pi` that prints `output`, only when
    /// it is given `--list-models` and the account's directory in `PI_CODING_AGENT_DIR`.
    fn listed(dir: &ScratchDir, output: &str, status: u8) -> Listing {
        let account = dir.join("account");
        std::fs::create_dir_all(&account).unwrap();
        let printed = dir.join("printed.txt");
        std::fs::write(&printed, output).unwrap();
        let env = fake_agent(dir, "pi", &script(&account, &printed, status));
        list(AgentId::PI, &env, Some(&account), COMMAND_TIMEOUT)
    }

    fn script(account: &Path, printed: &Path, status: u8) -> String {
        format!(
            "[ \"$1\" = --list-models ] && [ \"$#\" = 1 ] && \
             [ \"$PI_CODING_AGENT_DIR\" = '{}' ] || exit 64; cat '{}'; exit {status}",
            account.display(),
            printed.display()
        )
    }

    #[test]
    fn an_oauth_account_is_checked_without_refreshing_its_credential() {
        let dir = ScratchDir::new("pi-oauth-sign-in");
        std::fs::write(dir.join("auth.json"), r#"{"xai":{"type":"oauth"}}"#).unwrap();
        let env = fake_agent(
            &dir,
            "pi",
            r#"[ "$*" = "auth check --provider xai --json --no-refresh" ] || exit 64
printf '%s\n' '{"status":"ready","provider":"xai","authType":"oauth"}'"#,
        );
        assert!(matches!(
            sign_in_state(Some(&dir), &env),
            SignInReading::SignedIn { email: None }
        ));
    }

    #[test]
    fn the_catalogue_runs_pi_as_the_account_and_lists_what_it_prints() {
        let dir = ScratchDir::new("pi-models-listed");
        let listing = listed(&dir, TABLE, 0);
        assert_eq!(listing.unavailable, None);
        assert_eq!(
            ids(&listing.models),
            ["another/plain", "local/deep-think", "local/small-chat"]
        );
    }

    /// An account with no credential lists no models, under the code of a failed listing with
    /// Pi's own notice as the detail, not under a code of Pi's own.
    #[test]
    fn an_account_with_no_credential_lists_no_models_and_says_why_in_pis_words() {
        let dir = ScratchDir::new("pi-models-signed-out");
        let listing = listed(&dir, SIGNED_OUT, 0);
        assert!(listing.models.is_empty());
        let reason = listing.unavailable.expect("a reason");
        assert_eq!(reason.code, code::COMMAND_FAILED);
        assert_eq!(reason.params["agent"], "Pi");
        assert_eq!(reason.params["detail"], NOTICE);
        assert!(reason.message.contains(NOTICE), "{}", reason.message);
    }

    #[test]
    fn output_nobody_can_read_and_a_failed_command_are_told_apart() {
        let dir = ScratchDir::new("pi-models-unreadable");
        let garbage = listed(&dir, "garbage\n", 0);
        assert!(garbage.models.is_empty());
        assert_eq!(
            garbage.unavailable.expect("a reason").code,
            code::UNREADABLE_OUTPUT
        );

        let failed = listed(&dir, "boom: it broke\n", 1);
        assert!(failed.models.is_empty());
        let reason = failed.unavailable.expect("a reason");
        assert_eq!(reason.code, code::COMMAND_FAILED);
        assert!(
            reason.params["detail"].contains("boom: it broke"),
            "{reason:?}"
        );
    }

    /// The reading `pi --list-models` gives of the account, through the provider's account
    /// operations, with the same fake `pi` as the catalogue's.
    fn reading(dir: &ScratchDir, output: &str, status: u8) -> SignInReading {
        let account = dir.join("account");
        std::fs::create_dir_all(&account).unwrap();
        let printed = dir.join("printed.txt");
        std::fs::write(&printed, output).unwrap();
        let env = fake_agent(dir, "pi", &script(&account, &printed, status));
        crate::adapter::provider(AgentId::PI)
            .accounts()
            .expect("Pi has accounts")
            .sign_in_state(Some(&account), &env)
    }

    /// Signed in when Pi lists a model, signed out at its notice, and unable to tell for anything
    /// else: output that reads as neither, and a command that fails.
    #[test]
    fn the_account_is_signed_in_when_pi_lists_a_model_and_signed_out_at_its_notice() {
        let dir = ScratchDir::new("pi-sign-in");
        assert_eq!(
            reading(&dir, TABLE, 0),
            SignInReading::SignedIn { email: None }
        );
        assert_eq!(reading(&dir, SIGNED_OUT, 0), SignInReading::SignedOut);
        assert_eq!(reading(&dir, "garbage\n", 0), SignInReading::CannotTell);
        assert_eq!(reading(&dir, SIGNED_OUT, 1), SignInReading::CannotTell);
        assert_eq!(reading(&dir, TABLE, 1), SignInReading::CannotTell);
    }

    /// A machine with no `pi` on its path cannot tell either.
    #[test]
    fn a_missing_pi_cannot_tell() {
        let dir = ScratchDir::new("pi-sign-in-missing");
        let env = fake_agent(&dir, "not-pi", "exit 0");
        assert_eq!(sign_in_state(Some(&dir), &env), SignInReading::CannotTell);
    }
}
