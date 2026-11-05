//! Claude Code's gate for its native draft stash: Ctrl-S stashes the draft only on a measured
//! version, and only while the keybindings file of the configuration directory the launch
//! actually uses leaves that key bound to `chat:stash`.

use std::io::ErrorKind;

use serde_json::Value;

use crate::adapter::{self, LaunchPlan, LaunchSpec};
use crate::protocol::AgentId;
use crate::providers::claude::CLAUDE_KEY_BINDINGS;
use crate::providers::layout::layout;

/// The first version measured to stash with Ctrl-S.
const MINIMUM: adapter::AgentVersion = (2, 1, 274);

/// Whether this launch may use the stash. Reading the plan's environment matters for pinned
/// accounts, whose configuration directory the plan sets.
pub(super) fn supported(spec: &LaunchSpec<'_>, plan: &LaunchPlan) -> bool {
    if !spec.agent_version.is_some_and(|version| version >= MINIMUM) {
        return false;
    }
    let variable = layout(AgentId::CLAUDE).config_dir_var;
    let directory = plan
        .env
        .iter()
        .rev()
        .find(|(key, _)| key == variable)
        .map(|(_, value)| std::path::PathBuf::from(value))
        .unwrap_or_else(|| adapter::account_dir(AgentId::CLAUDE, spec.config_dir, spec.shell_env));
    if directory.as_os_str().is_empty() {
        return false;
    }
    let directory = if directory.is_absolute() {
        directory
    } else {
        spec.cwd.join(directory)
    };
    // A missing file keeps the default; a dangling symlink or an unreadable/nonregular file
    // cannot establish which binding the agent loads.
    let path = directory.join(CLAUDE_KEY_BINDINGS);
    match crate::hostfs::read_regular_text(&path, 1024 * 1024) {
        Ok(text) => default_stash_binding(&text),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            matches!(std::fs::symlink_metadata(path), Err(error) if error.kind() == ErrorKind::NotFound)
        }
        Err(_) => false,
    }
}

fn default_stash_binding(text: &str) -> bool {
    let Ok(Value::Object(document)) = serde_json::from_str::<Value>(text) else {
        return false;
    };
    if document
        .keys()
        .any(|key| !matches!(key.as_str(), "$schema" | "$docs" | "bindings"))
    {
        return false;
    }
    if document.get("$schema").is_some_and(|value| {
        value.as_str() != Some("https://www.schemastore.org/claude-code-keybindings.json")
    }) || document
        .get("$docs")
        .is_some_and(|value| !value.is_string())
    {
        return false;
    }
    let Some(blocks) = document.get("bindings").and_then(Value::as_array) else {
        return false;
    };
    for block in blocks {
        let Some(block) = block.as_object() else {
            return false;
        };
        if block
            .keys()
            .any(|key| !matches!(key.as_str(), "context" | "bindings"))
        {
            return false;
        }
        let Some(context) = block.get("context").and_then(Value::as_str) else {
            return false;
        };
        if !matches!(
            context,
            "Global"
                | "Chat"
                | "DiffPanel"
                | "FeedbackSurvey"
                | "Autocomplete"
                | "Settings"
                | "Confirmation"
                | "Tabs"
                | "Transcript"
                | "HistorySearch"
                | "Task"
                | "ThemePicker"
                | "Scroll"
                | "Help"
                | "Attachments"
                | "Footer"
                | "Select"
                | "Plugin"
                | "Agents"
                | "ProactivityMenu"
        ) {
            return false;
        }
        let Some(bindings) = block.get("bindings").and_then(Value::as_object) else {
            return false;
        };
        for (key, action) in bindings {
            if key.is_empty() || (!action.is_null() && !action.is_string()) {
                return false;
            }
            let key = key
                .to_ascii_lowercase()
                .replace(" +", "+")
                .replace("+ ", "+");
            let mut chord = key.split_whitespace();
            let first = chord.next();
            if matches!(context, "Global" | "Chat")
                && matches!(first, Some("ctrl+s" | "control+s"))
                && (context != "Chat"
                    || first != Some("ctrl+s")
                    || chord.next().is_some()
                    || action.as_str() != Some("chat:stash"))
            {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::spec_fixture;

    /// A version before the first measured one, or one that cannot be told, keeps the draft guard.
    #[test]
    fn an_unmeasured_version_does_not_stash() {
        let mut fixture = spec_fixture();
        for version in [None, Some((2, 1, 273))] {
            fixture.agent_version = version;
            assert!(
                !supported(&fixture.spec(), &LaunchPlan::default()),
                "{version:?}"
            );
        }
    }

    #[test]
    fn the_actual_launch_config_controls_the_binding() {
        let mut fixture = spec_fixture();
        fixture.agent_version = Some((2, 1, 274));
        let default = fixture.scratch.join("default");
        let pinned = fixture.scratch.join("pinned");
        let actual = fixture.scratch.join("actual");
        for directory in [&default, &pinned, &actual] {
            std::fs::create_dir_all(directory).unwrap();
        }
        fixture.shell_env.insert(
            "CLAUDE_CONFIG_DIR".into(),
            default.to_string_lossy().into_owned(),
        );
        fixture.config_dir = Some(pinned.clone());
        std::fs::write(pinned.join("keybindings.json"), "invalid").unwrap();
        assert!(!supported(&fixture.spec(), &LaunchPlan::default()));
        let plan = LaunchPlan {
            env: vec![(
                "CLAUDE_CONFIG_DIR".into(),
                actual.to_string_lossy().into_owned(),
            )],
            ..Default::default()
        };
        assert!(supported(&fixture.spec(), &plan));
        std::fs::write(
            actual.join("keybindings.json"),
            r#"{"bindings":[{"context":"Chat","bindings":{"ctrl+s":null}}]}"#,
        )
        .unwrap();
        assert!(!supported(&fixture.spec(), &plan));
        std::fs::remove_file(actual.join("keybindings.json")).unwrap();
        crate::test_support::symlink(actual.join("missing"), actual.join("keybindings.json"))
            .unwrap();
        assert!(!supported(&fixture.spec(), &plan));
    }

    #[test]
    fn ambiguous_or_rebound_shortcuts_fail_closed() {
        for text in [
            "null",
            "[]",
            "{}",
            r#"{"bindings":{},"future":true}"#,
            r#"{"bindings":[{"context":"Future","bindings":{}}]}"#,
            r#"{"bindings":[{"context":"Chat","bindings":{"ctrl+s":null}}]}"#,
            r#"{"bindings":[{"context":"Chat","bindings":{"Ctrl + S":"chat:submit"}}]}"#,
            r#"{"bindings":[{"context":"Global","bindings":{"ctrl+s":"chat:stash"}}]}"#,
            r#"{"bindings":[{"context":"Chat","bindings":{"ctrl+s x":"chat:stash"}}]}"#,
        ] {
            assert!(!default_stash_binding(text), "{text}");
        }
        for text in [
            r#"{"bindings":[]}"#,
            r#"{"bindings":[{"context":"Chat","bindings":{"ctrl+s":"chat:stash","ctrl+x ctrl+s":"chat:sendNow"}}]}"#,
            r#"{"bindings":[{"context":"Plugin","bindings":{"ctrl+s":"plugin:cycleMarketplace"}}]}"#,
        ] {
            assert!(default_stash_binding(text), "{text}");
        }
    }
}
