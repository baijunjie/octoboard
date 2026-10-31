use crate::mcp::exec::call;
use crate::mcp::test_support::{
    arguments, refused, settings_state, shared_console_state, RepoState,
};
use crate::protocol::Agent;
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
    assert_eq!(stored.launch.claude.model.as_deref(), Some("opus"));

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
            "rebase_before_merge": true,
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
        crate::git_workflow::Delivery::Merge
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
            "rebase_before_merge": null,
            "delivery": "pull_request",
            "revert_gate": null,
        })
    );
    assert_eq!(
        project["git_workflow"]["effective"],
        json!({
            "development": "in_place",
            "squash_before_delivery": false,
            "rebase_before_merge": true,
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
    console.project_session_agent = Some(Agent::Codex);
    console.project_session_launch.claude.model = Some("console-model".to_string());
    console.project_session_launch.claude.effort = Some("console-effort".to_string());
    state.store.update_console(&console).unwrap();
    let mut project = state.store.get_project("project-1").unwrap().unwrap();
    project.default_agent = None;
    project.launch.get_mut(Agent::Claude).model = Some("project-model".to_string());
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
        stored.project_session_launch.claude.model.as_deref(),
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
    assert_eq!(
        state.store.get_console("console-2").unwrap().unwrap().name,
        "Console"
    );

    for (args, expected) in [
        (json!({ "name": " " }), "`name` is required"),
        (json!({ "project_session_agent": "gemini" }), "gemini"),
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
/// the switch asking for questions as choices, and the interface language, are the user's alone:
/// an agent can neither read them nor change them through the settings tools, whatever it
/// passes.
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
    assert!(listed.get("interface_language").is_none());

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
        state.store.set_interface_language("ja").unwrap();
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
                "interface_language": "de",
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
        assert!(updated.get("interface_language").is_none());
        assert_eq!(updated["default_clone_dir"], "/tmp/clones");
        assert_eq!(state.store.auto_approve_session_requests().unwrap(), stored);
        assert_eq!(state.store.auto_approve_resume_requests().unwrap(), stored);
        assert_eq!(state.store.auto_approve_account_switches().unwrap(), stored);
        assert_eq!(state.store.auto_approve_handoffs().unwrap(), stored);
        assert_eq!(state.store.ask_questions_as_choices().unwrap(), stored);
        assert_eq!(
            state.store.interface_language().unwrap().as_deref(),
            Some("ja")
        );
    }
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
        .set_default_account_disabled(Agent::Codex, true)
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
/// the only writes are to the five settings a console session may change.
#[tokio::test]
async fn settings_are_listed_without_directories_and_five_of_them_are_writable() {
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
    let selected = call(
        &state,
        "a",
        "update_settings",
        &arguments(json!({ "auto_select_account": true })),
    )
    .await
    .unwrap();
    assert_eq!(selected["auto_select_account"], true);
    assert!(state.store.auto_select_account().unwrap());
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
    assert_eq!(read["accounts"].as_array().unwrap().len(), 4);
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
    let plain = call(&fixture.state, "lead", "get_project", &Map::new())
        .await
        .unwrap();
    assert!(plain.get("git_refresh").is_none());
    let refreshed = call(
        &fixture.state,
        "lead",
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
    use crate::mcp::settings::refresh_git_status_within;
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
