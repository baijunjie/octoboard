use super::*;
use crate::coordinator::accounts::create_account;
use crate::coordinator::handle;
use crate::coordinator::test_support::only_account;
use crate::protocol::{Agent, ApprovalReviewer, ClaudeApprovalMode, Event, RequestBody, Role};
use crate::test_support::bare_session;

#[test]
fn update_settings_applies_all_or_nothing_and_a_blank_clone_dir_clears_it() {
    let (state, _dir) = crate::test_support::app_state("coordinator-update-settings");
    let built_in = state.store.default_clone_dir().unwrap();

    update_settings(
        &state,
        SettingsChanges {
            default_clone_dir: Some("/work/repos/".to_string()),
            ..SettingsChanges::default()
        },
    )
    .unwrap();
    assert_eq!(state.store.default_clone_dir().unwrap(), "/work/repos");

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
    assert_eq!(state.store.default_clone_dir().unwrap(), "/work/repos");

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
    let turn_on = |sidebar_order_by_status: Option<bool>| {
        update_settings(
            &state,
            SettingsChanges {
                auto_start_quota_windows: Some(true),
                sidebar_order_by_status,
                ..SettingsChanges::default()
            },
        )
        .unwrap()
    };
    state.account_quotas.hold_for_test(&unstarted(Agent::Codex));
    turn_on(None);
    let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
    while !state.account_quotas.is_kicked_for_test(Agent::Codex, None) {
        assert!(
            std::time::Instant::now() < deadline,
            "the held reading was not kicked"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Already on, though the same request does change another setting, so the quota setting
    // itself is what has to be told apart from a request that changed something: a reading
    // held since is left alone.
    state.account_quotas.hold_for_test(&unstarted(Agent::Grok));
    let ordered = state.store.sidebar_order_by_status().unwrap();
    turn_on(Some(!ordered));
    assert_eq!(state.store.sidebar_order_by_status().unwrap(), !ordered);
    // The test's runtime has the one thread, so yielding runs whatever `turn_on` spawned up to
    // its first wait, which is where a kick of the reading would already be claimed.
    tokio::task::yield_now().await;
    assert!(!state.account_quotas.is_kicked_for_test(Agent::Grok, None));
}

#[tokio::test]
async fn the_codex_approval_reviewer_applies_to_every_codex_account_and_only_to_codex() {
    let (state, _dir) = crate::test_support::app_state("codex-approval-reviewer");
    create_account(
        &state,
        Agent::Codex,
        "Work".into(),
        "/tmp/codex-work".into(),
    )
    .unwrap();
    let account = only_account(&state);
    let mut session = bare_session("session-1", Role::Project);
    session.agent = Agent::Codex;
    assert_eq!(CODEX_REVIEWER.for_session(&state, &session).unwrap(), None);

    handle(
        &state,
        None,
        RequestBody::SetCodexApprovalReviewer {
            reviewer: Some(ApprovalReviewer::AutoReview),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        state.store.get_settings().unwrap().codex_approval_reviewer,
        Some(ApprovalReviewer::AutoReview)
    );
    // The default account, a pinned one and a legacy pinned directory with no account id all
    // resolve the same value.
    assert_eq!(
        CODEX_REVIEWER.for_session(&state, &session).unwrap(),
        Some(ApprovalReviewer::AutoReview)
    );
    session.config_dir = Some("/tmp/codex-legacy".into());
    assert_eq!(
        CODEX_REVIEWER.for_session(&state, &session).unwrap(),
        Some(ApprovalReviewer::AutoReview)
    );
    session.account_id = Some(account.id.clone());
    session.config_dir = Some(account.config_dir.clone());
    assert_eq!(
        CODEX_REVIEWER.for_session(&state, &session).unwrap(),
        Some(ApprovalReviewer::AutoReview)
    );
    // Setting the same value again broadcasts nothing.
    let mut events = state.subscribe();
    handle(
        &state,
        None,
        RequestBody::SetCodexApprovalReviewer {
            reviewer: Some(ApprovalReviewer::AutoReview),
        },
    )
    .await
    .unwrap();
    assert!(events.try_recv().is_err());
    // A session of another agent reads nothing.
    session.agent = Agent::Claude;
    assert_eq!(CODEX_REVIEWER.for_session(&state, &session).unwrap(), None);

    handle(
        &state,
        None,
        RequestBody::SetCodexApprovalReviewer { reviewer: None },
    )
    .await
    .unwrap();
    session.agent = Agent::Codex;
    assert_eq!(CODEX_REVIEWER.for_session(&state, &session).unwrap(), None);
}

#[tokio::test]
async fn the_claude_approval_mode_applies_to_every_claude_account_and_only_to_claude() {
    let (state, _dir) = crate::test_support::app_state("claude-approval-mode");
    create_account(
        &state,
        Agent::Claude,
        "Work".into(),
        "/tmp/claude-work".into(),
    )
    .unwrap();
    let account = only_account(&state);
    let mut session = bare_session("session-1", Role::Project);
    session.agent = Agent::Claude;
    assert_eq!(CLAUDE_MODE.for_session(&state, &session).unwrap(), None);

    handle(
        &state,
        None,
        RequestBody::SetClaudeApprovalMode {
            mode: Some(ClaudeApprovalMode::Manual),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        state.store.get_settings().unwrap().claude_approval_mode,
        Some(ClaudeApprovalMode::Manual)
    );
    assert_eq!(
        CLAUDE_MODE.for_session(&state, &session).unwrap(),
        Some(ClaudeApprovalMode::Manual)
    );
    session.config_dir = Some("/tmp/claude-legacy".into());
    assert_eq!(
        CLAUDE_MODE.for_session(&state, &session).unwrap(),
        Some(ClaudeApprovalMode::Manual)
    );
    session.account_id = Some(account.id.clone());
    session.config_dir = Some(account.config_dir.clone());
    assert_eq!(
        CLAUDE_MODE.for_session(&state, &session).unwrap(),
        Some(ClaudeApprovalMode::Manual)
    );
    session.agent = Agent::Codex;
    assert_eq!(CLAUDE_MODE.for_session(&state, &session).unwrap(), None);

    handle(
        &state,
        None,
        RequestBody::SetClaudeApprovalMode { mode: None },
    )
    .await
    .unwrap();
    session.agent = Agent::Claude;
    assert_eq!(CLAUDE_MODE.for_session(&state, &session).unwrap(), None);
}

/// Claude compatibility is set per agent, is refused for Claude Code and unknown names without
/// applying any of the request, and broadcasts only a change. Codex starts off here, since
/// Claude Code is not among the added agents.
#[tokio::test]
async fn claude_compatibility_is_set_per_agent_and_refused_for_claude_code() {
    let (state, _dir) = crate::test_support::app_state("coordinator-claude-compat");
    let set = |changes: &[(&str, bool)]| {
        update_settings(
            &state,
            SettingsChanges {
                claude_compatibility: changes
                    .iter()
                    .map(|(name, enabled)| (name.to_string(), *enabled))
                    .collect(),
                ..SettingsChanges::default()
            },
        )
    };
    assert!(!state.store.claude_compatibility_of(Agent::Codex).unwrap());

    let mut events = state.subscribe();
    set(&[("codex", true)]).unwrap();
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { settings })
            if settings.claude_compatibility
                == [("codex".to_string(), true), ("grok".to_string(), true)].into()
    ));
    assert!(state.store.claude_compatibility_of(Agent::Codex).unwrap());
    assert!(state.store.claude_compatibility_of(Agent::Grok).unwrap());
    set(&[("codex", true)]).unwrap();
    assert!(events.try_recv().is_err(), "no change, no broadcast");

    assert!(set(&[("grok", false), ("claude", true)]).is_err());
    assert!(set(&[("nobody", true)]).is_err());
    assert!(state.store.claude_compatibility_of(Agent::Grok).unwrap());
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
        .set_added_agents_if_undecided(&[Agent::Claude, Agent::Codex])
        .unwrap();
    state.store.remove_added_agent(Agent::Codex).unwrap();
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
