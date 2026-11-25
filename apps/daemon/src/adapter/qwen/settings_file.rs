//! Reading one of Qwen Code's settings files the way Qwen Code reads it, through the shared reader
//! of a user's JSON (`crate::adapter::user_json`): a UTF-8 byte order mark and JavaScript comments
//! are taken off before the JSON is parsed, and only an object is settings (0.28.3:
//! `JSON.parse(stripJsonComments(stripUtf8Bom(content)))`). Qwen Code keeps the comments of a file
//! it rewrites, so a file edited by hand stays that way.

use std::path::Path;

use serde_json::{Map, Value};

use crate::adapter::files::read_bounded_text;
use crate::adapter::user_json::{self, Takes};
use crate::hostfs::read_regular_text;

/// What a settings file holds.
#[derive(Debug, PartialEq)]
pub(super) enum SettingsFile {
    /// There is no such file.
    Missing,
    /// The file is there and Qwen Code could not use it either: it cannot be read, or it is not
    /// a JSON object once its comments are off. The text says why, in English, for a log or an
    /// error's detail.
    Unusable(String),
    Settings(Map<String, Value>),
}

impl SettingsFile {
    /// The settings, where the file holds any.
    pub(super) fn settings(self) -> Option<Value> {
        match self {
            SettingsFile::Settings(settings) => Some(Value::Object(settings)),
            SettingsFile::Missing | SettingsFile::Unusable(_) => None,
        }
    }
}

/// Reads the settings file at `path`, within `limit` bytes: a file of the user's or the host's,
/// read wherever it leads.
pub(super) fn read(path: &Path, limit: usize) -> SettingsFile {
    settings_of(read_regular_text(path, limit))
}

/// Reads the settings file at `path` that `project` (canonical) keeps, within `limit` bytes: a
/// file that leads out of the project is not followed, and is there and unusable.
pub(super) fn read_in_project(path: &Path, project: &Path, limit: usize) -> SettingsFile {
    settings_of(read_bounded_text(path, project, limit))
}

/// What the text read from a settings file, or the failure to read it, comes to.
fn settings_of(read: std::io::Result<String>) -> SettingsFile {
    match read {
        Ok(text) => match parse(&text) {
            Ok(settings) => SettingsFile::Settings(settings),
            Err(why) => SettingsFile::Unusable(why),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => SettingsFile::Missing,
        Err(err) => SettingsFile::Unusable(err.to_string()),
    }
}

/// What Qwen Code takes beyond JSON in a settings file (0.28.3): a byte order mark and comments,
/// and no comma before a closing bracket or brace.
const TAKES: Takes = Takes {
    byte_order_mark: true,
    comments: true,
    trailing_commas: false,
    leading_comment_lines: false,
};

/// The settings `text` holds, or why it holds none.
pub(super) fn parse(text: &str) -> Result<Map<String, Value>, String> {
    match user_json::parse(text, TAKES) {
        Ok(Value::Object(settings)) => Ok(settings),
        Ok(_) => Err("the file is not a JSON object".to_string()),
        Err(err) => Err(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_support::ScratchDir;

    fn parsed(text: &str) -> Value {
        Value::Object(parse(text).unwrap_or_else(|why| panic!("{text:?}: {why}")))
    }

    /// `//` and `/* */` comments are taken off, wherever they stand outside a string.
    #[test]
    fn comments_outside_strings_are_stripped() {
        let text = "{\n  // a line comment\n  \"a\": 1, /* a block\n  spanning lines */ \"b\": \"x\", // trailing\n  /* before */ \"c\": [1, /* inside */ 2]\n}\n// after the object";
        assert_eq!(parsed(text), json!({ "a": 1, "b": "x", "c": [1, 2] }));
        // A comment of any kind is also the whole of the text between values.
        assert_eq!(
            parsed("/**/{/**/\"a\"/**/:/**/1/**/}/**/"),
            json!({ "a": 1 })
        );
    }

    /// Comment markers inside a string are text, whatever escapes come before them.
    #[test]
    fn comment_markers_inside_strings_are_kept() {
        let text = r#"{
  "url": "https://example.test/a//b",
  "block": "/* not a comment */ text",
  "quote": "escaped \" // still the string",
  "path": "C:\\" // a comment after a string that ends in an escaped backslash
}"#;
        assert_eq!(
            parsed(text),
            json!({
                "url": "https://example.test/a//b",
                "block": "/* not a comment */ text",
                "quote": "escaped \" // still the string",
                "path": "C:\\",
            })
        );
    }

    #[test]
    fn a_byte_order_mark_is_taken_off() {
        assert_eq!(parsed("\u{FEFF}{\"a\": 1}"), json!({ "a": 1 }));
        assert_eq!(parsed("\u{FEFF}// comment\n{\"a\": 1}"), json!({ "a": 1 }));
    }

    /// Only an object is settings, and what is not one says why.
    #[test]
    fn text_that_is_not_a_json_object_is_unusable_with_a_reason() {
        for text in [
            "",
            "   \n",
            "// only a comment",
            "[1, 2]",
            "\"text\"",
            "null",
            "7",
            "{ not json",
            "{\"a\": 1,}",
        ] {
            let why = parse(text).expect_err(text);
            assert!(!why.trim().is_empty(), "{text:?}");
        }
        assert!(parse("[1, 2]").expect_err("array").contains("object"));
    }

    /// A file that is not there is missing, and one that is there and cannot be used is not.
    #[test]
    fn a_missing_file_is_not_an_unusable_one() {
        let dir = ScratchDir::new("qwen-settings-file");
        assert_eq!(read(&dir.join("none.json"), 1024), SettingsFile::Missing);

        std::fs::write(dir.join("broken.json"), "{ not json").expect("file");
        assert!(matches!(
            read(&dir.join("broken.json"), 1024),
            SettingsFile::Unusable(why) if !why.is_empty()
        ));

        std::fs::write(dir.join("array.json"), "[1, 2]").expect("file");
        assert!(matches!(
            read(&dir.join("array.json"), 1024),
            SettingsFile::Unusable(_)
        ));

        // A directory is there, and is no settings file.
        std::fs::create_dir(dir.join("a-directory.json")).expect("directory");
        assert!(matches!(
            read(&dir.join("a-directory.json"), 1024),
            SettingsFile::Unusable(_)
        ));
    }

    #[test]
    fn a_file_is_read_with_its_comments_and_mark_off_and_within_its_limit() {
        let dir = ScratchDir::new("qwen-settings-file-limit");
        let text = "\u{FEFF}{\n // kept out\n \"model\": { \"name\": \"m\" }\n}\n";
        std::fs::write(dir.join("settings.json"), text).expect("file");
        assert_eq!(
            read(&dir.join("settings.json"), text.len()).settings(),
            Some(json!({ "model": { "name": "m" } }))
        );
        // Over the limit it is not read at all, and is no empty layer.
        assert!(matches!(
            read(&dir.join("settings.json"), text.len() - 1),
            SettingsFile::Unusable(_)
        ));
    }

    #[test]
    fn only_a_usable_file_has_settings() {
        assert_eq!(SettingsFile::Missing.settings(), None);
        assert_eq!(SettingsFile::Unusable("why".to_string()).settings(), None);
        let map = parse("{\"a\": 1}").expect("object");
        assert_eq!(
            SettingsFile::Settings(map).settings(),
            Some(json!({ "a": 1 }))
        );
    }
}
