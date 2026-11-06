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
    apply_settings: bool,
    import_id: Option<&str>,
) -> Imported {
    apply_reporting(
        state,
        document,
        resolutions,
        apply_settings,
        None,
        true,
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
        true,
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
        false,
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
        false,
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
        false,
        None,
        true,
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
        false,
        None,
    )
    .await;

    assert!(reported(&mut events, "none").is_empty());
}
