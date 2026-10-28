//! The reasoning effort an agent runs a session at when nothing sets one, for a session whose
//! records name the model it runs but no effort. The model and effort shown for a session are
//! decided in "The model and effort a session is shown running with" in
//! `docs/product/model-and-reasoning-effort.md`; this value stands in there for an effort the
//! records leave unset.
//!
//! Only Codex needs it. A Codex turn's context record carries the effort its configuration
//! resolved to, every layer included (the account's `config.toml`, a trusted project's own
//! `.codex/config.toml`, a managed policy, a `-c` override), and leaves it out when none sets one;
//! Codex then runs the model at the `default_reasoning_level` its catalogue gives that model, a
//! catalogue it caches per account in `models_cache.json`. A context record that names a model and
//! no effort therefore says exactly which entry applies. Before the first turn there is no such
//! record, and the configuration layers are not read in its place: which of them apply depends on
//! the project's trust and on policy this daemon cannot see. Claude Code records the effort on
//! every reply and Grok Build's summary records it as configured, so neither leaves a gap for a
//! default to fill.

use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapter::files::read_regular_text;
use crate::agent_records::Locator;
use crate::line_file::{Signature, JSON_FILE_LIMIT};
use crate::protocol::Agent;

/// Codex's catalogue cache inside an account's home.
const CODEX_CATALOGUE_FILE: &str = "models_cache.json";

/// Each account's catalogue defaults, read again when the file changes and looked at no more than
/// once per pass of the collector.
#[derive(Default)]
pub struct DefaultEfforts {
    catalogues: HashMap<PathBuf, Catalogue>,
    /// The accounts whose file was looked at during the current pass.
    checked: HashSet<PathBuf>,
}

#[derive(Default)]
struct Catalogue {
    signature: Option<Signature>,
    /// Each model's default level, by the id its sessions run it under; `None` until the file has
    /// been read, or found missing.
    defaults: Option<HashMap<String, String>>,
}

impl DefaultEfforts {
    /// Starts a pass over the sessions. An account no session asked about during the last pass is
    /// forgotten.
    pub fn begin_pass(&mut self) {
        let checked = std::mem::take(&mut self.checked);
        self.catalogues.retain(|root, _| checked.contains(root));
    }

    /// The effort a session of `agent` under the account `config_dir` runs at by default, given the
    /// model and effort its records show: `Some(None)` where there is none to give, and `None` where
    /// it could not be looked up — the account's directory or its catalogue could not be read —
    /// and whatever was found before still stands.
    pub fn of(
        &mut self,
        agent: Agent,
        config_dir: Option<&str>,
        model: Option<&str>,
        effort: Option<&str>,
        locator: &Locator,
    ) -> Option<Option<String>> {
        let (Agent::Codex, Some(model), None) = (agent, model, effort) else {
            return Some(None);
        };
        let root = locator.root(agent, config_dir)?;
        let catalogue = self.catalogues.entry(root.clone()).or_default();
        if self.checked.insert(root.clone()) {
            catalogue.refresh(&root.join(CODEX_CATALOGUE_FILE));
        }
        let defaults = catalogue.defaults.as_ref()?;
        Some(defaults.get(model).cloned())
    }
}

impl Catalogue {
    /// Rereads the file when its size or modification time changed. A file that is gone leaves no
    /// defaults, since the account no longer vouches for any; one that cannot be read or parsed
    /// leaves the last ones read, as it may be caught halfway through being rewritten.
    fn refresh(&mut self, path: &Path) {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                *self = Self {
                    signature: None,
                    defaults: Some(HashMap::new()),
                };
                return;
            }
            Err(_) => return,
        };
        let signature = (metadata.len(), metadata.modified().ok());
        if self.signature == Some(signature) {
            return;
        }
        if metadata.len() > JSON_FILE_LIMIT {
            self.signature = Some(signature);
            return;
        }
        let Ok(text) = read_regular_text(path, JSON_FILE_LIMIT as usize) else {
            return;
        };
        self.signature = Some(signature);
        if let Ok(catalogue) = serde_json::from_str::<Value>(&text) {
            self.defaults = Some(codex_defaults(&catalogue));
        }
    }
}

/// Each entry's `default_reasoning_level` by its `slug`, hidden entries included, since a session
/// can run a model the pickers do not list.
fn codex_defaults(catalogue: &Value) -> HashMap<String, String> {
    let Some(entries) = catalogue.get("models").and_then(Value::as_array) else {
        return HashMap::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let slug = entry.get("slug").and_then(Value::as_str)?;
            let level = entry
                .get("default_reasoning_level")
                .and_then(Value::as_str)?;
            (!slug.is_empty() && !level.is_empty()).then(|| (slug.to_owned(), level.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    const CATALOGUE: &str = include_str!("../testdata/codex_models_cache_0.162.1.json");

    fn lookup(
        defaults: &mut DefaultEfforts,
        account: &Path,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Option<Option<String>> {
        let account = account.to_string_lossy();
        defaults.of(
            Agent::Codex,
            Some(&account),
            model,
            effort,
            &Locator::default(),
        )
    }

    #[test]
    fn no_default_stands_in_where_the_records_say_the_effort_or_not_the_model() {
        let account = ScratchDir::new("default-effort-gaps");
        std::fs::write(account.join(CODEX_CATALOGUE_FILE), CATALOGUE).unwrap();
        let mut defaults = DefaultEfforts::default();
        defaults.begin_pass();
        for (model, effort) in [
            (Some("gpt-6.1-sol"), Some("high")),
            (None, None),
            (Some("not-in-the-catalogue"), None),
        ] {
            assert_eq!(lookup(&mut defaults, &account, model, effort), Some(None));
        }
        let grok = defaults.of(
            Agent::Grok,
            Some(&account.to_string_lossy()),
            Some("gpt-6.1-sol"),
            None,
            &Locator::default(),
        );
        assert_eq!(grok, Some(None));
    }

    #[test]
    fn the_catalogue_is_read_again_only_on_a_new_pass_after_it_changed() {
        let account = ScratchDir::new("default-effort-refresh");
        let path = account.join(CODEX_CATALOGUE_FILE);
        std::fs::write(&path, CATALOGUE).unwrap();
        let mut defaults = DefaultEfforts::default();
        let sol =
            |defaults: &mut DefaultEfforts| lookup(defaults, &account, Some("gpt-6.1-sol"), None);
        defaults.begin_pass();
        assert_eq!(sol(&mut defaults), Some(Some("medium".to_owned())));
        let changed = CATALOGUE.replacen(
            r#""default_reasoning_level": "medium""#,
            r#""default_reasoning_level": "low""#,
            1,
        );
        assert_ne!(changed, CATALOGUE);
        std::fs::write(&path, changed).unwrap();
        assert_eq!(sol(&mut defaults), Some(Some("medium".to_owned())));
        defaults.begin_pass();
        assert_eq!(sol(&mut defaults), Some(Some("low".to_owned())));
        std::fs::remove_file(&path).unwrap();
        defaults.begin_pass();
        assert_eq!(sol(&mut defaults), Some(None));
    }

    /// A catalogue caught mid-write is no answer: the level read before stands, and with none read
    /// before there is nothing to say either way.
    #[test]
    fn a_catalogue_that_cannot_be_parsed_says_nothing_rather_than_no_default() {
        let account = ScratchDir::new("default-effort-unreadable");
        let path = account.join(CODEX_CATALOGUE_FILE);
        std::fs::write(&path, "{\"models\": [").unwrap();
        let mut defaults = DefaultEfforts::default();
        defaults.begin_pass();
        assert_eq!(
            lookup(&mut defaults, &account, Some("gpt-6.1-sol"), None),
            None
        );
        std::fs::write(&path, CATALOGUE).unwrap();
        defaults.begin_pass();
        assert_eq!(
            lookup(&mut defaults, &account, Some("gpt-6.1-sol"), None),
            Some(Some("medium".to_owned()))
        );
        std::fs::write(&path, "{").unwrap();
        defaults.begin_pass();
        assert_eq!(
            lookup(&mut defaults, &account, Some("gpt-6.1-sol"), None),
            Some(Some("medium".to_owned()))
        );
    }

    #[test]
    fn bounded_agent_files_preserve_catalogue_defaults() {
        use crate::adapter::tests::{agent_file, bounded_file_cases};
        bounded_file_cases(
            "default_effort::tests::bounded_agent_files_preserve_catalogue_defaults",
            &["regular", "linked", "fifo", "oversized"],
            |case| {
                let account = ScratchDir::new("bounded-catalogue");
                let path = account.join(CODEX_CATALOGUE_FILE);
                agent_file(&path, case, CATALOGUE, JSON_FILE_LIMIT as usize);
                let mut defaults = DefaultEfforts::default();
                let expected =
                    matches!(case, "regular" | "linked").then(|| Some("medium".to_owned()));
                assert_eq!(
                    lookup(&mut defaults, &account, Some("gpt-6.1-sol"), None),
                    expected
                );
                std::fs::remove_file(&path).unwrap();
                std::fs::write(&path, CATALOGUE).unwrap();
                defaults.begin_pass();
                assert_eq!(
                    lookup(&mut defaults, &account, Some("gpt-6.1-sol"), None),
                    Some(Some("medium".to_owned()))
                );
                agent_file(&path, case, CATALOGUE, JSON_FILE_LIMIT as usize);
                defaults.begin_pass();
                assert_eq!(
                    lookup(&mut defaults, &account, Some("gpt-6.1-sol"), None),
                    Some(Some("medium".to_owned()))
                );
            },
        );
    }
}
