//! Inspecting a document: the choices offered for each project, with nothing created.

use super::*;

#[tokio::test]
async fn a_project_recorded_by_its_directory_name_alone_is_offered_the_derived_directory_and_a_clone(
) {
    let area = ScratchDir::new("import-bare-area");
    let clone_dir = directory(&area, "projects");
    let (state, _dir) = state_cloning_into("import-bare", &clone_dir);
    directory(&clone_dir, "present");
    // A bare name is no recorded path: nothing is offered as the exported path, only the
    // derived directory in the clone directory.
    let mut present = project("present", "present");
    present.remote_url = Some("https://example.com/o/present.git".to_string());
    let mut cloneable = project("cloneable", "cloneable");
    cloneable.remote_url = Some("https://example.com/o/cloneable.git".to_string());
    let nameless = project("nameless", "..");

    let consoles = inspect(
        &state,
        &document(vec![console("Hub", vec![present, cloneable, nameless])]),
    )
    .await
    .expect("inspected")
    .consoles;
    let projects = &consoles[0].projects;

    assert!(projects.iter().all(|p| p.offers.exported_path.is_none()));
    assert_eq!(
        projects[0].offers.default_clone_dir.as_deref(),
        Some(&*clone_dir.join("present").to_string_lossy())
    );
    assert_eq!(
        projects[0].default,
        Some(DirectoryChoiceKind::DefaultCloneDir)
    );
    assert_eq!(projects[1].offers.default_clone_dir, None);
    assert_eq!(
        projects[1].offers.clone.as_deref(),
        Some(&*clone_dir.join("cloneable").to_string_lossy())
    );
    assert_eq!(projects[1].default, Some(DirectoryChoiceKind::Clone));
    assert_eq!(projects[2].default, None);
}

#[tokio::test]
async fn each_project_is_offered_the_choices_its_state_allows_and_the_first_is_the_default() {
    let area = ScratchDir::new("import-inspect-area");
    let clone_dir = directory(&area, "projects");
    let (state, _dir) = state_cloning_into("import-inspect", &clone_dir);
    let here = directory(&area, "elsewhere/present");
    directory(&clone_dir, "moved");
    directory(&clone_dir, "taken");
    directory(&clone_dir, "present");

    let mut present = project("present", &here.to_string_lossy());
    present.remote_url = Some("https://example.com/o/present.git".to_string());
    let mut moved = project("moved", "/gone/layout/moved");
    moved.remote_url = Some("https://example.com/o/moved-repo.git".to_string());
    let mut cloneable = project("cloneable", "/gone/layout/cloneable");
    cloneable.remote_url = Some("https://example.com/o/cloneable.git".to_string());
    // Its clone would land on the directory that exists, so there is nothing to clone into.
    let mut taken = project("taken", "/gone/layout/elsewhere");
    taken.remote_url = Some("https://example.com/o/taken.git".to_string());
    let mut unclonable = project("unclonable", "/gone/layout/unclonable-x/..");
    unclonable.remote_url = Some("ext::sh -c true".to_string());
    let hopeless = project("hopeless", "relative/hopeless");

    let consoles = inspect(
        &state,
        &document(vec![console(
            "Hub",
            vec![present, moved, cloneable, taken, unclonable, hopeless],
        )]),
    )
    .await
    .expect("inspected")
    .consoles;

    let projects = &consoles[0].projects;
    let summary: Vec<_> = projects
        .iter()
        .map(|p| {
            (
                p.offers.exported_path.is_some(),
                p.offers.default_clone_dir.is_some(),
                p.offers.clone.is_some(),
                p.default,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            // The recorded path and the derived directory both exist; the recorded wins. A
            // clone would land on the derived directory, so it is not offered.
            (true, true, false, Some(DirectoryChoiceKind::ExportedPath)),
            (
                false,
                true,
                true,
                Some(DirectoryChoiceKind::DefaultCloneDir)
            ),
            (false, false, true, Some(DirectoryChoiceKind::Clone)),
            (false, false, false, None),
            // The derived directory ("unclonable-x/.." resolves to "gone") does not exist.
            (false, false, false, None),
            (false, false, false, None),
        ]
    );
    assert_eq!(
        projects[1].offers.default_clone_dir.as_deref(),
        Some(&*clone_dir.join("moved").to_string_lossy())
    );
    assert_eq!(
        projects[2].offers.clone.as_deref(),
        Some(&*clone_dir.join("cloneable").to_string_lossy())
    );
    assert_eq!(
        projects[0].path,
        here.to_string_lossy(),
        "the recorded path is echoed as recorded"
    );
}

#[tokio::test]
async fn inspecting_reports_the_fallbacks_and_creates_nothing() {
    let (state, _dir) = app_state("import-inspect-readonly");
    let mut hub = console("Hub", vec![project("p", "/nowhere/p")]);
    hub.accounts.claude = Some(AccountName {
        name: "Missing".to_string(),
    });
    hub.projects[0].accounts.codex = ProjectAccountRef::Account {
        name: "Gone".to_string(),
    };

    let consoles = inspect(&state, &document(vec![hub]))
        .await
        .unwrap()
        .consoles;

    assert_eq!(consoles[0].account_fallbacks.len(), 1);
    assert_eq!(consoles[0].projects[0].account_fallbacks.len(), 1);
    assert!(state.store.list_consoles().unwrap().is_empty());
    assert!(stored_projects(&state).is_empty());
}
