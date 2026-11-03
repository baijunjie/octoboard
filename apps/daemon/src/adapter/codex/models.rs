//! Codex's model catalogue. Codex answers `codex debug models` with its catalogue as JSON: the
//! models whose `visibility` is `list`, ordered by ascending `priority`. The measurements are in
//! `docs/agent-cli-reference.md`, section "Codex".

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::agents::models::{
    entry_efforts, levels, run_catalogue_command, unreadable, Listing, ModelCatalogue,
};
use crate::protocol::{AgentId, AgentModel};

/// Codex's models, asked of its CLI.
pub(crate) struct CodexCatalogue;

impl ModelCatalogue for CodexCatalogue {
    fn list(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        timeout: Duration,
    ) -> Listing {
        run_catalogue_command(
            AgentId::CODEX,
            shell_env,
            config_dir,
            &["debug", "models"],
            timeout,
        )
        .and_then(|stdout| parse(&stdout).map_err(|()| unreadable(AgentId::CODEX)))
        .map_or_else(Listing::unavailable, Listing::found)
    }
}

/// What Codex's `model_reasoning_effort` parser accepts, measured on 0.160.0: the levels offered
/// for a model whose catalogue entry gives none.
const CODEX_EFFORTS: [&str; 6] = ["low", "medium", "high", "xhigh", "max", "ultra"];

/// Codex's `debug models` JSON: the entries whose `visibility` is `list`, by ascending `priority`
/// (the server sorts them; the bundled fallback does not, so this does), as slug, display name
/// and the efforts of `supported_reasoning_levels`, or Codex's whole set where the entry gives
/// none that can be read. `Err` when there is no `models` array or no
/// listed model in it.
fn parse(stdout: &str) -> Result<Vec<AgentModel>, ()> {
    let catalogue: Value = serde_json::from_str(stdout).map_err(|_| ())?;
    let entries = catalogue
        .get("models")
        .and_then(Value::as_array)
        .ok_or(())?;
    let mut listed: Vec<(i64, AgentModel)> = entries
        .iter()
        .filter(|entry| entry.get("visibility").and_then(Value::as_str) == Some("list"))
        .filter_map(|entry| {
            let slug = entry.get("slug").and_then(Value::as_str)?.trim();
            if slug.is_empty() {
                return None;
            }
            let name = entry
                .get("display_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(slug);
            let efforts = entry_efforts(entry, "supported_reasoning_levels", "effort")
                .unwrap_or_else(|| levels(&CODEX_EFFORTS));
            let priority = entry
                .get("priority")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX);
            Some((
                priority,
                AgentModel {
                    id: slug.to_string(),
                    name: name.to_string(),
                    efforts,
                    version: None,
                    available: None,
                },
            ))
        })
        .collect();
    if listed.is_empty() {
        return Err(());
    }
    // Stable, so models of equal priority keep the order the catalogue gave them.
    listed.sort_by_key(|(priority, _)| *priority);
    Ok(listed.into_iter().map(|(_, model)| model).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::models::{list, COMMAND_TIMEOUT};
    use crate::protocol::models_unavailable_code as code;
    use crate::test_support::{fake_agent, ScratchDir};

    const CODEX_CATALOGUE: &str = include_str!("../../../testdata/codex_debug_models.json");

    fn ids(models: &[AgentModel]) -> Vec<&str> {
        models.iter().map(|model| model.id.as_str()).collect()
    }

    fn reason(listing: &Listing) -> crate::protocol::Code {
        listing.unavailable.as_ref().expect("a reason").code
    }

    #[test]
    fn codex_keeps_the_listed_models_by_ascending_priority() {
        let models = parse(CODEX_CATALOGUE).expect("parses");
        // The fixture is in the bundled catalogue's order, which is not priority order, and holds
        // two hidden entries.
        assert_eq!(
            ids(&models),
            ["gpt-6.1-sol", "gpt-6-astra", "gpt-6-luna", "gpt-5.5"]
        );
        assert_eq!(models[0].name, "GPT-6.1-Sol");
        assert_eq!(
            models[3].efforts,
            ["low", "medium", "high", "xhigh"].map(String::from)
        );
        assert_eq!(models[1].efforts.len(), 6);
    }

    #[test]
    fn codex_entries_without_a_name_degrade_and_without_levels_take_codexs_whole_set() {
        let models = parse(
            r#"{"models":[{"slug":"a","visibility":"list"},{"visibility":"list"},
                {"slug":"b","display_name":" ","visibility":"list","priority":-1}]}"#,
        )
        .expect("parses");
        assert_eq!(ids(&models), ["b", "a"]);
        assert_eq!(models[0].name, "b");
        assert_eq!(models[0].efforts, levels(&CODEX_EFFORTS));
        assert_eq!(models[0].efforts.last().map(String::as_str), Some("ultra"));
    }

    #[test]
    fn codex_levels_are_the_models_own_none_or_the_agents_set() {
        let models = parse(
            r#"{"models":[
                {"slug":"own","visibility":"list","priority":1,
                 "supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}]},
                {"slug":"none","visibility":"list","priority":2,"supported_reasoning_levels":[]},
                {"slug":"junk","visibility":"list","priority":3,
                 "supported_reasoning_levels":[{"label":"x"},7]}]}"#,
        )
        .expect("parses");
        assert_eq!(models[0].efforts, ["low", "high"]);
        assert_eq!(models[1].efforts, Vec::<String>::new());
        assert_eq!(models[2].efforts, levels(&CODEX_EFFORTS));
    }

    #[test]
    fn codex_output_with_no_listed_model_or_no_json_is_unreadable() {
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"data":[]}"#).is_err());
        assert!(parse(r#"{"models":[{"slug":"x","visibility":"hide"}]}"#).is_err());
    }

    /// The command runs from the environment given, with the account's directory exported, and its
    /// stdout is parsed end to end. The fake agent prints the fixture only when `CODEX_HOME` is
    /// the directory the account names.
    #[test]
    fn codex_is_run_with_the_accounts_directory_and_its_output_parsed() {
        let dir = ScratchDir::new("agent-models-codex");
        let fixture = dir.join("catalogue.json");
        std::fs::write(&fixture, CODEX_CATALOGUE).unwrap();
        let account = dir.join("account");
        std::fs::create_dir(&account).unwrap();
        let env = fake_agent(
            &dir,
            "codex",
            &format!(
                "[ \"$1 $2\" = 'debug models' ] && [ \"$CODEX_HOME\" = '{}' ] && cat '{}'",
                account.display(),
                fixture.display()
            ),
        );

        let scoped = list(AgentId::CODEX, &env, Some(&account), COMMAND_TIMEOUT);
        assert_eq!(scoped.unavailable, None);
        assert_eq!(scoped.models.len(), 4);
        // Another directory is another account: the same command finds nothing to print.
        let other = list(AgentId::CODEX, &env, None, COMMAND_TIMEOUT);
        assert_eq!(reason(&other), code::COMMAND_FAILED);
    }
}
