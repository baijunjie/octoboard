//! What a running import reports to the clients, as `configuration_import_progress`.

use super::*;
use crate::protocol::{Event, ImportItem, ImportItemState};

/// The progress broadcast so far for `import_id`, each as `item: state`.
pub(in crate::config_document::import) fn reported(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
    import_id: &str,
) -> Vec<String> {
    let mut reported = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::ConfigurationImportProgress {
            import_id: id,
            item,
            state,
        } = event
        {
            assert_eq!(id, import_id);
            reported.push(format!("{}: {}", item_name(&item), state_name(&state)));
        }
    }
    reported
}

fn item_name(item: &ImportItem) -> String {
    match item {
        ImportItem::Console { console } => format!("console {console}"),
        ImportItem::Project { console, project } => format!("project {console}.{project}"),
        ImportItem::Account { agent, account } => format!("account {}.{account}", agent.label()),
        ImportItem::CloneDir => "clone directory".to_string(),
        ImportItem::AgentSettings => "agent settings".to_string(),
        ImportItem::Settings => "settings".to_string(),
    }
}

fn state_name(state: &ImportItemState) -> &'static str {
    match state {
        ImportItemState::Waiting => "waiting",
        ImportItemState::Creating => "creating",
        ImportItemState::Cloning => "cloning",
        ImportItemState::Done => "done",
        ImportItemState::Failed { .. } => "failed",
        ImportItemState::NotAttempted => "not attempted",
    }
}

async fn import_named(
    state: &Arc<AppState>,
    document: &serde_json::Value,
    resolutions: Vec<ProjectResolution>,
    apply_settings: Vec<SettingsSection>,
    import_id: Option<&str>,
) -> Imported {
    apply_reporting(
        state,
        document,
        resolutions,
        ApplyOptions {
            settings: apply_settings,
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
        import_id.map(str::to_string),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn every_item_is_announced_waiting_then_reported_as_it_is_handled() {
    let area = ScratchDir::new("import-progress-area");
    let (state, _dir) = app_state("import-progress");
    let good = directory(&area, "good");
    let hub = console(
        "Hub",
        vec![
            project("good", &good.to_string_lossy()),
            project("left-out", &good.to_string_lossy()),
            project("clone", "/gone/clone"),
            project("missing", "/gone/missing"),
        ],
    );
    let document = with_settings(document(vec![hub]), true, false);
    let mut events = state.subscribe();

    import_named(
        &state,
        &document,
        vec![
            at(0, 0, DirectoryChoice::ExportedPath),
            at(0, 2, DirectoryChoice::Clone),
            at(0, 3, DirectoryChoice::ExportedPath),
        ],
        carried_sections(&document),
        Some("one"),
    )
    .await;

    // A console is `done` once it is created, before its projects are handled.
    assert_eq!(
        reported(&mut events, "one"),
        [
            "console 0: waiting",
            "project 0.0: waiting",
            "project 0.2: waiting",
            "project 0.3: waiting",
            "settings: waiting",
            "console 0: creating",
            "console 0: done",
            "project 0.0: creating",
            "project 0.0: done",
            // A clone is told apart from a placement; this one has no remote and fails.
            "project 0.2: cloning",
            "project 0.2: failed",
            "project 0.3: creating",
            "project 0.3: failed",
            "settings: creating",
            "settings: done",
        ]
    );
}

/// The document an export of everything makes of a fresh state, which carries the settings and the
/// Agent accounts settings, with a default clone directory of its own.
fn document_with_clone_dir(clone_dir: &Path) -> serde_json::Value {
    let (source, _dir) = app_state("import-progress-source");
    let mut document = export_everything(&source);
    document["settings"]["default_clone_dir"] = json!(clone_dir.to_string_lossy());
    document
}

#[tokio::test]
async fn the_clone_directory_comes_first_and_the_agent_account_settings_follow_the_accounts() {
    let area = ScratchDir::new("import-progress-order-area");
    let document = document_with_clone_dir(&directory(&area, "theirs"));
    let (state, _dir) = app_state("import-progress-order");
    let mut events = state.subscribe();

    apply_reporting(
        &state,
        &document,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&document),
            accounts: Some(Vec::new()),
            consoles: true,
            clone_dir: true,
            agents: None,
        },
        Some("order".to_string()),
    )
    .await
    .unwrap();

    // The settings item stands for the application settings without those two.
    assert_eq!(
        reported(&mut events, "order"),
        [
            "clone directory: waiting",
            "agent settings: waiting",
            "settings: waiting",
            "clone directory: creating",
            "clone directory: done",
            "agent settings: creating",
            "agent settings: done",
            "settings: creating",
            "settings: done",
        ]
    );
}

#[tokio::test]
async fn the_clone_directory_is_not_announced_when_the_machines_own_is_kept() {
    let area = ScratchDir::new("import-progress-kept-area");
    let document = document_with_clone_dir(&directory(&area, "theirs"));
    let (state, _dir) = app_state("import-progress-kept");
    let mut events = state.subscribe();

    apply_reporting(
        &state,
        &document,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&document),
            accounts: None,
            consoles: true,
            clone_dir: false,
            agents: None,
        },
        Some("kept".to_string()),
    )
    .await
    .unwrap();

    assert_eq!(
        reported(&mut events, "kept"),
        ["settings: waiting", "settings: creating", "settings: done"]
    );
}

#[tokio::test]
async fn a_failed_console_fails_its_resolved_projects_as_not_attempted() {
    let area = ScratchDir::new("import-progress-console-area");
    let (state, dir) = app_state("import-progress-console");
    // The directory consoles' working directories are made in is a file, so none can be.
    std::fs::write(dir.join("consoles"), "").unwrap();
    let repo = directory(&area, "repo");
    let hub = console(
        "Hub",
        vec![
            project("p", &repo.to_string_lossy()),
            project("left-out", &repo.to_string_lossy()),
        ],
    );
    let mut events = state.subscribe();

    import_named(
        &state,
        &document(vec![hub]),
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
        Vec::new(),
        Some("two"),
    )
    .await;

    assert_eq!(
        reported(&mut events, "two"),
        [
            "console 0: waiting",
            "project 0.0: waiting",
            "console 0: creating",
            "console 0: failed",
            "project 0.0: not attempted",
        ]
    );
}

#[tokio::test]
async fn a_failure_is_reported_as_the_result_reports_it() {
    let (state, _dir) = app_state("import-progress-failure");
    let hub = console("Hub", vec![project("missing", "/gone/missing")]);
    let mut events = state.subscribe();

    let imported = import_named(
        &state,
        &document(vec![hub]),
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
        Vec::new(),
        Some("three"),
    )
    .await;

    let mut failure = None;
    while let Ok(event) = events.try_recv() {
        if let Event::ConfigurationImportProgress {
            item: ImportItem::Project { .. },
            state: ImportItemState::Failed { error },
            ..
        } = event
        {
            failure = Some(error);
        }
    }
    let failure = failure.expect("the project's failure is reported");
    let result = imported.consoles[0].projects[0].error.as_ref().unwrap();
    assert_eq!(failure.code, result.code);
    assert_eq!(failure.params, result.params);
    assert_eq!(failure.message, result.message);
}

#[tokio::test]
async fn a_refused_request_that_names_an_id_reports_nothing() {
    let (state, _dir) = app_state("import-progress-refused");
    let mut bad_icon = console("Second", Vec::new());
    bad_icon.icon = Some("not-an-image".to_string());
    let mut events = state.subscribe();

    let refused = apply_reporting(
        &state,
        &document(vec![console("Hub", Vec::new()), bad_icon]),
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
        Some("five".to_string()),
    )
    .await;

    assert_eq!(
        code_of(&refused.unwrap_err()),
        error_code::ICON_NOT_AN_IMAGE
    );
    assert!(reported(&mut events, "five").is_empty());
}

#[tokio::test]
async fn an_import_that_names_no_id_reports_nothing() {
    let (state, _dir) = app_state("import-progress-silent");
    let mut events = state.subscribe();

    import_named(
        &state,
        &document(vec![console("Hub", Vec::new())]),
        Vec::new(),
        Vec::new(),
        None,
    )
    .await;

    assert!(reported(&mut events, "none").is_empty());
}

#[tokio::test]
async fn only_the_accounts_of_the_agents_listed_are_announced() {
    let area = ScratchDir::new("import-progress-agents-area");
    let (state, _dir) = app_state("import-progress-agents");
    let entry = |agent: &str, name: &str| {
        json!({
            "agent": agent,
            "default_account": { "name": null, "position": 0, "disabled": false },
            "accounts": [{
                "name": name,
                "directory": directory(&area, name).to_string_lossy(),
                "disabled": false,
            }],
        })
    };
    let mut document = document(Vec::new());
    document["accounts"] = json!({
        "added_agents": ["claude", "codex"],
        "agents": [entry("claude", "Left"), entry("codex", "Kept")],
        "agent_settings": null,
    });
    let mut events = state.subscribe();

    apply_reporting(
        &state,
        &document,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: Some(Vec::new()),
            consoles: false,
            clone_dir: true,
            agents: Some(vec![AgentId::CODEX]),
        },
        Some("agents".to_string()),
    )
    .await
    .unwrap();

    let label = AgentId::CODEX.label();
    assert_eq!(
        reported(&mut events, "agents"),
        [
            format!("account {label}.0: waiting"),
            format!("account {label}.0: creating"),
            format!("account {label}.0: done"),
        ]
    );
}
