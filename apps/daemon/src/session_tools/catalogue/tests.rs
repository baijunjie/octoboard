use serde_json::json;

use super::requests::request_session_schema;
use super::schemas::{new_session_schema, NAMING_LANGUAGE};
use super::*;

/// Only the calls withdrawn when cancelled, and the one that sees to its own end, run on the
/// call's own task.
#[test]
fn only_the_withdrawn_calls_and_the_model_change_run_on_the_call() {
    for tool in WITHDRAWN_ON_CANCEL {
        assert!(runs_on_the_call(tool), "{tool}");
    }
    assert!(runs_on_the_call("set_session_model"));
    for tool in [
        "new_session",
        "report",
        "transfer_session",
        "handover_branches",
    ] {
        assert!(!runs_on_the_call(tool), "{tool}");
    }
}

/// A session names another agent unasked only by its judgement of the work: a project session
/// that owns sessions when it finds the agent fits better, weighing the agents' tags, and a
/// console session or a `request_session` only when the agent's tags fit; the account is
/// named only when the user asked, and `handoff_session` says nothing of tags.
#[test]
fn a_session_may_choose_another_agent_by_its_tags_but_an_account_only_when_asked() {
    let text = |role, tool: &str| {
        let tool = tool_by_name(role, false, tool).unwrap();
        let schema = (tool.schema)();
        (
            tool.description.to_string(),
            schema["properties"]["agent"]["description"]
                .as_str()
                .unwrap()
                .to_string(),
            schema["properties"]["account"]["description"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        )
    };
    let (description, agent, _) = text(Role::Project, "new_session");
    assert!(description.contains("fits the work better"));
    assert!(agent.contains("fits the work better"));
    assert!(description.contains("`list_agents`"));
    assert!(agent.contains("`list_agents`"));
    for (role, tool) in [
        (Role::Console, "new_session"),
        (Role::Console, "request_session"),
        (Role::Project, "request_session"),
    ] {
        let (description, agent, account) = text(role, tool);
        assert!(
            description.contains("its tags (`list_agents`) fit the work"),
            "{tool}: {description}"
        );
        assert!(
            agent.contains("its tags (`list_agents`) fit the work"),
            "{tool}: {agent}"
        );
        assert!(!agent.contains("fits the work better"), "{tool}: {agent}");
        assert!(
            account.contains("Pass one only when the user asked for it"),
            "{tool}: {account}"
        );
        assert!(!account.contains("`list_agents`"), "{tool}: {account}");
    }
    let (description, _, _) = text(Role::Console, "handoff_session");
    assert!(!description.contains("tags"), "{description}");
}

/// What a session is offered follows from its role and whether it is bound: only a session that
/// can own sessions starts, hands over or drives them, only an unbound one binds itself, only a
/// project session can report, every session shares information and asks for a session with no
/// owner, and every read-only tool but `get_project` is offered to all three alike.
#[test]
fn the_tools_offered_follow_from_role_and_binding() {
    let names = |role, worker| -> Vec<&'static str> {
        tools_for(role, worker)
            .iter()
            .map(|tool| tool.name)
            .collect()
    };
    assert_eq!(
        names(Role::Project, true),
        [
            "report",
            "get_session",
            "list_sessions",
            "get_session_branches",
            "list_project_changes",
            "list_project_branches",
            "read_project_diff",
            "read_session_history",
            "list_agent_models",
            "list_consoles",
            "list_projects",
            "list_archived",
            "send_message",
            "get_project",
            "list_agents",
            "request_session",
            "request_resume_session"
        ]
    );
    assert_eq!(
        names(Role::Project, false),
        [
            "new_session",
            "instruct_session",
            "interrupt_session",
            "get_session",
            "archive_session",
            "archive_self",
            "list_archived",
            "reopen_session",
            "rename_session",
            "set_session_pinned",
            "handoff_session",
            "handover_branches",
            "handover_processes",
            "switch_session_account",
            "set_session_model",
            "transfer_session",
            "release_session",
            "bind_to_session",
            "request_session",
            "request_resume_session",
            "report",
            "list_sessions",
            "get_session_branches",
            "list_project_changes",
            "list_project_branches",
            "read_project_diff",
            "read_session_history",
            "list_agent_models",
            "list_consoles",
            "list_projects",
            "send_message",
            "get_project",
            "list_agents",
            "list_pages",
            "show_page",
            "delete_pages"
        ]
    );
    assert_eq!(
        names(Role::Console, false),
        [
            "list_projects",
            "add_project",
            "update_project",
            "get_console",
            "update_console",
            "get_settings",
            "update_settings",
            "new_session",
            "instruct_session",
            "interrupt_session",
            "get_session",
            "rename_session",
            "set_session_pinned",
            "archive_session",
            "archive_self",
            "list_archived",
            "reopen_session",
            "handoff_session",
            "handover_branches",
            "handover_processes",
            "switch_session_account",
            "set_session_model",
            "transfer_session",
            "release_session",
            "request_session",
            "request_resume_session",
            "list_sessions",
            "get_session_branches",
            "list_project_changes",
            "list_project_branches",
            "read_project_diff",
            "read_session_history",
            "list_agent_models",
            "list_consoles",
            "send_message",
            "list_agents",
            "list_pages",
            "show_page",
            "delete_pages"
        ]
    );
    // The reads every role has, with one definition each, so that their descriptions cannot
    // differ by role.
    for read in [
        "get_session",
        "list_sessions",
        "read_session_history",
        "get_session_branches",
        "list_project_changes",
        "list_project_branches",
        "read_project_diff",
        "list_agent_models",
        "list_consoles",
        "list_projects",
        "list_archived",
        "list_agents",
    ] {
        let described = |role, worker| {
            let tool = tool_by_name(role, worker, read).unwrap_or_else(|| panic!("{read}"));
            (tool.description.to_string(), (tool.schema)())
        };
        let console = described(Role::Console, false);
        assert_eq!(console, described(Role::Project, false), "{read}");
        assert_eq!(console, described(Role::Project, true), "{read}");
    }
    // Console sessions are never bound, so the flag changes nothing for them.
    assert_eq!(names(Role::Console, false), names(Role::Console, true));
    // A bound project session is a subset of what it may have had while it could own sessions,
    // so a session whose binding changes while it runs never calls a tool its child did not
    // announce.
    for tool in names(Role::Project, true) {
        assert!(names(Role::Project, false).contains(&tool), "{tool}");
    }
}

/// An unbound project session starts sessions only in its own project, so it is not asked
/// which one.
#[test]
fn only_the_console_sessions_new_session_asks_for_a_project() {
    let schema = |tools: &'static [ToolDef]| {
        let tool = tools
            .iter()
            .find(|tool| tool.name == "new_session")
            .unwrap();
        (tool.schema)()
    };
    let console = schema(tools_for(Role::Console, false));
    let project = schema(tools_for(Role::Project, false));
    assert!(console["properties"]["project"].is_object());
    assert!(project["properties"]["project"].is_null());
    assert_eq!(project["required"], json!(["brief"]));
}

/// Every parameter that takes a model or an effort, `launch` settings included, sends the
/// caller to `list_agent_models` for the values, and each role has that tool.
#[test]
fn every_model_parameter_points_to_the_tool_that_lists_the_models() {
    let mut checked = 0;
    for (role, bound) in [
        (Role::Console, false),
        (Role::Project, true),
        (Role::Project, false),
    ] {
        let tools = tools_for(role, bound);
        assert!(tools.iter().any(|tool| tool.name == "list_agent_models"));
        for tool in tools {
            let schema = (tool.schema)();
            let mut described = Vec::new();
            for key in ["model", "effort", "launch", "project_session_launch"] {
                described.extend(schema["properties"][key]["description"].as_str());
            }
            for text in described {
                assert!(
                    text.contains("`list_agent_models`"),
                    "{}: {text}",
                    tool.name
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 10, "{checked}");
}

/// Every tool that names a session says, on its `title`, which language to write the name in.
#[test]
fn every_title_says_which_language_to_write_the_name_in() {
    let mut naming = Vec::new();
    for (role, bound) in [
        (Role::Console, false),
        (Role::Project, true),
        (Role::Project, false),
    ] {
        for tool in tools_for(role, bound) {
            let schema = (tool.schema)();
            let Some(title) = schema["properties"]["title"]["description"].as_str() else {
                continue;
            };
            assert!(title.ends_with(NAMING_LANGUAGE), "{}: {title}", tool.name);
            naming.push(tool.name);
        }
    }
    naming.sort_unstable();
    naming.dedup();
    assert_eq!(
        naming,
        [
            "handoff_session",
            "new_session",
            "rename_session",
            "request_resume_session",
            "request_session"
        ]
    );
}

/// The prefix is the agent's own, and a role description that names the wrong one points the
/// model at a tool it cannot see.
#[test]
fn the_qualified_name_follows_the_agent() {
    assert_eq!(
        qualified_tool_name(Agent::Claude, "report"),
        "mcp__octoboard__report"
    );
    assert_eq!(
        qualified_tool_name(Agent::Codex, "report"),
        "mcp__octoboard__report"
    );
    assert_eq!(
        qualified_tool_name(Agent::Grok, "report"),
        "octoboard__report"
    );
}

/// The additions that close the gaps of the reads and writes: each is in the schema its tool
/// announces, and the descriptions name the fields the results really carry.
#[test]
fn the_schemas_and_descriptions_name_what_the_tools_take_and_return() {
    let tool = |name| tool_by_name(Role::Console, false, name).unwrap();
    let schema = |name| (tool(name).schema)();
    // A session reads itself by leaving `session` out.
    assert_eq!(schema("get_session")["required"], json!([]));
    assert!(
        tool("get_session")
            .description
            .contains("your own Octoboard id")
            || tool("get_session")
                .description
                .contains("its own Octoboard id")
    );
    for (name, key) in [
        ("list_archived", "before"),
        ("list_archived", "yours"),
        ("list_archived", "branch"),
        ("list_archived", "check_resumable"),
        ("list_sessions", "kind"),
        ("list_sessions", "branch"),
        ("list_projects", "project"),
        ("list_projects", "refresh"),
        ("archive_session", "keep"),
    ] {
        assert!(schema(name)["properties"][key].is_object(), "{name}.{key}");
        assert!(
            tool(name).description.contains(&format!("`{key}`")),
            "{name}.{key}"
        );
    }
    // A session reads its own history by leaving `session` out.
    assert_eq!(schema("read_session_history")["required"], json!([]));
    assert!(tool("read_session_history")
        .description
        .contains("`session` left out"));
    // The new fields are named where the results carry them.
    for (name, fields) in [
        (
            "get_session_branches",
            &[
                "`target_branch`",
                "`fork_point`",
                "`responsible_since`",
                "`archive`",
            ][..],
        ),
        (
            "list_project_branches",
            &[
                "`tip`",
                "`upstream`",
                "`worktree`",
                "`origin`",
                "`session_state`",
                "`target_branch`",
                "`fork_point`",
                "`default_branch`",
            ][..],
        ),
        ("get_session", &["`resumable`"][..]),
        ("list_archived", &["`resumable`"][..]),
    ] {
        for field in fields {
            assert!(tool(name).description.contains(field), "{name}: {field}");
        }
    }
    let project = tool_by_name(Role::Project, true, "get_project").unwrap();
    assert!((project.schema)()["properties"]["refresh"].is_object());
    // The project entry is described by what it carries.
    assert!(!tool("list_projects").description.contains("`worktrees`"));
    assert!(tool("list_projects").description.contains("`git_workflow`"));
    for field in ["`project`", "`host`", "`trust_consent`"] {
        assert!(project.description.contains(field), "get_project: {field}");
    }
    assert!(tool("get_console").description.contains("`console`"));
    for field in ["`account_name`", "`account`"] {
        assert!(
            tool("new_session").description.contains(field),
            "new_session: {field}"
        );
    }
    assert!(tool("reopen_session").description.contains("`effort`"));
    for field in [
        "`owned_sessions`",
        "`agent_session_id`",
        "`display_title`",
        "`origin`",
    ] {
        assert!(
            tool("get_session").description.contains(field),
            "get_session: {field}"
        );
    }
}

/// A project session is pointed at `get_project` for what its project sets, a console session
/// at `get_settings` for the accounts to choose from, and neither at what it cannot read.
#[test]
fn the_new_session_schema_points_each_role_at_what_it_can_read() {
    let text = |schema: &Value, field: &str| {
        schema["properties"][field]["description"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let project = new_session_schema(false);
    let account = text(&project, "account");
    assert!(
        account.contains("for the agent (`get_project` gives it in `effective.accounts`)."),
        "{account}"
    );
    assert!(
        account.contains("an account's id or name, or `default`"),
        "{account}"
    );
    assert!(!account.contains("get_settings"), "{account}");
    for field in ["model", "effort"] {
        let text = text(&project, field);
        assert!(
            text.contains("`get_project` gives it in `effective.launch`"),
            "{text}"
        );
    }

    let console = new_session_schema(true);
    let account = text(&console, "account");
    assert!(
        account.contains("an account's id or name (see `get_settings`), or `default`"),
        "{account}"
    );
    assert!(account.contains("for the agent."), "{account}");
    assert!(!account.contains("get_project"), "{account}");
    for schema in [&project, &console] {
        let account = text(schema, "account");
        assert!(
            account.contains("Pass one only when the user asked for it"),
            "{account}"
        );
        assert!(
            account.contains("the server selects it according to the configured policy"),
            "{account}"
        );
    }
    for field in ["model", "effort"] {
        let text = text(&console, field);
        assert!(!text.contains("get_project"), "{text}");
    }

    // `request_session` can target any project and is a console session's tool too, so its
    // pointer names a project session's own project.
    let request = request_session_schema();
    for field in ["account", "model", "effort"] {
        let text = text(&request, field);
        assert!(
            text.contains("project session reads its own project's in `get_project`'s"),
            "{text}"
        );
    }
}

#[test]
fn every_tool_announces_an_object_schema() {
    for (role, bound) in [
        (Role::Console, false),
        (Role::Project, true),
        (Role::Project, false),
    ] {
        for tool in tools_for(role, bound) {
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert!(schema["properties"].is_object(), "{}", tool.name);
            assert!(schema["required"].is_array(), "{}", tool.name);
        }
    }
}
