//! Imports that fail in part or whole, and what they leave behind.

use super::*;

#[tokio::test]
async fn a_project_that_cannot_be_placed_is_reported_and_the_rest_are_imported() {
    let area = ScratchDir::new("import-partial-area");
    let (state, _dir) = app_state("import-partial");
    let good = directory(&area, "good");
    let mut no_remote = project("no-remote", "/gone/no-remote");
    no_remote.remote_url = None;
    let mut bad_remote = project("bad-remote", "/gone/bad-remote");
    bad_remote.remote_url = Some("ext::sh -c true".to_string());
    let hub = console(
        "Hub",
        vec![
            project("missing", "/gone/missing"),
            no_remote,
            bad_remote,
            project("good", &good.to_string_lossy()),
            project("left-out", &good.to_string_lossy()),
        ],
    );

    let imported = import(
        &state,
        &document(vec![hub]),
        vec![
            at(0, 0, DirectoryChoice::ExportedPath),
            at(0, 1, DirectoryChoice::Clone),
            at(0, 2, DirectoryChoice::Clone),
            at(0, 3, DirectoryChoice::ExportedPath),
        ],
    )
    .await
    .expect("a failing project does not fail the import");

    let outcomes: Vec<(ImportStatus, Option<crate::protocol::Code>)> = imported[0]
        .projects
        .iter()
        .map(|p| (p.status, p.error.as_ref().map(|e| e.code)))
        .collect();
    assert_eq!(
        outcomes,
        [
            (ImportStatus::Failed, Some(error_code::PATH_NOT_FOUND)),
            (
                ImportStatus::Failed,
                Some(error_code::IMPORT_CHOICE_UNAVAILABLE)
            ),
            (
                ImportStatus::Failed,
                Some(error_code::IMPORT_CHOICE_UNAVAILABLE)
            ),
            (ImportStatus::Created, None),
            (ImportStatus::LeftOut, None),
        ]
    );
    let names: Vec<String> = stored_projects(&state)
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["good"]);
}

#[tokio::test]
async fn a_refused_import_leaves_nothing_behind() {
    let area = ScratchDir::new("import-refused-area");
    let (state, _dir) = app_state("import-refused");
    let repo = directory(&area, "repo");
    let valid = |extra: Vec<DocumentConsole>| {
        let mut consoles = vec![console("Hub", vec![project("p", &repo.to_string_lossy())])];
        consoles.extend(extra);
        document(consoles)
    };
    let mut bad_icon = console("Second", Vec::new());
    bad_icon.icon = Some("not-an-image".to_string());
    let mut huge_icon = console("Third", Vec::new());
    huge_icon.icon = Some(format!("data:image/png;base64,{}", "A".repeat(256 * 1024)));
    let mut newer = valid(Vec::new());
    newer["version"] = json!(export::VERSION + 1);
    let resolve = || DirectoryChoice::ExportedPath;

    let refusals = [
        (
            newer,
            Vec::new(),
            error_code::CONFIGURATION_VERSION_UNSUPPORTED,
        ),
        // The first console is fine; the second one's avatar refuses the whole import.
        (
            valid(vec![bad_icon]),
            Vec::new(),
            error_code::ICON_NOT_AN_IMAGE,
        ),
        (
            valid(vec![huge_icon]),
            Vec::new(),
            error_code::ICON_TOO_LARGE,
        ),
        (
            valid(Vec::new()),
            vec![at(1, 0, resolve())],
            error_code::IMPORT_RESOLUTION_INVALID,
        ),
        (
            valid(Vec::new()),
            vec![at(0, 1, resolve())],
            error_code::IMPORT_RESOLUTION_INVALID,
        ),
        (
            valid(Vec::new()),
            vec![at(0, 0, resolve()), at(0, 0, DirectoryChoice::Clone)],
            error_code::IMPORT_RESOLUTION_INVALID,
        ),
        (
            valid(Vec::new()),
            vec![at(
                0,
                0,
                DirectoryChoice::Directory {
                    path: "relative/dir".to_string(),
                },
            )],
            error_code::IMPORT_RESOLUTION_INVALID,
        ),
    ];
    for (document, resolutions, code) in refusals {
        let err = apply(&state, &document, resolutions, false, None, true)
            .await
            .expect_err(code.as_str());
        assert_eq!(code_of(&err), code);
        // A refused avatar names the console it belongs to.
        let coded = err.downcast_ref::<CodedError>().unwrap();
        match code {
            error_code::ICON_NOT_AN_IMAGE => assert_eq!(coded.params["console_name"], "Second"),
            error_code::ICON_TOO_LARGE => assert_eq!(coded.params["console_name"], "Third"),
            _ => assert!(!coded.params.contains_key("console_name"), "{code}"),
        }
        assert!(state.store.list_consoles().unwrap().is_empty(), "{code}");
        assert!(stored_projects(&state).is_empty(), "{code}");
    }
}

#[tokio::test]
async fn an_existence_check_that_does_not_come_back_offers_nothing_and_is_not_waited_for() {
    let probe = |project| Probe {
        console: 0,
        project,
        exported: None,
        derived: None,
        clone: None,
    };
    let offered = Offers {
        exported_path: Some("/there".to_string()),
        ..Offers::default()
    };
    let answer = offered.clone();

    // The deadline stays short, so a machine that stalls for seconds can keep the quick probe
    // from landing inside it; the scenario then runs again until `PATIENCE` is used up.
    let start = std::time::Instant::now();
    loop {
        let (answer, stalled_for) = (answer.clone(), Duration::from_secs(2));
        let found = probe_offers(
            vec![probe(0), probe(1)],
            Duration::from_millis(200),
            move |probe| {
                if probe.project == 0 {
                    // A stat on a mount that has stopped answering.
                    std::thread::sleep(stalled_for);
                }
                ((probe.console, probe.project), answer.clone())
            },
        )
        .await;
        assert!(!found.contains_key(&(0, 0)));
        if found.get(&(0, 1)) == Some(&offered) {
            break;
        }
        assert!(
            start.elapsed() < PATIENCE,
            "the quick probe never landed in time"
        );
    }
}

#[tokio::test]
async fn a_console_that_cannot_be_created_is_reported_and_the_import_goes_on_to_the_result() {
    let area = ScratchDir::new("import-console-fails-area");
    let (state, dir) = app_state("import-console-fails");
    // The directory consoles' working directories are made in is a file, so none can be.
    std::fs::write(dir.join("consoles"), "").unwrap();
    let repo = directory(&area, "repo");
    let hub = console("Hub", vec![project("p", &repo.to_string_lossy())]);
    let applied = with_settings(document(vec![hub]), true, false);

    let imported = apply(
        &state,
        &applied,
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
        true,
        None,
        true,
    )
    .await
    .expect("a console that fails is reported, not raised");

    let console = &imported.consoles[0];
    assert_eq!(console.status, ImportStatus::Failed);
    assert_eq!(console.console, None);
    assert!(console.error.is_some());
    assert_eq!(console.projects[0].status, ImportStatus::NotAttempted);
    assert!(stored_projects(&state).is_empty());
    // The rest of the import still happens and is still reported.
    assert!(imported.settings_applied.is_some());
    assert!(state.store.auto_pull_repositories().unwrap());
}

#[tokio::test]
async fn a_clone_whose_target_directory_exists_fails_that_project_alone() {
    let area = ScratchDir::new("import-clone-exists-area");
    let clone_dir = directory(&area, "projects");
    let (state, _dir) = state_cloning_into("import-clone-exists", &clone_dir);
    directory(&clone_dir, "taken");
    let good = directory(&area, "good");
    let mut cloned = project("cloned", "/gone/cloned");
    cloned.remote_url = Some("https://example.invalid/o/taken.git".to_string());
    let hub = console(
        "Hub",
        vec![cloned, project("good", &good.to_string_lossy())],
    );

    let imported = import(
        &state,
        &document(vec![hub]),
        vec![
            at(0, 0, DirectoryChoice::Clone),
            at(0, 1, DirectoryChoice::ExportedPath),
        ],
    )
    .await
    .unwrap();

    assert_eq!(imported[0].projects[0].status, ImportStatus::Failed);
    assert_eq!(
        imported[0].projects[0].error.as_ref().unwrap().code,
        error_code::PATH_ALREADY_EXISTS
    );
    assert_eq!(imported[0].projects[1].status, ImportStatus::Created);
    assert_eq!(stored_projects(&state).len(), 1);
}
