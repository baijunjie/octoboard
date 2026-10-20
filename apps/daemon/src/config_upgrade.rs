//! Brings a configuration document of an older version up to the current one, so an export a user
//! kept stays importable after [`config_export::VERSION`] is raised. `apps/daemon/PROTOCOL.md`
//! ("Configuration export", the "Version." bullet) is the specification.
//!
//! The upgrade works on the raw JSON, before the document is read into
//! [`config_export::ConfigurationDocument`], which only has the current shape. Each step turns the
//! JSON of one version into that of the next; a field that was merely added or renamed is not a
//! step, since serde's `default` and `alias` on the struct read it.
//!
//! To raise the version: write a step function and append it to [`STEPS`]. Raising
//! [`config_export::VERSION`] without it does not compile.

use serde_json::{Map, Value};

use crate::config_export;

/// Turns the document of one version into that of the next. The version field is set by [`upgrade`].
/// A step keeps the count and order of `consoles` and of each console's `projects`: an import's
/// resolutions and inspection results address them by index into the document the client sent.
type Step = fn(&mut Map<String, Value>);

/// `STEPS[n - 1]` upgrades a document of version `n` to version `n + 1`.
const STEPS: &[Step] = &[];

// A version without the step that leads to it would leave older documents unreadable.
const _: () = assert!(STEPS.len() + 1 == config_export::VERSION as usize);

/// Upgrades `document`, of version `from` (between 1 and [`config_export::VERSION`], which the
/// caller has checked), to the current version in place.
pub fn upgrade(document: &mut Map<String, Value>, from: u32) {
    for (step, version) in STEPS.iter().skip(from as usize - 1).zip(from + 1..) {
        step(document);
        document.insert("version".to_string(), Value::from(version));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_document_of_the_current_version_is_returned_unchanged() {
        let document = json!({
            "format": config_export::FORMAT,
            "version": config_export::VERSION,
            "consoles": [],
        });
        let mut upgraded = document.as_object().unwrap().clone();

        upgrade(&mut upgraded, config_export::VERSION);

        assert_eq!(Value::Object(upgraded), document);
    }

    #[test]
    fn a_document_of_the_first_version_is_brought_to_the_current_one() {
        let mut document = json!({
            "format": config_export::FORMAT,
            "version": 1,
            "consoles": [],
        })
        .as_object()
        .unwrap()
        .clone();

        upgrade(&mut document, 1);

        assert_eq!(document["version"], config_export::VERSION);
    }
}
