use super::*;
use crate::coordinator::accounts::create_account;
use crate::coordinator::handle;
use crate::protocol::{AgentId, Event, RequestBody};
use crate::providers::SettingValue;

#[test]
fn update_settings_applies_all_or_nothing_and_a_blank_clone_dir_clears_it() {
    let (state, _dir) = crate::test_support::app_state("coordinator-update-settings");
    let built_in = state.store.default_clone_dir().unwrap();

    update_settings(
        &state,
        SettingsChanges {
            default_clone_dir: Some(crate::test_support::host_path("/work/repos/")),
            ..SettingsChanges::default()
        },
    )
    .unwrap();
    assert_eq!(
        state.store.default_clone_dir().unwrap(),
        crate::test_support::host_path("/work/repos")
    );

    // A refused directory leaves the other field of the same request unapplied too.
    assert!(update_settings(
        &state,
        SettingsChanges {
            auto_pull_repositories: Some(true),
            default_clone_dir: Some("relative".to_string()),
            ..SettingsChanges::default()
        },
    )
    .is_err());
    assert!(!state.store.get_settings().unwrap().auto_pull_repositories);
    assert_eq!(
        state.store.default_clone_dir().unwrap(),
        crate::test_support::host_path("/work/repos")
    );

    update_settings(
        &state,
        SettingsChanges {
            default_clone_dir: Some("  ".to_string()),
            ..SettingsChanges::default()
        },
    )
    .unwrap();
    assert_eq!(state.store.default_clone_dir().unwrap(), built_in);

    // The interface language is one of the tags the interface offers; any other is refused
    // before anything of the same request is written.
    assert!(update_settings(
        &state,
        SettingsChanges {
            auto_pull_repositories: Some(true),
            interface_language: Some("klingon".to_string()),
            ..SettingsChanges::default()
        },
    )
    .is_err());
    assert!(!state.store.get_settings().unwrap().auto_pull_repositories);
    assert_eq!(state.store.interface_language().unwrap(), None);
    let mut events = state.subscribe();
    update_settings(
        &state,
        SettingsChanges {
            interface_language: Some("zh-Hans".to_string()),
            ..SettingsChanges::default()
        },
    )
    .unwrap();
    assert_eq!(
        state.store.interface_language().unwrap().as_deref(),
        Some("zh-Hans")
    );
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { settings })
            if settings.interface_language.as_deref() == Some("zh-Hans")
    ));
    update_settings(
        &state,
        SettingsChanges {
            interface_language: Some("zh-Hans".to_string()),
            ..SettingsChanges::default()
        },
    )
    .unwrap();
    assert!(events.try_recv().is_err());

    let mut events = state.subscribe();
    update_settings(
        &state,
        SettingsChanges {
            auto_delete_unresumable_archives: Some(false),
            ..SettingsChanges::default()
        },
    )
    .unwrap();
    assert!(!state.store.auto_delete_unresumable_archives().unwrap());
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { settings }) if !settings.auto_delete_unresumable_archives
    ));
    update_settings(
        &state,
        SettingsChanges {
            auto_delete_unresumable_archives: Some(false),
            ..SettingsChanges::default()
        },
    )
    .unwrap();
    assert!(events.try_recv().is_err(), "a repeat changes nothing");
}

/// Turning the setting that starts quota windows on looks over the readings already held;
/// saying it is on again changes nothing, so it looks at nothing.
#[tokio::test]
async fn turning_the_quota_window_setting_on_kicks_held_readings_and_repeating_it_does_not() {
    use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};
    let (state, _dir) = crate::test_support::app_state("coordinator-quota-windows");
    let unstarted = |agent| AccountQuota {
        reset_credits: None,
        agent,
        account: None,
        fetched_at: 1,
        plan: None,
        email: None,
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::Session,
            label: None,
            used_percent: 0.0,
            resets_at: None,
            window_minutes: None,
        }],
        unavailable: None,
    };
    let turn_on = |ask_questions_as_choices: Option<bool>| {
        update_settings(
            &state,
            SettingsChanges {
                auto_start_quota_windows: Some(true),
                ask_questions_as_choices,
                ..SettingsChanges::default()
            },
        )
        .unwrap()
    };
    state
        .account_quotas
        .hold_for_test(&unstarted(AgentId::CODEX));
    turn_on(None);
    let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
    while !state
        .account_quotas
        .is_kicked_for_test(AgentId::CODEX, None)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the held reading was not kicked"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Already on, though the same request does change another setting, so the quota setting
    // itself is what has to be told apart from a request that changed something: a reading
    // held since is left alone.
    state
        .account_quotas
        .hold_for_test(&unstarted(AgentId::GROK));
    let asking = state.store.ask_questions_as_choices().unwrap();
    turn_on(Some(!asking));
    assert_eq!(state.store.ask_questions_as_choices().unwrap(), !asking);
    // The test's runtime has the one thread, so yielding runs whatever `turn_on` spawned up to
    // its first wait, which is where a kick of the reading would already be claimed.
    tokio::task::yield_now().await;
    assert!(!state.account_quotas.is_kicked_for_test(AgentId::GROK, None));
}

/// The value of the setting `id` of `agent`'s provider, as the session's launch reads it.
fn launch_choice(state: &Arc<AppState>, agent: AgentId, id: &str) -> Option<String> {
    state
        .store
        .provider_settings_of(agent)
        .unwrap()
        .choice(id)
        .map(str::to_string)
}

async fn set(
    state: &Arc<AppState>,
    agent: AgentId,
    setting: &str,
    value: Option<SettingValue>,
) -> Result<()> {
    handle(
        state,
        None,
        RequestBody::SetProviderSetting {
            agent,
            setting: setting.to_string(),
            value,
        },
    )
    .await
    .map(|_| ())
}

fn choice(text: &str) -> Option<SettingValue> {
    Some(SettingValue::Choice(text.to_string()))
}

#[tokio::test]
async fn the_codex_approval_reviewer_applies_to_every_codex_account_and_only_to_codex() {
    let (state, _dir) = crate::test_support::app_state("codex-approval-reviewer");
    create_account(
        &state,
        AgentId::CODEX,
        "Work".into(),
        crate::test_support::host_path("/tmp/codex-work"),
    )
    .unwrap();
    assert_eq!(
        launch_choice(&state, AgentId::CODEX, "approval_reviewer"),
        None
    );

    set(
        &state,
        AgentId::CODEX,
        "approval_reviewer",
        choice("auto_review"),
    )
    .await
    .unwrap();
    assert_eq!(
        state.store.get_settings().unwrap().provider_settings["codex"]["approval_reviewer"],
        choice("auto_review")
    );
    // The setting belongs to the provider, so every account of it resolves the same value, and
    // another provider reads nothing of it.
    assert_eq!(
        launch_choice(&state, AgentId::CODEX, "approval_reviewer").as_deref(),
        Some("auto_review")
    );
    assert_eq!(
        launch_choice(&state, AgentId::CLAUDE, "approval_reviewer"),
        None
    );
    // Setting the same value again broadcasts nothing.
    let mut events = state.subscribe();
    set(
        &state,
        AgentId::CODEX,
        "approval_reviewer",
        choice("auto_review"),
    )
    .await
    .unwrap();
    assert!(events.try_recv().is_err());

    set(&state, AgentId::CODEX, "approval_reviewer", None)
        .await
        .unwrap();
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { .. })
    ));
    assert_eq!(
        launch_choice(&state, AgentId::CODEX, "approval_reviewer"),
        None
    );
}

#[tokio::test]
async fn the_claude_approval_mode_applies_to_every_claude_account_and_only_to_claude() {
    let (state, _dir) = crate::test_support::app_state("claude-approval-mode");
    create_account(
        &state,
        AgentId::CLAUDE,
        "Work".into(),
        crate::test_support::host_path("/tmp/claude-work"),
    )
    .unwrap();
    assert_eq!(
        launch_choice(&state, AgentId::CLAUDE, "approval_mode"),
        None
    );

    set(&state, AgentId::CLAUDE, "approval_mode", choice("manual"))
        .await
        .unwrap();
    assert_eq!(
        state.store.get_settings().unwrap().provider_settings["claude"]["approval_mode"],
        choice("manual")
    );
    assert_eq!(
        launch_choice(&state, AgentId::CLAUDE, "approval_mode").as_deref(),
        Some("manual")
    );
    assert_eq!(launch_choice(&state, AgentId::CODEX, "approval_mode"), None);

    set(&state, AgentId::CLAUDE, "approval_mode", None)
        .await
        .unwrap();
    assert_eq!(
        launch_choice(&state, AgentId::CLAUDE, "approval_mode"),
        None
    );
}

/// A request for a setting the provider does not declare, a value of the other kind or a choice it
/// does not offer is refused as unreadable, and changes nothing.
#[tokio::test]
async fn a_provider_setting_request_is_refused_unless_the_provider_declares_it_and_takes_the_value()
{
    let (state, _dir) = crate::test_support::app_state("provider-setting-refused");
    let mut events = state.subscribe();
    for (agent, setting, value) in [
        (AgentId::CLAUDE, "nothing", choice("auto")),
        (
            AgentId::CLAUDE,
            "claude_compatibility",
            Some(SettingValue::Switch(true)),
        ),
        (AgentId::CLAUDE, "approval_mode", choice("everything")),
        (
            AgentId::CLAUDE,
            "approval_mode",
            Some(SettingValue::Switch(true)),
        ),
        (AgentId::CODEX, "claude_compatibility", choice("on")),
    ] {
        let error = set(&state, agent, setting, value.clone())
            .await
            .unwrap_err();
        assert_eq!(
            crate::protocol::error_parts(&error).0,
            crate::protocol::error_code::UNREADABLE_REQUEST,
            "{agent:?} {setting} {value:?}"
        );
    }
    assert!(state.store.provider_setting_choices().unwrap().is_empty());
    assert!(events.try_recv().is_err());
}

/// A switch is set per provider, null puts it back on its default rule, and only a change is
/// broadcast. Codex starts off here, since Claude Code is not among the added agents.
#[tokio::test]
async fn a_switch_is_set_per_provider_and_cleared_to_its_default_rule() {
    let (state, _dir) = crate::test_support::app_state("coordinator-provider-switch");
    let on = |agent: AgentId| {
        state
            .store
            .provider_settings_of(agent)
            .unwrap()
            .switch("claude_compatibility")
    };
    let switch = |on| Some(SettingValue::Switch(on));
    assert!(!on(AgentId::CODEX));
    assert!(on(AgentId::GROK));

    let mut events = state.subscribe();
    set(&state, AgentId::CODEX, "claude_compatibility", switch(true))
        .await
        .unwrap();
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { settings })
            if settings.provider_settings["codex"]["claude_compatibility"] == switch(true)
                && settings.provider_settings["grok"]["claude_compatibility"] == switch(true)
    ));
    assert!(on(AgentId::CODEX));
    assert!(on(AgentId::GROK));
    set(&state, AgentId::CODEX, "claude_compatibility", switch(true))
        .await
        .unwrap();
    assert!(events.try_recv().is_err(), "no change, no broadcast");

    set(&state, AgentId::GROK, "claude_compatibility", switch(false))
        .await
        .unwrap();
    assert!(!on(AgentId::GROK));
    set(&state, AgentId::GROK, "claude_compatibility", None)
        .await
        .unwrap();
    assert!(on(AgentId::GROK));
}

/// Choices applied in bulk (a configuration import) are refused, applying none of the request,
/// for an unknown provider, an undeclared setting or a value the setting does not take.
#[tokio::test]
async fn bulk_provider_settings_are_refused_whole() {
    let (state, _dir) = crate::test_support::app_state("coordinator-bulk-provider-settings");
    let apply = |settings: &[(&str, &str, SettingValue)]| {
        let mut provider_settings: BTreeMap<String, BTreeMap<String, SettingValue>> =
            BTreeMap::new();
        for (provider, setting, value) in settings {
            provider_settings
                .entry(provider.to_string())
                .or_default()
                .insert(setting.to_string(), value.clone());
        }
        update_settings(
            &state,
            SettingsChanges {
                provider_settings,
                ..SettingsChanges::default()
            },
        )
    };
    let off = SettingValue::Switch(false);
    apply(&[("codex", "claude_compatibility", off.clone())]).unwrap();
    assert!(apply(&[
        ("grok", "claude_compatibility", off.clone()),
        ("claude", "claude_compatibility", off.clone()),
    ])
    .is_err());
    assert!(apply(&[("nobody", "claude_compatibility", off.clone())]).is_err());
    assert!(apply(&[(
        "grok",
        "claude_compatibility",
        SettingValue::Choice("on".into())
    )])
    .is_err());
    assert_eq!(
        state.store.provider_setting_choices().unwrap(),
        [(
            "codex".to_string(),
            [("claude_compatibility".to_string(), off)].into()
        )]
        .into()
    );
}

/// Agent tags are normalised as a project's are, replace the agent's list, broadcast only a
/// change, are refused for an unknown agent without applying any of the request, and are kept
/// when the agent is removed from the added agents.
#[tokio::test]
async fn agent_tags_are_normalised_replaced_and_kept_when_the_agent_is_removed() {
    let (state, _dir) = crate::test_support::app_state("coordinator-agent-tags");
    let set = |changes: &[(&str, &[&str])]| {
        update_settings(
            &state,
            SettingsChanges {
                agent_tags: changes
                    .iter()
                    .map(|(name, tags)| {
                        (
                            name.to_string(),
                            tags.iter().map(|tag| tag.to_string()).collect(),
                        )
                    })
                    .collect(),
                ..SettingsChanges::default()
            },
        )
    };
    let tags_of = |agent: &str| state.store.agent_tags().unwrap().remove(agent);

    let mut events = state.subscribe();
    set(&[("codex", &["  Review ", "", "review", "UI", "ui", "tests"])]).unwrap();
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { settings })
            if settings.agent_tags
                == [(
                    "codex".to_string(),
                    vec!["Review".to_string(), "UI".to_string(), "tests".to_string()],
                )]
                .into()
    ));
    // The same tags after normalising are no change.
    set(&[("codex", &["Review", " review", "UI", "tests"])]).unwrap();
    assert!(events.try_recv().is_err(), "no change, no broadcast");
    assert_eq!(tags_of("codex").unwrap(), ["Review", "UI", "tests"]);

    // An agent left out is left alone; the other agents are set independently.
    set(&[("claude", &["planning"])]).unwrap();
    assert_eq!(tags_of("codex").unwrap(), ["Review", "UI", "tests"]);
    assert_eq!(tags_of("claude").unwrap(), ["planning"]);

    // A name that is no agent refuses the whole request.
    assert!(set(&[("claude", &["other"]), ("nobody", &["x"])]).is_err());
    assert_eq!(tags_of("claude").unwrap(), ["planning"]);

    // Removing the agent keeps its tags; clearing is an empty (or blank-only) list.
    state
        .store
        .set_added_agents_if_undecided(&[AgentId::CLAUDE, AgentId::CODEX])
        .unwrap();
    state.store.remove_added_agent(AgentId::CODEX).unwrap();
    assert_eq!(tags_of("codex").unwrap(), ["Review", "UI", "tests"]);
    set(&[("codex", &[" "])]).unwrap();
    assert_eq!(tags_of("codex"), None);
}

/// The control request carries agent tags and answers a refusal for an unknown agent.
#[tokio::test]
async fn the_update_settings_request_sets_agent_tags() {
    let (state, _dir) = crate::test_support::app_state("request-agent-tags");
    let body: RequestBody = serde_json::from_value(serde_json::json!({
        "type": "update_settings",
        "agent_tags": { "grok": ["Research", "research"] },
    }))
    .unwrap();
    handle(&state, None, body).await.unwrap();
    assert_eq!(state.store.agent_tags().unwrap()["grok"], ["Research"]);
    let body: RequestBody = serde_json::from_value(serde_json::json!({
        "type": "update_settings",
        "agent_tags": { "nobody": ["x"] },
    }))
    .unwrap();
    assert!(handle(&state, None, body).await.is_err());
}

/// `update_settings` changes the git workflow settings item by item, broadcasts
/// `settings_updated` with them only when one actually changed, and refuses an unknown
/// delivery at parsing.
#[tokio::test]
async fn update_settings_changes_the_git_workflow_settings_and_broadcasts_only_a_change() {
    use crate::protocol::{GitDelivery, GitWorkflowSettings};
    let (state, _dir) = crate::test_support::app_state("coordinator-git-workflow-settings");
    let request = |json: &str| {
        serde_json::from_str::<crate::protocol::Request>(json)
            .unwrap()
            .body
    };
    let mut events = state.subscribe();

    handle(
        &state,
        None,
        request(
            r#"{"type":"update_settings","git_workflow":{"delivery":"merge","revert_gate":true}}"#,
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        state.store.git_workflow_settings().unwrap(),
        GitWorkflowSettings {
            delivery: GitDelivery::Merge,
            revert_gate: true,
            ..GitWorkflowSettings::default()
        }
    );
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { settings })
            if settings.git_workflow.delivery == GitDelivery::Merge
                && settings.git_workflow.revert_gate
    ));

    for unchanged in [
        r#"{"type":"update_settings","git_workflow":{"delivery":"merge"}}"#,
        r#"{"type":"update_settings","git_workflow":{}}"#,
        r#"{"type":"update_settings"}"#,
    ] {
        handle(&state, None, request(unchanged)).await.unwrap();
        assert!(events.try_recv().is_err(), "{unchanged}");
    }

    assert!(serde_json::from_str::<crate::protocol::Request>(
        r#"{"type":"update_settings","git_workflow":{"delivery":"squash"}}"#
    )
    .is_err());
}
