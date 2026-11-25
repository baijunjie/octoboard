//! Antigravity CLI's model catalogue: `agy models`, which asks Google's service for the models
//! the signed-in account may use and prints `Fetching available models...` and then a line a
//! model, its id and its label separated by a tab (1.5.14, `testdata/antigravity_models.txt`).
//! No effort is listed beside a model: the ids that have a reasoning level carry it in their name
//! (`gemini-3.8-flash-high`, labelled `Gemini 3.8 Flash (High)`). `--effort` is that level: with
//! the base id (`--model gemini-3.8-flash --effort high`) it runs the same model at that level,
//! a level the base has no id for is refused (`gemini-3.1-pro has no "medium" effort (available:
//! low, high)`), so is a level beside an id that carries another, and so is any `--effort` for a
//! model with no levels (`--effort is not supported for model "claude-sonnet-4-6"`) (1.5.14). So
//! the ids of one base are listed as one model under the base id, its levels as its efforts in
//! the order printed, and a model with no level keeps its id and takes no effort. The run has the
//! CLI's own update turned off (`super::home::without_update`).
//!
//! The command is never run against the user's home, where it writes: it runs the CLI's
//! migration there, which moves the entries of an earlier layout and replaces a file already at
//! their new place, and in a home with nothing to move it still writes the migration's marker and
//! an empty `config/mcp_config.json` (measured; `super::home`). It is run as `agy
//! --gemini_dir=<a directory of Octoboard's own, holding its own marker> models`, which was
//! measured to list the same models, the login being an item of the system keychain and no file
//! of the home, and to change nothing under `~/.gemini`. The directory is removed after the run
//! with what the CLI's start left in it.
//!
//! The catalogue still asks first what a launch asks, and where a launch would be refused it runs
//! nothing and gives the refusal as the reason the list is unavailable: a session could not be
//! started with any of the models. An account with a directory of its own is refused the same
//! way: the CLI has the default account only (`Capabilities::owned_accounts`).

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::adapter::account_cli::{account_dir, RunHome};
use crate::agents::models::{failed, run_catalogue_command, unreadable, Listing, ModelCatalogue};
use crate::protocol::{AgentId, AgentModel, CodedError, ModelsUnavailable, ProviderOperation};

/// The command that lists the account's models, after the flag that names the run's directory.
const LIST_MODELS: &str = "models";

/// What the name of a listing's directory starts with, under the directory of the sessions'
/// scratch directories, which the daemon clears when it starts: a directory a daemon that was
/// killed left behind goes then.
const LISTING_DIR_PREFIX: &str = "antigravity-models-";

pub(super) struct AntigravityCatalogue;

impl ModelCatalogue for AntigravityCatalogue {
    fn list(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _run_homes: &Path,
        timeout: Duration,
    ) -> Listing {
        list_in(&crate::paths::run_dir(), shell_env, config_dir, timeout)
    }
}

/// The listing, its run given a directory of its own made under `parent` and removed after.
pub(super) fn list_in(
    parent: &Path,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
) -> Listing {
    if config_dir.is_some() {
        return Listing::unavailable(refusal(&CodedError::provider_unsupported(
            AgentId::ANTIGRAVITY,
            ProviderOperation::OwnedAccounts,
        )));
    }
    let account = account_dir(AgentId::ANTIGRAVITY, None, shell_env);
    if let Err(refused) =
        super::home::require_migrated_layout(&account, "its models were not listed")
    {
        return Listing::unavailable(refusal(&refused));
    }
    let own = match listing_dir(parent) {
        Ok(own) => own,
        Err(err) => {
            return Listing::unavailable(failed(AgentId::ANTIGRAVITY, &format!("{err:#}")));
        }
    };
    let flag = super::home::directory_argument(own.path());
    run_catalogue_command(
        AgentId::ANTIGRAVITY,
        &super::home::without_update(shell_env),
        None,
        &[flag.as_str(), LIST_MODELS],
        timeout,
    )
    .and_then(|stdout| {
        let models = parse(&stdout);
        if models.is_empty() {
            Err(unreadable(AgentId::ANTIGRAVITY))
        } else {
            Ok(models)
        }
    })
    .map_or_else(Listing::unavailable, Listing::found)
}

/// A refusal as the reason a list is unavailable.
fn refusal(refused: &anyhow::Error) -> ModelsUnavailable {
    let refused = CodedError::of(refused);
    ModelsUnavailable {
        code: refused.code,
        params: refused.params,
        message: refused.message,
    }
}

/// Makes the directory one listing's run is given in place of `~/.gemini` under `parent`, with the
/// marker of a finished migration in it so that the run migrates nothing; removed when dropped,
/// however the run ended.
fn listing_dir(parent: &Path) -> anyhow::Result<RunHome> {
    let own = RunHome::make_private_run_dir(parent, LISTING_DIR_PREFIX)?;
    super::home::write_marker(own.path())?;
    Ok(own)
}

/// The reasoning levels a model id can end in, as `--effort` takes them (1.5.14).
const LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// The base id and the level of a model id that ends in one (`gemini-3.8-flash-high`), `None`
/// for an id with no level.
pub(super) fn split_level(id: &str) -> Option<(&str, &str)> {
    let (base, level) = id.rsplit_once('-')?;
    (!base.is_empty() && LEVELS.contains(&level)).then_some((base, level))
}

/// The models `agy models` printed: every line that is an id, a tab and a label, in the order
/// printed, with the ids of one base and their levels as one model (see the module header) where
/// its first id came. Any other line, the notice it starts with among them, is passed over.
pub(super) fn parse(stdout: &str) -> Vec<AgentModel> {
    let mut models: Vec<AgentModel> = Vec::new();
    for line in stdout.lines() {
        let Some((id, label)) = line.split_once('\t') else {
            continue;
        };
        let (id, label) = (id.trim(), label.trim());
        let plain = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'));
        if !plain || label.is_empty() {
            continue;
        }
        match leveled(id, label) {
            Some((base, name, level)) => match models.iter_mut().find(|model| model.id == base) {
                Some(model) => {
                    if !model.efforts.iter().any(|known| known == level) {
                        model.efforts.push(level.to_string());
                    }
                }
                None => models.push(model_of(base, name, vec![level.to_string()])),
            },
            None => models.push(model_of(id, label, Vec::new())),
        }
    }
    models
}

/// The base id, the name without its level and the level of a listed model whose id ends in a
/// level and whose label ends in that level in parentheses (`Gemini 3.8 Flash (High)`); `None`
/// for any other, whose label says nothing of a level (`Claude Sonnet 4.6 (Thinking)`).
fn leveled<'a>(id: &'a str, label: &'a str) -> Option<(&'a str, &'a str, &'a str)> {
    let (base, level) = split_level(id)?;
    let name = label.strip_suffix(')')?;
    let (name, named) = name.rsplit_once(" (")?;
    (named.eq_ignore_ascii_case(level) && !name.trim().is_empty())
        .then(|| (base, name.trim(), level))
}

fn model_of(id: &str, name: &str, efforts: Vec<String>) -> AgentModel {
    AgentModel {
        id: id.to_string(),
        name: name.to_string(),
        efforts,
        version: None,
        available: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::models::COMMAND_TIMEOUT;
    use crate::protocol::models_unavailable_code as code;
    use crate::test_support::{fake_agent, ScratchDir};

    /// What `agy models` printed for a signed-in account on the free plan (1.5.14,
    /// `testdata/antigravity-captures.md`).
    const PRINTED: &str = include_str!("../../../testdata/antigravity_models.txt");

    /// The six ids of the capture are four models in the order printed: the three levels of
    /// Gemini 3.8 Flash as one model under its base id with its levels as efforts, GPT-OSS's one
    /// level as its effort, and the two Claude models, whose labels name no level, under their
    /// ids with no effort. The notice the CLI starts with is no model.
    #[test]
    fn the_capture_gives_each_base_once_with_its_levels_as_efforts() {
        assert!(PRINTED.starts_with("Fetching available models..."));
        let models = parse(PRINTED);
        let listed: Vec<(&str, &str, Vec<&str>)> = models
            .iter()
            .map(|model| {
                (
                    model.id.as_str(),
                    model.name.as_str(),
                    model.efforts.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                (
                    "gemini-3.8-flash",
                    "Gemini 3.8 Flash",
                    vec!["high", "medium", "low"]
                ),
                ("claude-sonnet-4-6", "Claude Sonnet 4.6 (Thinking)", vec![]),
                (
                    "claude-opus-4-6-thinking",
                    "Claude Opus 4.6 (Thinking)",
                    vec![]
                ),
                ("gpt-oss-120b", "GPT-OSS 120B", vec!["medium"]),
            ]
        );
        for model in &models {
            assert_eq!(model.available, Some(true), "{}", model.id);
        }
    }

    /// An id ends in a level only where its last part is one of the five `--effort` takes; a
    /// label that does not name the id's level keeps the id as it is.
    #[test]
    fn only_an_id_and_label_that_both_name_the_level_are_grouped() {
        assert_eq!(
            split_level("gemini-4-neon-xhigh"),
            Some(("gemini-4-neon", "xhigh"))
        );
        assert_eq!(
            split_level("gemini-3.1-pro-max"),
            Some(("gemini-3.1-pro", "max"))
        );
        assert_eq!(split_level("claude-opus-4-6-thinking"), None);
        assert_eq!(split_level("claude-sonnet-4-6"), None);
        assert_eq!(split_level("-high"), None);
        let models = parse(
            "gemini-3.1-pro-high\tGemini 3.1 Pro (High)\n\
             gemini-x-low\tSomething Else\n\
             gemini-3.1-pro-low\tGemini 3.1 Pro (Low)\n",
        );
        let listed: Vec<(&str, Vec<&str>)> = models
            .iter()
            .map(|model| {
                (
                    model.id.as_str(),
                    model.efforts.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                ("gemini-3.1-pro", vec!["high", "low"]),
                ("gemini-x-low", vec![]),
            ]
        );
    }

    /// A line is a model only when it is an id of plain characters, a tab and a label: padding
    /// and line ends are not part of either, and a line of any other shape is passed over.
    #[test]
    fn only_a_line_of_an_id_a_tab_and_a_label_is_a_model() {
        let printed = "Fetching available models...\r\n\
                       model-a\tModel A\r\n\
                       \n\
                       two words\tNot an id\n\
                       no-label\t\n\
                       no tab here\n\
                       \tno id\n\
                       model-b \t  Model B  \n\
                       model.c_d\tModel C\n";
        let models = parse(printed);
        assert_eq!(
            models
                .iter()
                .map(|model| (model.id.as_str(), model.name.as_str()))
                .collect::<Vec<_>>(),
            [
                ("model-a", "Model A"),
                ("model-b", "Model B"),
                ("model.c_d", "Model C")
            ]
        );
        assert!(parse("").is_empty());
        assert!(parse("Fetching available models...\n").is_empty());
    }

    /// The listing of the default account, its run's directory made under the test's own.
    fn listed(dir: &ScratchDir, env: &HashMap<String, String>) -> Listing {
        list_in(&dir.join("listings"), env, None, COMMAND_TIMEOUT)
    }

    /// The entries left under the directory the listings' own directories are made in.
    fn left_behind(dir: &ScratchDir) -> Vec<std::ffi::OsString> {
        std::fs::read_dir(dir.join("listings")).map_or_else(
            |_| Vec::new(),
            |entries| entries.flatten().map(|entry| entry.file_name()).collect(),
        )
    }

    fn reason(listing: &crate::agents::models::Listing) -> crate::protocol::Code {
        listing.unavailable.as_ref().expect("a reason").code
    }

    /// In a home a launch would be refused for, the catalogue does not run the CLI at all (no
    /// session could be started with a model it listed) and gives the launch's own refusal as the reason:
    /// the plain one for an earlier layout not yet moved, the narrower one, with its two paths,
    /// where a file is in both places. Once the home has its marker the command runs.
    #[test]
    fn the_catalogue_runs_nothing_in_a_home_a_launch_would_be_refused_for() {
        use crate::protocol::Code;
        use crate::providers::antigravity::{
            EARLIER_LAYOUT_FILE_IN_BOTH_PLACES, EARLIER_LAYOUT_NOT_MIGRATED,
        };

        let dir = ScratchDir::new("antigravity-models-unmigrated");
        let printed = dir.join("models.txt");
        std::fs::write(&printed, PRINTED).expect("capture");
        let runs = dir.join("runs");
        let mut env = fake_agent(
            &dir,
            "agy",
            &format!(
                "echo run >> '{}'; cat '{}'",
                runs.display(),
                printed.display()
            ),
        );
        let home = dir.join("home");
        env.insert("HOME".to_string(), home.to_string_lossy().into_owned());
        let ran = || std::fs::read_to_string(&runs).map_or(0, |text| text.lines().count());
        let data = home.join(".gemini").join("antigravity-cli");
        let config = home.join(".gemini").join("config");

        std::fs::create_dir_all(data.join("skills")).expect("an earlier layout's directory");
        let listing = listed(&dir, &env);
        assert!(listing.models.is_empty());
        assert_eq!(
            reason(&listing),
            Code::of_provider(EARLIER_LAYOUT_NOT_MIGRATED)
        );
        assert_eq!(ran(), 0, "the CLI was run");

        std::fs::create_dir_all(&config).expect("config directory");
        std::fs::write(data.join("mcp_config.json"), r#"{"earlier":{}}"#).expect("file");
        std::fs::write(config.join("mcp_config.json"), r#"{"current":{}}"#).expect("file");
        let listing = listed(&dir, &env);
        assert!(listing.models.is_empty());
        assert_eq!(
            reason(&listing),
            Code::of_provider(EARLIER_LAYOUT_FILE_IN_BOTH_PLACES)
        );
        let params = &listing.unavailable.as_ref().expect("a reason").params;
        assert_eq!(
            params.get("path").map(String::as_str),
            Some(config.join("mcp_config.json").to_string_lossy().as_ref())
        );
        assert_eq!(
            params.get("earlier_path").map(String::as_str),
            Some(data.join("mcp_config.json").to_string_lossy().as_ref())
        );
        assert_eq!(ran(), 0, "the CLI was run");
        assert_eq!(
            std::fs::read_to_string(config.join("mcp_config.json")).expect("file"),
            r#"{"current":{}}"#
        );

        std::fs::write(config.join(".migrated"), "").expect("marker");
        let listing = listed(&dir, &env);
        assert_eq!(listing.unavailable, None);
        assert_eq!(listing.models.len(), 4);
        assert_eq!(ran(), 1);
    }

    /// The command is run with a directory of Octoboard's own in the home's place, which holds the
    /// marker of a finished migration when the CLI starts, and never against the home: what the
    /// CLI's start writes lands in that directory, which is gone after the run, as it is after a
    /// run that failed, and the home, which had no `.gemini`, has none after either.
    #[test]
    fn the_listing_runs_in_a_directory_of_its_own_and_leaves_nothing() {
        let dir = ScratchDir::new("antigravity-models-own-directory");
        let printed = dir.join("models.txt");
        std::fs::write(&printed, PRINTED).expect("capture");
        let flag = dir.join("flag");
        let marker = dir.join("marker");
        let script = |ending: &str| {
            format!(
                "own=\"${{1#--gemini_dir=}}\"\n\
                 echo \"$1\" > '{}'\n\
                 [ -f \"$own/config/.migrated\" ] && echo found > '{}'\n\
                 mkdir -p \"$own/antigravity-cli\" && echo left > \"$own/config/mcp_config.json\"\n\
                 {ending}",
                flag.display(),
                marker.display()
            )
        };
        let home = dir.join("home");
        std::fs::create_dir_all(&home).expect("home");

        let mut env = fake_agent(
            &dir,
            "agy",
            &script(&format!("cat '{}'", printed.display())),
        );
        env.insert("HOME".to_string(), home.to_string_lossy().into_owned());
        let listing = listed(&dir, &env);
        assert_eq!(listing.unavailable, None);
        assert_eq!(listing.models.len(), 4);
        let given = std::fs::read_to_string(&flag).expect("the flag was given first");
        let own = dir.join("listings").join("antigravity-models-");
        assert!(
            given.starts_with(&format!("--gemini_dir={}", own.display())),
            "{given}"
        );
        assert_eq!(
            std::fs::read_to_string(&marker).expect("marker").trim(),
            "found"
        );
        assert_eq!(left_behind(&dir), Vec::<std::ffi::OsString>::new());
        assert!(!home.join(".gemini").exists(), "the home was written to");

        let mut env = fake_agent(&dir, "agy", &script("exit 1"));
        env.insert("HOME".to_string(), home.to_string_lossy().into_owned());
        let listing = listed(&dir, &env);
        assert_eq!(reason(&listing), code::COMMAND_FAILED);
        assert_eq!(left_behind(&dir), Vec::<std::ffi::OsString>::new());
        assert!(!home.join(".gemini").exists(), "the home was written to");
    }

    /// An account with a directory of its own is refused before anything is run or made: the CLI
    /// has the default account only, and the reason is the one a provider gives for what it does
    /// not offer.
    #[test]
    fn an_account_with_a_directory_of_its_own_is_refused_and_runs_nothing() {
        let dir = ScratchDir::new("antigravity-models-account-directory");
        let runs = dir.join("runs");
        let env = fake_agent(&dir, "agy", &format!("echo run >> '{}'", runs.display()));
        let account = dir.join("account").join(".gemini");
        std::fs::create_dir_all(&account).expect("account directory");

        let listing = list_in(&dir.join("listings"), &env, Some(&account), COMMAND_TIMEOUT);
        assert!(listing.models.is_empty());
        let unavailable = listing.unavailable.expect("a reason");
        assert_eq!(
            unavailable.code,
            crate::protocol::error_code::PROVIDER_UNSUPPORTED
        );
        assert_eq!(
            unavailable.params.get("operation").map(String::as_str),
            Some("owned_accounts")
        );
        assert!(!runs.exists(), "the CLI was run");
        assert_eq!(left_behind(&dir), Vec::<std::ffi::OsString>::new());
    }

    /// The catalogue runs `agy models` and lists what it prints; output with no model in it and a
    /// command that fails leave the list unavailable, with the reason, instead of an empty list.
    #[test]
    fn the_catalogue_runs_the_models_command() {
        let dir = ScratchDir::new("antigravity-models");
        let printed = dir.join("models.txt");
        std::fs::write(&printed, PRINTED).expect("capture");
        let seen = dir.join("update-variable");
        let env = fake_agent(
            &dir,
            "agy",
            &format!(
                "[ \"$2\" = models ] && {{ echo \"$AGY_CLI_DISABLE_AUTO_UPDATE\" > '{}'; cat '{}'; }}",
                seen.display(),
                printed.display()
            ),
        );
        let listing = listed(&dir, &env);
        assert_eq!(listing.unavailable, None);
        assert_eq!(listing.models, parse(PRINTED));
        assert_eq!(listing.models.len(), 4);
        // The run has the CLI's own update turned off, which the caller's environment does not.
        assert_eq!(std::fs::read_to_string(seen).expect("seen").trim(), "true");
        assert!(!env.contains_key("AGY_CLI_DISABLE_AUTO_UPDATE"));

        let env = fake_agent(&dir, "agy", "echo 'Fetching available models...'");
        let listing = listed(&dir, &env);
        assert!(listing.models.is_empty());
        assert_eq!(reason(&listing), code::UNREADABLE_OUTPUT);

        let env = fake_agent(
            &dir,
            "agy",
            "echo 'Error: Please sign in to view available models.' >&2\nexit 1",
        );
        let listing = listed(&dir, &env);
        assert!(listing.models.is_empty());
        assert_eq!(reason(&listing), code::COMMAND_FAILED);
    }
}
