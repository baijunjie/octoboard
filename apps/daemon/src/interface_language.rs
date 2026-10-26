//! The languages Octoboard's interface is offered in, as the daemon knows them: the tag the
//! client sends when the user picks one, and the English name sessions are told it by.
//!
//! The table mirrors `LANGUAGES` in `packages/ui/src/i18n/languages.ts`; a tag the interface does
//! not offer is refused, so nothing but a language the user could have picked is ever put into a
//! session's instructions.

/// Each language's tag and its name in English, in the order the interface lists them.
const LANGUAGES: &[(&str, &str)] = &[
    ("ar", "Arabic"),
    ("de", "German"),
    ("en", "English"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("hi", "Hindi"),
    ("id", "Indonesian"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("pt-BR", "Brazilian Portuguese"),
    ("ru", "Russian"),
    ("th", "Thai"),
    ("tr", "Turkish"),
    ("vi", "Vietnamese"),
    ("zh-Hans", "Simplified Chinese"),
    ("zh-Hant", "Traditional Chinese"),
];

/// The language as sessions are told it, such as `Simplified Chinese (zh-Hans)`; `None` for a tag
/// the interface does not offer.
pub fn display_name(tag: &str) -> Option<String> {
    LANGUAGES
        .iter()
        .find(|(known, _)| *known == tag)
        .map(|(known, name)| format!("{name} ({known})"))
}

/// Whether the interface offers `tag`.
pub fn is_offered(tag: &str) -> bool {
    display_name(tag).is_some()
}

/// Whether `tag` is English in any region. Sessions are told nothing for English: they already
/// follow the language the user writes in, and English must not be forced on someone who writes
/// in another.
pub fn is_english(tag: &str) -> bool {
    tag == "en" || tag.starts_with("en-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_named_in_english_beside_itself() {
        assert_eq!(
            display_name("zh-Hans").as_deref(),
            Some("Simplified Chinese (zh-Hans)")
        );
        assert_eq!(
            display_name("zh-Hant").as_deref(),
            Some("Traditional Chinese (zh-Hant)")
        );
        assert_eq!(
            display_name("pt-BR").as_deref(),
            Some("Brazilian Portuguese (pt-BR)")
        );
        assert_eq!(display_name("ja").as_deref(), Some("Japanese (ja)"));
    }

    #[test]
    fn a_tag_the_interface_does_not_offer_has_no_name() {
        for tag in ["", "xx", "zh", "pt", "ZH-Hans", "zh-hans", "en-GB", " ja"] {
            assert_eq!(display_name(tag), None, "{tag:?}");
        }
    }

    #[test]
    fn a_tag_is_offered_only_when_the_interface_lists_it() {
        assert!(is_offered("zh-Hans"));
        assert!(!is_offered("zh"));
        assert!(!is_offered("en-GB"));
    }

    /// The table must not drift from the interface's own list: a language added there and not here
    /// could be picked but never reach a session.
    #[test]
    fn the_table_offers_the_tags_the_interface_does() {
        let source = include_str!("../../../packages/ui/src/i18n/languages.ts");
        let list = source
            .split("export const LANGUAGES = [")
            .nth(1)
            .and_then(|rest| rest.split("] as const").next())
            .expect("LANGUAGES list in languages.ts");
        let mut ui: Vec<&str> = list
            .split(',')
            .map(|item| item.trim().trim_matches('"'))
            .filter(|tag| !tag.is_empty())
            .collect();
        let mut daemon: Vec<&str> = LANGUAGES.iter().map(|(tag, _)| *tag).collect();
        ui.sort_unstable();
        daemon.sort_unstable();
        assert_eq!(ui, daemon);
    }

    #[test]
    fn english_is_any_en_tag() {
        for tag in ["en", "en-GB", "en-US"] {
            assert!(is_english(tag), "{tag}");
        }
        for tag in ["de", "es", "id", "enx", ""] {
            assert!(!is_english(tag), "{tag}");
        }
    }
}
