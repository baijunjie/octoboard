//! Qwen Code's model catalogue: the entries of `modelProviders` under the sign-in method chosen
//! (`security.auth.selectedType`), read from the settings files with no process started.
//!
//! The files are the layers Qwen Code merges them from, lowest first: the system defaults, the
//! account's `settings.json`, the project's `.qwen/settings.json` and the system settings file
//! (0.28.3's `mergeSettings`). A higher layer's method wins, and its list of a method replaces a
//! lower one's whole, other methods kept (measured: a system file listing `openai` and
//! `anthropic` models over an account listing `openai` and `gemini` ones showed the system's
//! `openai` and `anthropic` entries and the account's `gemini` one). The project layer is not read:
//! the catalogue is not given the session's folder, so a project that lists models of its own in
//! `.qwen/settings.json` has them missing from the list (a gap of `ModelCatalogue::list`, which takes
//! no folder). Qwen Code's own picker lists every method's entries, but `--model` takes only the
//! chosen method's (measured: with `openai` chosen, `--model` naming an `anthropic` entry ran the
//! first `openai` one), so only those are offered.
//!
//! Qwen Code has no command that lists models. Its ACP mode lists exactly these entries, entry for
//! entry (0.28.3, `docs/agent-cli-reference.md`, "Qwen Code", "ACP mode"), but it takes a process of
//! 6 to 15 seconds and writes into the Qwen home it runs in (P9 of
//! `docs/conventions/principles.md`); the effort levels it adds are of no use to a launch, since
//! Qwen Code has no flag that takes one. So the file is read instead. A model picked for a session
//! is the entry's `id`, which `--model` takes, and Qwen Code was measured to send that id in its
//! requests (0.28.3).

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::settings::{system_defaults_path, system_settings_path, SYSTEM_FILE_LIMIT};
use super::settings_file::{self, SettingsFile};
use crate::adapter::account_dir;
use crate::adapter::files::LARGE_FILE_LIMIT;
use crate::agents::models::{failed, Listing, ModelCatalogue};
use crate::protocol::{AgentId, AgentModel};
use crate::providers::qwen::QWEN_SETTINGS_FILE;

/// Qwen Code's models, read from the account's settings and the system's.
pub(super) struct QwenCatalogue;

impl ModelCatalogue for QwenCatalogue {
    fn list(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _run_homes: &Path,
        _timeout: Duration,
    ) -> Listing {
        // Lowest first; the two system files within the size a launch reads them in.
        let layers = [
            (system_defaults_path(shell_env), SYSTEM_FILE_LIMIT),
            (
                account_dir(AgentId::QWEN, config_dir, shell_env).join(QWEN_SETTINGS_FILE),
                LARGE_FILE_LIMIT,
            ),
            (system_settings_path(shell_env), SYSTEM_FILE_LIMIT),
        ];
        let mut read = Vec::new();
        for (file, limit) in layers {
            match settings_file::read(&file, limit) {
                SettingsFile::Settings(settings) => read.push(Value::Object(settings)),
                // A layer that is not there sets nothing; none at all lists no model.
                SettingsFile::Missing => {}
                SettingsFile::Unusable(why) => {
                    return Listing::unavailable(failed(
                        AgentId::QWEN,
                        &format!("{} cannot be read: {why}", file.display()),
                    ))
                }
            }
        }
        Listing::found(models(&merged(&read)))
    }

    /// The model the settings name (`model.name`): the system settings file's, which outranks the
    /// others, else the project's (`<cwd>/.qwen/settings.json`), else the account's.
    fn configured_model(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        cwd: Option<&Path>,
    ) -> Option<String> {
        let named = |file: SettingsFile| model_name(&file.settings()?);
        // The system settings file within the size a launch reads it in, as `list` reads it.
        named(settings_file::read(
            &system_settings_path(shell_env),
            SYSTEM_FILE_LIMIT,
        ))
        .or_else(|| {
            let cwd = cwd?;
            let project = crate::hostfs::canonical(cwd).ok()?;
            named(settings_file::read_in_project(
                &project.join(".qwen").join(QWEN_SETTINGS_FILE),
                &project,
                LARGE_FILE_LIMIT,
            ))
        })
        .or_else(|| {
            let dir = account_dir(AgentId::QWEN, config_dir, shell_env);
            named(settings_file::read(
                &dir.join(QWEN_SETTINGS_FILE),
                LARGE_FILE_LIMIT,
            ))
        })
    }
}

/// The method chosen and the `modelProviders` of `layers`, lowest first, merged as Qwen Code merges
/// them: the highest layer that chooses a method chooses it, and each method's list is the highest
/// layer's that has one.
fn merged(layers: &[Value]) -> Value {
    let mut method = None;
    let mut providers = serde_json::Map::new();
    for layer in layers {
        if let Some(chosen) = layer.pointer("/security/auth/selectedType") {
            method = Some(chosen.clone());
        }
        if let Some(lists) = layer.get("modelProviders").and_then(Value::as_object) {
            for (name, list) in lists {
                providers.insert(name.clone(), list.clone());
            }
        }
    }
    let mut settings = serde_json::json!({ "modelProviders": providers });
    if let Some(method) = method {
        settings["security"] = serde_json::json!({ "auth": { "selectedType": method } });
    }
    settings
}

/// `model.name` of `settings`, where it names one.
fn model_name(settings: &Value) -> Option<String> {
    settings
        .pointer("/model/name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
}

/// The models `settings` offers: the entries listed under the chosen method in `modelProviders`,
/// in their order, each id once (the first entry's name where several share it, since `--model`
/// takes only the id). No method chosen lists none, and an entry with no id is passed over. A
/// model takes no effort level, Qwen Code having no flag for one.
pub(super) fn models(settings: &Value) -> Vec<AgentModel> {
    let Some(method) = settings
        .pointer("/security/auth/selectedType")
        .and_then(Value::as_str)
    else {
        return Vec::new();
    };
    let entries = settings
        .get("modelProviders")
        .and_then(|providers| providers.get(method))
        .and_then(Value::as_array);
    let mut models: Vec<AgentModel> = Vec::new();
    for entry in entries.into_iter().flatten() {
        let Some(id) = entry
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        if models.iter().any(|model| model.id == id) {
            continue;
        }
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(id);
        models.push(AgentModel {
            id: id.to_string(),
            name: name.to_string(),
            efforts: Vec::new(),
            version: None,
            available: Some(true),
        });
    }
    models
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_support::ScratchDir;

    fn ids(models: &[AgentModel]) -> Vec<(&str, &str)> {
        models
            .iter()
            .map(|model| (model.id.as_str(), model.name.as_str()))
            .collect()
    }

    /// The account's entries under the chosen method are the list, in their order, named as the
    /// settings name them, with no effort level and each one usable.
    #[test]
    fn the_chosen_methods_entries_are_the_models() {
        let settings = json!({
            "security": { "auth": { "selectedType": "openai" } },
            "modelProviders": {
                "openai": [
                    { "id": "stub-a", "name": "Stub A", "baseUrl": "http://a", "envKey": "K" },
                    { "id": "stub-b", "baseUrl": "http://b", "envKey": "K" },
                ],
                "anthropic": [{ "id": "other", "name": "Other" }],
            },
        });
        let listed = models(&settings);
        assert_eq!(ids(&listed), [("stub-a", "Stub A"), ("stub-b", "stub-b")]);
        for model in &listed {
            assert!(model.efforts.is_empty());
            assert_eq!(model.available, Some(true));
            assert_eq!(model.version, None);
        }
    }

    /// An id listed twice (two base URLs) is one model, `--model` taking only the id; an entry with
    /// no id is passed over; no method chosen, or none listed under it, lists nothing.
    #[test]
    fn an_id_is_listed_once_and_nothing_is_listed_without_a_method() {
        let settings = json!({
            "security": { "auth": { "selectedType": "openai" } },
            "modelProviders": { "openai": [
                { "id": "same", "name": "First", "baseUrl": "http://a" },
                { "name": "No id" },
                { "id": "  " },
                { "id": "same", "name": "Second", "baseUrl": "http://b" },
            ]},
        });
        assert_eq!(ids(&models(&settings)), [("same", "First")]);
        assert!(models(&json!({ "modelProviders": { "openai": [{ "id": "x" }] } })).is_empty());
        assert!(models(&json!({
            "security": { "auth": { "selectedType": "anthropic" } },
            "modelProviders": { "openai": [{ "id": "x" }] },
        }))
        .is_empty());
    }

    fn shell_for(dir: &Path) -> HashMap<String, String> {
        HashMap::from([
            ("HOME".to_string(), dir.to_string_lossy().into_owned()),
            (
                "QWEN_CODE_SYSTEM_SETTINGS_PATH".to_string(),
                dir.join("system.json").to_string_lossy().into_owned(),
            ),
        ])
    }

    /// The list is read from the pinned account's `settings.json`, comments and all; an account
    /// with no settings lists none, and one whose settings cannot be read says why.
    #[test]
    fn the_pinned_accounts_settings_file_is_read() {
        let scratch = ScratchDir::new("qwen-models");
        let account = scratch.join("account");
        std::fs::create_dir_all(&account).unwrap();
        let shell = shell_for(&scratch);
        let list = |dir: &Path| QwenCatalogue.list(&shell, Some(dir), dir, Duration::from_secs(1));

        let missing = list(&account);
        assert!(missing.models.is_empty());
        assert!(missing.unavailable.is_none());

        std::fs::write(
            account.join(QWEN_SETTINGS_FILE),
            "// mine\n{\"security\":{\"auth\":{\"selectedType\":\"openai\"}},\
             \"modelProviders\":{\"openai\":[{\"id\":\"m1\",\"name\":\"M1\"}]}}",
        )
        .unwrap();
        assert_eq!(ids(&list(&account).models), [("m1", "M1")]);

        std::fs::write(account.join(QWEN_SETTINGS_FILE), "[1, 2]").unwrap();
        let unusable = list(&account);
        assert!(unusable.models.is_empty());
        assert!(unusable.unavailable.is_some());
    }

    /// The system settings file and the system defaults are merged with the account's as Qwen Code
    /// merges them: the system file's method and lists win, a method only the account lists is
    /// kept, and the defaults give what nothing above them does.
    #[test]
    fn the_system_layers_are_merged_with_the_accounts() {
        let scratch = ScratchDir::new("qwen-models-layers");
        let account = scratch.join("account");
        std::fs::create_dir_all(&account).unwrap();
        let mut shell = shell_for(&scratch);
        shell.insert(
            "QWEN_CODE_SYSTEM_DEFAULTS_PATH".to_string(),
            scratch.join("defaults.json").to_string_lossy().into_owned(),
        );
        let list = || {
            ids(&QwenCatalogue
                .list(&shell, Some(&account), &account, Duration::from_secs(1))
                .models)
            .into_iter()
            .map(|(id, _)| id.to_string())
            .collect::<Vec<_>>()
        };
        std::fs::write(
            account.join(QWEN_SETTINGS_FILE),
            r#"{"security":{"auth":{"selectedType":"gemini"}},
                "modelProviders":{"openai":[{"id":"user-openai"}],"gemini":[{"id":"user-gemini"}]}}"#,
        )
        .unwrap();
        std::fs::write(
            scratch.join("defaults.json"),
            r#"{"security":{"auth":{"selectedType":"anthropic"}},
                "modelProviders":{"anthropic":[{"id":"default-claude"}]}}"#,
        )
        .unwrap();
        // The account's method wins over the defaults'.
        assert_eq!(list(), ["user-gemini"]);
        std::fs::write(
            scratch.join("system.json"),
            r#"{"security":{"auth":{"selectedType":"openai"}},
                "modelProviders":{"openai":[{"id":"sys-openai"}]}}"#,
        )
        .unwrap();
        // The system file's method, and its `openai` list in place of the account's.
        assert_eq!(list(), ["sys-openai"]);
        std::fs::write(
            scratch.join("system.json"),
            r#"{"security":{"auth":{"selectedType":"anthropic"}}}"#,
        )
        .unwrap();
        // A method chosen above, listed only by the defaults.
        assert_eq!(list(), ["default-claude"]);
        std::fs::write(scratch.join("system.json"), "[1]").unwrap();
        let unusable = QwenCatalogue.list(&shell, Some(&account), &account, Duration::from_secs(1));
        assert!(unusable.models.is_empty());
        assert!(unusable.unavailable.is_some());
    }

    /// The model the settings name: the system file's over the project's over the account's.
    #[test]
    fn the_configured_model_is_the_highest_layers() {
        let scratch = ScratchDir::new("qwen-configured-model");
        let account = scratch.join("account");
        let project = scratch.join("project");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::create_dir_all(project.join(".qwen")).unwrap();
        let shell = shell_for(&scratch);
        let configured =
            || QwenCatalogue.configured_model(&shell, Some(&account), Some(project.as_path()));

        assert_eq!(configured(), None);
        std::fs::write(
            account.join(QWEN_SETTINGS_FILE),
            r#"{"model":{"name":"from-account"}}"#,
        )
        .unwrap();
        assert_eq!(configured().as_deref(), Some("from-account"));
        std::fs::write(
            project.join(".qwen").join(QWEN_SETTINGS_FILE),
            r#"{"model":{"name":"from-project"}}"#,
        )
        .unwrap();
        assert_eq!(configured().as_deref(), Some("from-project"));
        std::fs::write(
            scratch.join("system.json"),
            r#"{"model":{"name":"from-system"}}"#,
        )
        .unwrap();
        assert_eq!(configured().as_deref(), Some("from-system"));
        // A system file past the size a launch reads it in names nothing, as it lists nothing.
        let padding = " ".repeat(SYSTEM_FILE_LIMIT);
        std::fs::write(
            scratch.join("system.json"),
            format!(r#"{{"model":{{"name":"from-system"}}}}{padding}"#),
        )
        .unwrap();
        assert_eq!(configured().as_deref(), Some("from-project"));
    }
}
