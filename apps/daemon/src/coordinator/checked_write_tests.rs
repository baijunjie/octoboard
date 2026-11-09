//! Writes that carry the versions they were based on (`base_versions`), and the versions every
//! other write advances.

use std::collections::BTreeMap;

use serde_json::json;

use crate::coordinator::consoles::{update_console_checked, ConsoleChanges};
use crate::coordinator::handle;
use crate::coordinator::settings::{update_settings_checked, SettingsChanges};
use crate::protocol::{
    error_code, Account, AgentId, CodedError, Console, LaunchChange, Project, RequestBody, Settings,
};
use crate::providers::SettingKind;
use crate::state::{AppState, Stamp};
use std::sync::Arc;

fn request(body: serde_json::Value) -> RequestBody {
    serde_json::from_value(body).unwrap()
}

/// What a client holds of the settings after a `snapshot`.
fn read_settings(state: &AppState) -> Settings {
    let mut settings = state.store.get_settings().unwrap();
    state
        .field_versions
        .stamp_settings(&mut settings, Stamp::Snapshot);
    settings
}

fn read_console(state: &AppState) -> Console {
    let mut console = state.store.require_console("console-1").unwrap();
    state
        .field_versions
        .stamp_console(&mut console, Stamp::Snapshot);
    console
}

fn read_project(state: &AppState) -> Project {
    let mut project = state.store.require_project("project-1").unwrap();
    state
        .field_versions
        .stamp_project(&mut project, Stamp::Snapshot);
    project
}

fn read_account(state: &AppState, id: &str) -> Account {
    read_settings(state)
        .accounts
        .into_iter()
        .find(|account| account.id == id)
        .unwrap()
}

/// The conflict `result` was refused with: the paths and the versions it names.
fn conflict(result: anyhow::Result<Option<crate::protocol::Event>>) -> (String, serde_json::Value) {
    let err = result.unwrap_err();
    let coded = err.downcast_ref::<CodedError>().expect("a coded refusal");
    assert_eq!(coded.code, error_code::WRITE_CONFLICT);
    (
        coded.params["fields"].clone(),
        serde_json::from_str(&coded.params["versions"]).unwrap(),
    )
}

async fn send(
    state: &Arc<AppState>,
    body: serde_json::Value,
) -> anyhow::Result<Option<crate::protocol::Event>> {
    handle(state, None, request(body)).await
}

#[tokio::test]
async fn a_stale_settings_write_is_refused_whole_and_names_the_current_versions() {
    let (state, _dir) = crate::test_support::app_state("checked-settings-stale");
    let read = read_settings(&state);
    send(
        &state,
        json!({"type": "update_settings", "auto_pull_repositories": true}),
    )
    .await
    .unwrap();
    let current = read_settings(&state);
    assert!(current.versions["auto_pull_repositories"] > read.versions["auto_pull_repositories"]);

    let refused = send(
        &state,
        json!({
            "type": "update_settings",
            "auto_pull_repositories": false,
            "ask_questions_as_choices": false,
            "base_versions": {
                "auto_pull_repositories": read.versions["auto_pull_repositories"],
                "ask_questions_as_choices": read.versions["ask_questions_as_choices"],
            },
        }),
    )
    .await;
    let (fields, versions) = conflict(refused);
    assert_eq!(fields, "auto_pull_repositories");
    assert_eq!(
        versions,
        json!({"auto_pull_repositories": current.versions["auto_pull_repositories"]})
    );
    // Nothing of the refused write was applied, the field that was current included.
    let stored = state.store.get_settings().unwrap();
    assert!(stored.auto_pull_repositories);
    assert!(stored.ask_questions_as_choices);
}

#[tokio::test]
async fn two_settings_writers_changing_different_fields_both_succeed() {
    let (state, _dir) = crate::test_support::app_state("checked-settings-different");
    let read = read_settings(&state);
    for (field, other) in [
        ("auto_pull_repositories", true),
        ("auto_select_account", true),
    ] {
        send(
            &state,
            json!({
                "type": "update_settings",
                field: other,
                "base_versions": {field: read.versions[field]},
            }),
        )
        .await
        .unwrap();
    }
    let stored = state.store.get_settings().unwrap();
    assert!(stored.auto_pull_repositories && stored.auto_select_account);
}

#[tokio::test]
async fn a_version_that_names_no_field_is_refused_as_unreadable() {
    let (state, _dir) = crate::test_support::app_state("checked-settings-unknown-path");
    for path in ["no_such_field", "agent_tags.no-such-agent", "accounts"] {
        let err = send(
            &state,
            json!({"type": "update_settings", "auto_pull_repositories": true, "base_versions": {path: 1}}),
        )
        .await
        .unwrap_err();
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::UNREADABLE_REQUEST, "{path}");
    }
    assert!(!state.store.get_settings().unwrap().auto_pull_repositories);
}

#[tokio::test]
async fn a_write_with_no_versions_is_not_checked_but_advances_them() {
    let (state, _dir) = crate::test_support::app_state("checked-settings-unchecked");
    let read = read_settings(&state);
    send(
        &state,
        json!({"type": "update_settings", "auto_pull_repositories": true}),
    )
    .await
    .unwrap();
    // The same value again is no change: nothing is published and nothing advances.
    let after = read_settings(&state);
    send(
        &state,
        json!({"type": "update_settings", "auto_pull_repositories": true, "base_versions": {}}),
    )
    .await
    .unwrap();
    assert_eq!(read_settings(&state).versions, after.versions);
    assert!(after.versions["auto_pull_repositories"] > read.versions["auto_pull_repositories"]);
    assert_eq!(
        after.versions["default_clone_dir"],
        read.versions["default_clone_dir"]
    );
}

#[tokio::test]
async fn a_single_action_advances_the_version_of_its_field() {
    let (state, _dir) = crate::test_support::app_state("checked-settings-single-action");
    let setting = AgentId::ALL.into_iter().find_map(|agent| {
        agent
            .descriptor()
            .settings
            .iter()
            .find(|setting| matches!(setting.kind, SettingKind::Switch { .. }))
            .map(|setting| (agent, setting.id))
    });
    let (agent, id) = setting.expect("an agent declares a switch setting");
    let path = format!("provider_settings.{}.{id}", agent.wire_name());
    let read = read_settings(&state);
    let value = !read.provider_settings[agent.wire_name()][id]
        .as_ref()
        .and_then(|value| serde_json::to_value(value).ok())
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    send(
        &state,
        json!({"type": "set_provider_setting", "agent": agent.wire_name(), "setting": id, "value": value}),
    )
    .await
    .unwrap();
    let after = read_settings(&state);
    assert!(after.versions[&path] > read.versions[&path]);
    // A form based on the old value now conflicts.
    let stale = send(
        &state,
        json!({"type": "update_settings", "auto_pull_repositories": true, "base_versions": {&path: read.versions[&path]}}),
    )
    .await;
    assert_eq!(conflict(stale).0, path);
}

#[tokio::test]
async fn a_stale_console_write_is_refused_and_a_write_of_another_field_succeeds() {
    let (state, _dir) = crate::test_support::console_with_project_and_accounts("checked-console");
    let read = read_console(&state);
    send(
        &state,
        json!({"type": "update_console", "console": "console-1", "name": "Renamed"}),
    )
    .await
    .unwrap();
    let current = read_console(&state);
    assert!(current.versions["name"] > read.versions["name"]);

    let refused = send(
        &state,
        json!({
            "type": "update_console", "console": "console-1", "name": "Mine",
            "base_versions": {"name": read.versions["name"]},
        }),
    )
    .await;
    let (fields, versions) = conflict(refused);
    assert_eq!(fields, "name");
    assert_eq!(versions, json!({"name": current.versions["name"]}));
    assert_eq!(
        state.store.require_console("console-1").unwrap().name,
        "Renamed"
    );

    send(
        &state,
        json!({
            "type": "update_console", "console": "console-1", "console_session_prompt": "Hello",
            "base_versions": {"console_session_prompt": read.versions["console_session_prompt"]},
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        state
            .store
            .require_console("console-1")
            .unwrap()
            .console_session_prompt
            .as_deref(),
        Some("Hello")
    );
}

#[tokio::test]
async fn a_pin_advances_the_project_and_a_stale_project_write_is_refused() {
    let (state, _dir) = crate::test_support::console_with_project_and_accounts("checked-project");
    let read = read_project(&state);
    send(
        &state,
        json!({"type": "update_project", "project": "project-1", "pinned": true}),
    )
    .await
    .unwrap();
    let current = read_project(&state);
    assert!(current.versions["pinned"] > read.versions["pinned"]);
    assert_eq!(current.versions["name"], read.versions["name"]);

    let refused = send(
        &state,
        json!({
            "type": "update_project", "project": "project-1", "pinned": false, "name": "Other",
            "base_versions": {"pinned": read.versions["pinned"], "name": read.versions["name"]},
        }),
    )
    .await;
    assert_eq!(conflict(refused).0, "pinned");
    let stored = state.store.require_project("project-1").unwrap();
    assert!(stored.pinned);
    assert_eq!(stored.name, "Project");

    send(
        &state,
        json!({
            "type": "update_project", "project": "project-1", "description": "Mine",
            "base_versions": {"description": read.versions["description"]},
        }),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn a_stale_account_write_is_refused_and_a_write_of_another_field_succeeds() {
    let (state, dir) = crate::test_support::console_with_project_and_accounts("checked-account");
    let read = read_account(&state, "claude-a");
    send(
        &state,
        json!({"type": "update_account", "account": "claude-a", "name": "First"}),
    )
    .await
    .unwrap();
    let current = read_account(&state, "claude-a");
    assert!(current.versions["name"] > read.versions["name"]);
    assert_eq!(current.versions["config_dir"], read.versions["config_dir"]);

    let refused = send(
        &state,
        json!({
            "type": "update_account", "account": "claude-a", "name": "Second",
            "base_versions": {"name": read.versions["name"]},
        }),
    )
    .await;
    assert_eq!(conflict(refused).0, "name");
    assert_eq!(
        state.store.require_account("claude-a").unwrap().name,
        "First"
    );

    let moved = dir.join("moved").to_string_lossy().into_owned();
    send(
        &state,
        json!({
            "type": "update_account", "account": "claude-a", "config_dir": moved,
            "base_versions": {"config_dir": read.versions["config_dir"]},
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        state.store.require_account("claude-a").unwrap().config_dir,
        moved
    );
}

#[tokio::test]
async fn a_deleted_console_keeps_its_versions() {
    let (state, _dir) = crate::test_support::console_with_project_and_accounts("checked-deleted");
    send(
        &state,
        json!({"type": "update_console", "console": "console-1", "name": "Renamed"}),
    )
    .await
    .unwrap();
    let before = read_console(&state);
    let record = state.store.require_console("console-1").unwrap();
    send(
        &state,
        json!({"type": "delete_console", "console": "console-1"}),
    )
    .await
    .unwrap();
    // The same console written back is told apart from a fresh one by the versions it still has.
    let mut again = record;
    state
        .field_versions
        .stamp_console(&mut again, Stamp::Snapshot);
    assert_eq!(again.versions, before.versions);
}

/// The refusal `result` carries, which must be a `write_conflict`.
fn refused_as_conflict<T>(result: anyhow::Result<T>) -> CodedError {
    let err = match result {
        Ok(_) => panic!("the write was not refused"),
        Err(err) => err,
    };
    let coded = err.downcast::<CodedError>().expect("a coded refusal");
    assert_eq!(coded.code, error_code::WRITE_CONFLICT);
    coded
}

/// Both writers start on the current-thread runtime and nothing in the write awaits before the
/// check, so they run one after the other rather than at the same time: this guards the check
/// refusing the second writer, not contention between them.
#[tokio::test]
async fn of_two_settings_writers_based_on_the_same_version_only_one_succeeds() {
    let (state, _dir) = crate::test_support::app_state("checked-settings-race");
    let read = read_settings(&state);
    let write = |value: bool| {
        send(
            &state,
            json!({
                "type": "update_settings",
                "auto_pull_repositories": value,
                "base_versions": {
                    "auto_pull_repositories": read.versions["auto_pull_repositories"],
                },
            }),
        )
    };
    let (first, second) = tokio::join!(write(true), write(false));
    let (written, refused) = match (first, second) {
        (Ok(_), refused @ Err(_)) => (true, refused),
        (refused @ Err(_), Ok(_)) => (false, refused),
        _ => panic!("exactly one of the two writers succeeds"),
    };
    assert_eq!(conflict(refused).0, "auto_pull_repositories");
    assert_eq!(
        state.store.get_settings().unwrap().auto_pull_repositories,
        written
    );
}

/// Both writers start on the current-thread runtime and nothing in the write awaits before the
/// check, so they run one after the other rather than at the same time: this guards the check
/// refusing the second writer, not contention between them.
#[tokio::test]
async fn of_two_console_writers_based_on_the_same_version_only_one_succeeds() {
    let (state, _dir) =
        crate::test_support::console_with_project_and_accounts("checked-console-race");
    let read = read_console(&state);
    let write = |name: &str| {
        send(
            &state,
            json!({
                "type": "update_console", "console": "console-1", "name": name,
                "base_versions": {"name": read.versions["name"]},
            }),
        )
    };
    let (first, second) = tokio::join!(write("First"), write("Second"));
    let (written, refused) = match (first, second) {
        (Ok(_), refused @ Err(_)) => ("First", refused),
        (refused @ Err(_), Ok(_)) => ("Second", refused),
        _ => panic!("exactly one of the two writers succeeds"),
    };
    assert_eq!(conflict(refused).0, "name");
    assert_eq!(
        state.store.require_console("console-1").unwrap().name,
        written
    );
}

/// What an import does at its settings write, after the up-front check and the steps between
/// (see `config_document::import`): `update_settings_checked` with the versions the preview read
/// of the fields it writes. A settings field changing between the two steps of a whole import is
/// not staged deterministically here, so the re-check is tested where it is made.
#[tokio::test]
async fn an_import_write_of_settings_is_checked_again_against_the_fields_it_writes() {
    let (state, _dir) = crate::test_support::app_state("checked-settings-import-recheck");
    // The client the import runs for holds the settings from its snapshot, which is what later
    // changes are told from; the preview then reads the current versions.
    read_settings(&state);
    let read = state.settings_versions().unwrap();
    send(
        &state,
        json!({"type": "update_settings", "ask_questions_as_choices": false}),
    )
    .await
    .unwrap();

    // A field the import does not write having moved is no conflict.
    let changes = SettingsChanges {
        auto_pull_repositories: Some(true),
        ..SettingsChanges::default()
    };
    let base = changes.versions_read(&read);
    update_settings_checked(&state, changes, &base)
        .await
        .unwrap();
    assert!(state.store.get_settings().unwrap().auto_pull_repositories);

    // One it writes having moved is, and nothing is written.
    let changes = SettingsChanges {
        ask_questions_as_choices: Some(true),
        auto_select_account: Some(true),
        ..SettingsChanges::default()
    };
    let base = changes.versions_read(&read);
    let coded = refused_as_conflict(update_settings_checked(&state, changes, &base).await);
    assert_eq!(coded.params["fields"], "ask_questions_as_choices");
    let stored = state.store.get_settings().unwrap();
    assert!(!stored.ask_questions_as_choices);
    assert!(!stored.auto_select_account);
}

/// The version check of a console write that has to ask the agent for its models is made after
/// the answer, not before: a rename that lands while the write waits for it conflicts. The
/// write is spawned on the single-threaded test runtime and runs until it waits for the models
/// (off the runtime); the rename is then made without the test yielding, so it lands in that wait.
#[tokio::test]
async fn a_checked_console_write_that_waits_for_the_models_conflicts_with_a_rename_during_the_wait()
{
    let (state, _dir) =
        crate::test_support::console_with_project_and_accounts("checked-console-wait");
    crate::test_support::offer_models(&state, AgentId::CLAUDE, &[("fallback", &["low"])]);
    let read = read_console(&state);
    let write = tokio::spawn({
        let state = state.clone();
        let base = BTreeMap::from([("name".to_string(), read.versions["name"])]);
        async move {
            let mut changes = ConsoleChanges {
                name: Some("Mine".to_string()),
                ..ConsoleChanges::default()
            };
            changes.launch.insert(
                AgentId::CLAUDE,
                LaunchChange {
                    model: Some(Some("fallback".to_string())),
                    effort: None,
                },
            );
            update_console_checked(&state, "console-1", changes, &base).await
        }
    });
    tokio::task::yield_now().await;
    send(
        &state,
        json!({"type": "update_console", "console": "console-1", "name": "Renamed"}),
    )
    .await
    .unwrap();

    let coded = refused_as_conflict(write.await.unwrap());
    assert_eq!(coded.params["fields"], "name");
    let stored = state.store.require_console("console-1").unwrap();
    assert_eq!(stored.name, "Renamed");
    assert_eq!(stored.launch[AgentId::CLAUDE].model, None);
}
