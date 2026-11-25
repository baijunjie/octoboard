//! OpenCode's model catalogue: `opencode models` prints one `provider/model` id per line, for
//! every model provider the account can use, which is also the spelling `--model` takes. It
//! names no display name and no reasoning effort, and OpenCode's TUI has no effort flag, so a
//! model is listed under its id with no effort to offer. Measured on 1.19.6
//! (`docs/agent-cli-reference.md`, section "OpenCode").

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::agents::models::{run_catalogue_command, unreadable, Listing, ModelCatalogue};
use crate::protocol::{AgentId, AgentModel};
use crate::terminal::plain_text::strip_escapes;

pub(super) struct OpenCodeCatalogue;

impl ModelCatalogue for OpenCodeCatalogue {
    fn list(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _run_homes: &Path,
        timeout: Duration,
    ) -> Listing {
        let args = match super::version::background_v2(shell_env) {
            Some(true) => &["models", "--standalone"][..],
            Some(false) => &["models"][..],
            None => return Listing::unavailable(unreadable(AgentId::OPENCODE)),
        };
        run_catalogue_command(AgentId::OPENCODE, shell_env, config_dir, args, timeout)
            .and_then(|stdout| {
                let models = parse_list(&stdout);
                if models.is_empty() {
                    return Err(unreadable(AgentId::OPENCODE));
                }
                Ok(models)
            })
            .map_or_else(Listing::unavailable, Listing::found)
    }
}

/// The models `opencode models` printed, in its order: every line that is one `provider/model`
/// id and nothing else. Anything else it prints (a notice, a blank line) is not a model.
fn parse_list(stdout: &str) -> Vec<AgentModel> {
    strip_escapes(stdout)
        .lines()
        .map(str::trim)
        .filter(|line| is_model_id(line))
        .map(|id| AgentModel {
            id: id.to_string(),
            name: id.to_string(),
            efforts: Vec::new(),
            version: None,
            available: None,
        })
        .collect()
}

/// Whether `line` is a `provider/model` id: one word with a provider before its first slash and
/// a model after it.
fn is_model_id(line: &str) -> bool {
    !line.chars().any(char::is_whitespace)
        && line
            .split_once('/')
            .is_some_and(|(provider, model)| !provider.is_empty() && !model.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::models::{list, COMMAND_TIMEOUT};
    use crate::protocol::models_unavailable_code as code;
    use crate::test_support::{fake_agent, ScratchDir};

    /// What `opencode models` printed on 1.19.6, signed out and with `ANTHROPIC_API_KEY` in the
    /// environment (`testdata/opencode-captures.md`).
    const SIGNED_OUT: &str =
        include_str!("../../../testdata/opencode_models_signed_out_1.19.6.txt");
    const ENVIRONMENT: &str =
        include_str!("../../../testdata/opencode_models_environment_1.19.6.txt");

    fn ids(models: &[AgentModel]) -> Vec<&str> {
        models.iter().map(|model| model.id.as_str()).collect()
    }

    /// A model is listed under the id the CLI printed, in its order, and names no effort.
    #[test]
    fn a_model_carries_its_id_as_its_name_and_no_effort() {
        let models = parse_list(SIGNED_OUT);
        assert_eq!(models.len(), 12);
        assert_eq!(
            ids(&models)[..3],
            [
                "opencode/beam-free",
                "opencode/big-pickle",
                "opencode/deepseek-v4.1-flash-free"
            ]
        );
        assert_eq!(ids(&models), SIGNED_OUT.lines().collect::<Vec<_>>());
        for model in &models {
            assert_eq!(model.name, model.id);
            assert!(model.efforts.is_empty(), "{}", model.id);
            assert_eq!(model.version, None, "{}", model.id);
            assert_eq!(model.available, None, "{}", model.id);
        }
    }

    /// A key in the environment adds that provider's models after the free ones.
    #[test]
    fn a_providers_models_follow_the_free_ones() {
        let models = parse_list(ENVIRONMENT);
        assert_eq!(ids(&models), ENVIRONMENT.lines().collect::<Vec<_>>());
        assert_eq!(models.len(), 32);
        assert_eq!(models[0].id, "opencode/beam-free");
        assert_eq!(models[12].id, "anthropic/claude-fable-5");
        assert!(ids(&models)[12..]
            .iter()
            .all(|id| id.starts_with("anthropic/")));
    }

    /// Escapes, blanks and padding are not part of an id; a line that is not one `provider/model`
    /// word is not a model.
    #[test]
    fn only_a_line_that_is_one_provider_and_model_id_is_a_model() {
        let printed = "\u{1b}[90mopencode/a-free\u{1b}[0m\n\n  anthropic/claude-x  \r\n\
                       A notice for the user\nplain\n/model\nprovider/\nopenrouter/anthropic/claude-y\n";
        assert_eq!(
            ids(&parse_list(printed)),
            [
                "opencode/a-free",
                "anthropic/claude-x",
                "openrouter/anthropic/claude-y"
            ]
        );
        assert!(parse_list("").is_empty());
        assert!(parse_list("No models\n").is_empty());
    }

    fn reason(listing: &Listing) -> crate::protocol::Code {
        listing.unavailable.as_ref().expect("a reason").code
    }

    /// The catalogue runs `opencode models` and lists what it prints.
    #[test]
    fn the_catalogue_runs_the_models_command() {
        let dir = ScratchDir::new("opencode-models");
        let printed = dir.join("models.txt");
        std::fs::write(&printed, SIGNED_OUT).unwrap();
        let env = fake_agent(
            &dir,
            "opencode",
            &format!("[ \"$1\" = models ] && cat '{}'", printed.display()),
        );
        let listing = list(AgentId::OPENCODE, &env, None, COMMAND_TIMEOUT);
        assert_eq!(listing.unavailable, None);
        assert_eq!(listing.models, parse_list(SIGNED_OUT));
    }

    /// Output with no model in it, and a command that fails, are why the list is not there, not an
    /// empty list.
    #[test]
    fn output_with_no_model_and_a_failing_command_leave_the_list_unavailable() {
        let dir = ScratchDir::new("opencode-models-unavailable");
        let env = fake_agent(&dir, "opencode", "echo 'nothing to list'");
        let listing = list(AgentId::OPENCODE, &env, None, COMMAND_TIMEOUT);
        assert!(listing.models.is_empty());
        assert_eq!(reason(&listing), code::UNREADABLE_OUTPUT);

        let env = fake_agent(&dir, "opencode", "echo boom >&2\nexit 1");
        let listing = list(AgentId::OPENCODE, &env, None, COMMAND_TIMEOUT);
        assert!(listing.models.is_empty());
        assert_eq!(reason(&listing), code::COMMAND_FAILED);

        let env = HashMap::from([("PATH".to_string(), "/usr/bin:/bin".to_string())]);
        let listing = list(AgentId::OPENCODE, &env, None, COMMAND_TIMEOUT);
        assert_eq!(reason(&listing), code::BINARY_NOT_FOUND);
    }
}
