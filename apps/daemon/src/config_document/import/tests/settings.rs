//! The application settings a document carries, and the export round trips.

use super::*;

#[tokio::test]
async fn an_exported_configuration_is_imported_as_it_is() {
    let area = ScratchDir::new("import-roundtrip-area");
    let (source, _source_dir) = app_state("import-roundtrip-source");
    let repo = directory(&area, "repo");
    let exported_console = crate::protocol::Console {
        id: "console-1".to_string(),
        name: "Hub".to_string(),
        workdir: "/consoles/console-1".to_string(),
        default_agent: AgentId::CLAUDE,
        project_session_agent: Some(AgentId::GROK),
        accounts: Default::default(),
        launch: Default::default(),
        project_session_launch: crate::protocol::PerAgent::default().with(
            AgentId::CLAUDE,
            crate::protocol::Launch {
                model: Some("opus".to_string()),
                effort: Some("high".to_string()),
            },
        ),
        icon: None,
        console_session_prompt: None,
        created_at: 1,
    };
    source.store.insert_console(&exported_console).unwrap();
    let mut stored = Project {
        id: "project-1".to_string(),
        console_id: "console-1".to_string(),
        host_id: crate::store::LOCAL_HOST_ID.to_string(),
        name: "repo".to_string(),
        path: repo.to_string_lossy().into_owned(),
        default_agent: None,
        source: ProjectSource::Local,
        remote_url: None,
        accounts: Default::default(),
        launch: Default::default(),
        trust_consent: true,
        pinned: false,
        tags: vec!["t".to_string()],
        description: Some("Round trip.".to_string()),
        git_workflow: Default::default(),
    };
    stored.default_agent = Some(AgentId::CODEX);
    source.store.insert_project(&stored).unwrap();
    let exported =
        serde_json::to_value(export::build(&source.store, &export::Selection::full(true)).unwrap())
            .unwrap();

    let (target, _target_dir) = app_state("import-roundtrip-target");
    let consoles = inspect(&target, &exported).await.unwrap().consoles;
    assert_eq!(
        consoles[0].projects[0].default,
        Some(DirectoryChoiceKind::ExportedPath)
    );
    import(
        &target,
        &exported,
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
    )
    .await
    .unwrap();

    let created = stored_projects(&target).remove(0);
    assert_eq!(
        (created.name.as_str(), created.default_agent),
        ("repo", Some(AgentId::CODEX))
    );
    assert_eq!(created.tags, ["t"]);
    assert_eq!(created.description.as_deref(), Some("Round trip."));
    assert!(!created.trust_consent, "trust is not carried over");
    let console = target.store.list_consoles().unwrap().remove(0);
    assert_eq!(console.project_session_agent, Some(AgentId::GROK));
    assert_eq!(
        console.project_session_launch[AgentId::CLAUDE]
            .model
            .as_deref(),
        Some("opus")
    );
    assert_eq!(
        console.project_session_launch[AgentId::CLAUDE]
            .effort
            .as_deref(),
        Some("high")
    );

    // A document that names no agent for project sessions follows `default_agent`.
    let mut unnamed = exported.clone();
    unnamed["consoles"][0]
        .as_object_mut()
        .unwrap()
        .remove("project_session_agent")
        .expect("the export carries the setting");
    let (other, _other_dir) = app_state("import-roundtrip-unnamed");
    import(
        &other,
        &unnamed,
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
    )
    .await
    .unwrap();
    let console = other.store.list_consoles().unwrap().remove(0);
    assert_eq!(console.project_session_agent, None);
    assert_eq!(console.project_session_default(), AgentId::CLAUDE);
}

#[tokio::test]
async fn the_documents_settings_are_shown_by_inspecting_and_applied_only_when_asked_for() {
    let (state, _dir) = app_state("import-settings");
    let full = with_settings(document(vec![console("Hub", Vec::new())]), true, false);
    assert!(!state.store.auto_pull_repositories().unwrap());
    assert!(state.store.auto_delete_unresumable_archives().unwrap());

    let inspection = inspect(&state, &full).await.unwrap();
    let settings = inspection.settings.expect("the document carries settings");
    assert!(settings.auto_pull_repositories);
    assert!(!settings.current_auto_pull_repositories);
    assert!(!settings.auto_delete_unresumable_archives);
    assert!(settings.current_auto_delete_unresumable_archives);
    assert!(!state.store.auto_pull_repositories().unwrap());
    assert!(state.store.auto_delete_unresumable_archives().unwrap());
    assert!(inspect(&state, &document(Vec::new()))
        .await
        .unwrap()
        .settings
        .is_none());

    let imported = apply(&state, &full, Vec::new(), false, None, true)
        .await
        .unwrap();
    assert_eq!(imported.settings_applied, None);
    assert!(!state.store.auto_pull_repositories().unwrap());
    assert!(state.store.auto_delete_unresumable_archives().unwrap());

    let imported = apply(&state, &full, Vec::new(), true, None, true)
        .await
        .unwrap();
    assert_eq!(
        imported.settings_applied,
        Some(DocumentSettings {
            auto_pull_repositories: true,
            auto_delete_unresumable_archives: false,
            auto_select_account: false,
            auto_switch_on_usage_limit: false,
            sidebar_order_by_status: true,
            auto_start_quota_windows: false,
            auto_approve_session_requests: false,
            auto_approve_resume_requests: false,
            auto_approve_account_switches: false,
            auto_approve_handoffs: false,
            ask_questions_as_choices: true,
            git_workflow: GitWorkflowSettings::default(),
            provider_settings: BTreeMap::new(),
            agent_tags: BTreeMap::new(),
            keyboard_shortcuts: None,
        })
    );
    assert!(state.store.auto_pull_repositories().unwrap());
    assert!(!state.store.auto_delete_unresumable_archives().unwrap());
}

/// A full export carries both application settings and a single console's carries none, and
/// importing the full one on another machine sets both.
#[tokio::test]
async fn a_full_export_round_trips_both_settings_and_a_console_export_carries_none() {
    let (source, source_dir) = app_state("import-round-trip-source");
    source.store.set_auto_pull_repositories(true).unwrap();
    source
        .store
        .set_auto_delete_unresumable_archives(false)
        .unwrap();
    source.store.set_auto_select_account(true).unwrap();
    source.store.set_auto_switch_on_usage_limit(true).unwrap();
    source.store.set_sidebar_order_by_status(false).unwrap();
    source.store.set_auto_start_quota_windows(true).unwrap();
    let hub = crate::test_support::console(&source_dir);
    source.store.insert_console(&hub).unwrap();
    let full = serde_json::to_value(
        crate::config_document::export::build(&source.store, &export::Selection::full(true))
            .unwrap(),
    )
    .unwrap();
    let single = serde_json::to_value(
        crate::config_document::export::build(
            &source.store,
            &export::Selection::console(&hub.id, true),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(single
        .get("settings")
        .is_none_or(|settings| settings.is_null()));

    let (target, _target_dir) = app_state("import-round-trip-target");
    apply(&target, &single, Vec::new(), false, None, true)
        .await
        .unwrap();
    assert!(!target.store.auto_pull_repositories().unwrap());
    assert!(target.store.auto_delete_unresumable_archives().unwrap());
    apply(&target, &full, Vec::new(), true, None, true)
        .await
        .unwrap();
    assert!(target.store.auto_pull_repositories().unwrap());
    assert!(!target.store.auto_delete_unresumable_archives().unwrap());
    assert!(target.store.auto_select_account().unwrap());
    assert!(full["settings"]["auto_select_account"].as_bool().unwrap());
    assert!(target.store.auto_switch_on_usage_limit().unwrap());
    assert!(full["settings"]["auto_switch_on_usage_limit"]
        .as_bool()
        .unwrap());
    assert!(!target.store.sidebar_order_by_status().unwrap());
    assert_eq!(full["settings"]["sidebar_order_by_status"], false);
    assert!(target.store.auto_start_quota_windows().unwrap());
    assert_eq!(full["settings"]["auto_start_quota_windows"], true);
}

/// The automatic approval of session requests and of resume requests travels in a full export
/// and is applied on import, like the other portable settings.
#[tokio::test]
async fn the_automatic_approvals_round_trip_in_a_full_export() {
    let (source, source_dir) = app_state("import-auto-approve-source");
    source
        .store
        .set_auto_approve_session_requests(true)
        .unwrap();
    source.store.set_auto_approve_resume_requests(true).unwrap();
    source
        .store
        .set_auto_approve_account_switches(true)
        .unwrap();
    source.store.set_auto_approve_handoffs(true).unwrap();
    source
        .store
        .insert_console(&crate::test_support::console(&source_dir))
        .unwrap();
    let full = serde_json::to_value(
        crate::config_document::export::build(&source.store, &export::Selection::full(true))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(full["settings"]["auto_approve_session_requests"], true);
    assert_eq!(full["settings"]["auto_approve_resume_requests"], true);
    assert_eq!(full["settings"]["auto_approve_account_switches"], true);
    assert_eq!(full["settings"]["auto_approve_handoffs"], true);

    let (target, _target_dir) = app_state("import-auto-approve-target");
    let inspection = inspect(&target, &full).await.unwrap();
    let settings = inspection.settings.unwrap();
    assert!(settings.auto_approve_session_requests);
    assert!(!settings.current_auto_approve_session_requests);
    assert!(settings.auto_approve_resume_requests);
    assert!(!settings.current_auto_approve_resume_requests);
    assert!(settings.auto_approve_account_switches);
    assert!(!settings.current_auto_approve_account_switches);
    assert!(settings.auto_approve_handoffs);
    assert!(!settings.current_auto_approve_handoffs);

    apply(&target, &full, Vec::new(), false, None, true)
        .await
        .unwrap();
    assert!(!target.store.auto_approve_session_requests().unwrap());
    assert!(!target.store.auto_approve_resume_requests().unwrap());
    assert!(!target.store.auto_approve_account_switches().unwrap());
    assert!(!target.store.auto_approve_handoffs().unwrap());
    apply(&target, &full, Vec::new(), true, None, true)
        .await
        .unwrap();
    assert!(target.store.auto_approve_session_requests().unwrap());
    assert!(target.store.auto_approve_resume_requests().unwrap());
    assert!(target.store.auto_approve_account_switches().unwrap());
    assert!(target.store.auto_approve_handoffs().unwrap());
}

/// The question-asking switch is on by default, so what travels is the user turning it off:
/// the importing machine takes it only when settings are applied.
#[tokio::test]
async fn turning_off_asking_questions_as_choices_round_trips_in_a_full_export() {
    let (source, source_dir) = app_state("import-ask-choices-source");
    source.store.set_ask_questions_as_choices(false).unwrap();
    source
        .store
        .insert_console(&crate::test_support::console(&source_dir))
        .unwrap();
    let full = serde_json::to_value(
        crate::config_document::export::build(&source.store, &export::Selection::full(true))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(full["settings"]["ask_questions_as_choices"], false);

    let (target, _target_dir) = app_state("import-ask-choices-target");
    let settings = inspect(&target, &full).await.unwrap().settings.unwrap();
    assert!(!settings.ask_questions_as_choices);
    assert!(settings.current_ask_questions_as_choices);

    apply(&target, &full, Vec::new(), false, None, true)
        .await
        .unwrap();
    assert!(target.store.ask_questions_as_choices().unwrap());
    apply(&target, &full, Vec::new(), true, None, true)
        .await
        .unwrap();
    assert!(!target.store.ask_questions_as_choices().unwrap());
}

/// What travels is the user's choice, not the value in effect: a setting never chosen for is not
/// exported. The importing machine takes the choices only when settings are applied, per setting.
/// The approval settings are never exported, whatever the user chose; a document that lists them,
/// or a provider or setting this build does not declare, or a value a setting does not take, has
/// those entries skipped and the rest applied.
#[tokio::test]
async fn provider_settings_round_trip_in_a_full_export_and_approval_settings_stay_out() {
    let (source, source_dir) = app_state("import-provider-settings-source");
    let set = |state: &Arc<AppState>, agent, setting: &str, value| {
        state
            .store
            .set_provider_setting(agent, setting, Some(&value))
            .unwrap()
    };
    set(
        &source,
        AgentId::CODEX,
        "claude_compatibility",
        SettingValue::Switch(false),
    );
    set(
        &source,
        AgentId::CODEX,
        "approval_reviewer",
        SettingValue::Choice("auto_review".into()),
    );
    set(
        &source,
        AgentId::CLAUDE,
        "approval_mode",
        SettingValue::Choice("bypass".into()),
    );
    source
        .store
        .insert_console(&crate::test_support::console(&source_dir))
        .unwrap();
    let full = serde_json::to_value(
        crate::config_document::export::build(&source.store, &export::Selection::full(true))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        full["settings"]["provider_settings"],
        serde_json::json!({ "codex": { "claude_compatibility": false } })
    );
    assert!(!full.to_string().contains("approval"));

    let (target, _target_dir) = app_state("import-provider-settings-target");
    set(
        &target,
        AgentId::GROK,
        "claude_compatibility",
        SettingValue::Switch(false),
    );
    set(
        &target,
        AgentId::CODEX,
        "claude_compatibility",
        SettingValue::Switch(true),
    );
    set(
        &target,
        AgentId::CODEX,
        "approval_reviewer",
        SettingValue::Choice("user".into()),
    );
    // Entries a document may carry that this build does not take.
    let mut hostile = full.clone();
    hostile["settings"]["provider_settings"] = serde_json::json!({
        "codex": {
            "claude_compatibility": false,
            "approval_reviewer": "auto_review",
            "invented": true
        },
        "claude": { "approval_mode": "bypass" },
        "grok": { "claude_compatibility": "yes" },
        "future": { "thing": true }
    });
    let inspected = inspect(&target, &hostile).await.unwrap().settings.unwrap();
    assert_eq!(
        inspected.provider_settings,
        [(
            "codex".to_string(),
            [(
                "claude_compatibility".to_string(),
                SettingValue::Switch(false)
            )]
            .into()
        )]
        .into()
    );
    let current = &inspected.current_provider_settings;
    assert_eq!(
        current["grok"]["claude_compatibility"],
        Some(SettingValue::Switch(false))
    );
    assert_eq!(
        current["codex"]["claude_compatibility"],
        Some(SettingValue::Switch(true))
    );
    assert!(!current["codex"].contains_key("approval_reviewer"));
    assert!(!current.contains_key("claude"));

    let on = |agent| {
        target
            .store
            .provider_settings_of(agent)
            .unwrap()
            .switch("claude_compatibility")
    };
    apply(&target, &hostile, Vec::new(), false, None, true)
        .await
        .unwrap();
    assert!(on(AgentId::CODEX));
    apply(&target, &hostile, Vec::new(), true, None, true)
        .await
        .unwrap();
    assert!(!on(AgentId::CODEX));
    // The document makes no valid choice for Grok Build, so the target's stays.
    assert!(!on(AgentId::GROK));
    // Nothing the document held about approvals reached the target.
    let choices = target.store.provider_setting_choices().unwrap();
    assert_eq!(
        choices["codex"]["approval_reviewer"],
        SettingValue::Choice("user".into())
    );
    assert!(!choices.contains_key("claude"));
    assert!(!choices.contains_key("future"));
}

/// The tags of every agent travel in a full export, removed agents' included, and an import
/// replaces the tags of each agent the document lists and leaves the others; a key for an
/// agent this build does not know is skipped.
#[tokio::test]
async fn agent_tags_round_trip_in_a_full_export() {
    let (source, source_dir) = app_state("import-agent-tags-source");
    source
        .store
        .set_agent_tags(
            AgentId::CODEX,
            vec!["review".to_string(), "tests".to_string()],
        )
        .unwrap();
    source
        .store
        .insert_console(&crate::test_support::console(&source_dir))
        .unwrap();
    let full = serde_json::to_value(
        crate::config_document::export::build(&source.store, &export::Selection::full(true))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        full["settings"]["agent_tags"],
        serde_json::json!({ "codex": ["review", "tests"] })
    );

    let (target, _target_dir) = app_state("import-agent-tags-target");
    target
        .store
        .set_agent_tags(AgentId::CODEX, vec!["old".to_string()])
        .unwrap();
    target
        .store
        .set_agent_tags(AgentId::GROK, vec!["research".to_string()])
        .unwrap();
    let settings = inspect(&target, &full).await.unwrap().settings.unwrap();
    assert_eq!(settings.agent_tags["codex"], ["review", "tests"]);
    assert_eq!(settings.current_agent_tags["codex"], ["old"]);

    // Without settings applied, nothing changes.
    apply(&target, &full, Vec::new(), false, None, true)
        .await
        .unwrap();
    assert_eq!(target.store.agent_tags().unwrap()["codex"], ["old"]);
    apply(&target, &full, Vec::new(), true, None, true)
        .await
        .unwrap();
    let tags = target.store.agent_tags().unwrap();
    assert_eq!(tags["codex"], ["review", "tests"]);
    assert_eq!(tags["grok"], ["research"]);

    // A key for an unknown agent.
    let mut newer = full.clone();
    newer["settings"]["agent_tags"] = serde_json::json!({ "codex": ["x"], "someone_new": ["y"] });
    apply(&target, &newer, Vec::new(), true, None, true)
        .await
        .unwrap();
    let tags = target.store.agent_tags().unwrap();
    assert_eq!(tags["codex"], ["x"]);
    assert!(!tags.contains_key("someone_new"));
}

/// The interface language is a client's report, not part of the document: an export does not
/// carry it and an import leaves it alone. A settings key this build does not know, such as
/// `reply_in_interface_language`, is ignored rather than refused.
#[tokio::test]
async fn the_interface_language_is_not_carried_and_an_unknown_setting_is_ignored() {
    let (source, source_dir) = app_state("import-reply-language-source");
    source.store.set_interface_language("ja").unwrap();
    source
        .store
        .insert_console(&crate::test_support::console(&source_dir))
        .unwrap();
    let mut full = serde_json::to_value(
        crate::config_document::export::build(&source.store, &export::Selection::full(true))
            .unwrap(),
    )
    .unwrap();
    assert!(full["settings"].get("interface_language").is_none());
    assert!(full["settings"]
        .get("reply_in_interface_language")
        .is_none());

    full["settings"]["reply_in_interface_language"] = serde_json::json!(false);
    let (target, _target_dir) = app_state("import-reply-language-target");
    inspect(&target, &full).await.unwrap().settings.unwrap();
    apply(&target, &full, Vec::new(), true, None, true)
        .await
        .unwrap();
    assert_eq!(target.store.interface_language().unwrap(), None);
}

/// The git workflow settings travel in a full export beside each project's own override: the
/// inspection shows the document's and the current ones, applying settings sets the global
/// ones, and a created project gets its override from the document. A single console's export
/// carries the override of its projects but no global settings.
#[tokio::test]
async fn the_git_workflow_settings_and_a_projects_override_round_trip_in_an_export() {
    use crate::protocol::{GitDelivery, GitDevelopment, GitWorkflowOverride, GitWorkflowSettings};
    let area = ScratchDir::new("import-git-workflow-area");
    let repo = directory(&area, "repo");
    let (source, source_dir) = app_state("import-git-workflow-source");
    let hub = crate::test_support::console(&source_dir);
    source.store.insert_console(&hub).unwrap();
    let own = GitWorkflowOverride {
        development: Some(GitDevelopment::InPlace),
        squash_before_delivery: Some(false),
        delivery: Some(GitDelivery::PullRequest),
        ..GitWorkflowOverride::default()
    };
    let mut stored = crate::test_support::insert_project(&source, &repo);
    stored.git_workflow = own;
    source.store.update_project(&stored).unwrap();
    source
        .store
        .change_git_workflow_settings(&crate::protocol::GitWorkflowSettingsChanges {
            squash_before_delivery: Some(true),
            rebase_before_delivery: Some(false),
            delivery: Some(GitDelivery::Merge),
            ..Default::default()
        })
        .unwrap();
    let global = source.store.git_workflow_settings().unwrap();
    let full = serde_json::to_value(
        crate::config_document::export::build(&source.store, &export::Selection::full(true))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(full["settings"]["git_workflow"]["delivery"], "merge");
    assert_eq!(
        full["settings"]["git_workflow"]["squash_before_delivery"],
        true
    );
    assert_eq!(
        full["consoles"][0]["projects"][0]["git_workflow"]["squash_before_delivery"],
        false
    );
    assert_eq!(
        full["consoles"][0]["projects"][0]["git_workflow"]["delivery"],
        "pull_request"
    );
    assert!(full["consoles"][0]["projects"][0]["git_workflow"]["revert_gate"].is_null());
    let single = serde_json::to_value(
        crate::config_document::export::build(
            &source.store,
            &export::Selection::console(&hub.id, true),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(single["settings"].is_null());
    assert_eq!(
        single["consoles"][0]["projects"][0]["git_workflow"]["development"],
        "in_place"
    );

    let (target, _target_dir) = app_state("import-git-workflow-target");
    let settings = inspect(&target, &full).await.unwrap().settings.unwrap();
    assert_eq!(settings.git_workflow, global);
    assert_eq!(
        settings.current_git_workflow,
        GitWorkflowSettings::default()
    );

    apply(
        &target,
        &full,
        vec![at(0, 0, DirectoryChoice::ExportedPath)],
        false,
        None,
        true,
    )
    .await
    .unwrap();
    assert_eq!(stored_projects(&target)[0].git_workflow, own);
    assert_eq!(
        target.store.git_workflow_settings().unwrap(),
        GitWorkflowSettings::default(),
        "settings are applied only when asked for"
    );
    let mut events = target.subscribe();
    apply(&target, &full, Vec::new(), true, None, true)
        .await
        .unwrap();
    assert_eq!(target.store.git_workflow_settings().unwrap(), global);
    let mut broadcast = false;
    while let Ok(event) = events.try_recv() {
        broadcast |= matches!(
            event,
            crate::protocol::Event::SettingsUpdated { settings } if settings.git_workflow == global
        );
    }
    assert!(broadcast);
}

/// The keyboard shortcuts a client adds to a full export belong to the client: an import
/// inspects and applies a document that carries them, and hands them back as they came, and
/// one without them is still read.
#[tokio::test]
async fn the_keyboard_shortcuts_of_a_document_pass_through_untouched() {
    let (state, _dir) = app_state("import-keyboard-shortcuts");
    let shortcuts = json!({ "lock": { "code": "KeyK", "primary": true, "shift": true } });
    let mut full = with_settings(document(vec![console("Hub", Vec::new())]), true, false);
    full["settings"]["keyboard_shortcuts"] = shortcuts.clone();

    inspect(&state, &full).await.unwrap();
    let imported = apply(&state, &full, Vec::new(), true, None, true)
        .await
        .unwrap();
    assert_eq!(
        imported.settings_applied.unwrap().keyboard_shortcuts,
        Some(shortcuts)
    );

    let without = with_settings(document(vec![console("Hub", Vec::new())]), true, false);
    let imported = apply(&state, &without, Vec::new(), true, None, true)
        .await
        .unwrap();
    assert_eq!(imported.settings_applied.unwrap().keyboard_shortcuts, None);
}

#[tokio::test]
async fn asking_for_settings_a_document_does_not_carry_is_refused_before_anything_is_created() {
    let (state, _dir) = app_state("import-settings-missing");

    let err = apply(
        &state,
        &document(vec![console("Hub", Vec::new())]),
        Vec::new(),
        true,
        None,
        true,
    )
    .await
    .expect_err("refused");

    assert_eq!(code_of(&err), error_code::IMPORT_SETTINGS_MISSING);
    assert!(state.store.list_consoles().unwrap().is_empty());
}
