use crate::mcp::dispatch::call;
use crate::mcp::test_support::{
    arguments, refused, settings_state, shared_console_state, RepoState,
};
use crate::protocol::AgentId;
use serde_json::{json, Map, Value};

/// `update_project` sets a description to the given text, leaves it alone when absent, clears
/// it with `null`, and refuses a value that is not text.
#[tokio::test]
async fn update_project_sets_leaves_and_clears_the_description() {
    let (state, _dir) = settings_state("update-project-description");
    let update = |args: Value| {
        let state = state.clone();
        async move { call(&state, "a", "update_project", &arguments(args)).await }
    };

    let set = update(json!({ "project": "project-1", "description": "Billing." }))
        .await
        .unwrap();
    assert_eq!(set["description"], "Billing.");

    let kept = update(json!({ "project": "project-1", "pinned": true }))
        .await
        .unwrap();
    assert_eq!(kept["description"], "Billing.");

    let message = refused(
        &state,
        "a",
        "update_project",
        json!({ "project": "project-1", "description": 3 }),
    )
    .await;
    assert!(message.contains("string or null"), "{message}");
    let stored = state.store.get_project("project-1").unwrap().unwrap();
    assert_eq!(stored.description.as_deref(), Some("Billing."));

    let answer = update(json!({ "project": "project-1", "description": null }))
        .await
        .unwrap();
    assert!(answer["description"].is_null());
    let stored = state.store.get_project("project-1").unwrap().unwrap();
    assert_eq!(stored.description, None);
}

/// `update_project` leaves what it is not given alone, adds and removes tags without caring
/// about case, resolves an account by name within the agent, and refuses what the shared
/// function refuses (a model the agent does not offer, another agent's account, a blank name)
/// without writing any of it; another console's project is out of reach.
#[tokio::test]
async fn update_project_changes_only_what_it_is_given() {
    let (state, _dir) = settings_state("update-project");
    let update = |args: Value| {
        let state = state.clone();
        async move { call(&state, "a", "update_project", &arguments(args)).await }
    };

    let first = update(json!({
        "project": "project-1",
        "tags": ["Backend", "api"],
        "pinned": true,
        "accounts": { "codex": "work", "grok": "default" },
        "launch": { "claude": { "model": "opus", "effort": "high" } },
    }))
    .await
    .unwrap();
    assert_eq!(first["tags"], json!(["Backend", "api"]));
    assert_eq!(first["accounts"]["codex"], "acct-codex");
    assert_eq!(first["accounts"]["grok"], "default");
    assert_eq!(first["accounts"]["claude"], "inherit");
    assert_eq!(first["launch"]["claude"]["model"], "opus");

    let second = update(json!({
        "project": "project-1",
        "add_tags": ["API", "docs"],
        "remove_tags": ["backend"],
    }))
    .await
    .unwrap();
    assert_eq!(second["tags"], json!(["api", "docs"]));
    assert_eq!(second["pinned"], true);
    assert_eq!(second["launch"]["claude"]["effort"], "high");
    assert_eq!(second["name"], "project-1");

    // `null` is no list, so it does not empty the tags; a value that is not a list, or a
    // boolean that is not one, is refused rather than ignored.
    let kept = update(json!({ "project": "project-1", "tags": null }))
        .await
        .unwrap();
    assert_eq!(kept["tags"], json!(["api", "docs"]));
    for (args, expected) in [
        (json!({ "tags": "api" }), "array of strings"),
        (json!({ "add_tags": [1] }), "array of strings"),
        (json!({ "pinned": "false" }), "true or false"),
    ] {
        let mut args = args;
        args["project"] = json!("project-1");
        let message = refused(&state, "a", "update_project", args).await;
        assert!(message.contains(expected), "{message}");
    }

    let cleared = update(json!({ "project": "project-1", "default_agent": null }))
        .await
        .unwrap();
    assert!(cleared["default_agent"].is_null());

    for (args, expected) in [
        (json!({ "tags": ["x"], "add_tags": ["y"] }), "cannot both"),
        (json!({ "name": "  " }), "`name` is required"),
        (
            json!({ "launch": { "claude": { "model": "gpt-9" } } }),
            "is not a model",
        ),
        (
            json!({ "accounts": { "claude": "acct-codex" } }),
            "unknown account",
        ),
        (
            json!({ "accounts": { "claude": "Work" } }),
            "unknown account",
        ),
    ] {
        let mut args = args;
        args["project"] = json!("project-1");
        let message = refused(&state, "a", "update_project", args).await;
        assert!(message.contains(expected), "{message}");
    }
    let stored = state.store.get_project("project-1").unwrap().unwrap();
    assert_eq!(stored.tags, ["api", "docs"]);
    assert_eq!(
        stored.launch[AgentId::CLAUDE].model.as_deref(),
        Some("opus")
    );

    let message = refused(
        &state,
        "a",
        "update_project",
        json!({ "project": "project-2" }),
    )
    .await;
    assert!(message.contains("no project"), "{message}");
}

/// Both listings carry the settings and show the remote without the token in it, and a project
/// session's `get_project` takes no project: whatever it names, it reads its own.
#[tokio::test]
async fn project_settings_are_listed_without_credentials_and_get_project_stays_in_its_own() {
    let (state, _dir) = settings_state("get-project");
    let listed = call(&state, "a", "list_projects", &Map::new())
        .await
        .unwrap();
    let entry = &listed["projects"][0];
    assert_eq!(entry["remote_url"], "https://example.com/r.git");
    assert_eq!(entry["source"], "local");
    assert_eq!(entry["trust_consent"], false);
    assert_eq!(entry["accounts"]["claude"], "inherit");
    assert!(entry["launch"]["grok"]["model"].is_null());
    assert_eq!(entry["sessions"].as_array().unwrap().len(), 3);

    for caller in ["of-a", "loose"] {
        let own = call(
            &state,
            caller,
            "get_project",
            &arguments(json!({ "project": "project-2" })),
        )
        .await
        .unwrap();
        assert_eq!(own["project"], "project-1");
        assert_eq!(own["remote_url"], "https://example.com/r.git");
        assert!(own.get("sessions").is_none());
    }
    let other = call(&state, "foreign", "get_project", &Map::new())
        .await
        .unwrap();
    assert_eq!(other["project"], "project-2");
    // A console session reads projects through `list_projects`.
    assert!(refused(&state, "a", "get_project", json!({}))
        .await
        .contains("not a tool"));
}

/// The git workflow settings are read and changed through the settings tools: the
/// application-wide ones by `get_settings` / `update_settings`, a project's own by
/// `update_project`, and `get_project` and `list_projects` report the override beside the
/// values in force.
#[tokio::test]
async fn the_git_workflow_settings_are_read_and_changed_through_the_tools() {
    let (state, _dir) = settings_state("git-workflow-settings-tools");
    let read = call(&state, "a", "get_settings", &Map::new())
        .await
        .unwrap();
    assert_eq!(
        read["git_workflow"],
        json!({
            "development": "worktree",
            "squash_before_delivery": false,
            "rebase_before_delivery": true,
            "delivery": "ask",
            "revert_gate": false,
        })
    );

    let updated = call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({ "git_workflow": { "delivery": "merge", "revert_gate": true, "squash_before_delivery": true } })),
    )
    .await
    .unwrap();
    assert_eq!(updated["git_workflow"]["delivery"], "merge");
    assert_eq!(updated["git_workflow"]["squash_before_delivery"], true);
    assert_eq!(updated["git_workflow"]["revert_gate"], true);
    assert_eq!(updated["git_workflow"]["development"], "worktree");
    for bad in [
        json!({ "git_workflow": { "delivery": "rebase" } }),
        json!({ "git_workflow": { "development": "yes" } }),
        json!({ "git_workflow": { "typo": true } }),
    ] {
        let message = refused(&state, "a", "update_settings", bad).await;
        assert!(message.contains("git_workflow"), "{message}");
    }
    assert_eq!(
        state.store.git_workflow_settings().unwrap().delivery,
        crate::protocol::GitDelivery::Merge
    );

    let project = call(
        &state,
        "a",
        "update_project",
        &arguments(json!({
            "project": "project-1",
            "git_workflow": { "development": "in_place", "delivery": "pull_request", "squash_before_delivery": false },
        })),
    )
    .await
    .unwrap();
    assert_eq!(
        project["git_workflow"]["override"],
        json!({
            "development": "in_place",
            "squash_before_delivery": false,
            "rebase_before_delivery": null,
            "delivery": "pull_request",
            "revert_gate": null,
        })
    );
    assert_eq!(
        project["git_workflow"]["effective"],
        json!({
            "development": "in_place",
            "squash_before_delivery": false,
            "rebase_before_delivery": true,
            "delivery": "pull_request",
            "revert_gate": true,
        })
    );

    // An item left out stays; `null` goes back to following the application-wide value.
    let project = call(
        &state,
        "a",
        "update_project",
        &arguments(json!({
            "project": "project-1",
            "git_workflow": { "delivery": null, "revert_gate": false, "squash_before_delivery": null },
        })),
    )
    .await
    .unwrap();
    assert_eq!(
        project["git_workflow"]["override"]["development"],
        "in_place"
    );
    assert!(project["git_workflow"]["override"]["delivery"].is_null());
    assert_eq!(project["git_workflow"]["effective"]["delivery"], "merge");
    assert_eq!(project["git_workflow"]["effective"]["revert_gate"], false);
    assert!(project["git_workflow"]["override"]["squash_before_delivery"].is_null());
    assert_eq!(
        project["git_workflow"]["effective"]["squash_before_delivery"],
        true
    );

    let message = refused(
        &state,
        "a",
        "update_project",
        json!({ "project": "project-1", "git_workflow": { "delivery": "rebase" } }),
    )
    .await;
    assert!(message.contains("git_workflow"), "{message}");

    let listed = call(&state, "a", "list_projects", &Map::new())
        .await
        .unwrap();
    assert_eq!(
        listed["projects"][0]["git_workflow"]["effective"]["development"],
        "in_place"
    );
    let own = call(&state, "of-a", "get_project", &Map::new())
        .await
        .unwrap();
    assert_eq!(own["git_workflow"]["override"]["revert_gate"], false);
    assert_eq!(own["git_workflow"]["effective"]["delivery"], "merge");
    // A project session cannot change them.
    assert!(refused(&state, "of-a", "update_settings", json!({}))
        .await
        .contains("not a tool"));
}

/// `get_project` resolves what a project leaves unset against its console, since a project
/// session cannot read the console: the project's own value wins, the console's project
/// session value fills in the rest, and the raw settings stay as stored.
#[tokio::test]
async fn get_project_gives_the_settings_resolved_against_the_console() {
    let (state, _dir) = shared_console_state("get-project-effective");
    let mut console = state.store.get_console("console-1").unwrap().unwrap();
    console.project_session_agent = Some(AgentId::CODEX);
    console.project_session_launch[AgentId::CLAUDE].model = Some("console-model".to_string());
    console.project_session_launch[AgentId::CLAUDE].effort = Some("console-effort".to_string());
    state.store.update_console(&console).unwrap();
    let mut project = state.store.get_project("project-1").unwrap().unwrap();
    project.default_agent = None;
    project.launch.get_mut(AgentId::CLAUDE).model = Some("project-model".to_string());
    state.store.update_project(&project).unwrap();

    let own = call(&state, "of-a", "get_project", &Map::new())
        .await
        .unwrap();
    assert!(own["default_agent"].is_null());
    assert_eq!(own["accounts"]["claude"], "inherit");
    assert_eq!(own["effective"]["default_agent"], "codex");
    assert_eq!(own["effective"]["accounts"]["claude"], "default");
    assert_eq!(
        own["effective"]["launch"]["claude"]["model"],
        "project-model"
    );
    assert_eq!(
        own["effective"]["launch"]["claude"]["effort"],
        "console-effort"
    );
    assert!(own["effective"]["launch"]["codex"]["model"].is_null());
}

/// The console session reads and changes its own console only, never sees the avatar, and an
/// account that does not belong to the agent is refused.
#[tokio::test]
async fn the_console_session_reads_and_changes_its_own_console() {
    let (state, _dir) = settings_state("console-settings");
    let mut with_icon = state.store.get_console("console-1").unwrap().unwrap();
    with_icon.icon = Some("data:image/png;base64,AAAA".to_string());
    state.store.update_console(&with_icon).unwrap();

    let read = call(&state, "a", "get_console", &Map::new()).await.unwrap();
    assert_eq!(read["console"], "console-1");
    assert_eq!(read["has_icon"], true);
    assert_eq!(read["accounts"]["claude"], "default");
    assert!(read["project_session_agent"].is_null());
    assert!(!read.to_string().contains("AAAA"));

    let updated = call(
        &state,
        "a",
        "update_console",
        &arguments(json!({
            "name": "Renamed",
            "default_agent": "codex",
            "accounts": { "codex": "Work" },
            "launch": { "claude": { "model": "sonnet" } },
        })),
    )
    .await
    .unwrap();
    assert_eq!(updated["name"], "Renamed");
    assert_eq!(updated["default_agent"], "codex");
    assert!(updated["project_session_agent"].is_null());
    assert_eq!(updated["accounts"]["codex"], "acct-codex");
    assert_eq!(updated["launch"]["claude"]["model"], "sonnet");

    // The project session pair is a setting of its own, read and written beside `launch`.
    assert!(updated["project_session_launch"]["claude"]["model"].is_null());
    let pair = call(
        &state,
        "a",
        "update_console",
        &arguments(json!({ "project_session_launch": { "claude": { "model": "sonnet" } } })),
    )
    .await
    .unwrap();
    assert_eq!(pair["project_session_launch"]["claude"]["model"], "sonnet");
    assert_eq!(pair["launch"]["claude"]["model"], "sonnet");
    let stored = state.store.get_console("console-1").unwrap().unwrap();
    assert_eq!(
        stored.project_session_launch[AgentId::CLAUDE]
            .model
            .as_deref(),
        Some("sonnet")
    );
    let cleared_pair = call(
        &state,
        "a",
        "update_console",
        &arguments(json!({ "project_session_launch": { "claude": { "model": null } } })),
    )
    .await
    .unwrap();
    assert!(cleared_pair["project_session_launch"]["claude"]["model"].is_null());
    assert_eq!(cleared_pair["launch"]["claude"]["model"], "sonnet");

    // Absent leaves the project session agent alone; `null` makes it follow again.
    let set = call(
        &state,
        "a",
        "update_console",
        &arguments(json!({ "project_session_agent": "grok" })),
    )
    .await
    .unwrap();
    assert_eq!(set["project_session_agent"], "grok");
    assert_eq!(set["default_agent"], "codex");
    let kept = call(
        &state,
        "a",
        "update_console",
        &arguments(json!({ "name": "Renamed" })),
    )
    .await
    .unwrap();
    assert_eq!(kept["project_session_agent"], "grok");
    let cleared = call(
        &state,
        "a",
        "update_console",
        &arguments(json!({ "project_session_agent": null })),
    )
    .await
    .unwrap();
    assert!(cleared["project_session_agent"].is_null());
    let stored = state.store.get_console("console-1").unwrap().unwrap();
    assert_eq!(stored.icon, with_icon.icon);

    // The avatar is set as an image `data:` URL and removed with `null`, never read back.
    let avatar = call(
        &state,
        "a",
        "update_console",
        &arguments(json!({ "icon": "data:image/png;base64,BBBB" })),
    )
    .await
    .unwrap();
    assert_eq!(avatar["has_icon"], true);
    assert!(!avatar.to_string().contains("BBBB"));
    assert_eq!(
        state
            .store
            .get_console("console-1")
            .unwrap()
            .unwrap()
            .icon
            .as_deref(),
        Some("data:image/png;base64,BBBB")
    );
    let removed = call(
        &state,
        "a",
        "update_console",
        &arguments(json!({ "icon": null })),
    )
    .await
    .unwrap();
    assert_eq!(removed["has_icon"], false);
    assert_eq!(
        state.store.get_console("console-2").unwrap().unwrap().name,
        "Console"
    );

    for (args, expected) in [
        (json!({ "name": " " }), "`name` is required"),
        (json!({ "project_session_agent": "gemini" }), "gemini"),
        (
            json!({ "icon": "https://example.com/a.png" }),
            "image `data:` URL",
        ),
        (json!({ "icon": 7 }), "string or null"),
        (
            json!({ "accounts": { "claude": "acct-codex" } }),
            "unknown account",
        ),
        (
            json!({ "launch": { "claude": { "model": "gpt-9" } } }),
            "is not a model",
        ),
    ] {
        let message = refused(&state, "a", "update_console", args).await;
        assert!(message.contains(expected), "{message}");
    }
}

/// The automatic approvals of session requests, resume requests, account switches and handoffs,
/// and the switch asking for questions as choices, are the user's alone: an agent can neither read
/// them nor change them through the settings tools, whatever it passes.
#[tokio::test]
async fn the_users_own_switches_are_out_of_the_agents_reach() {
    let (state, _dir) = settings_state("app-settings-auto-approve");
    let listed = call(&state, "a", "get_settings", &arguments(json!({})))
        .await
        .unwrap();
    assert!(listed.get("auto_approve_session_requests").is_none());
    assert!(listed.get("auto_approve_resume_requests").is_none());
    assert!(listed.get("auto_approve_account_switches").is_none());
    assert!(listed.get("auto_approve_handoffs").is_none());
    assert!(listed.get("ask_questions_as_choices").is_none());

    for (stored, passed) in [(false, true), (true, false)] {
        state
            .store
            .set_auto_approve_session_requests(stored)
            .unwrap();
        state
            .store
            .set_auto_approve_resume_requests(stored)
            .unwrap();
        state
            .store
            .set_auto_approve_account_switches(stored)
            .unwrap();
        state.store.set_auto_approve_handoffs(stored).unwrap();
        state.store.set_ask_questions_as_choices(stored).unwrap();
        let updated = call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({
                "auto_approve_session_requests": passed,
                "auto_approve_resume_requests": passed,
                "auto_approve_account_switches": passed,
                "auto_approve_handoffs": passed,
                "ask_questions_as_choices": passed,
                "default_clone_dir": "/tmp/clones",
            })),
        )
        .await
        .unwrap();
        assert!(updated.get("auto_approve_session_requests").is_none());
        assert!(updated.get("auto_approve_resume_requests").is_none());
        assert!(updated.get("auto_approve_account_switches").is_none());
        assert!(updated.get("auto_approve_handoffs").is_none());
        assert!(updated.get("ask_questions_as_choices").is_none());
        assert_eq!(updated["default_clone_dir"], "/tmp/clones");
        assert_eq!(state.store.auto_approve_session_requests().unwrap(), stored);
        assert_eq!(state.store.auto_approve_resume_requests().unwrap(), stored);
        assert_eq!(state.store.auto_approve_account_switches().unwrap(), stored);
        assert_eq!(state.store.auto_approve_handoffs().unwrap(), stored);
        assert_eq!(state.store.ask_questions_as_choices().unwrap(), stored);
    }
}

/// The language the daemon holds for the user is read and changed through the settings tools,
/// and a tag the interface does not offer is refused without changing it.
#[tokio::test]
async fn the_interface_language_is_read_and_changed_through_the_tools() {
    let (state, _dir) = settings_state("app-settings-interface-language");
    let read = call(&state, "a", "get_settings", &Map::new())
        .await
        .unwrap();
    assert!(read["interface_language"].is_null());

    let updated = call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({ "interface_language": " ja " })),
    )
    .await
    .unwrap();
    assert_eq!(updated["interface_language"], "ja");
    assert_eq!(
        state.store.interface_language().unwrap().as_deref(),
        Some("ja")
    );

    for (bad, expected) in [
        (json!("klingon"), "not a language"),
        (json!(7), "must be a string"),
    ] {
        let message = refused(
            &state,
            "a",
            "update_settings",
            json!({ "interface_language": bad }),
        )
        .await;
        assert!(message.contains(expected), "{message}");
    }
    assert_eq!(
        state.store.interface_language().unwrap().as_deref(),
        Some("ja")
    );
}

/// The settings of the Agent accounts section, automatic account selection, the providers'
/// settings and the agents' tags, are read through the tools but never changed by them, whatever
/// an agent passes. Of the providers' settings only those declared `shown_to_agents` are read:
/// not the approval settings, which stay the user's alone.
#[tokio::test]
async fn the_agent_account_settings_are_read_only_to_agents() {
    let (state, _dir) = settings_state("app-settings-agent-accounts");
    state
        .store
        .set_agent_tags(AgentId::CODEX, vec!["review".to_string()])
        .unwrap();
    let before = call(&state, "a", "get_settings", &Map::new())
        .await
        .unwrap();
    assert_eq!(before["auto_select_account"], false);
    assert_eq!(before["agent_tags"], json!({ "codex": ["review"] }));
    // Every provider's switches are shown with the value in effect; the approval settings and a
    // provider with nothing to show are not.
    assert_eq!(
        before["provider_settings"],
        json!({
            "codex": { "claude_compatibility": false },
            "grok": { "claude_compatibility": true },
        })
    );
    state
        .store
        .set_provider_setting(
            AgentId::CODEX,
            "approval_reviewer",
            Some(&crate::providers::SettingValue::Choice(
                "auto_review".into(),
            )),
        )
        .unwrap();
    state
        .store
        .set_provider_setting(
            AgentId::CLAUDE,
            "approval_mode",
            Some(&crate::providers::SettingValue::Choice("bypass".into())),
        )
        .unwrap();
    let with_approvals = call(&state, "a", "get_settings", &Map::new())
        .await
        .unwrap();
    assert_eq!(
        with_approvals["provider_settings"],
        before["provider_settings"]
    );
    assert!(!with_approvals.to_string().contains("approval"));

    let updated = call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({
            "auto_select_account": true,
            "provider_settings": { "codex": { "claude_compatibility": true } },
            "claude_compatibility": { "codex": true, "grok": false },
            "agent_tags": { "codex": [], "grok": ["research"] },
            "default_clone_dir": "/tmp/clones",
        })),
    )
    .await
    .unwrap();
    assert_eq!(updated["default_clone_dir"], "/tmp/clones");
    assert_eq!(
        updated["auto_select_account"],
        before["auto_select_account"]
    );
    assert_eq!(updated["provider_settings"], before["provider_settings"]);
    assert_eq!(updated["agent_tags"], before["agent_tags"]);
    assert!(!state.store.auto_select_account().unwrap());
    assert_eq!(state.store.agent_tags().unwrap()["codex"], ["review"]);

    // A removed agent's tags are no longer shown; the store keeps them.
    state
        .store
        .set_added_agents_if_undecided(&[AgentId::CLAUDE, AgentId::CODEX])
        .unwrap();
    state.store.remove_added_agent(AgentId::CODEX).unwrap();
    let read = call(&state, "a", "get_settings", &Map::new())
        .await
        .unwrap();
    assert_eq!(read["agent_tags"], json!({}));
    assert_eq!(state.store.agent_tags().unwrap()["codex"], ["review"]);
}

/// `get_settings` marks each account the user turned off, the default account of an agent
/// included, so an agent can tell which ones a new session cannot take.
#[tokio::test]
async fn get_settings_marks_the_accounts_that_are_turned_off() {
    let (state, _dir) = settings_state("app-settings-disabled-accounts");
    let disabled_of = |listed: &Value| -> Vec<(String, String, bool)> {
        listed["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|account| {
                (
                    account["agent"].as_str().unwrap().to_string(),
                    account["id"].as_str().unwrap().to_string(),
                    account["disabled"].as_bool().unwrap(),
                )
            })
            .collect()
    };
    let listed = call(&state, "a", "get_settings", &arguments(json!({})))
        .await
        .unwrap();
    assert!(disabled_of(&listed)
        .iter()
        .all(|(_, _, disabled)| !disabled));

    state
        .store
        .set_default_account_disabled(AgentId::CODEX, true)
        .unwrap();
    let first = state.store.list_accounts().unwrap().remove(0);
    state.store.set_account_disabled(&first.id, true).unwrap();
    let listed = call(&state, "a", "get_settings", &arguments(json!({})))
        .await
        .unwrap();
    let on: Vec<_> = disabled_of(&listed)
        .into_iter()
        .filter(|(_, _, disabled)| *disabled)
        .map(|(agent, id, _)| (agent, id))
        .collect();
    assert_eq!(on.len(), 2, "{on:?}");
    assert!(on.contains(&("codex".to_string(), "default".to_string())));
    assert!(on.contains(&(first.agent.wire_name().to_string(), first.id)));
}

/// Starting quota windows spends the user's quota, so an agent can neither read the setting
/// nor change it through the settings tools, whatever it passes.
#[tokio::test]
async fn the_quota_window_setting_is_out_of_the_agents_reach() {
    let (state, _dir) = settings_state("app-settings-quota-windows");
    let listed = call(&state, "a", "get_settings", &arguments(json!({})))
        .await
        .unwrap();
    assert!(listed.get("auto_start_quota_windows").is_none());

    let updated = call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({
            "auto_start_quota_windows": true,
            "default_clone_dir": "/tmp/clones",
        })),
    )
    .await
    .unwrap();
    assert!(updated.get("auto_start_quota_windows").is_none());
    assert_eq!(updated["default_clone_dir"], "/tmp/clones");
    assert!(!state.store.auto_start_quota_windows().unwrap());
}

/// The settings listing names every agent's default account, never an account's directory, and
/// a console session changes the switches that are not the user's alone.
#[tokio::test]
async fn settings_are_listed_without_directories_and_the_general_switches_are_writable() {
    let (state, _dir) = settings_state("app-settings");
    let updated = call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({ "default_clone_dir": "/tmp/clones" })),
    )
    .await
    .unwrap();
    assert_eq!(updated["default_clone_dir"], "/tmp/clones");
    assert_eq!(updated["auto_delete_unresumable_archives"], true);
    let updated = call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({ "auto_delete_unresumable_archives": false })),
    )
    .await
    .unwrap();
    assert_eq!(updated["auto_delete_unresumable_archives"], false);
    assert!(!state.store.auto_delete_unresumable_archives().unwrap());
    let refused_value = refused(
        &state,
        "a",
        "update_settings",
        json!({ "auto_delete_unresumable_archives": "no" }),
    )
    .await;
    assert!(refused_value.contains("true or false"), "{refused_value}");
    let read = call(&state, "a", "get_settings", &Map::new())
        .await
        .unwrap();
    assert_eq!(read["auto_delete_unresumable_archives"], false);
    assert_eq!(read["auto_select_account"], false);
    assert_eq!(read["sidebar_order_by_status"], true);
    // Octoboard's own switching of sessions is the user's alone to turn on: it is neither
    // listed nor taken.
    let ignored = call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({ "auto_switch_on_usage_limit": true })),
    )
    .await
    .unwrap();
    assert!(ignored.get("auto_switch_on_usage_limit").is_none());
    assert!(!state.store.auto_switch_on_usage_limit().unwrap());
    let ordered = call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({ "sidebar_order_by_status": false })),
    )
    .await
    .unwrap();
    assert_eq!(ordered["sidebar_order_by_status"], false);
    assert!(!state.store.sidebar_order_by_status().unwrap());
    // Each agent's default account, and the one account the state holds.
    assert_eq!(
        read["accounts"].as_array().unwrap().len(),
        AgentId::ALL.len() + 1
    );
    assert_eq!(read["accounts"][0]["id"], "default");
    assert_eq!(read["accounts"][0]["default"], true);
    assert!(read["trusted_directories"].is_array());
    assert!(!read.to_string().contains("/secret"));
    let message = refused(
        &state,
        "a",
        "update_settings",
        json!({ "default_clone_dir": "relative" }),
    )
    .await;
    assert!(message.contains("not an absolute path"), "{message}");
}

/// `get_project` with `refresh` checks the project's git status before answering.
#[tokio::test]
async fn get_project_can_refresh_the_git_status_first() {
    let fixture = RepoState::new("get-project-refresh");
    let plain = call(&fixture.state, "coordinator", "get_project", &Map::new())
        .await
        .unwrap();
    assert!(plain.get("git_refresh").is_none());
    let refreshed = call(
        &fixture.state,
        "coordinator",
        "get_project",
        &arguments(json!({ "refresh": true })),
    )
    .await
    .unwrap();
    assert_eq!(refreshed["git_refresh"], json!({ "finished": true }));
    assert!(refreshed["git_status"].is_object(), "{refreshed}");
}

/// A refresh that is not over when the wait is says so, and names the projects still being
/// checked; one the one-minute floor skips counts as over.
#[tokio::test]
async fn a_git_refresh_reports_the_checks_that_were_not_over_in_time() {
    use crate::mcp::tools::settings::refresh_git_status_within;
    use std::time::Duration;

    let fixture = RepoState::new("git-refresh-unfinished");
    let state = &fixture.state;
    let project = state.store.get_project("repo").unwrap().unwrap();

    let late = refresh_git_status_within(state, vec![project.clone()], Duration::ZERO).await;
    assert_eq!(late["finished"], false);
    assert_eq!(late["unfinished"], json!(["repo"]));
    assert!(
        late["note"].as_str().unwrap().contains("had not finished"),
        "{late}"
    );

    // A check completed within the floor is not started again, and not waited for.
    state.record_git_check_completed("repo");
    let skipped = refresh_git_status_within(state, vec![project], Duration::ZERO).await;
    assert_eq!(skipped, json!({ "finished": true }));
}

/// `get_settings` lists each agent's accounts in the order automatic account selection weighs
/// them: the order the user keeps them in, not the default account first.
#[tokio::test]
async fn get_settings_lists_the_accounts_in_the_order_the_user_keeps_them_in() {
    let (state, _dir) = settings_state("app-settings-account-order");
    let first = state.store.list_accounts().unwrap().remove(0);
    let agent = first.agent;
    let mut order: Vec<Option<String>> = state
        .store
        .list_accounts()
        .unwrap()
        .into_iter()
        .filter(|account| account.agent == agent)
        .map(|account| Some(account.id))
        .rev()
        .collect();
    order.push(None);
    state.store.reorder_accounts(agent, &order).unwrap();

    let listed = call(&state, "a", "get_settings", &arguments(json!({})))
        .await
        .unwrap();
    let ids: Vec<Option<String>> = listed["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|account| account["agent"] == agent.wire_name())
        .map(|account| match account["id"].as_str().unwrap() {
            "default" => None,
            id => Some(id.to_string()),
        })
        .collect();
    assert_eq!(ids, order);
}

/// A clone URL with a credential in it is refused without echoing the credential, so is one
/// that is not a remote at all; a local association takes its settings and returns them.
#[tokio::test]
async fn add_project_refuses_a_credentialed_url_and_takes_its_settings() {
    let (state, dir) = settings_state("add-project");
    // Should the refusal ever fail, a clone lands in the scratch directory, not the user's.
    let clones = dir.join("clones");
    state
        .store
        .set_default_clone_dir(Some(&clones.to_string_lossy()))
        .unwrap();
    for (url, expected) in [
        (
            "https://user:token@example.com/r.git",
            "carries credentials",
        ),
        ("https://token@example.com/r.git", "carries credentials"),
        ("/etc/passwd", "can be cloned"),
    ] {
        let args = json!({ "source": "git", "remote_url": url });
        let message = refused(&state, "a", "add_project", args).await;
        assert!(message.contains(expected), "{message}");
        assert!(
            !message.contains("token@") && !message.contains("user:"),
            "{message}"
        );
    }
    assert_eq!(state.store.list_projects().unwrap().len(), 2);

    let message = refused(
        &state,
        "a",
        "add_project",
        json!({ "source": "local", "path": "/tmp", "description": 3 }),
    )
    .await;
    assert!(message.contains("description"), "{message}");
    assert_eq!(state.store.list_projects().unwrap().len(), 2);

    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    let added = call(
        &state,
        "a",
        "add_project",
        &arguments(json!({
            "source": "local",
            "path": repo.to_string_lossy(),
            "tags": ["one"],
            "description": "  A billing service.  ",
            "pinned": true,
            "accounts": { "codex": "acct-codex" },
            "launch": { "claude": { "effort": "low" } },
        })),
    )
    .await
    .unwrap();
    let project = &added["projects"][0];
    assert_eq!(project["pinned"], true);
    assert_eq!(project["tags"], json!(["one"]));
    assert_eq!(project["description"], "A billing service.");
    assert_eq!(project["accounts"]["codex"], "acct-codex");
    assert_eq!(project["launch"]["claude"]["effort"], "low");

    let bare = dir.join("bare");
    std::fs::create_dir(&bare).unwrap();
    let added = call(
        &state,
        "a",
        "add_project",
        &arguments(json!({
            "source": "local",
            "path": bare.to_string_lossy(),
            "description": null,
        })),
    )
    .await
    .unwrap();
    assert!(added["projects"][0]["description"].is_null());
}

/// `add_project` takes the project's own git workflow values as `update_project` does, except
/// that an item left out or `null` follows the application-wide value; a parent source gives
/// them to every project it adds, and a bad value is refused before anything is stored.
#[tokio::test]
async fn add_project_takes_the_projects_own_git_workflow_values() {
    let (state, dir) = settings_state("add-project-git-workflow");
    let before = state.store.list_projects().unwrap().len();
    for bad in [
        json!({ "delivery": "rebase" }),
        json!({ "development": "yes" }),
        json!({ "typo": true }),
    ] {
        let args = json!({ "source": "local", "path": "/tmp", "git_workflow": bad });
        let message = refused(&state, "a", "add_project", args).await;
        assert!(message.contains("git_workflow"), "{message}");
    }
    assert_eq!(state.store.list_projects().unwrap().len(), before);

    call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({ "git_workflow": { "revert_gate": true } })),
    )
    .await
    .unwrap();
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    let added = call(
        &state,
        "a",
        "add_project",
        &arguments(json!({
            "source": "local",
            "path": repo.to_string_lossy(),
            "git_workflow": { "development": "in_place", "delivery": "merge", "revert_gate": null },
        })),
    )
    .await
    .unwrap();
    assert_eq!(
        added["projects"][0]["git_workflow"]["override"],
        json!({
            "development": "in_place",
            "squash_before_delivery": null,
            "rebase_before_delivery": null,
            "delivery": "merge",
            "revert_gate": null,
        })
    );
    assert_eq!(
        added["projects"][0]["git_workflow"]["effective"]["revert_gate"],
        true
    );

    let parent = dir.join("parent");
    for name in ["one", "two"] {
        std::fs::create_dir_all(parent.join(name).join(".git")).unwrap();
    }
    let added = call(
        &state,
        "a",
        "add_project",
        &arguments(json!({
            "source": "parent",
            "path": parent.to_string_lossy(),
            "git_workflow": { "rebase_before_delivery": false },
        })),
    )
    .await
    .unwrap();
    let projects = added["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2);
    assert!(projects
        .iter()
        .all(|project| project["git_workflow"]["override"]["rebase_before_delivery"] == false));

    let bare = dir.join("bare");
    std::fs::create_dir(&bare).unwrap();
    let added = call(
        &state,
        "a",
        "add_project",
        &arguments(json!({ "source": "local", "path": bare.to_string_lossy() })),
    )
    .await
    .unwrap();
    assert!(added["projects"][0]["git_workflow"]["override"]["delivery"].is_null());
}
