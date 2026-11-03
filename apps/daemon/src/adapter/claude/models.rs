//! Claude Code's model catalogue. Claude Code has no list command, but its `fable`, `opus`,
//! `sonnet` and `haiku` aliases always resolve to the newest model of their family, so those four
//! are the answer, best first, and no subprocess is run. Claude Code caches a catalogue per account
//! on disk (`cache/model-catalog/` in its configuration directory, found through the organization
//! and account ids in the global config file), which says which model each alias resolves to and
//! whether the account may use it; each entry carries that as `version` and `available`. A
//! missing or unreadable catalogue leaves both unknown, and the four aliases still listed. The
//! `best` alias is not offered: it falls back to Opus without checking that the account may use
//! it, so the first available entry of the list is what a session with no model setting gets
//! (`Listing::default_model`), unless Claude Code's own configuration names a model, which it then
//! applies itself (see `crate::agents::launch_settings::launch_default_model`). The listing is two
//! small file reads, so it is read afresh each time rather than cached. The measurements are in
//! `docs/agent-cli-reference.md`, section "Claude Code".

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::settings::{configured_model, global_config_file};
use crate::agents::models::{entry_levels, levels, Listing, ModelCatalogue, OUTPUT_LIMIT};
use crate::protocol::{AgentId, AgentModel};
use crate::providers::claude::CLAUDE_MODEL_CATALOGUE_DIR;

/// Claude Code's models: the aliases it resolves itself, read from its cached catalogue.
pub(crate) struct ClaudeCatalogue;

impl ModelCatalogue for ClaudeCatalogue {
    fn list(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _timeout: Duration,
    ) -> Listing {
        listing(shell_env, config_dir)
    }

    /// Claude Code's own default is not checked against what the account may use.
    fn names_default_model(&self) -> bool {
        true
    }

    fn configured_model(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        cwd: Option<&Path>,
    ) -> Option<String> {
        configured_model(shell_env, config_dir, cwd)
    }

    fn fixed_model_ids(&self) -> &'static [&'static str] {
        AgentId::CLAUDE.descriptor().model_display.aliases
    }
}

/// Claude Code's aliases and how to show them, best first: each resolves to the newest model of
/// its family, which an account may or may not be allowed to use.
const CLAUDE_MODELS: [(&str, &str); 4] = [
    ("fable", "Fable"),
    ("opus", "Opus"),
    ("sonnet", "Sonnet"),
    ("haiku", "Haiku"),
];

/// What `--effort` takes. Claude Code lowers a level the chosen model does not support to `high`
/// rather than refusing it, so it is offered for every alias whose catalogue row gives none.
/// Measured on 2.1.296.
const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// The aliases of [`CLAUDE_MODELS`] with what the account's cached catalogue says about each (see
/// [`read_catalogue`]); every alias is listed whatever it says, and one it cannot answer for has
/// no `version` and no `available`.
fn listing(shell_env: &HashMap<String, String>, config_dir: Option<&Path>) -> Listing {
    let (catalogue, denied) = read_catalogue(shell_env, config_dir);
    Listing::found(
        CLAUDE_MODELS
            .iter()
            .map(|(id, name)| alias_model(id, name, catalogue.as_ref(), &denied))
            .collect(),
    )
}

/// The cached catalogue of the account whose global config file `shell_env` and `config_dir` lead
/// to, and the model ids that config file says the account is not entitled to. Claude Code names
/// the catalogue file `<organization id>-<first 12 hex digits of the SHA-256 of the account
/// id>-cc.json`, both ids lower-cased, because several accounts' catalogues can share a
/// directory. Either is `None` or empty when the file is not there or cannot be read. A catalogue
/// past its own `staleAt` is still the best answer there is, so it is read all the same.
fn read_catalogue(
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
) -> (Option<Value>, Vec<String>) {
    let read = |file: &Path| -> Option<Value> {
        serde_json::from_str(&crate::hostfs::read_regular_text(file, OUTPUT_LIMIT).ok()?).ok()
    };
    let Some(global) = read(&global_config_file(shell_env, config_dir)) else {
        return (None, Vec::new());
    };
    let denied = global
        .get("modelAccessCache")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.get("entitled").and_then(Value::as_bool) == Some(false))
        .filter_map(|entry| entry.get("apiName").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let account = global.get("oauthAccount");
    let id_of = |key: &str| {
        account
            .and_then(|account| account.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_lowercase)
    };
    let catalogue = id_of("organizationUuid")
        .zip(id_of("accountUuid"))
        .and_then(|(organization, account)| {
            let file = catalogue_file(
                &crate::adapter::account_dir(AgentId::CLAUDE, config_dir, shell_env),
                &organization,
                &account,
            );
            read(&file)
        });
    (catalogue, denied)
}

/// Where Claude Code caches the catalogue of the account `account_id` of organization
/// `organization_id` (both lower-cased) under the configuration directory `dir`.
fn catalogue_file(dir: &Path, organization_id: &str, account_id: &str) -> PathBuf {
    let digest = Sha256::digest(account_id.as_bytes());
    let hash: String = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    dir.join(CLAUDE_MODEL_CATALOGUE_DIR)
        .join(format!("{organization_id}-{hash}-cc.json"))
}

/// One alias as the catalogue describes it: the row of the model the alias targets
/// (`provider_alias_targets`) gives its version name (`name`, "Opus 5.5"), its effort levels, and
/// whether the account can use it: not `disabled`, not in the `deprecated` section, and not
/// refused by `denied`. A catalogue with no such row leaves all three at their fallbacks.
fn alias_model(
    alias: &str,
    name: &str,
    catalogue: Option<&Value>,
    denied: &[String],
) -> AgentModel {
    let config = catalogue.and_then(|catalogue| catalogue.get("catalog")?.get("config"));
    let target = config
        .and_then(|config| {
            config
                .get("provider_alias_targets")?
                .get(alias)?
                .get("default")
        })
        .and_then(Value::as_str);
    let row = target.and_then(|target| {
        config?
            .get("models")?
            .as_array()?
            .iter()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(target))
    });
    let version = row
        .and_then(|row| row.get("name")?.as_str())
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(str::to_string);
    let available = row.map(|row| {
        row.get("disabled").and_then(Value::as_bool) != Some(true)
            && row.get("section").and_then(Value::as_str) != Some("deprecated")
            && !target.is_some_and(|target| denied.iter().any(|api_name| api_name == target))
    });
    let efforts = row
        .and_then(|row| entry_levels(row.get("runtime")?.get("effort_levels")?))
        .unwrap_or_else(|| levels(&CLAUDE_EFFORTS));
    AgentModel {
        id: alias.to_string(),
        name: name.to_string(),
        efforts,
        version,
        available,
    }
}

/// A scratch Claude Code home and catalogue for tests that must not read the machine's own.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::collections::HashMap;
    use std::path::Path;

    use super::catalogue_file;
    use crate::test_support::ScratchDir;

    /// A Claude Code catalogue with the four aliases targeting rows that are usable (Opus),
    /// `disabled` with a notice (Fable), `deprecated` (Sonnet), and missing (Haiku).
    pub const CLAUDE_CATALOGUE: &str = r#"{"version":2,"fetchedAt":1,"staleAt":2,"catalog":{
        "surface":"cc","config":{
        "models":[
            {"id":"claude-opus-5-5","name":"Opus 5.5","section":"main",
             "runtime":{"effort_levels":["low","high"]}},
            {"id":"claude-fable-5-1","name":"Fable 5.1","section":"main","disabled":true,
             "notice":{"text":"Fable needs Max"}},
            {"id":"claude-sonnet-5-5","name":"Sonnet 5.5","section":"deprecated"}],
        "provider_alias_targets":{
            "fable":{"default":"claude-fable-5-1"},"opus":{"default":"claude-opus-5-5"},
            "sonnet":{"default":"claude-sonnet-5-5"},"haiku":{"default":"claude-haiku-5-5"}}}}}"#;

    /// Claude Code's global config file `config` for one account and, when `catalogue` is given,
    /// its cached catalogue where Claude Code keeps it under the configuration directory `dir`.
    pub fn write_claude_account(
        config: &Path,
        dir: &Path,
        access_cache: &str,
        catalogue: Option<&str>,
    ) {
        std::fs::write(
            config,
            format!(
                r#"{{"oauthAccount":{{"organizationUuid":"ORG-1","accountUuid":"Acct-1"}},
                    "modelAccessCache":{access_cache}}}"#
            ),
        )
        .unwrap();
        if let Some(catalogue) = catalogue {
            let file = catalogue_file(dir, "org-1", "acct-1");
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, catalogue).unwrap();
        }
    }

    /// A home with the account of [`write_claude_account`] in it. Returns the shell environment
    /// pointing at it.
    pub fn claude_home(
        dir: &ScratchDir,
        access_cache: &str,
        catalogue: Option<&str>,
    ) -> HashMap<String, String> {
        let home: &Path = dir;
        write_claude_account(
            &home.join(".claude.json"),
            &home.join(".claude"),
            access_cache,
            catalogue,
        );
        HashMap::from([("HOME".to_string(), home.to_string_lossy().into_owned())])
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{claude_home, CLAUDE_CATALOGUE};
    use super::*;
    use crate::agents::models::{list, COMMAND_TIMEOUT};
    use crate::test_support::ScratchDir;

    fn ids(models: &[AgentModel]) -> Vec<&str> {
        models.iter().map(|model| model.id.as_str()).collect()
    }

    #[test]
    fn the_fixed_ids_are_the_aliases_listed_and_the_ones_the_descriptor_shows() {
        let listed: Vec<&str> = CLAUDE_MODELS.iter().map(|(id, _)| *id).collect();
        assert_eq!(listed, ClaudeCatalogue.fixed_model_ids());
    }

    #[test]
    fn the_catalogue_file_is_named_by_the_organization_and_a_hash_of_the_account() {
        // The digest of `acct-1`, first twelve hex digits.
        assert_eq!(
            catalogue_file(Path::new("/c"), "org-1", "acct-1"),
            Path::new("/c/cache/model-catalog/org-1-ba36a4edd92d-cc.json")
        );
    }

    #[test]
    fn claude_lists_its_aliases_best_first_with_what_the_catalogue_says() {
        let dir = ScratchDir::new("agent-models-claude-catalogue");
        let env = claude_home(&dir, "[]", Some(CLAUDE_CATALOGUE));
        let listing = listing(&env, None);
        assert_eq!(ids(&listing.models), ["fable", "opus", "sonnet", "haiku"]);
        assert!(listing.unavailable.is_none());
        let facts: Vec<_> = listing
            .models
            .iter()
            .map(|model| (model.version.as_deref(), model.available))
            .collect();
        assert_eq!(
            facts,
            [
                (Some("Fable 5.1"), Some(false)),
                (Some("Opus 5.5"), Some(true)),
                (Some("Sonnet 5.5"), Some(false)),
                (None, None),
            ]
        );
        // The row's own levels where it gives them, else the measured set.
        assert_eq!(listing.models[1].efforts, ["low", "high"].map(String::from));
        assert_eq!(listing.models[3].efforts, CLAUDE_EFFORTS.map(String::from));
        // The first usable model is the one a session without a setting gets.
        assert_eq!(listing.default_model(), Some("opus"));
    }

    #[test]
    fn an_account_the_config_file_says_is_not_entitled_is_unavailable() {
        let dir = ScratchDir::new("agent-models-claude-denied");
        let env = claude_home(
            &dir,
            r#"[{"apiName":"claude-opus-5-5","entitled":false},
                {"apiName":"claude-fable-5-1","entitled":true}]"#,
            Some(&CLAUDE_CATALOGUE.replace(r#","disabled":true"#, "")),
        );
        let listing = listing(&env, None);
        let available: Vec<_> = listing.models.iter().map(|m| m.available).collect();
        assert_eq!(available, [Some(true), Some(false), Some(false), None]);
        assert_eq!(listing.default_model(), Some("fable"));
    }

    #[test]
    fn a_missing_or_unreadable_catalogue_leaves_the_aliases_listed_and_unknown() {
        let dir = ScratchDir::new("agent-models-claude-missing");
        let broken = claude_home(&dir, "[]", Some("not json"));
        let missing = HashMap::from([(
            "HOME".to_string(),
            dir.join("nowhere").to_string_lossy().into_owned(),
        )]);
        for env in [broken, missing] {
            let listing = listing(&env, None);
            assert_eq!(ids(&listing.models), ["fable", "opus", "sonnet", "haiku"]);
            for model in &listing.models {
                assert_eq!((model.version.as_deref(), model.available), (None, None));
                assert_eq!(model.efforts, CLAUDE_EFFORTS.map(String::from));
            }
            // Nothing is known usable, so Claude Code is left to choose.
            assert_eq!(listing.default_model(), None);
        }
    }

    #[test]
    fn a_pinned_account_reads_the_catalogue_in_its_own_directory() {
        let dir = ScratchDir::new("agent-models-claude-pinned");
        let pinned = dir.join("pinned");
        let file = catalogue_file(&pinned, "org-1", "acct-1");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, CLAUDE_CATALOGUE).unwrap();
        std::fs::write(
            pinned.join(".claude.json"),
            r#"{"oauthAccount":{"organizationUuid":"Org-1","accountUuid":"ACCT-1"}}"#,
        )
        .unwrap();
        let listing = listing(&HashMap::new(), Some(&pinned));
        assert_eq!(listing.models[1].version.as_deref(), Some("Opus 5.5"));
    }

    #[test]
    fn the_retired_best_alias_is_recognised_but_never_offered() {
        let dir = ScratchDir::new("agent-models-retired-best");
        let env = HashMap::from([("HOME".to_string(), dir.to_string_lossy().into_owned())]);
        let retired = AgentId::CLAUDE.descriptor().model_display.retired;
        assert!(retired.contains(&"best"));
        assert!(!retired.contains(&"opus"));
        let listing = listing(&env, None);
        assert!(!ids(&listing.models).contains(&"best"));
        // No binary on PATH and still an answer, because nothing is run.
        assert_eq!(list(AgentId::CLAUDE, &env, None, COMMAND_TIMEOUT), listing);
    }
}
