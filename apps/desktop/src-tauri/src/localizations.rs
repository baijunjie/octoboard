//! The shell's native catalogs (`localizations/<language>.json`, one for each of the UI's languages, named by its tag in
//! `packages/ui/src/i18n/languages.ts`), for text the shell writes where no UI is there to send it: the clean-up the
//! Windows uninstaller runs (`system_changes/headless.rs`). The same files hold the macOS permission prompts, which
//! `scripts/native-localizations.mjs` turns into bundle resources; that script keeps only those keys, and this module
//! reads the rest.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The language the catalogs fall back to: a language with no catalog, and a key missing from one, are English.
const FALLBACK: &str = "en";

/// Every catalog, by language tag; `native-localizations.test.mjs` holds the list to the files.
const SOURCES: [(&str, &str); 17] = [
    ("ar", include_str!("../localizations/ar.json")),
    ("de", include_str!("../localizations/de.json")),
    ("en", include_str!("../localizations/en.json")),
    ("es", include_str!("../localizations/es.json")),
    ("fr", include_str!("../localizations/fr.json")),
    ("hi", include_str!("../localizations/hi.json")),
    ("id", include_str!("../localizations/id.json")),
    ("it", include_str!("../localizations/it.json")),
    ("ja", include_str!("../localizations/ja.json")),
    ("ko", include_str!("../localizations/ko.json")),
    ("pt", include_str!("../localizations/pt.json")),
    ("ru", include_str!("../localizations/ru.json")),
    ("th", include_str!("../localizations/th.json")),
    ("tr", include_str!("../localizations/tr.json")),
    ("vi", include_str!("../localizations/vi.json")),
    ("zh-CN", include_str!("../localizations/zh-CN.json")),
    ("zh-TW", include_str!("../localizations/zh-TW.json")),
];

type Messages = HashMap<String, String>;

/// Every language tag with a catalog.
#[cfg(test)]
pub fn tags() -> impl Iterator<Item = &'static str> {
    SOURCES.iter().map(|(tag, _)| *tag)
}

fn catalogs() -> &'static HashMap<&'static str, Messages> {
    static CATALOGS: OnceLock<HashMap<&'static str, Messages>> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        SOURCES
            .iter()
            // A catalog that does not parse is left out, and its language falls back; the tests parse each one.
            .filter_map(|(tag, source)| Some((*tag, serde_json::from_str(source).ok()?)))
            .collect()
    })
}

/// One language's catalog, with English behind it.
#[derive(Clone, Copy)]
pub struct Catalog {
    messages: &'static Messages,
    fallback: &'static Messages,
}

impl Catalog {
    /// The catalog of the language tagged `language`, or English's where there is none (or none is given).
    pub fn of(language: Option<&str>) -> Self {
        let catalogs = catalogs();
        let empty = || {
            static EMPTY: OnceLock<Messages> = OnceLock::new();
            EMPTY.get_or_init(HashMap::new)
        };
        let fallback = catalogs.get(FALLBACK).unwrap_or_else(empty);
        let messages = language
            .and_then(|tag| catalogs.get(tag))
            .unwrap_or(fallback);
        Self { messages, fallback }
    }

    /// The message `key` with each `{name}` of `params` filled in, in one pass, so that a value holding braces is
    /// written as it is. A key no catalog has is written as itself.
    pub fn text(&self, key: &str, params: &[(&str, &str)]) -> String {
        let template = self
            .messages
            .get(key)
            .or_else(|| self.fallback.get(key))
            .map_or(key, String::as_str);
        let mut text = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            text.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let filled = after.find('}').and_then(|close| {
                let name = &after[..close];
                let value = params.iter().find(|(param, _)| *param == name)?.1;
                Some((value, &after[close + 1..]))
            });
            match filled {
                Some((value, remaining)) => {
                    text.push_str(value);
                    rest = remaining;
                }
                None => {
                    text.push('{');
                    rest = after;
                }
            }
        }
        text.push_str(rest);
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_parses_and_holds_english_s_keys() {
        let catalogs = catalogs();
        assert_eq!(catalogs.len(), SOURCES.len());
        let english = &catalogs[FALLBACK];
        for (tag, messages) in catalogs {
            let mut keys: Vec<_> = messages.keys().collect();
            let mut expected: Vec<_> = english.keys().collect();
            keys.sort();
            expected.sort();
            assert_eq!(keys, expected, "{tag}");
        }
    }

    #[test]
    fn an_unknown_or_missing_language_is_english() {
        let key = "uninstallCleanUp.notRead";
        let english = Catalog::of(Some("en")).text(key, &[]);
        assert_eq!(Catalog::of(None).text(key, &[]), english);
        assert_eq!(Catalog::of(Some("xx")).text(key, &[]), english);
        assert_ne!(Catalog::of(Some("ja")).text(key, &[]), english);
    }

    #[test]
    fn parameters_are_filled_once_and_unknown_braces_are_kept() {
        let catalog = Catalog::of(Some("en"));
        assert_eq!(
            catalog.text("uninstallCleanUp.saved", &[("path", "C:\\{path}")]),
            "This list is saved in C:\\{path}."
        );
        assert_eq!(catalog.text("no.such.key", &[]), "no.such.key");
    }
}
