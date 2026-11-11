use serde_json::json;

use super::requests::request_session_schema;
use super::schemas::{open_session_schema, NAMING_LANGUAGE};
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
        "open_session",
        "report",
        "transfer_session",
        "transfer_branches",
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
    let text = |kind, tool: &str| {
        let tool = tool_by_name(kind, false, tool).unwrap();
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
    let (description, agent, _) = text(SessionKind::Project, "open_session");
    assert!(description.contains("fits the work better"));
    assert!(agent.contains("fits the work better"));
    assert!(description.contains("`list_agents`"));
    assert!(agent.contains("`list_agents`"));
    for (kind, tool) in [
        (SessionKind::Console, "open_session"),
        (SessionKind::Console, "request_session"),
        (SessionKind::Project, "request_session"),
    ] {
        let (description, agent, account) = text(kind, tool);
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
    let (description, _, _) = text(SessionKind::Console, "handoff_session");
    assert!(!description.contains("tags"), "{description}");
}

/// What a session is offered follows from its kind and whether it is bound: only a session that
/// can own sessions starts, transfers, hands off or drives them, only an unbound one binds itself,
/// only a project session can report, every session shares information and asks for a session with
/// no owner, and every read-only tool but `get_project` is offered to all three alike.
#[test]
fn the_tools_offered_follow_from_kind_and_binding() {
    let names = |kind, worker| -> Vec<&'static str> {
        tools_for(kind, worker)
            .iter()
            .map(|tool| tool.name)
            .collect()
    };
    assert_eq!(
        names(SessionKind::Project, true),
        [
            "report",
            "get_session",
            "list_sessions",
            "list_session_branches",
            "list_project_changes",
            "list_project_branches",
            "read_project_diff",
            "read_session_history",
            "list_agent_models",
            "list_consoles",
            "list_projects",
            "list_archived_sessions",
            "inform_session",
            "get_project",
            "list_agents",
            "request_session",
            "request_resume_session"
        ]
    );
    assert_eq!(
        names(SessionKind::Project, false),
        [
            "open_session",
            "instruct_session",
            "interrupt_session",
            "get_session",
            "archive_session",
            "archive_self",
            "list_archived_sessions",
            "resume_session",
            "rename_session",
            "set_session_pinned",
            "handoff_session",
            "transfer_branches",
            "transfer_processes",
            "switch_session_account",
            "set_session_model",
            "transfer_session",
            "release_session",
            "bind_to_session",
            "request_session",
            "request_resume_session",
            "report",
            "list_sessions",
            "list_session_branches",
            "list_project_changes",
            "list_project_branches",
            "read_project_diff",
            "read_session_history",
            "list_agent_models",
            "list_consoles",
            "list_projects",
            "inform_session",
            "get_project",
            "list_agents",
            "list_pages",
            "show_page",
            "page_guide",
            "delete_pages"
        ]
    );
    assert_eq!(
        names(SessionKind::Console, false),
        [
            "list_projects",
            "add_project",
            "update_project",
            "get_console",
            "update_console",
            "get_settings",
            "update_settings",
            "open_session",
            "instruct_session",
            "interrupt_session",
            "get_session",
            "rename_session",
            "set_session_pinned",
            "archive_session",
            "archive_self",
            "list_archived_sessions",
            "resume_session",
            "handoff_session",
            "transfer_branches",
            "transfer_processes",
            "switch_session_account",
            "set_session_model",
            "transfer_session",
            "release_session",
            "request_session",
            "request_resume_session",
            "list_sessions",
            "list_session_branches",
            "list_project_changes",
            "list_project_branches",
            "read_project_diff",
            "read_session_history",
            "list_agent_models",
            "list_consoles",
            "inform_session",
            "list_agents",
            "list_pages",
            "show_page",
            "page_guide",
            "delete_pages"
        ]
    );
    // The reads every role has, with one definition each, so that their descriptions cannot
    // differ by role.
    for read in [
        "get_session",
        "list_sessions",
        "read_session_history",
        "list_session_branches",
        "list_project_changes",
        "list_project_branches",
        "read_project_diff",
        "list_agent_models",
        "list_consoles",
        "list_projects",
        "list_archived_sessions",
        "list_agents",
    ] {
        let described = |kind, worker| {
            let tool = tool_by_name(kind, worker, read).unwrap_or_else(|| panic!("{read}"));
            (tool.description.to_string(), (tool.schema)())
        };
        let console = described(SessionKind::Console, false);
        assert_eq!(console, described(SessionKind::Project, false), "{read}");
        assert_eq!(console, described(SessionKind::Project, true), "{read}");
    }
    // Console sessions are never bound, so the flag changes nothing for them.
    assert_eq!(
        names(SessionKind::Console, false),
        names(SessionKind::Console, true)
    );
    // A bound project session is a subset of what it may have had while it could own sessions,
    // so a session whose binding changes while it runs never calls a tool its child did not
    // announce.
    for tool in names(SessionKind::Project, true) {
        assert!(names(SessionKind::Project, false).contains(&tool), "{tool}");
    }
}

/// An unbound project session starts sessions only in its own project, so it is not asked
/// which one.
#[test]
fn only_the_console_sessions_open_session_asks_for_a_project() {
    let schema = |tools: &'static [ToolDef]| {
        let tool = tools
            .iter()
            .find(|tool| tool.name == "open_session")
            .unwrap();
        (tool.schema)()
    };
    let console = schema(tools_for(SessionKind::Console, false));
    let project = schema(tools_for(SessionKind::Project, false));
    assert!(console["properties"]["project"].is_object());
    assert!(project["properties"]["project"].is_null());
    assert_eq!(project["required"], json!(["brief"]));
}

/// Every parameter that takes a model or an effort, `launch` settings included, sends the
/// caller to `list_agent_models` for the values, and each role has that tool.
#[test]
fn every_model_parameter_points_to_the_tool_that_lists_the_models() {
    let mut checked = 0;
    for (kind, bound) in [
        (SessionKind::Console, false),
        (SessionKind::Project, true),
        (SessionKind::Project, false),
    ] {
        let tools = tools_for(kind, bound);
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
    for (kind, bound) in [
        (SessionKind::Console, false),
        (SessionKind::Project, true),
        (SessionKind::Project, false),
    ] {
        for tool in tools_for(kind, bound) {
            // A page's title names a page, not a session.
            if tool.name == "show_page" {
                continue;
            }
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
            "open_session",
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
        qualified_tool_name(AgentId::CLAUDE, "report"),
        "mcp__octoboard__report"
    );
    assert_eq!(
        qualified_tool_name(AgentId::CODEX, "report"),
        "mcp__octoboard__report"
    );
    assert_eq!(
        qualified_tool_name(AgentId::GROK, "report"),
        "octoboard__report"
    );
}

/// The additions that close the gaps of the reads and writes: each is in the schema its tool
/// announces, and the descriptions name the fields the results really carry.
#[test]
fn the_schemas_and_descriptions_name_what_the_tools_take_and_return() {
    let tool = |name| tool_by_name(SessionKind::Console, false, name).unwrap();
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
        ("list_archived_sessions", "before"),
        ("list_archived_sessions", "yours"),
        ("list_archived_sessions", "branch"),
        ("list_archived_sessions", "check_resumable"),
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
            "list_session_branches",
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
        ("list_archived_sessions", &["`resumable`"][..]),
    ] {
        for field in fields {
            assert!(tool(name).description.contains(field), "{name}: {field}");
        }
    }
    let project = tool_by_name(SessionKind::Project, true, "get_project").unwrap();
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
            tool("open_session").description.contains(field),
            "open_session: {field}"
        );
    }
    assert!(tool("resume_session").description.contains("`effort`"));
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
fn the_open_session_schema_points_each_role_at_what_it_can_read() {
    let text = |schema: &Value, field: &str| {
        schema["properties"][field]["description"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let project = open_session_schema(false);
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

    let console = open_session_schema(true);
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
    for (kind, bound) in [
        (SessionKind::Console, false),
        (SessionKind::Project, true),
        (SessionKind::Project, false),
    ] {
        for tool in tools_for(kind, bound) {
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert!(schema["properties"].is_object(), "{}", tool.name);
            assert!(schema["required"].is_array(), "{}", tool.name);
        }
    }
}

/// PROTOCOL.md's table of the tools of each role names exactly the tools the catalogues announce,
/// so a tool added, renamed or dropped in one place cannot be missed in the other.
#[test]
fn the_protocol_tools_table_names_every_tool_of_the_catalogues() {
    use std::collections::BTreeSet;

    let protocol = include_str!("../../../PROTOCOL.md");
    let table = protocol
        .split("### The tools of each role\n")
        .nth(1)
        .expect("PROTOCOL.md has the section \"The tools of each role\"");
    let documented: BTreeSet<&str> = table
        .lines()
        .skip_while(|line| !line.starts_with('|'))
        .take_while(|line| line.starts_with('|'))
        .skip(2)
        .flat_map(|row| {
            let tools = row.split('|').nth(1).unwrap_or_default();
            tools.split('`').skip(1).step_by(2)
        })
        .collect();
    let catalogued: BTreeSet<&str> = [
        CONSOLE_SESSION_TOOLS.as_slice(),
        COORDINATOR_PROJECT_SESSION_TOOLS.as_slice(),
        WORKER_PROJECT_SESSION_TOOLS.as_slice(),
    ]
    .into_iter()
    .flatten()
    .map(|tool| tool.name)
    .collect();
    assert_eq!(documented, catalogued);
}

/// The five-hour window's names come from the providers that report one, in the registry's order.
#[test]
fn list_agents_names_the_five_hour_window_as_each_provider_reporting_one_does() {
    let tool = tool_by_name(SessionKind::Console, false, "list_agents").unwrap();
    let named: Vec<String> = crate::providers::descriptors()
        .filter(|descriptor| {
            descriptor
                .quota_windows
                .kinds
                .contains(&crate::protocol::QuotaWindowKind::Session)
        })
        .map(|descriptor| {
            format!(
                "{}'s {} limit",
                descriptor.label,
                descriptor.quota_windows.session_window.text()
            )
        })
        .collect();
    assert!(tool.description.contains(&format!(
        "(`kind` `session` is the five-hour window ({}), `weekly` the weekly one",
        named.join(", ")
    )));
    // The two real wordings, and the test-only provider, which reports a session window too.
    assert!(named.starts_with(&[
        "Claude Code's session limit".to_string(),
        "Codex's 5h limit".to_string(),
    ]));
    assert!(named.contains(&"Test Agent's session limit".to_string()));
}

#[test]
fn compatibility_session_and_launch_tools_advertise_nullable_boolean_overrides() {
    for name in [
        "open_session",
        "request_session",
        "request_resume_session",
        "handoff_session",
        "resume_session",
        "set_session_model",
    ] {
        let tool = tools_for(SessionKind::Console, false)
            .iter()
            .find(|tool| tool.name == name)
            .unwrap();
        let schema = (tool.schema)();
        assert_eq!(
            schema["properties"]["compatibility"]["type"], "object",
            "{name}"
        );
        assert_eq!(
            schema["properties"]["compatibility"]["additionalProperties"]["type"],
            json!(["boolean", "null"]),
            "{name}"
        );
    }
    for name in ["add_project", "update_project", "update_console"] {
        let tool = tools_for(SessionKind::Console, false)
            .iter()
            .find(|tool| tool.name == name)
            .unwrap();
        let schema = (tool.schema)();
        assert!(schema.to_string().contains("compatibility"), "{name}");
    }
    let tool = tools_for(SessionKind::Console, false)
        .iter()
        .find(|tool| tool.name == "update_settings")
        .unwrap();
    assert!((tool.schema)()["properties"].get("compatibility").is_none());
}

#[test]
fn compatibility_schema_names_the_launch_level_it_edits() {
    for name in ["add_project", "update_project", "update_console"] {
        let tool = tools_for(SessionKind::Console, false)
            .iter()
            .find(|tool| tool.name == name)
            .unwrap();
        let schema = (tool.schema)();
        for field in ["launch", "project_session_launch"] {
            let Some(launch) = schema["properties"].get(field) else {
                continue;
            };
            let description = launch["additionalProperties"]["properties"]["compatibility"]
                ["description"]
                .as_str()
                .unwrap();
            assert!(
                description.contains("for these launch settings"),
                "{name}.{field}: {description}"
            );
            assert!(
                !description.contains("for this session"),
                "{name}.{field}: {description}"
            );
        }
    }
}
