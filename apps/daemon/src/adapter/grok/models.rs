//! Grok Build's model catalogue. Grok Build answers `grok models` with the visible ids as text,
//! which also refreshes `models_cache.json` in its home; that file, a map keyed by id, supplies
//! each model's name, `hidden` flag and reasoning efforts, which the text does not carry. The text
//! is the list and the file only decorates it, so a missing or unreadable file still leaves a
//! list, of bare ids. The measurements are in `docs/agent-cli-reference.md`, section "Grok Build".
//!
//! The command runs with `GROK_HOME` pointed at the account directory itself, unlike a session,
//! which runs against a per-session private home precisely so that its settings cannot leak into
//! the user's `config.toml`: that is necessarily so, since it is what refreshes the
//! `models_cache.json` read afterwards. The side effect is that opening the Advanced section of a
//! dialog writes into the user's Grok account directory.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::agents::models::{
    entry_efforts, levels, run_catalogue_command, unreadable, Listing, ModelCatalogue, OUTPUT_LIMIT,
};
use crate::protocol::{AgentId, AgentModel};
use crate::providers::grok::GROK_MODELS_CACHE;

/// Grok Build's models, asked of its CLI and decorated from its models cache.
pub(crate) struct GrokCatalogue;

impl ModelCatalogue for GrokCatalogue {
    fn list(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        timeout: Duration,
    ) -> Listing {
        run_catalogue_command(AgentId::GROK, shell_env, config_dir, &["models"], timeout)
            .and_then(|stdout| {
                let ids = parse_list(&stdout);
                if ids.is_empty() {
                    return Err(unreadable(AgentId::GROK));
                }
                let home = crate::adapter::account_dir(AgentId::GROK, config_dir, shell_env);
                // Read after the command, which is what refreshes it.
                let cache =
                    crate::hostfs::read_regular_text(&home.join(GROK_MODELS_CACHE), OUTPUT_LIMIT)
                        .ok();
                // The cache can drop every printed id as hidden; an empty list needs its reason.
                let models = decorate(&ids, cache.as_deref());
                if models.is_empty() {
                    return Err(unreadable(AgentId::GROK));
                }
                Ok(models)
            })
            .map_or_else(Listing::unavailable, Listing::found)
    }
}

/// What Grok Build's `--reasoning-effort` parser accepts, measured on 1.0.50: the levels offered
/// for a model whose catalogue entry gives none.
const GROK_EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];

/// The ids in `grok models`' output: the run of bullet lines (`  * id (default)`, `  - id`) right
/// under its `Available models:` heading, in the order printed. The run ends at the first line
/// that is not a bullet, so whatever Grok prints after the block (tips, a rule) is never read as
/// a model. Empty when the heading or the lines are not there, which the caller reads as
/// unreadable output.
fn parse_list(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Available models"))
        .skip(1)
        .skip_while(|line| line.trim().is_empty())
        .map_while(bullet_id)
        .map(str::to_string)
        .collect()
}

/// The id of one bullet line, or `None` for any other line. A bullet whose remainder is empty or
/// starts with another `-` is a rule (`-----`), not a model.
fn bullet_id(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix(['*', '-'])?;
    let id = rest.split_whitespace().next()?;
    (!id.starts_with('-')).then_some(id)
}

/// Turns the ids `grok models` printed into models, using `models_cache.json` (`cache`, when it
/// could be read) for what the text lacks: an entry's `info.name`, its `hidden` flag, which drops
/// it, and its `reasoning_efforts` values. An id the file does not know, or no file at all, gives
/// the id as its own name; an entry with no usable efforts gets Grok's whole set.
fn decorate(ids: &[String], cache: Option<&str>) -> Vec<AgentModel> {
    let cache: Option<Value> = cache.and_then(|text| serde_json::from_str(text).ok());
    ids.iter()
        .filter_map(|id| {
            let info = cache
                .as_ref()
                .and_then(|cache| cache.get("models"))
                .and_then(|models| models.get(id))
                .and_then(|entry| entry.get("info"));
            let Some(info) = info else {
                return Some(AgentModel {
                    id: id.clone(),
                    name: id.clone(),
                    efforts: levels(&GROK_EFFORTS),
                    version: None,
                    available: None,
                });
            };
            if info.get("hidden").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let name = info
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(id);
            let efforts = if info
                .get("supports_reasoning_effort")
                .and_then(Value::as_bool)
                == Some(false)
            {
                Vec::new()
            } else {
                entry_efforts(info, "reasoning_efforts", "value")
                    .unwrap_or_else(|| levels(&GROK_EFFORTS))
            };
            Some(AgentModel {
                id: id.clone(),
                name: name.to_string(),
                efforts,
                version: None,
                available: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::models::{list, COMMAND_TIMEOUT};
    use crate::protocol::models_unavailable_code as code;
    use crate::test_support::{fake_agent, ScratchDir};

    const GROK_LIST: &str = include_str!("../../../testdata/grok_models.txt");
    const GROK_CACHE: &str = include_str!("../../../testdata/grok_models_cache.json");

    fn ids(models: &[AgentModel]) -> Vec<&str> {
        models.iter().map(|model| model.id.as_str()).collect()
    }

    fn reason(listing: &Listing) -> crate::protocol::Code {
        listing.unavailable.as_ref().expect("a reason").code
    }

    #[test]
    fn grok_ids_come_from_the_text_in_the_order_printed() {
        assert_eq!(
            parse_list(GROK_LIST),
            ["grok-4.7", "grok-4.7-build-fast", "grok-4.6", "grok-4.5"]
        );
        assert!(parse_list("You are logged in.\n\nDefault model: x\n").is_empty());
        assert!(parse_list("").is_empty());
    }

    #[test]
    fn grok_ids_stop_at_the_end_of_the_block() {
        let tips = format!("{GROK_LIST}\nTips:\n  - run `grok --help` for more\n  * grok-fake\n");
        let rule = format!("{GROK_LIST}  -----\n  - grok-fake\n");
        let blank_then_bullet = format!("{GROK_LIST}\n  - run `grok --help` for more\n");
        for text in [tips, rule, blank_then_bullet] {
            assert_eq!(parse_list(&text).len(), 4, "{text}");
        }
        assert_eq!(bullet_id("  -----"), None);
        assert_eq!(bullet_id("  -"), None);
        assert_eq!(bullet_id("  * grok-4.7 (default)"), Some("grok-4.7"));
    }

    #[test]
    fn grok_names_and_efforts_come_from_the_cache_and_hidden_models_are_skipped() {
        let mut printed = parse_list(GROK_LIST);
        printed.push("grok-internal".to_string());
        let models = decorate(&printed, Some(GROK_CACHE));
        assert_eq!(
            ids(&models),
            ["grok-4.7", "grok-4.7-build-fast", "grok-4.6", "grok-4.5"]
        );
        assert_eq!(models[1].name, "Grok 4.7 Fast");
        assert_eq!(
            models[0].efforts,
            ["xhigh", "high", "medium", "low"].map(String::from)
        );
        assert_eq!(models[3].efforts.len(), 3);
    }

    #[test]
    fn grok_without_a_readable_cache_still_lists_bare_ids_with_groks_whole_set() {
        let printed = parse_list(GROK_LIST);
        for cache in [None, Some("garbage")] {
            let models = decorate(&printed, cache);
            assert_eq!(models.len(), 4);
            assert_eq!(models[2].name, "grok-4.6");
            assert_eq!(models[2].efforts, levels(&GROK_EFFORTS));
        }
    }

    #[test]
    fn a_grok_model_that_takes_no_effort_reports_none_and_one_that_says_nothing_the_set() {
        let cache = r#"{"models":{
            "m":{"info":{"name":"M","hidden":false,
                "supports_reasoning_effort":false,"reasoning_efforts":[]}},
            "e":{"info":{"name":"E","hidden":false,"reasoning_efforts":[]}},
            "n":{"info":{"name":"N","hidden":false}},
            "j":{"info":{"name":"J","hidden":false,"reasoning_efforts":[{"id":"x"}]}}}}"#;
        let ids = ["m", "e", "n", "j"].map(String::from);
        let models = decorate(&ids, Some(cache));
        assert_eq!(models[0].efforts, Vec::<String>::new());
        assert_eq!(models[1].efforts, Vec::<String>::new());
        assert_eq!(models[2].efforts, levels(&GROK_EFFORTS));
        assert_eq!(models[3].efforts, levels(&GROK_EFFORTS));
    }

    #[test]
    fn a_grok_cache_that_hides_every_printed_model_is_unreadable_not_empty() {
        let dir = ScratchDir::new("agent-models-grok-hidden");
        let home = dir.join("grok-home");
        std::fs::create_dir_all(&home).unwrap();
        let cache = ["grok-4.7", "grok-4.7-build-fast", "grok-4.6", "grok-4.5"]
            .map(|id| format!(r#""{id}":{{"info":{{"hidden":true}}}}"#))
            .join(",");
        std::fs::write(
            home.join(GROK_MODELS_CACHE),
            format!(r#"{{"models":{{{cache}}}}}"#),
        )
        .unwrap();
        let fixture = dir.join("models.txt");
        std::fs::write(&fixture, GROK_LIST).unwrap();
        let env = fake_agent(&dir, "grok", &format!("cat '{}'", fixture.display()));

        let listing = list(AgentId::GROK, &env, Some(&home), COMMAND_TIMEOUT);
        assert!(listing.models.is_empty());
        assert_eq!(reason(&listing), code::UNREADABLE_OUTPUT);
    }

    #[test]
    fn grok_is_run_in_the_accounts_home_and_its_cache_read_from_there() {
        let dir = ScratchDir::new("agent-models-grok");
        let home = dir.join("grok-home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(GROK_MODELS_CACHE), GROK_CACHE).unwrap();
        let fixture = dir.join("models.txt");
        std::fs::write(&fixture, GROK_LIST).unwrap();
        let env = fake_agent(
            &dir,
            "grok",
            &format!(
                "[ \"$1\" = models ] && [ \"$GROK_HOME\" = '{}' ] && cat '{}'",
                home.display(),
                fixture.display()
            ),
        );

        let listing = list(AgentId::GROK, &env, Some(&home), COMMAND_TIMEOUT);
        assert_eq!(listing.unavailable, None);
        assert_eq!(listing.models[1].name, "Grok 4.7 Fast");
    }

    #[test]
    fn bounded_agent_files_leave_grok_models_available() {
        use crate::adapter::tests::{agent_file, bounded_file_cases};
        bounded_file_cases(
            "adapter::grok::models::tests::bounded_agent_files_leave_grok_models_available",
            crate::adapter::tests::FILE_CASES,
            |case| {
                let home = ScratchDir::new("bounded-model-cache");
                agent_file(
                    &home.join(GROK_MODELS_CACHE),
                    case,
                    GROK_CACHE,
                    OUTPUT_LIMIT,
                );
                let env = fake_agent(&home, "grok", &format!("printf '%s' '{}'", GROK_LIST));
                let listing = list(AgentId::GROK, &env, Some(&home), COMMAND_TIMEOUT);
                let readable = matches!(case, "regular" | "linked");
                assert_eq!(listing.unavailable, None);
                assert_eq!(
                    listing.models,
                    decorate(&parse_list(GROK_LIST), readable.then_some(GROK_CACHE))
                );
            },
        );
    }
}
