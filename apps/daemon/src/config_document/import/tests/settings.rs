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
                compatibility: Default::default(),
                model: Some("opus".to_string()),
                effort: Some("high".to_string()),
            },
        ),
        icon: None,
        console_session_prompt: None,
        created_at: 1,
        versions: Default::default(),
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
        versions: Default::default(),
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

    let imported = apply(
        &state,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(imported.settings_applied, None);
    assert!(!state.store.auto_pull_repositories().unwrap());
    assert!(state.store.auto_delete_unresumable_archives().unwrap());

    let imported = apply(
        &state,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        imported.settings_applied,
        Some(DocumentSettings {
            auto_close_idle_shell_terminals: Some(false),
            auto_pull_repositories: Some(true),
            auto_delete_unresumable_archives: Some(false),
            auto_approve_session_requests: Some(false),
            auto_approve_resume_requests: Some(false),
            auto_approve_account_switches: Some(false),
            auto_approve_handoffs: Some(false),
            ask_questions_as_choices: Some(true),
            git_workflow: Some(GitWorkflowSettings::default()),
            ..Default::default()
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
    source.store.set_resume_sessions_on_start(true).unwrap();
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
    apply(
        &target,
        &single,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(!target.store.auto_pull_repositories().unwrap());
    assert!(target.store.auto_delete_unresumable_archives().unwrap());
    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(target.store.auto_pull_repositories().unwrap());
    assert!(!target.store.auto_delete_unresumable_archives().unwrap());
    assert!(target.store.resume_sessions_on_start().unwrap());
    assert_eq!(full["settings"]["resume_sessions_on_start"], true);
}

/// A document from a build that had no Resume sessions on start setting leaves this machine's
/// choice alone, and the review reads it as no change.
#[tokio::test]
async fn a_document_without_the_resume_setting_leaves_it_as_this_machine_has_it() {
    let (state, dir) = app_state("import-resume-setting-absent");
    state.store.set_resume_sessions_on_start(true).unwrap();
    let hub = crate::test_support::console(&dir);
    state.store.insert_console(&hub).unwrap();
    let mut full = serde_json::to_value(
        crate::config_document::export::build(&state.store, &export::Selection::full(true))
            .unwrap(),
    )
    .unwrap();
    full["settings"]
        .as_object_mut()
        .unwrap()
        .remove("resume_sessions_on_start");

    let inspected = inspect(&state, &full).await.unwrap().settings.unwrap();
    assert!(inspected.resume_sessions_on_start);
    assert!(inspected.current_resume_sessions_on_start);
    apply(
        &state,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(state.store.resume_sessions_on_start().unwrap());
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

    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(!target.store.auto_approve_session_requests().unwrap());
    assert!(!target.store.auto_approve_resume_requests().unwrap());
    assert!(!target.store.auto_approve_account_switches().unwrap());
    assert!(!target.store.auto_approve_handoffs().unwrap());
    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
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

    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(target.store.ask_questions_as_choices().unwrap());
    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(!target.store.ask_questions_as_choices().unwrap());
}

/// The switch that keeps sessions to their own project is off by default, so what travels is the
/// user turning it on: the document carries it in the Agent behavior section, the inspection
/// reports it beside the machine's own value, and the importing machine takes it only when
/// settings are applied. A document without it leaves the machine's own value as it is.
#[tokio::test]
async fn keeping_sessions_to_their_own_project_round_trips_in_a_full_export() {
    let (source, source_dir) = app_state("import-own-project-source");
    source.store.set_keep_sessions_in_own_project(true).unwrap();
    source
        .store
        .insert_console(&crate::test_support::console(&source_dir))
        .unwrap();
    let full = serde_json::to_value(
        crate::config_document::export::build(&source.store, &export::Selection::full(true))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(full["settings"]["keep_sessions_in_own_project"], true);

    let (target, _target_dir) = app_state("import-own-project-target");
    let settings = inspect(&target, &full).await.unwrap().settings.unwrap();
    assert!(settings.keep_sessions_in_own_project);
    assert!(!settings.current_keep_sessions_in_own_project);

    let apply_with = |sections: Vec<SettingsSection>, document: serde_json::Value| {
        let target = target.clone();
        async move {
            apply(
                &target,
                &document,
                Vec::new(),
                ApplyOptions {
                    settings: sections,
                    accounts: None,
                    consoles: true,
                    clone_dir: true,
                    agents: None,
                },
            )
            .await
            .unwrap();
        }
    };
    apply_with(Vec::new(), full.clone()).await;
    assert!(!target.store.keep_sessions_in_own_project().unwrap());
    apply_with(carried_sections(&full), full.clone()).await;
    assert!(target.store.keep_sessions_in_own_project().unwrap());

    // An older document, whose section lacks the key, leaves the value as it is, and the
    // inspection reads as no change.
    let mut older = full.clone();
    older["settings"]
        .as_object_mut()
        .unwrap()
        .remove("keep_sessions_in_own_project");
    let settings = inspect(&target, &older).await.unwrap().settings.unwrap();
    assert!(settings.keep_sessions_in_own_project);
    assert!(settings.current_keep_sessions_in_own_project);
    apply_with(carried_sections(&older), older).await;
    assert!(target.store.keep_sessions_in_own_project().unwrap());
}

/// What travels is the user's choice, not the value in effect: a setting never chosen for is not
/// exported. They travel in the accounts region, and the importing machine takes them only when the
/// accounts are applied, per setting. The approval settings are exported too, since they only
/// become launch flags. A provider or setting this build does not declare, or a value a setting
/// does not take, has its entry skipped and the rest applied.
#[tokio::test]
async fn provider_settings_round_trip_with_the_accounts_approval_settings_included() {
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
    set(
        &source,
        AgentId::GROK,
        "approval_mode",
        SettingValue::Choice("always_approve".into()),
    );
    source
        .store
        .insert_console(&crate::test_support::console(&source_dir))
        .unwrap();
    let full = export_everything(&source);
    assert!(full["settings"].get("provider_settings").is_none());
    assert_eq!(
        full["accounts"]["agent_settings"]["provider_settings"],
        serde_json::json!({
            "codex": { "claude_compatibility": false, "approval_reviewer": "auto_review" },
            "claude": { "approval_mode": "bypass" },
            "grok": { "approval_mode": "always_approve" },
        })
    );

    let (target, _target_dir) = app_state("import-provider-settings-target");
    set(
        &target,
        AgentId::GROK,
        "claude_compatibility",
        SettingValue::Switch(true),
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
    hostile["accounts"]["agent_settings"]["provider_settings"] = serde_json::json!({
        "codex": {
            "claude_compatibility": false,
            "approval_reviewer": "auto_review",
            "invented": true
        },
        "claude": { "approval_mode": "invented" },
        "grok": { "claude_compatibility": "yes" },
        "future": { "thing": true }
    });
    let inspected = inspect(&target, &hostile)
        .await
        .unwrap()
        .accounts
        .unwrap()
        .agent_settings
        .unwrap();
    assert_eq!(
        inspected.provider_settings,
        [(
            "codex".to_string(),
            [
                (
                    "claude_compatibility".to_string(),
                    SettingValue::Switch(false)
                ),
                (
                    "approval_reviewer".to_string(),
                    SettingValue::Choice("auto_review".into())
                ),
            ]
            .into()
        )]
        .into()
    );
    let current = &inspected.current_provider_settings;
    assert_eq!(
        current["grok"]["claude_compatibility"],
        Some(SettingValue::Switch(true))
    );
    assert_eq!(
        current["codex"]["claude_compatibility"],
        Some(SettingValue::Switch(true))
    );
    assert_eq!(
        current["codex"]["approval_reviewer"],
        Some(SettingValue::Choice("user".into()))
    );

    let on = |agent| {
        target
            .store
            .provider_settings_of(agent)
            .unwrap()
            .switch("claude_compatibility")
    };
    // The settings alone leave them be.
    apply(
        &target,
        &hostile,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&hostile),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(on(AgentId::CODEX));
    apply(
        &target,
        &hostile,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: Some(Vec::new()),
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(!on(AgentId::CODEX));
    // The document makes no valid choice for Grok Build, so the target's stays.
    assert!(on(AgentId::GROK));
    let choices = target.store.provider_setting_choices().unwrap();
    // The imported `false` is a stored choice, not the setting's default.
    assert_eq!(
        choices["codex"]["claude_compatibility"],
        SettingValue::Switch(false)
    );
    assert_eq!(
        choices["codex"]["approval_reviewer"],
        SettingValue::Choice("auto_review".into())
    );
    assert!(!choices.contains_key("claude"));
    assert!(!choices.contains_key("future"));

    let inspected = inspect(&target, &full)
        .await
        .unwrap()
        .accounts
        .unwrap()
        .agent_settings
        .unwrap();
    assert_eq!(
        inspected.provider_settings["grok"]["approval_mode"],
        SettingValue::Choice("always_approve".into())
    );
    assert_eq!(
        inspected.current_provider_settings["grok"]["approval_mode"],
        None
    );
    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: Some(Vec::new()),
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        target
            .store
            .provider_settings_of(AgentId::GROK)
            .unwrap()
            .choice("approval_mode"),
        Some("always_approve")
    );
}

/// The tags of every agent travel with the accounts, removed agents' included, and an import
/// replaces the tags of each agent the document lists and leaves the others; a key for an
/// agent this build does not know is skipped.
#[tokio::test]
async fn agent_tags_round_trip_with_the_accounts() {
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
    let full = export_everything(&source);
    assert_eq!(
        full["accounts"]["agent_settings"]["agent_tags"],
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
    let settings = inspect(&target, &full)
        .await
        .unwrap()
        .accounts
        .unwrap()
        .agent_settings
        .unwrap();
    assert_eq!(settings.agent_tags["codex"], ["review", "tests"]);
    assert_eq!(settings.current_agent_tags["codex"], ["old"]);

    // Without the accounts applied, nothing changes.
    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(target.store.agent_tags().unwrap()["codex"], ["old"]);
    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: Some(Vec::new()),
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    let tags = target.store.agent_tags().unwrap();
    assert_eq!(tags["codex"], ["review", "tests"]);
    assert_eq!(tags["grok"], ["research"]);

    // A key for an unknown agent.
    let mut newer = full.clone();
    newer["accounts"]["agent_settings"]["agent_tags"] =
        serde_json::json!({ "codex": ["x"], "someone_new": ["y"] });
    apply(
        &target,
        &newer,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: Some(Vec::new()),
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    let tags = target.store.agent_tags().unwrap();
    assert_eq!(tags["codex"], ["x"]);
    assert!(!tags.contains_key("someone_new"));
}

/// The three switches of the Agent accounts section travel with the accounts, and are applied
/// only with them.
#[tokio::test]
async fn the_agent_account_switches_round_trip_with_the_accounts() {
    let (source, _source_dir) = app_state("import-agent-switches-source");
    source.store.set_auto_select_account(true).unwrap();
    source.store.set_auto_switch_on_usage_limit(true).unwrap();
    source.store.set_auto_start_quota_windows(true).unwrap();
    let full = export_everything(&source);
    assert_eq!(
        full["accounts"]["agent_settings"]["auto_select_account"],
        true
    );
    assert!(full["settings"].get("auto_select_account").is_none());

    let (target, _target_dir) = app_state("import-agent-switches-target");
    let inspected = inspect(&target, &full)
        .await
        .unwrap()
        .accounts
        .unwrap()
        .agent_settings
        .unwrap();
    assert!(inspected.auto_select_account && !inspected.current_auto_select_account);
    assert!(inspected.auto_start_quota_windows && !inspected.current_auto_start_quota_windows);

    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(!target.store.auto_select_account().unwrap());
    let imported = apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: Some(Vec::new()),
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(target.store.auto_select_account().unwrap());
    assert!(target.store.auto_switch_on_usage_limit().unwrap());
    assert!(target.store.auto_start_quota_windows().unwrap());
    let echoed = imported.accounts.unwrap().agent_settings_applied.unwrap();
    assert!(echoed.auto_select_account);
}

/// Prevent sleep is the daemon's own: every export carries it, an inspection shows it beside this
/// machine's, and an import stores it, a name of when to hold this build does not know being left
/// out of the application and of the reply.
#[tokio::test]
async fn prevent_sleep_round_trips_through_the_daemon() {
    let (source, _source_dir) = app_state("import-keep-awake-source");
    source.store.set_keep_awake(true).unwrap();
    source
        .store
        .set_keep_awake_when(KeepAwakeWhen::Always)
        .unwrap();
    let full = export_everything(&source);
    assert_eq!(full["settings"]["keep_awake"], true);
    assert_eq!(full["settings"]["keep_awake_when"], "always");

    let (target, _target_dir) = app_state("import-keep-awake-target");
    let inspected = inspect(&target, &full).await.unwrap().settings.unwrap();
    assert!(inspected.keep_awake && !inspected.current_keep_awake);
    assert_eq!(inspected.keep_awake_when, KeepAwakeWhen::Always);
    assert_eq!(inspected.current_keep_awake_when, KeepAwakeWhen::Working);

    let options = |document: &serde_json::Value| ApplyOptions {
        settings: carried_sections(document),
        accounts: None,
        consoles: true,
        clone_dir: true,
        agents: None,
    };
    let imported = apply(&target, &full, Vec::new(), options(&full))
        .await
        .unwrap();
    assert!(target.store.keep_awake().unwrap());
    assert_eq!(
        target.store.keep_awake_when().unwrap(),
        KeepAwakeWhen::Always
    );
    let echoed = imported.settings_applied.unwrap();
    assert_eq!(echoed.keep_awake, Some(true));
    assert_eq!(echoed.keep_awake_when.as_deref(), Some("always"));

    let mut unknown = full.clone();
    unknown["settings"]["keep_awake"] = json!(false);
    unknown["settings"]["keep_awake_when"] = json!("sometimes");
    let inspected = inspect(&target, &unknown).await.unwrap().settings.unwrap();
    assert_eq!(inspected.keep_awake_when, KeepAwakeWhen::Always);
    let imported = apply(&target, &unknown, Vec::new(), options(&unknown))
        .await
        .unwrap();
    assert!(!target.store.keep_awake().unwrap());
    assert_eq!(
        target.store.keep_awake_when().unwrap(),
        KeepAwakeWhen::Always
    );
    assert_eq!(imported.settings_applied.unwrap().keep_awake_when, None);
}

/// A version 1 document, as the beta releases wrote it, is upgraded before it is read: the Agent
/// accounts settings move from `settings` into the accounts region, an account's sharing flag is
/// dropped, and what version 1 never carried reads as absent. Inspecting and applying see the same
/// upgraded document.
#[tokio::test]
async fn a_version_1_document_is_upgraded_and_applies_its_agent_account_settings_with_the_accounts()
{
    let older: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/configuration-v1-beta2.json")).unwrap();
    let upgraded = parse(&older).unwrap();
    assert_eq!(upgraded.version, CONFIGURATION_VERSION);
    let settings = upgraded.settings.as_ref().unwrap();
    assert_eq!(settings.default_clone_dir, None);
    let accounts = upgraded.accounts.as_ref().unwrap();
    let agent_settings = accounts.agent_settings.as_ref().unwrap();
    assert!(agent_settings.auto_select_account && agent_settings.auto_start_quota_windows);
    assert!(!agent_settings.auto_switch_on_usage_limit);
    assert_eq!(agent_settings.agent_tags["codex"], ["review"]);
    assert_eq!(accounts.agents[0].default_account.disabled, None);

    let (target, _dir) = app_state("import-v1-agent-settings");
    let inspection = inspect(&target, &older).await.unwrap();
    let inspected = inspection.accounts.unwrap().agent_settings.unwrap();
    assert!(inspected.auto_select_account && !inspected.current_auto_select_account);
    assert!(inspection.settings.unwrap().default_clone_dir.is_none());

    // The settings region no longer carries them, so applying it leaves them as they are.
    apply(
        &target,
        &older,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&older),
            accounts: None,
            consoles: false,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(!target.store.auto_select_account().unwrap());

    let imported = apply(
        &target,
        &older,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: Some(vec![AccountResolution {
                agent: AgentId::CLAUDE,
                account: 0,
                directory: None,
                leave_out: true,
            }]),
            consoles: false,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(target.store.auto_select_account().unwrap());
    assert!(!target.store.auto_switch_on_usage_limit().unwrap());
    assert!(target.store.auto_start_quota_windows().unwrap());
    assert_eq!(target.store.agent_tags().unwrap()["codex"], ["review"]);
    // The document's explicit `false` is stored as a choice, not left to the setting's default.
    assert_eq!(
        target.store.provider_setting_choices().unwrap()["codex"]["claude_compatibility"],
        SettingValue::Switch(false)
    );
    assert!(imported.accounts.unwrap().agent_settings_applied.is_some());
}

/// A version 2 document still carries `settings.sidebar_order_by_status`; the upgrade drops it, on
/// its own and as the last step of a version 1 document's chain.
#[test]
fn a_version_2_document_loses_the_sidebar_order_setting() {
    let mut older = with_settings(document(Vec::new()), true, false);
    older["version"] = json!(2);
    older["settings"]["sidebar_order_by_status"] = json!(true);
    let mut raw = older.as_object().unwrap().clone();
    crate::config_document::upgrade::upgrade(&mut raw, 2);
    assert_eq!(raw["version"], json!(CONFIGURATION_VERSION));
    assert!(!raw["settings"]
        .as_object()
        .unwrap()
        .contains_key("sidebar_order_by_status"));

    let v1: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/configuration-v1-beta2.json")).unwrap();
    assert!(v1["settings"]
        .as_object()
        .unwrap()
        .contains_key("sidebar_order_by_status"));
    let mut raw = v1.as_object().unwrap().clone();
    crate::config_document::upgrade::upgrade(&mut raw, 1);
    assert_eq!(raw["version"], json!(CONFIGURATION_VERSION));
    assert!(!raw["settings"]
        .as_object()
        .unwrap()
        .contains_key("sidebar_order_by_status"));
}

/// A version 1 document with settings but no accounts region keeps its Agent accounts settings:
/// the upgrade gives it an accounts region that holds only them. One with accounts but no settings
/// has none to apply, and one with neither has no accounts region either.
#[test]
fn a_version_1_document_without_one_of_the_two_regions_is_upgraded_all_the_same() {
    let mut settings_only: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/configuration-v1-beta2.json")).unwrap();
    settings_only.as_object_mut().unwrap().remove("accounts");
    let parsed = parse(&settings_only).unwrap();
    let accounts = parsed.accounts.unwrap();
    assert!(accounts.agents.is_empty() && accounts.added_agents.is_none());
    assert!(accounts.agent_settings.unwrap().auto_select_account);

    let mut accounts_only: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/configuration-v1-beta2.json")).unwrap();
    accounts_only.as_object_mut().unwrap().remove("settings");
    let parsed = parse(&accounts_only).unwrap();
    assert!(parsed.settings.is_none());
    assert!(parsed.accounts.unwrap().agent_settings.is_none());

    let mut neither = accounts_only.clone();
    neither.as_object_mut().unwrap().remove("accounts");
    assert!(parse(&neither).unwrap().accounts.is_none());
}

/// The interface language travels: an export holds the one the daemon has, and an import sets it
/// when this build offers the tag, and skips it otherwise. A settings key this build does not
/// know, such as `reply_in_interface_language`, is ignored rather than refused.
#[tokio::test]
async fn the_interface_language_is_carried_and_an_unknown_setting_is_ignored() {
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
    assert_eq!(full["settings"]["interface_language"], "ja");
    assert!(full["settings"]
        .get("reply_in_interface_language")
        .is_none());

    full["settings"]["reply_in_interface_language"] = serde_json::json!(false);
    let (target, _target_dir) = app_state("import-reply-language-target");
    inspect(&target, &full).await.unwrap().settings.unwrap();
    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(target.store.interface_language().unwrap(), None);
    let imported = apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        target.store.interface_language().unwrap().as_deref(),
        Some("ja")
    );
    assert_eq!(
        imported
            .settings_applied
            .unwrap()
            .interface_language
            .as_deref(),
        Some("ja")
    );

    // A tag this build does not offer is skipped and the rest of the settings still apply.
    full["settings"]["interface_language"] = serde_json::json!("tlh");
    full["settings"]["auto_pull_repositories"] = serde_json::json!(true);
    let imported = apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(imported.settings_error.is_none());
    // The echo names the language only where it was applied.
    assert_eq!(imported.settings_applied.unwrap().interface_language, None);
    assert_eq!(
        target.store.interface_language().unwrap().as_deref(),
        Some("ja")
    );
    assert!(target.store.auto_pull_repositories().unwrap());
}

/// The values only a client holds (appearance, Markdown style, running in the background, the menu
/// bar icon) are not built by the daemon and pass through an import's reply as they
/// came.
#[tokio::test]
async fn the_clients_own_values_pass_through_an_import_untouched() {
    let (source, _dir) = app_state("import-client-values-source");
    let full = export_everything(&source);
    for client_held in [
        "appearance",
        "markdown_style",
        "terminal_font_size",
        "run_in_background",
        "show_menu_bar_icon",
    ] {
        assert!(full["settings"].get(client_held).is_none());
    }

    let mut carried = full.clone();
    carried["settings"]["appearance"] = json!("dark");
    carried["settings"]["markdown_style"] = json!("github");
    carried["settings"]["terminal_font_size"] = json!(18);
    carried["settings"]["run_in_background"] = json!(true);
    carried["settings"]["show_menu_bar_icon"] = json!(false);
    let (target, _target_dir) = app_state("import-client-values-target");
    let imported = apply(
        &target,
        &carried,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&carried),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    let echoed = imported.settings_applied.unwrap();
    assert_eq!(echoed.appearance.as_deref(), Some("dark"));
    assert_eq!(echoed.markdown_style.as_deref(), Some("github"));
    assert_eq!(echoed.terminal_font_size, Some(18));
    assert_eq!(echoed.run_in_background, Some(true));
    assert_eq!(echoed.show_menu_bar_icon, Some(false));

    // Absent in the document: absent in the reply, so a client leaves its own alone.
    let imported = apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    let echoed = imported.settings_applied.unwrap();
    assert_eq!(
        (
            echoed.appearance,
            echoed.markdown_style,
            echoed.terminal_font_size,
            echoed.run_in_background,
            echoed.show_menu_bar_icon
        ),
        (None, None, None, None, None)
    );
}

/// The default clone directory travels in the settings, whatever the export's paths choice, written
/// relative to the home directory where it is under it. Importing applies it only when the user
/// asks to, before the projects are placed, so a project of the same import goes where the choice
/// puts it; a directory the document carries that is not usable here is left out.
#[tokio::test]
async fn a_default_clone_directory_written_on_another_operating_system_is_marked_and_left_out() {
    let (state, _dir) = app_state("import-foreign-clone-dir");
    let mut carried = with_settings(document(Vec::new()), false, false);
    carried["settings"]["default_clone_dir"] = json!(foreign_path());

    let settings = inspect(&state, &carried).await.unwrap().settings.unwrap();

    assert!(settings.foreign_default_clone_dir);
    assert_eq!(settings.default_clone_dir, None);

    carried["settings"]["default_clone_dir"] = json!("Projects");
    let settings = inspect(&state, &carried).await.unwrap().settings.unwrap();
    assert!(!settings.foreign_default_clone_dir);
    assert_eq!(settings.default_clone_dir, None);
}

#[tokio::test]
async fn the_default_clone_directory_travels_and_the_user_may_keep_the_machines_own() {
    let area = ScratchDir::new("import-clone-dir-area");
    let theirs = directory(&area, "theirs");
    let own = directory(&area, "own");
    directory(&theirs, "derived");
    directory(&own, "derived");
    let hub = console(
        "Hub",
        vec![project("derived", &host_path("/gone/layout/derived"))],
    );
    let mut carried = with_settings(document(vec![hub]), false, false);
    carried["settings"]["default_clone_dir"] = json!(theirs.to_string_lossy());
    let place = || vec![at(0, 0, DirectoryChoice::DefaultCloneDir)];

    let (state, _dir) = state_cloning_into("import-clone-dir-keep", &own);
    let settings = inspect(&state, &carried).await.unwrap().settings.unwrap();
    assert_eq!(
        settings.default_clone_dir.as_deref(),
        Some(theirs.to_string_lossy().as_ref())
    );
    assert_eq!(settings.current_default_clone_dir, own.to_string_lossy());

    // Keeping the machine's own: it stays, and the project goes under it.
    let imported = apply(
        &state,
        &carried,
        place(),
        ApplyOptions {
            settings: carried_sections(&carried),
            accounts: None,
            consoles: true,
            clone_dir: false,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        state.store.default_clone_dir().unwrap(),
        own.to_string_lossy()
    );
    assert_eq!(imported.clone_dir_applied, None);
    assert_eq!(imported.settings_applied.unwrap().default_clone_dir, None);
    assert_eq!(
        stored_projects(&state)[0].path,
        own.join("derived").to_string_lossy()
    );

    // Taking the document's: written before the projects, which go under it.
    let (state, _dir) = state_cloning_into("import-clone-dir-take", &own);
    let imported = apply(
        &state,
        &carried,
        place(),
        ApplyOptions {
            settings: carried_sections(&carried),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        state.store.default_clone_dir().unwrap(),
        theirs.to_string_lossy()
    );
    assert_eq!(
        imported.clone_dir_applied.as_deref(),
        Some(theirs.to_string_lossy().as_ref())
    );
    assert_eq!(
        imported
            .settings_applied
            .unwrap()
            .default_clone_dir
            .as_deref(),
        Some(theirs.to_string_lossy().as_ref())
    );
    assert_eq!(
        stored_projects(&state)[0].path,
        theirs.join("derived").to_string_lossy()
    );

    // Settings not asked for: nothing is written, whatever `apply_clone_dir` says.
    let (state, _dir) = state_cloning_into("import-clone-dir-no-settings", &own);
    apply(
        &state,
        &carried,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        state.store.default_clone_dir().unwrap(),
        own.to_string_lossy()
    );

    // A directory that is not usable here is left out, and the other settings still apply.
    let mut unusable = carried.clone();
    unusable["settings"]["default_clone_dir"] = json!("relative/projects");
    unusable["settings"]["auto_pull_repositories"] = json!(true);
    let settings = inspect(&state, &unusable).await.unwrap().settings.unwrap();
    assert_eq!(settings.default_clone_dir, None);
    let imported = apply(
        &state,
        &unusable,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&unusable),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(imported.settings_error.is_none());
    assert_eq!(
        state.store.default_clone_dir().unwrap(),
        own.to_string_lossy()
    );
    assert!(state.store.auto_pull_repositories().unwrap());

    // A `~/` directory is expanded on this machine.
    let mut home_relative = carried.clone();
    home_relative["settings"]["default_clone_dir"] = json!("~/octoboard-test-clones");
    apply(
        &state,
        &home_relative,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&home_relative),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        state.store.default_clone_dir().unwrap(),
        crate::paths::home_dir()
            .join("octoboard-test-clones")
            .to_string_lossy()
    );
}

/// The review shows the targets the import will use for either choice about the default clone
/// directory: the machine's own offers as ever, and, beside them, the offers under the document's
/// directory, only where it carries a usable one that differs from this machine's.
#[tokio::test]
async fn the_inspection_offers_the_clone_dependent_targets_under_the_documents_clone_directory_too()
{
    let area = ScratchDir::new("import-clone-dir-offers-area");
    let theirs = directory(&area, "theirs");
    let own = directory(&area, "own");
    directory(&theirs, "only-theirs");
    directory(&own, "only-own");
    let hub = console(
        "Hub",
        vec![
            project("only-theirs", &host_path("/gone/only-theirs")),
            project("only-own", &host_path("/gone/only-own")),
        ],
    );
    let mut carried = with_settings(document(vec![hub]), false, false);
    carried["settings"]["default_clone_dir"] = json!(theirs.to_string_lossy());
    let (state, _dir) = state_cloning_into("import-clone-dir-offers", &own);

    let consoles = inspect(&state, &carried).await.unwrap().consoles;

    let [theirs_only, own_only] = &consoles[0].projects[..] else {
        panic!("two projects")
    };
    assert_eq!(theirs_only.offers.default_clone_dir, None);
    assert_eq!(theirs_only.default, None);
    let under = theirs_only.in_document_clone_dir.as_ref().unwrap();
    assert_eq!(
        under.offers.default_clone_dir.as_deref(),
        Some(theirs.join("only-theirs").to_string_lossy().as_ref())
    );
    assert_eq!(under.default, Some(DirectoryChoiceKind::DefaultCloneDir));
    assert_eq!(own_only.default, Some(DirectoryChoiceKind::DefaultCloneDir));
    assert_eq!(
        own_only.in_document_clone_dir.as_ref().unwrap().default,
        None
    );

    // What the review showed is what applying does.
    let imported = apply(
        &state,
        &carried,
        vec![at(0, 0, DirectoryChoice::DefaultCloneDir)],
        ApplyOptions {
            settings: carried_sections(&carried),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        imported.consoles[0].projects[0].status,
        ImportStatus::Created
    );
    assert_eq!(
        stored_projects(&state)[0].path,
        theirs.join("only-theirs").to_string_lossy()
    );

    // No alternative where the document has no directory, or the machine's is the same.
    let mut same = carried.clone();
    same["settings"]["default_clone_dir"] = json!(own.to_string_lossy());
    // (A fresh machine: the import above took the file's directory as its own.)
    let (state, _dir) = state_cloning_into("import-clone-dir-offers-none", &own);
    for document in [carried_without_dir(&carried), same] {
        let consoles = inspect(&state, &document).await.unwrap().consoles;
        assert!(consoles[0]
            .projects
            .iter()
            .all(|p| p.in_document_clone_dir.is_none()));
    }
    let consoles = inspect(&state, &carried_consoles_only(&carried))
        .await
        .unwrap()
        .consoles;
    assert!(consoles[0].projects[0].in_document_clone_dir.is_none());
}

fn carried_without_dir(carried: &serde_json::Value) -> serde_json::Value {
    let mut document = carried.clone();
    document["settings"]["default_clone_dir"] = serde_json::Value::Null;
    document
}

fn carried_consoles_only(carried: &serde_json::Value) -> serde_json::Value {
    let mut document = carried.clone();
    document["settings"] = serde_json::Value::Null;
    document
}

/// An export carries the default clone directory whether or not paths are asked for: under the home
/// directory as `~/…`, elsewhere as the absolute path.
#[tokio::test]
async fn an_export_carries_the_default_clone_directory_without_the_paths_option() {
    let (state, _dir) = app_state("export-clone-dir");
    let selection = |paths| export::Selection {
        consoles: export::ConsoleScope::None,
        settings: SettingsSection::ALL.to_vec(),
        accounts: false,
        paths,
    };
    let clone_dir = |paths| {
        export::build(&state.store, &selection(paths))
            .unwrap()
            .settings
            .unwrap()
            .default_clone_dir
    };

    state
        .store
        .set_default_clone_dir(Some("/srv/clones"))
        .unwrap();
    assert_eq!(clone_dir(false).as_deref(), Some("/srv/clones"));
    assert_eq!(clone_dir(true).as_deref(), Some("/srv/clones"));

    let under_home = crate::paths::home_dir().join("Code/clones");
    state
        .store
        .set_default_clone_dir(Some(&under_home.to_string_lossy()))
        .unwrap();
    assert_eq!(clone_dir(false).as_deref(), Some("~/Code/clones"));
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
        ApplyOptions {
            settings: Vec::new(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
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
    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
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
    let imported = apply(
        &state,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        imported.settings_applied.unwrap().keyboard_shortcuts,
        Some(shortcuts)
    );

    let without = with_settings(document(vec![console("Hub", Vec::new())]), true, false);
    let imported = apply(
        &state,
        &without,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&without),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
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
        ApplyOptions {
            settings: SettingsSection::ALL.to_vec(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .expect_err("refused");

    assert_eq!(code_of(&err), error_code::IMPORT_SETTINGS_MISSING);
    assert!(state.store.list_consoles().unwrap().is_empty());
}

#[tokio::test]
async fn idle_terminal_cleanup_round_trips_through_configuration() {
    let (source, _source_dir) = app_state("import-idle-cleanup-source");
    source
        .store
        .set_auto_close_idle_shell_terminals(true)
        .unwrap();
    let document = export::build(&source.store, &export::Selection::full(false)).unwrap();
    assert_eq!(
        document
            .settings
            .as_ref()
            .unwrap()
            .auto_close_idle_shell_terminals,
        Some(true)
    );
    let document = serde_json::to_value(document).unwrap();
    let (target, _target_dir) = app_state("import-idle-cleanup-target");
    let inspection = inspect(&target, &document).await.unwrap();
    let settings = inspection.settings.unwrap();
    assert!(settings.auto_close_idle_shell_terminals);
    assert!(!settings.current_auto_close_idle_shell_terminals);
    apply(
        &target,
        &document,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&document),
            accounts: None,
            consoles: false,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(target.store.auto_close_idle_shell_terminals().unwrap());
}

#[tokio::test]
async fn a_setting_changed_after_the_preview_makes_the_import_conflict_and_creates_nothing() {
    let (state, _dir) = app_state("import-settings-conflict");
    let full = with_settings(document(vec![console("Hub", Vec::new())]), true, false);
    let options = || ApplyOptions {
        settings: carried_sections(&full),
        accounts: None,
        agents: None,
        consoles: true,
        clone_dir: true,
    };
    // The client holds the settings from its snapshot, which is what later changes are told from.
    let mut held = state.store.get_settings().unwrap();
    state
        .field_versions
        .stamp_settings(&mut held, crate::state::Stamp::Snapshot);
    let preview = inspect(&state, &full).await.unwrap().settings_versions;

    // Another client changes a setting the import would write.
    crate::coordinator::settings::update_settings(
        &state,
        crate::coordinator::settings::SettingsChanges {
            ask_questions_as_choices: Some(false),
            ..Default::default()
        },
    )
    .unwrap();

    let err = apply_reporting(&state, &full, Vec::new(), options(), None, Some(preview))
        .await
        .unwrap_err();
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.code, error_code::WRITE_CONFLICT);
    assert_eq!(coded.params["fields"], "ask_questions_as_choices");
    assert!(state.store.list_consoles().unwrap().is_empty());
    assert!(!state.store.auto_pull_repositories().unwrap());

    // Previewing again reads the current versions, and the import then goes through.
    let preview = inspect(&state, &full).await.unwrap().settings_versions;
    let imported = apply_reporting(&state, &full, Vec::new(), options(), None, Some(preview))
        .await
        .unwrap();
    assert!(imported.settings_applied.is_some());
    assert_eq!(imported.consoles.len(), 1);
    assert!(state.store.auto_pull_repositories().unwrap());
}

fn options(settings: Vec<SettingsSection>) -> ApplyOptions {
    ApplyOptions {
        settings,
        accounts: None,
        consoles: false,
        clone_dir: true,
        agents: None,
    }
}

/// A settings region carrying every section: the daemon's own keys plus the client-held ones of
/// General and Keyboard shortcuts.
fn sectioned_document() -> serde_json::Value {
    let mut value = with_settings(document(Vec::new()), true, true);
    value["settings"]["default_clone_dir"] = json!("/srv/from-the-file");
    value["settings"]["appearance"] = json!("dark");
    value["settings"]["session_order"] =
        json!({ "sessions": "started", "active_projects": "name", "inactive_projects": "used" });
    value["settings"]["keyboard_shortcuts"] = json!({});
    value
}

#[tokio::test]
async fn only_the_listed_sections_are_applied_and_echoed() {
    use SettingsSection::{AgentBehavior, Cleanup, General, Git};
    let (state, _dir) = app_state("import-sections-applied");
    let document = sectioned_document();
    let before = (
        state.store.auto_pull_repositories().unwrap(),
        state.store.auto_delete_unresumable_archives().unwrap(),
    );
    assert!(!before.0);

    let imported = apply(
        &state,
        &document,
        Vec::new(),
        options(vec![Git, Git, General]),
    )
    .await
    .unwrap();

    assert!(state.store.auto_pull_repositories().unwrap());
    assert_eq!(
        state.store.auto_delete_unresumable_archives().unwrap(),
        before.1
    );
    assert_eq!(
        state.store.default_clone_dir().unwrap(),
        "/srv/from-the-file"
    );
    let echoed = imported.settings_applied.unwrap();
    assert_eq!(echoed.auto_pull_repositories, Some(true));
    assert_eq!(
        echoed.default_clone_dir.as_deref(),
        Some("/srv/from-the-file")
    );
    // General's client-held keys come back as they were; the other sections' keys do not.
    assert!(echoed.session_order.is_some());
    assert_eq!(echoed.appearance, None);
    assert_eq!(echoed.auto_delete_unresumable_archives, None);
    assert_eq!(echoed.ask_questions_as_choices, None);
    assert_eq!(echoed.keyboard_shortcuts, None);

    // Without `git` the directory stays this machine's, whatever else is applied.
    let (other, _other_dir) = app_state("import-sections-no-git");
    let own = other.store.default_clone_dir().unwrap();
    let imported = apply(
        &other,
        &document,
        Vec::new(),
        options(vec![Cleanup, AgentBehavior]),
    )
    .await
    .unwrap();
    assert_eq!(other.store.default_clone_dir().unwrap(), own);
    assert_eq!(imported.clone_dir_applied, None);
    assert!(!other.store.auto_pull_repositories().unwrap());
    let echoed = imported.settings_applied.unwrap();
    assert_eq!(echoed.default_clone_dir, None);
    assert_eq!(echoed.auto_pull_repositories, None);
    assert_eq!(echoed.appearance, None);
    assert_eq!(echoed.auto_delete_unresumable_archives, Some(true));
}

#[tokio::test]
async fn listing_a_section_the_document_does_not_carry_is_refused_before_anything_is_done() {
    let (state, _dir) = app_state("import-sections-missing");
    let mut document = with_settings(document(vec![console("Hub", Vec::new())]), true, true);
    document["settings"]
        .as_object_mut()
        .unwrap()
        .remove("auto_close_idle_shell_terminals");

    let err = apply(
        &state,
        &document,
        Vec::new(),
        ApplyOptions {
            consoles: true,
            ..options(vec![SettingsSection::Git, SettingsSection::General])
        },
    )
    .await
    .expect_err("refused");

    assert_eq!(code_of(&err), error_code::IMPORT_SETTINGS_MISSING);
    assert_eq!(param_of(&err, "section"), "general");
    assert!(state.store.list_consoles().unwrap().is_empty());
    assert!(!state.store.auto_pull_repositories().unwrap());
}

#[tokio::test]
async fn the_inspection_lists_the_sections_the_document_carries() {
    let (state, _dir) = app_state("import-sections-inspected");
    let carried = |document: serde_json::Value| {
        let state = state.clone();
        async move { inspect(&state, &document).await.unwrap().settings_sections }
    };
    use SettingsSection::*;
    assert_eq!(
        carried(sectioned_document()).await,
        [
            General,
            Appearance,
            Git,
            AgentBehavior,
            KeyboardShortcuts,
            Cleanup
        ]
    );
    assert_eq!(carried(document(Vec::new())).await, []);
    let mut partial = document(Vec::new());
    partial["settings"] = json!({ "keep_awake": true, "session_order": { "sessions": "status" } });
    assert_eq!(carried(partial).await, [General]);
}

#[tokio::test]
async fn a_version_1_and_a_version_2_document_carry_every_section_they_had() {
    let (state, _dir) = app_state("import-sections-upgraded");
    use SettingsSection::*;
    for version in [1, 2] {
        let mut older = with_settings(document(Vec::new()), true, true);
        older["version"] = json!(version);
        let sections = inspect(&state, &older).await.unwrap().settings_sections;
        assert_eq!(
            sections,
            [General, Git, AgentBehavior, Cleanup],
            "version {version}"
        );
    }
}

/// How a daemon updates itself is a matter of the machine it runs on: an export never carries the
/// update check or its channel, and an import of one leaves them as this machine has them.
#[tokio::test]
async fn the_daemons_update_settings_stay_out_of_the_configuration() {
    let (source, _source_dir) = app_state("import-daemon-update-source");
    source.store.set_daemon_update_check(false).unwrap();
    source
        .store
        .set_daemon_update_channel(crate::protocol::DaemonUpdateChannel::Beta)
        .unwrap();
    let full = export_everything(&source);
    assert!(
        !full.to_string().contains("daemon_update"),
        "the export names an update setting: {full}"
    );
    assert!(!full.to_string().contains("\"beta\""));

    let (target, _target_dir) = app_state("import-daemon-update-target");
    apply(
        &target,
        &full,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&full),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(target.store.daemon_update_check().unwrap());
    assert_eq!(
        target.store.daemon_update_channel().unwrap(),
        crate::protocol::DaemonUpdateChannel::Stable
    );
}

/// A document whose settings are flat keys, the theme and the Markdown style beside the language
/// and the keys of the rest of General: what a section carries is told from the keys, not named.
fn document_with_flat_general_and_appearance_keys() -> serde_json::Value {
    let mut value = document(Vec::new());
    value["settings"] = json!({
        "appearance": "dark",
        "interface_language": "de",
        "markdown_style": "github",
        "session_order": { "sessions": "started", "active_projects": "name", "inactive_projects": "used" },
        "resume_sessions_on_start": false,
        "run_in_background": true,
        "show_menu_bar_icon": true,
        "keep_awake": true,
        "keep_awake_when": "always",
        "auto_close_idle_shell_terminals": true,
    });
    value
}

#[tokio::test]
async fn a_document_with_flat_general_and_appearance_keys_reports_its_sections_from_its_keys() {
    let (state, _dir) = app_state("import-sections-old-document");
    use SettingsSection::*;
    let sections = inspect(&state, &document_with_flat_general_and_appearance_keys())
        .await
        .unwrap()
        .settings_sections;
    assert_eq!(sections, [General, Appearance]);

    // The theme keys alone are the Appearance section; the other General keys alone, General.
    let mut only_theme = document(Vec::new());
    only_theme["settings"] = json!({ "appearance": "light" });
    assert_eq!(
        inspect(&state, &only_theme)
            .await
            .unwrap()
            .settings_sections,
        [Appearance]
    );
    let mut only_markdown = document(Vec::new());
    only_markdown["settings"] = json!({ "markdown_style": "github" });
    assert_eq!(
        inspect(&state, &only_markdown)
            .await
            .unwrap()
            .settings_sections,
        [Appearance]
    );
    let mut only_behavior = document(Vec::new());
    only_behavior["settings"] = json!({ "run_in_background": false });
    assert_eq!(
        inspect(&state, &only_behavior)
            .await
            .unwrap()
            .settings_sections,
        [General]
    );
}

#[tokio::test]
async fn a_document_with_flat_general_and_appearance_keys_imports_with_the_daemons_keys_applied() {
    let (state, _dir) = app_state("import-old-document-applied");
    let imported = apply(
        &state,
        &document_with_flat_general_and_appearance_keys(),
        Vec::new(),
        options(SettingsSection::ALL[..2].to_vec()),
    )
    .await
    .unwrap();

    assert!(state.store.keep_awake().unwrap());
    assert!(state.store.auto_close_idle_shell_terminals().unwrap());
    assert!(!state.store.resume_sessions_on_start().unwrap());
    let echoed = imported.settings_applied.unwrap();
    assert_eq!(echoed.appearance.as_deref(), Some("dark"));
    assert_eq!(echoed.markdown_style.as_deref(), Some("github"));
    assert_eq!(echoed.run_in_background, Some(true));
}

#[tokio::test]
async fn applying_only_appearance_carries_the_theme_keys_and_leaves_the_daemons_settings_alone() {
    let (state, _dir) = app_state("import-sections-appearance-only");
    let document = document_with_flat_general_and_appearance_keys();
    let before = (
        state.store.keep_awake().unwrap(),
        state.store.resume_sessions_on_start().unwrap(),
        state.store.auto_close_idle_shell_terminals().unwrap(),
    );

    let imported = apply(
        &state,
        &document,
        Vec::new(),
        options(vec![SettingsSection::Appearance]),
    )
    .await
    .unwrap();

    assert_eq!(
        before,
        (
            state.store.keep_awake().unwrap(),
            state.store.resume_sessions_on_start().unwrap(),
            state.store.auto_close_idle_shell_terminals().unwrap(),
        )
    );
    let echoed = imported.settings_applied.unwrap();
    assert_eq!(echoed.appearance.as_deref(), Some("dark"));
    assert_eq!(echoed.markdown_style.as_deref(), Some("github"));
    assert_eq!(echoed.session_order, None);
    assert_eq!(echoed.interface_language, None);
    assert_eq!(echoed.run_in_background, None);
    assert_eq!(echoed.show_menu_bar_icon, None);
    assert_eq!(echoed.keep_awake, None);
    assert_eq!(echoed.keep_awake_when, None);
    assert_eq!(echoed.resume_sessions_on_start, None);
    assert_eq!(echoed.auto_close_idle_shell_terminals, None);
}

#[tokio::test]
async fn the_terminal_font_size_is_echoed_with_the_appearance_section_alone() {
    let mut document = document_with_flat_general_and_appearance_keys();
    document["settings"]["terminal_font_size"] = json!(18);

    let (state, _dir) = app_state("import-sections-font-size-appearance");
    let imported = apply(
        &state,
        &document,
        Vec::new(),
        options(vec![SettingsSection::Appearance]),
    )
    .await
    .unwrap();
    assert_eq!(
        imported.settings_applied.unwrap().terminal_font_size,
        Some(18)
    );

    let (state, _dir) = app_state("import-sections-font-size-general");
    let imported = apply(
        &state,
        &document,
        Vec::new(),
        options(vec![SettingsSection::General]),
    )
    .await
    .unwrap();
    assert_eq!(imported.settings_applied.unwrap().terminal_font_size, None);
}

#[tokio::test]
async fn applying_only_general_carries_its_keys_and_not_the_theme_keys() {
    let (state, _dir) = app_state("import-sections-general-only");
    let imported = apply(
        &state,
        &document_with_flat_general_and_appearance_keys(),
        Vec::new(),
        options(vec![SettingsSection::General]),
    )
    .await
    .unwrap();

    assert!(state.store.keep_awake().unwrap());
    assert!(state.store.auto_close_idle_shell_terminals().unwrap());
    assert!(!state.store.resume_sessions_on_start().unwrap());
    let echoed = imported.settings_applied.unwrap();
    assert_eq!(echoed.appearance, None);
    assert_eq!(echoed.markdown_style, None);
    assert!(echoed.session_order.is_some());
    assert_eq!(echoed.run_in_background, Some(true));
    assert_eq!(echoed.show_menu_bar_icon, Some(true));
    assert_eq!(echoed.keep_awake, Some(true));
    assert_eq!(echoed.keep_awake_when.as_deref(), Some("always"));
    assert_eq!(echoed.resume_sessions_on_start, Some(false));
    assert_eq!(echoed.auto_close_idle_shell_terminals, Some(true));
}

#[tokio::test]
async fn listing_appearance_for_a_document_without_its_keys_is_refused() {
    let (state, _dir) = app_state("import-sections-appearance-missing");
    let mut document = document(Vec::new());
    document["settings"] = json!({ "keep_awake": true });

    let err = apply(
        &state,
        &document,
        Vec::new(),
        options(vec![SettingsSection::Appearance]),
    )
    .await
    .expect_err("refused");

    assert_eq!(code_of(&err), error_code::IMPORT_SETTINGS_MISSING);
    assert_eq!(param_of(&err, "section"), "appearance");
    assert!(!state.store.keep_awake().unwrap());
}
