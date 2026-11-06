//! Importing a document: the consoles and projects it creates, and the places they are put in.

use super::*;

#[tokio::test]
async fn an_import_creates_a_console_and_its_projects_with_their_configuration() {
    let area = ScratchDir::new("import-apply-area");
    let (state, _dir) = app_state("import-apply");
    state
        .store
        .insert_account(&account("acct-1", AgentId::CLAUDE, "work"))
        .unwrap();
    // The document's Codex models below are not ones Codex offers here; they are stored as they
    // are all the same.
    crate::test_support::offer_models(&state, AgentId::CODEX, &[("offered", &[])]);
    let repo = directory(&area, "repo");
    let mut hub = console("Hub", vec![project("renamed", &repo.to_string_lossy())]);
    hub.icon = Some("data:image/png;base64,AAAA".to_string());
    hub.launch[AgentId::CODEX].model = Some("gpt-x".to_string());
    hub.console_session_prompt = Some("\n Be brief.\n".to_string());
    hub.accounts[AgentId::CLAUDE] = Some(AccountName {
        name: "WORK".to_string(),
    });
    let p = &mut hub.projects[0];
    p.pinned = true;
    p.tags = vec!["backend".to_string(), "Backend".to_string()];
    p.description = Some("\n A billing service.\n".to_string());
    p.default_agent = Some(AgentId::GROK);
    p.launch[AgentId::CODEX].model = Some("gpt-y".to_string());
    p.launch[AgentId::GROK].effort = Some("high".to_string());
    p.accounts[AgentId::CLAUDE] = ProjectAccountRef::Account {
        name: "Work".to_string(),
    };
    p.accounts[AgentId::CODEX] = ProjectAccountRef::Default;

    let imported = import(
        &state,
        &document(vec![hub]),
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
    )
    .await
    .expect("imported");

    let stored_console = state.store.list_consoles().unwrap().remove(0);
    assert_eq!(imported[0].console.as_deref(), Some(&*stored_console.id));
    assert_eq!(imported[0].status, ImportStatus::Created);
    assert_eq!(stored_console.name, "Hub");
    assert_eq!(
        stored_console.icon.as_deref(),
        Some("data:image/png;base64,AAAA")
    );
    assert_eq!(
        stored_console.launch[AgentId::CODEX].model.as_deref(),
        Some("gpt-x")
    );
    assert_eq!(
        stored_console.console_session_prompt.as_deref(),
        Some("Be brief.")
    );
    assert_eq!(
        stored_console.accounts[AgentId::CLAUDE].as_deref(),
        Some("acct-1")
    );
    assert!(Path::new(&stored_console.workdir).is_dir());

    let created = stored_projects(&state).remove(0);
    assert_eq!(
        imported[0].projects[0].project.as_deref(),
        Some(&*created.id)
    );
    assert_eq!(imported[0].projects[0].status, ImportStatus::Created);
    assert_eq!(
        imported[0].projects[0].via,
        Some(DirectoryChoiceKind::ExportedPath)
    );
    assert_eq!(created.console_id, stored_console.id);
    assert_eq!(created.name, "renamed");
    assert_eq!(created.path, repo.to_string_lossy());
    assert!(created.pinned);
    assert_eq!(created.tags, ["backend"]);
    assert_eq!(created.description.as_deref(), Some("A billing service."));
    assert_eq!(created.default_agent, Some(AgentId::GROK));
    assert_eq!(
        created.launch[AgentId::CODEX].model.as_deref(),
        Some("gpt-y")
    );
    assert_eq!(
        created.launch[AgentId::GROK].effort.as_deref(),
        Some("high")
    );
    assert_eq!(
        created.accounts[AgentId::CLAUDE],
        ProjectAccount::Account {
            id: "acct-1".to_string()
        }
    );
    assert_eq!(created.accounts[AgentId::CODEX], ProjectAccount::Default);
    assert_eq!(created.accounts[AgentId::GROK], ProjectAccount::Inherit);
    // Trust does not travel: an imported project asks like a hand-added one.
    assert!(!created.trust_consent);
}

#[tokio::test]
async fn an_unmatched_account_is_bound_to_the_default_and_listed_in_the_result() {
    let area = ScratchDir::new("import-fallback-area");
    let (state, _dir) = app_state("import-fallback");
    let repo = directory(&area, "repo");
    let mut hub = console("Hub", vec![project("p", &repo.to_string_lossy())]);
    hub.accounts[AgentId::GROK] = Some(AccountName {
        name: "Team".to_string(),
    });
    hub.projects[0].accounts[AgentId::CODEX] = ProjectAccountRef::Account {
        name: "Solo".to_string(),
    };

    let imported = import(
        &state,
        &document(vec![hub]),
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
    )
    .await
    .unwrap();

    assert_eq!(
        state.store.list_consoles().unwrap()[0].accounts[AgentId::GROK],
        None
    );
    assert_eq!(
        stored_projects(&state)[0].accounts[AgentId::CODEX],
        ProjectAccount::Default
    );
    let fallback = |agent, name: &str| AccountFallback {
        agent,
        name: name.to_string(),
    };
    assert_eq!(
        imported[0].account_fallbacks,
        [fallback(AgentId::GROK, "Team")]
    );
    assert_eq!(
        imported[0].projects[0].account_fallbacks,
        [fallback(AgentId::CODEX, "Solo")]
    );
}

#[tokio::test]
async fn each_choice_places_the_project_in_its_own_directory() {
    let area = ScratchDir::new("import-choices-area");
    let clone_dir = directory(&area, "projects");
    let (state, _dir) = state_cloning_into("import-choices", &clone_dir);
    let recorded = directory(&area, "layout/recorded");
    // A marker the association would detect an agent from if it were asked to; the document
    // says the project has none.
    std::fs::write(recorded.join("CLAUDE.md"), "").unwrap();
    let derived = directory(&clone_dir, "derived");
    let chosen = directory(&area, "chosen");
    let hub = console(
        "Hub",
        vec![
            project("recorded", &recorded.to_string_lossy()),
            project("derived", "/gone/layout/derived"),
            project("pointed", "/gone/layout/pointed"),
        ],
    );

    let imported = import(
        &state,
        &document(vec![hub]),
        vec![
            at(0, 0, DirectoryChoice::ExportedPath),
            at(0, 1, DirectoryChoice::DefaultCloneDir),
            at(
                0,
                2,
                DirectoryChoice::Directory {
                    path: chosen.to_string_lossy().into_owned(),
                },
            ),
        ],
    )
    .await
    .unwrap();

    let paths: Vec<String> = stored_projects(&state)
        .into_iter()
        .map(|p| format!("{}={}", p.name, p.path))
        .collect();
    for expected in [
        format!("recorded={}", recorded.display()),
        format!("derived={}", derived.display()),
        format!("pointed={}", chosen.display()),
    ] {
        assert!(paths.contains(&expected), "{expected} in {paths:?}");
    }
    let recorded_project = stored_projects(&state)
        .into_iter()
        .find(|p| p.name == "recorded")
        .unwrap();
    assert_eq!(recorded_project.default_agent, None);
    let via: Vec<_> = imported[0].projects.iter().map(|p| p.via).collect();
    assert_eq!(
        via,
        [
            Some(DirectoryChoiceKind::ExportedPath),
            Some(DirectoryChoiceKind::DefaultCloneDir),
            Some(DirectoryChoiceKind::Directory)
        ]
    );
}

#[tokio::test]
async fn a_remote_carrying_a_credential_is_not_cloned_from_nor_shown_with_it() {
    let area = ScratchDir::new("import-credential-area");
    let clone_dir = directory(&area, "projects");
    let (state, _dir) = state_cloning_into("import-credential", &clone_dir);
    let mut planted = project("planted", "/gone/planted");
    planted.remote_url = Some("https://user:token@example.com/o/planted.git".to_string());
    let doc = document(vec![console("Hub", vec![planted])]);

    let inspected = inspect(&state, &doc).await.expect("inspected").consoles;
    let shown = &inspected[0].projects[0];
    assert_eq!(shown.offers.clone, None);
    assert_eq!(
        shown.remote_url.as_deref(),
        Some("https://example.com/o/planted.git")
    );

    let imported = import(&state, &doc, vec![at(0, 0, DirectoryChoice::Clone)])
        .await
        .unwrap();
    assert_eq!(imported[0].projects[0].status, ImportStatus::Failed);
    assert_eq!(
        imported[0].projects[0].error.as_ref().unwrap().code,
        error_code::IMPORT_CHOICE_UNAVAILABLE
    );
    assert!(stored_projects(&state).is_empty());
}

#[tokio::test]
async fn a_directory_already_associated_with_the_console_is_not_associated_twice() {
    let area = ScratchDir::new("import-duplicate-area");
    let (state, _dir) = app_state("import-duplicate");
    let repo = directory(&area, "repo");
    let path = repo.to_string_lossy().into_owned();
    let hub = console(
        "Hub",
        vec![project("first", &path), project("second", &path)],
    );

    let imported = import(
        &state,
        &document(vec![hub]),
        vec![
            at(0, 0, DirectoryChoice::ExportedPath),
            at(0, 1, DirectoryChoice::ExportedPath),
        ],
    )
    .await
    .unwrap();

    assert_eq!(imported[0].projects[0].status, ImportStatus::Created);
    assert_eq!(imported[0].projects[1].status, ImportStatus::Failed);
    assert_eq!(
        imported[0].projects[1].error.as_ref().unwrap().code,
        error_code::ALL_PROJECTS_ALREADY_ADDED
    );
    assert_eq!(stored_projects(&state).len(), 1);
}

#[tokio::test]
async fn importing_always_creates_a_console_even_beside_one_of_the_same_name() {
    let (state, _dir) = app_state("import-same-name");
    let hub = document(vec![console("Hub", Vec::new())]);

    import(&state, &hub, Vec::new()).await.unwrap();
    import(&state, &hub, Vec::new()).await.unwrap();

    let consoles = state.store.list_consoles().unwrap();
    assert_eq!(consoles.len(), 2);
    assert_ne!(consoles[0].id, consoles[1].id);
    assert_ne!(consoles[0].workdir, consoles[1].workdir);
}

#[tokio::test]
async fn a_document_without_a_console_session_prompt_imports_a_console_with_none() {
    let (state, _dir) = app_state("import-no-prompt");
    let mut without = document(vec![console("Hub", Vec::new())]);
    without["consoles"][0]
        .as_object_mut()
        .unwrap()
        .remove("console_session_prompt");

    apply(
        &state,
        &without,
        Vec::new(),
        ApplyOptions {
            settings: false,
            accounts: None,
            consoles: true,
            clone_dir: true,
        },
    )
    .await
    .unwrap();
    let stored = state.store.list_consoles().unwrap().remove(0);
    assert_eq!(stored.console_session_prompt, None);
}

#[tokio::test]
async fn importing_preserves_legacy_project_text_beyond_editing_limits() {
    let area = ScratchDir::new("import-legacy-project-text-area");
    let (state, _dir) = app_state("import-legacy-project-text");
    let repo = directory(&area, "repo");
    let mut legacy = project(&"界".repeat(101), &repo.to_string_lossy());
    legacy.description = Some("😀".repeat(2_001));
    let original = document(vec![console("Hub", vec![legacy.clone()])]);
    import(
        &state,
        &original,
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
    )
    .await
    .unwrap();
    let exported =
        serde_json::to_value(export::build(&state.store, &export::Selection::full(true)).unwrap())
            .unwrap();
    let (target, _target_dir) = app_state("import-legacy-project-text-target");
    import(
        &target,
        &exported,
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
    )
    .await
    .unwrap();
    let restored = stored_projects(&target).remove(0);
    assert_eq!(restored.name, legacy.name);
    assert_eq!(restored.description, legacy.description);
}

#[tokio::test]
async fn a_document_without_a_project_description_imports_a_project_with_none() {
    let area = ScratchDir::new("import-no-description-area");
    let (state, _dir) = app_state("import-no-description");
    let repo = directory(&area, "repo");
    let mut without = document(vec![console(
        "Hub",
        vec![project("repo", &repo.to_string_lossy())],
    )]);
    without["consoles"][0]["projects"][0]
        .as_object_mut()
        .unwrap()
        .remove("description");

    import(
        &state,
        &without,
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
    )
    .await
    .unwrap();
    assert_eq!(stored_projects(&state).remove(0).description, None);
}
