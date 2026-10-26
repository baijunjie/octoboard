//! Brings a configuration document of an older version up to the current one, so an export a user
//! kept stays importable after [`config_export::VERSION`] is raised. `apps/daemon/PROTOCOL.md`
//! ("Configuration export", the "Version." bullet) is the specification.
//!
//! The upgrade works on the raw JSON, before the document is read into
//! [`config_export::ConfigurationDocument`], which only has the current shape. Each step turns the
//! JSON of one version into that of the next; a field that was merely added or renamed is not a
//! step, since serde's `default` and `alias` on the struct read it; a field given another shape, or
//! a container renamed with the fields in it changed, is.
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
const STEPS: &[Step] = &[worktrees_to_git_workflow];

/// Version 1 to 2: the worktree settings became the git workflow settings. The application-wide
/// container `settings.worktrees` and each project's `worktrees` are renamed `git_workflow`, and
/// their boolean `use_worktrees` becomes `development`: `true` is `"worktree"`, `false`
/// `"in_place"`, and a project's `null`, which follows the application-wide value, stays `null`.
fn worktrees_to_git_workflow(document: &mut Map<String, Value>) {
    if let Some(settings) = document.get_mut("settings").and_then(Value::as_object_mut) {
        rename_worktrees(settings);
    }
    let consoles = document.get_mut("consoles").and_then(Value::as_array_mut);
    for console in consoles.into_iter().flatten() {
        let projects = console.get_mut("projects").and_then(Value::as_array_mut);
        for project in projects.into_iter().flatten() {
            if let Some(project) = project.as_object_mut() {
                rename_worktrees(project);
            }
        }
    }
}

/// Moves the `worktrees` entry of `owner` to `git_workflow`, with `use_worktrees` turned into
/// `development`. An entry that is not an object is left under its old name, where nothing reads
/// it and the item follows the application-wide value.
fn rename_worktrees(owner: &mut Map<String, Value>) {
    match owner.remove("worktrees") {
        Some(Value::Object(mut items)) => {
            if let Some(use_worktrees) = items.remove("use_worktrees") {
                let development = match use_worktrees {
                    Value::Bool(true) => Value::from("worktree"),
                    Value::Bool(false) => Value::from("in_place"),
                    other => other,
                };
                items.insert("development".to_string(), development);
            }
            owner.insert("git_workflow".to_string(), Value::Object(items));
        }
        Some(other) => {
            owner.insert("worktrees".to_string(), other);
        }
        None => {}
    }
}

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

    #[test]
    fn the_worktree_settings_of_a_first_version_document_become_the_git_workflow_settings() {
        let mut document = json!({
            "format": config_export::FORMAT,
            "version": 1,
            "settings": {
                "auto_pull_repositories": true,
                "worktrees": {
                    "use_worktrees": false,
                    "rebase_before_merge": true,
                    "delivery": "merge",
                    "revert_gate": true,
                },
            },
            "consoles": [{
                "name": "Work",
                "projects": [
                    { "name": "off", "worktrees": { "use_worktrees": false, "delivery": null } },
                    { "name": "on", "worktrees": { "use_worktrees": true } },
                    { "name": "follows", "worktrees": { "use_worktrees": null, "revert_gate": true } },
                    { "name": "none" },
                ],
            }],
        })
        .as_object()
        .unwrap()
        .clone();

        upgrade(&mut document, 1);

        assert_eq!(
            Value::Object(document),
            json!({
                "format": config_export::FORMAT,
                "version": 2,
                "settings": {
                    "auto_pull_repositories": true,
                    "git_workflow": {
                        "development": "in_place",
                        "rebase_before_merge": true,
                        "delivery": "merge",
                        "revert_gate": true,
                    },
                },
                "consoles": [{
                    "name": "Work",
                    "projects": [
                        { "name": "off", "git_workflow": { "development": "in_place", "delivery": null } },
                        { "name": "on", "git_workflow": { "development": "worktree" } },
                        { "name": "follows", "git_workflow": { "development": null, "revert_gate": true } },
                        { "name": "none" },
                    ],
                }],
            })
        );
    }
}
