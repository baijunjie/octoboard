//! Tests of OpenCode's launch plan: its arguments, the files it writes into the scratch directory
//! and the inline configuration it hands over in `OPENCODE_CONFIG_CONTENT`; also its terminal
//! titles and the way its CLI is installed.

use std::path::PathBuf;

use serde_json::{json, Value};

use super::*;
use crate::adapter::tests::SpecFixture;
use crate::providers::{ProviderSettings, SettingValue};

fn spec_fixture() -> SpecFixture {
    let mut fixture = crate::adapter::tests::spec_fixture();
    fixture.agent_version = Some((1, 19, 6));
    fixture
}

const RESUMED: &str = "ses_resumed";

fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|arg| arg == flag)?;
    args.get(at + 1).map(String::as_str)
}

fn approval_mode(fixture: &mut SpecFixture, mode: &str) {
    fixture.settings = ProviderSettings::new([(
        "approval_mode".to_string(),
        Some(SettingValue::Choice(mode.to_string())),
    )]);
}

/// The value the plan gives `OPENCODE_CONFIG_CONTENT`, parsed.
fn config_of(plan: &LaunchPlan) -> Value {
    let (_, content) = plan
        .env
        .iter()
        .find(|(name, _)| name == "OPENCODE_CONFIG_CONTENT")
        .expect("the configuration is set on every launch");
    serde_json::from_str(content).expect("the configuration is JSON")
}

fn injection_dir(fixture: &SpecFixture) -> PathBuf {
    fixture.scratch.join("opencode")
}

/// The data file the launch wrote for the plugin, parsed.
fn plugin_data(fixture: &SpecFixture) -> Value {
    let text = std::fs::read_to_string(injection_dir(fixture).join("octoboard.json"))
        .expect("the data file is written on every launch");
    serde_json::from_str(&text).expect("the data file is JSON")
}

fn paths_in(list: &Value) -> Vec<String> {
    list.as_array()
        .expect("a list")
        .iter()
        .map(|entry| entry.as_str().expect("a path").to_string())
        .collect()
}

/// The path of a file of the launch's injection, as the configuration spells it.
fn written(fixture: &SpecFixture, name: &str) -> String {
    injection_dir(fixture)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn it_has_a_status_integration() {
    assert!(OpenCodeProvider.has_status_integration());
}

/// The first task waits for the TUI in the outbox. A model is still passed to the standalone
/// fallback, and OpenCode has no reasoning-effort flag.
#[test]
fn a_new_session_queues_the_first_task_and_passes_the_model() {
    let mut fixture = spec_fixture();
    fixture.model = Some("opencode/big-pickle".to_string());
    fixture.effort = Some("high".to_string());
    let mut spec = fixture.spec();
    spec.agent_version = Some((1, 19, 6));
    spec.task = Some("- fix the build\n- add the tests");
    let plan = OpenCodeProvider.plan(&spec).expect("plan");
    assert_eq!(plan.args, ["--model", "opencode/big-pickle"]);
    assert_eq!(plan.first_message.as_deref(), spec.task);
    assert!(plan.private_server.is_some());
}

/// A new session with no task starts with an empty input box: no `--prompt`, and no model flag
/// when none is chosen.
#[test]
fn a_new_session_without_a_task_or_a_model_passes_nothing() {
    let fixture = spec_fixture();
    let plan = OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    assert_eq!(plan.args, Vec::<String>::new());
}

/// A resume names the conversation with `--session` and never repeats the first task.
#[test]
fn a_resume_names_the_session_and_does_not_repeat_the_task() {
    let mut fixture = spec_fixture();
    fixture.model = Some("opencode/big-pickle".to_string());
    let mut spec = fixture.spec();
    spec.resume_agent_session_id = Some(RESUMED);
    spec.task = Some("first task");
    let plan = OpenCodeProvider.plan(&spec).expect("plan");
    assert_eq!(value_after(&plan.args, "--session"), Some(RESUMED));
    assert_eq!(
        value_after(&plan.args, "--model"),
        Some("opencode/big-pickle")
    );
    assert!(!plan.args.iter().any(|arg| arg.starts_with("--prompt")));
    assert!(!plan.args.iter().any(|arg| arg.contains("first task")));
}

/// `--auto` is passed for the `auto` approval mode only, a resume included, since the flag holds
/// for one process only.
#[test]
fn auto_is_passed_only_for_the_auto_approval_mode() {
    let auto_in = |mode: Option<&str>, resume: Option<&str>| {
        let mut fixture = spec_fixture();
        if let Some(mode) = mode {
            approval_mode(&mut fixture, mode);
        }
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = resume;
        OpenCodeProvider
            .plan(&spec)
            .expect("plan")
            .args
            .contains(&"--auto".to_string())
    };
    assert!(auto_in(Some("auto"), None));
    assert!(auto_in(Some("auto"), Some(RESUMED)), "a resume too");
    assert!(!auto_in(Some("ask"), None));
    assert!(!auto_in(None, None));
    assert!(!auto_in(None, Some(RESUMED)));
}

/// The plugin, its data file and the role text are written into `<scratch>/opencode`, readable
/// by the owner only; the data file names the hook script the plugin runs.
#[test]
fn the_plugin_its_data_and_the_role_are_written_under_the_scratch_directory() {
    let fixture = spec_fixture();
    OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    let dir = injection_dir(&fixture);

    let plugin = std::fs::read_to_string(dir.join("octoboard.js")).expect("plugin");
    assert_eq!(plugin, include_str!("plugin.js"));

    let data: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("octoboard.json")).expect("data"))
            .expect("JSON");
    assert_eq!(data["hook"], fixture.hook_script.to_string_lossy().as_ref());
    assert!(data.get("user_config_content").is_none());
    assert_eq!(data["user_title_off"], Value::Null);
    assert_eq!(data["default_action"], Value::Null);
    assert_eq!(data["tools_rule"], "octoboard_**");

    let role = std::fs::read_to_string(dir.join("octoboard-role.md")).expect("role");
    assert!(!role.trim().is_empty());
    assert!(
        role.contains("octoboard_"),
        "tools are named as OpenCode names them"
    );

    #[cfg(unix)]
    for name in ["octoboard.js", "octoboard.json", "octoboard-role.md"] {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join(name))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{name} is private, was {mode:o}");
    }
}

/// Every launch writes the files afresh, a resume included, so a file the user changed or lost
/// between two launches is restored.
#[test]
fn every_launch_writes_the_files_afresh_a_resume_included() {
    let fixture = spec_fixture();
    OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    let dir = injection_dir(&fixture);
    std::fs::write(dir.join("octoboard.js"), "tampered").expect("tamper");
    std::fs::remove_file(dir.join("octoboard-role.md")).expect("remove");
    let mut spec = fixture.spec();
    spec.resume_agent_session_id = Some(RESUMED);
    OpenCodeProvider.plan(&spec).expect("plan");
    assert_eq!(
        std::fs::read_to_string(dir.join("octoboard.js")).expect("plugin"),
        include_str!("plugin.js")
    );
    assert!(dir.join("octoboard-role.md").is_file());
}

/// The inline configuration carries the Octoboard MCP server with a timeout above Octoboard's own
/// forwarding limit, the role file as instructions and the plugin, the last two by the paths of the
/// files written for the launch; and no permission rule, which the plugin writes.
#[test]
fn the_configuration_carries_the_server_the_role_and_the_plugin_and_no_permission_rule() {
    let fixture = spec_fixture();
    let plan = OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    let config = config_of(&plan);

    let (command, args) = crate::adapter::mcp_server_command(&fixture.spec());
    let mut expected_command = vec![command];
    expected_command.extend(args);
    let server = &config["mcp"]["octoboard"];
    assert_eq!(server["type"], "local");
    assert_eq!(server["enabled"], true);
    assert_eq!(server["command"], json!(expected_command));
    assert_eq!(server["timeout"], MCP_TOOL_TIMEOUT_SECS * 1000);
    assert!(
        server["timeout"].as_u64().expect("milliseconds")
            > crate::session_tools::CALL_TIMEOUT.as_millis() as u64
    );

    assert_eq!(config.get("permission"), None);
    assert_eq!(
        paths_in(&config["instructions"]),
        [written(&fixture, "octoboard-role.md")]
    );
    assert_eq!(
        paths_in(&config["plugin"]),
        [written(&fixture, "octoboard.js")]
    );
}

/// OpenCode has the default account only, so a launch given an account's directory is refused as
/// an operation the provider does not offer, before anything is written; planned, it would run as
/// the default account under the other's name.
#[test]
fn a_launch_given_an_account_directory_is_refused() {
    let mut fixture = spec_fixture();
    let dir = fixture.scratch.join("account").join("opencode");
    std::fs::create_dir_all(&dir).expect("dir");
    fixture.config_dir = Some(dir);
    let Err(refusal) = OpenCodeProvider.plan(&fixture.spec()) else {
        panic!("a launch given an account directory was planned");
    };
    assert_eq!(
        CodedError::of(&refusal),
        CodedError::of(&CodedError::provider_unsupported(
            AgentId::OPENCODE,
            ProviderOperation::OwnedAccounts,
        ))
    );
    assert!(!injection_dir(&fixture).exists());
}

/// Compatibility is scoped to the launch without redirecting the default account's data.
#[test]
fn the_default_account_exports_no_directory() {
    let fixture = spec_fixture();
    let plan = OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    assert!(plan.env.iter().any(|(name, _)| name == CONFIG_CONTENT_VAR));
    for name in CLAUDE_COMPATIBILITY_VARS {
        assert!(plan
            .env
            .iter()
            .any(|(key, value)| key == name && value == "true"));
    }
    assert!(!plan.env.iter().any(|(name, _)| name.starts_with("XDG_")));
}

/// A configuration in the user's shell is kept and Octoboard's merged over it: the instruction and
/// plugin lists are added to, any other list and any scalar Octoboard sets replaces theirs, and
/// what Octoboard does not set stays. The plugin is given the user's own value to hand back to the
/// shells the agent starts.
#[test]
fn the_users_own_configuration_is_merged_under_octoboards() {
    let mut fixture = spec_fixture();
    let user = json!({
        "theme": "tokyonight",
        "instructions": ["/home/me/rules.md"],
        "plugin": ["/home/me/plugin.js"],
        "permission": { "*": "ask", "octoboard_*": "deny" },
        "mcp": {
            "octoboard": { "command": ["old", "--flag"], "environment": { "KEEP": "1" } },
            "theirs": { "type": "local", "command": ["theirs"] },
        },
        "disabled_providers": ["a", "b"],
    })
    .to_string();
    fixture
        .shell_env
        .insert("OPENCODE_CONFIG_CONTENT".to_string(), user.clone());
    let plan = OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    let config = config_of(&plan);

    assert_eq!(config["theme"], "tokyonight");
    assert_eq!(config["disabled_providers"], json!(["a", "b"]));
    assert_eq!(
        paths_in(&config["instructions"]),
        [
            "/home/me/rules.md".to_string(),
            written(&fixture, "octoboard-role.md")
        ]
    );
    assert_eq!(
        paths_in(&config["plugin"]),
        [
            "/home/me/plugin.js".to_string(),
            written(&fixture, "octoboard.js")
        ]
    );
    assert_eq!(
        config["permission"],
        json!({ "*": "ask", "octoboard_*": "deny" })
    );
    assert_eq!(config["mcp"]["theirs"]["command"], json!(["theirs"]));
    let server = &config["mcp"]["octoboard"];
    assert_eq!(server["environment"], json!({ "KEEP": "1" }));
    assert_eq!(server["command"][0], "/opt/octoboard/octoboardd");
    assert_eq!(server["command"][1], "mcp");

    assert!(plugin_data(&fixture).get("user_config_content").is_none());
    assert!(plan.env.contains(&(ORIGINAL_CONFIG.into(), user)));
}

/// A value of the user's that is not a JSON object is dropped, as OpenCode would have refused it,
/// and a blank one is no value at all.
#[test]
fn a_user_value_that_is_not_an_object_is_dropped() {
    for user in ["[1, 2]", "\"text\"", "42", "null", "{broken"] {
        let mut fixture = spec_fixture();
        let plain = config_of(&OpenCodeProvider.plan(&fixture.spec()).expect("plan"));
        fixture
            .shell_env
            .insert("OPENCODE_CONFIG_CONTENT".to_string(), user.to_string());
        let config = config_of(&OpenCodeProvider.plan(&fixture.spec()).expect("plan"));
        assert_eq!(config, plain, "{user}: nothing of it is kept");
    }

    for blank in ["", "  \n"] {
        let mut fixture = spec_fixture();
        fixture
            .shell_env
            .insert("OPENCODE_CONFIG_CONTENT".to_string(), blank.to_string());
        let plan = OpenCodeProvider.plan(&fixture.spec()).expect("plan");
        assert!(
            plan.env.contains(&(ORIGINAL_CONFIG.into(), String::new())),
            "{blank:?}"
        );
    }
}

/// Merging is repeatable: the entries of the added lists already there are not added again, so a
/// configuration that already holds Octoboard's own does not list its plugin twice.
#[test]
fn an_entry_already_in_an_added_list_is_not_added_again() {
    let mut base = json!({ "plugin": ["a.js", "b.js"], "instructions": ["r.md"] });
    merge_over(
        &mut base,
        json!({ "plugin": ["b.js", "c.js"], "instructions": ["r.md"] }),
    );
    assert_eq!(base["plugin"], json!(["a.js", "b.js", "c.js"]));
    assert_eq!(base["instructions"], json!(["r.md"]));
}

/// Objects merge key by key at every depth; a list other than the added ones and a scalar are
/// replaced whole, and a value of another kind replaces what was there.
#[test]
fn other_lists_and_scalars_are_replaced_and_objects_merge_key_by_key() {
    let mut base = json!({
        "list": [1, 2],
        "scalar": "old",
        "kind": { "a": 1 },
        "deep": { "inner": { "keep": true, "list": ["x", "y"], "set": 1 } },
    });
    merge_over(
        &mut base,
        json!({
            "list": [3],
            "scalar": "new",
            "kind": "now a string",
            "deep": { "inner": { "list": ["z"], "set": 2, "added": 3 } },
        }),
    );
    assert_eq!(
        base,
        json!({
            "list": [3],
            "scalar": "new",
            "kind": "now a string",
            "deep": { "inner": { "keep": true, "list": ["z"], "set": 2, "added": 3 } },
        })
    );
}

/// The start of a session is read off the terminal title, so a shell that turns the title off is
/// overruled for this process, with the empty value OpenCode reads as unset; a shell that does not
/// is left alone.
#[test]
fn the_title_override_is_applied_only_where_the_shell_turns_the_title_off() {
    let title_in = |shell: Option<&str>| {
        let mut fixture = spec_fixture();
        if let Some(value) = shell {
            fixture.shell_env.insert(
                "OPENCODE_DISABLE_TERMINAL_TITLE".to_string(),
                value.to_string(),
            );
        }
        OpenCodeProvider
            .plan(&fixture.spec())
            .expect("plan")
            .env
            .into_iter()
            .find(|(name, _)| name == "OPENCODE_DISABLE_TERMINAL_TITLE")
    };
    assert_eq!(title_in(None), None);
    for value in ["1", "true"] {
        assert_eq!(
            title_in(Some(value)),
            Some(("OPENCODE_DISABLE_TERMINAL_TITLE".to_string(), String::new())),
            "{value}"
        );
    }
}

/// The shell's own value of the variable is kept in the plugin's data file for the shells the
/// agent starts (null where the shell sets none), since the launch overrules it for OpenCode's own
/// process.
#[test]
fn the_plugin_is_given_the_shells_own_title_setting() {
    for shell in [None, Some("1"), Some("true"), Some("")] {
        let mut fixture = spec_fixture();
        if let Some(value) = shell {
            fixture.shell_env.insert(
                "OPENCODE_DISABLE_TERMINAL_TITLE".to_string(),
                value.to_string(),
            );
        }
        OpenCodeProvider.plan(&fixture.spec()).expect("plan");
        let data: Value = serde_json::from_str(
            &std::fs::read_to_string(injection_dir(&fixture).join("octoboard.json")).expect("data"),
        )
        .expect("JSON");
        assert_eq!(data["user_title_off"], json!(shell), "{shell:?}");
    }
}

/// A session's title is `OC | <title>`; the bare title outside a session, a blank one and the name
/// OpenCode gives a session before it has generated a title carry no task.
#[test]
fn terminal_titles_give_a_task_only_for_a_session_with_a_title() {
    let task = |raw: &str| OpenCodeProvider.task_title(raw, &[]);
    assert_eq!(task("OC | x").as_deref(), Some("x"));
    assert_eq!(task("OC | Fix the build").as_deref(), Some("Fix the build"));
    assert_eq!(
        task("  OC | Fix the build \n").as_deref(),
        Some("Fix the build"),
        "surrounding blanks are not part of it"
    );
    for raw in [
        "OpenCode",
        "",
        "OC | ",
        "OC |",
        "OC | New session - 2026-11-21T08:30:00.000Z",
    ] {
        assert_eq!(task(raw), None, "{raw:?}");
    }
}

/// OpenCode's own installer, npm packages, Homebrew formula and update command, which
/// `crate::agents::cli` tells an install's method and update by.
#[test]
fn its_cli_is_installed_by_its_own_installer_npm_and_homebrew() {
    let packaging = OpenCodeProvider.cli_packaging();
    assert_eq!(packaging.native_layouts, [[".opencode", "bin"].as_slice()]);
    assert_eq!(packaging.native_update, ["upgrade"]);
    assert!(packaging.native_latest.is_none());
    assert_eq!(packaging.npm_packages, ["opencode-ai", "@opencode/cli"]);
    assert_eq!(packaging.homebrew_formula, Some("opencode"));
    assert_eq!(packaging.homebrew_cask, None);
}

/// The user's `OPENCODE_CONFIG_CONTENT`, put through a launch: the configuration it comes out as.
fn config_with_user(user: &str) -> Value {
    let mut fixture = spec_fixture();
    fixture
        .shell_env
        .insert("OPENCODE_CONFIG_CONTENT".to_string(), user.to_string());
    config_of(&OpenCodeProvider.plan(&fixture.spec()).expect("plan"))
}

/// The names of `value`'s keys, in the order they are written.
fn keys_of(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect()
}

/// The launch writes no permission rule into the inline value, under any approval mode: a key
/// keeps the place it first had when OpenCode merges one configuration over another, so a rule
/// written there cannot be placed beneath or after a rule of a file's, and an inline `"*": "ask"`
/// lands after a file's rules and turns a deny of the user's into a prompt. A `permission` of the
/// user's own inline value goes through as they wrote it, its keys in their order, a rule under
/// Octoboard's own key included, which the plugin places in the merged configuration.
#[test]
fn no_permission_rule_is_written_inline_and_the_users_own_goes_through_as_written() {
    let user = r#"{"permission": {"octoboard_**": "deny", "bash": {"git *": "allow"}, "*": "allow", "edit": "deny"}}"#;
    for mode in [None, Some("ask"), Some("auto")] {
        let mut fixture = spec_fixture();
        if let Some(mode) = mode {
            approval_mode(&mut fixture, mode);
        }
        let plan = OpenCodeProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(config_of(&plan).get("permission"), None, "{mode:?}");

        fixture
            .shell_env
            .insert("OPENCODE_CONFIG_CONTENT".to_string(), user.to_string());
        let config = config_of(&OpenCodeProvider.plan(&fixture.spec()).expect("plan"));
        let rules = &config["permission"];
        assert_eq!(
            keys_of(rules),
            ["octoboard_**", "bash", "*", "edit"],
            "{mode:?}"
        );
        assert_eq!(
            rules,
            &json!({ "octoboard_**": "deny", "bash": { "git *": "allow" }, "*": "allow", "edit": "deny" }),
            "{mode:?}"
        );
    }
    let config = config_with_user(r#"{"permission": "deny"}"#);
    assert_eq!(config["permission"], "deny");
}

/// The plugin is told the rules to write into the configuration OpenCode merged: on every launch
/// the pattern of Octoboard's own tools, as OpenCode names them, and under `ask` alone the action
/// that becomes the default beneath the user's rules. With no choice, and under `auto`, it is told
/// of no default, so the user's rules are left as they are.
#[test]
fn the_plugin_is_told_the_tools_rule_always_and_the_default_only_under_ask() {
    for (mode, default_action) in [
        (None, Value::Null),
        (Some("ask"), json!("ask")),
        (Some("auto"), Value::Null),
    ] {
        let mut fixture = spec_fixture();
        if let Some(mode) = mode {
            approval_mode(&mut fixture, mode);
        }
        OpenCodeProvider.plan(&fixture.spec()).expect("plan");
        let data = plugin_data(&fixture);
        assert_eq!(data["default_action"], default_action, "{mode:?}");
        assert_eq!(data["tools_rule"], "octoboard_**", "{mode:?}");
    }
    assert!(
        crate::session_tools::catalogue::qualified_tool_name(AgentId::OPENCODE, "get_session")
            .starts_with("octoboard_")
    );
}

/// The three choices launch three ways: `auto` by its flag alone, `ask` by the default the plugin
/// is told of alone, no choice by neither. A resume launches as a new session does.
#[test]
fn ask_auto_and_no_choice_each_launch_their_own_way() {
    let launch = |mode: Option<&str>, resume: Option<&str>| {
        let mut fixture = spec_fixture();
        if let Some(mode) = mode {
            approval_mode(&mut fixture, mode);
        }
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = resume;
        let plan = OpenCodeProvider.plan(&spec).expect("plan");
        (
            plan.args.contains(&"--auto".to_string()),
            plugin_data(&fixture)["default_action"].clone(),
        )
    };
    for resume in [None, Some(RESUMED)] {
        assert_eq!(launch(Some("auto"), resume), (true, Value::Null));
        assert_eq!(launch(Some("ask"), resume), (false, json!("ask")));
        assert_eq!(launch(None, resume), (false, Value::Null));
    }
}

/// The plugin writes the rules from its `config` hook alone, into the object it is handed: it
/// swallows a failure there, and writes no file anywhere. (A source scan: what the hook does to a
/// configuration is tested by running it, in `plugin.test.mjs` beside the plugin.)
#[test]
fn the_plugin_writes_the_rules_in_its_config_hook_and_no_file() {
    let hook = PLUGIN_SOURCE
        .find("    config: async (config) => {")
        .expect("the config hook");
    let body = &PLUGIN_SOURCE[hook..];
    let body = &body[..body.find("\n    },").expect("the hook's end")];
    assert!(
        body.contains("try {") && body.contains("} catch {}"),
        "{body}"
    );
    assert!(body.contains("writeRules(config, data)"), "{body}");
    assert_eq!(PLUGIN_SOURCE.matches("writeRules(").count(), 2);
    for writer in [
        "writeFile",
        "appendFile",
        "createWriteStream",
        "Bun.write",
        "mkdir",
        "rename",
        "unlink",
    ] {
        assert!(!PLUGIN_SOURCE.contains(writer), "{writer}");
    }
}

/// The user's inline value with `shell` as the launch's environment beside it, put through a
/// launch.
fn config_with_user_and_env(user: &str, shell: &[(&str, &str)]) -> Value {
    let mut fixture = spec_fixture();
    fixture
        .shell_env
        .insert("OPENCODE_CONFIG_CONTENT".to_string(), user.to_string());
    for (name, value) in shell {
        fixture
            .shell_env
            .insert((*name).to_string(), (*value).to_string());
    }
    config_of(&OpenCodeProvider.plan(&fixture.spec()).expect("plan"))
}

/// A value that is JSON only once OpenCode has put a variable's value in place of a `{env:…}`
/// written outside a string is read with the launch's variables in place, as OpenCode reads it,
/// and merged: a quoted string, a whole object, a number. One written inside a string beside it
/// is put in place too.
#[test]
fn a_value_that_is_json_only_after_substitution_is_merged() {
    let config = config_with_user_and_env(
        r#"{"theme": {env:THEME}, "username": "{env:NAME}", "plugin": ["/me/p.js"]}"#,
        &[("THEME", "\"tokyonight\""), ("NAME", "me")],
    );
    assert_eq!(config["theme"], "tokyonight");
    assert_eq!(config["username"], "me");
    assert_eq!(config["plugin"][0], "/me/p.js");
    assert_eq!(config["mcp"]["octoboard"]["type"], "local");

    let config = config_with_user_and_env("{env:WHOLE}", &[("WHOLE", r#"{"theme": "whole"}"#)]);
    assert_eq!(config["theme"], "whole");

    let config = config_with_user_and_env(
        r#"{"snapshot": {env:FLAG}, "theme": "x"}"#,
        &[("FLAG", "false")],
    );
    assert_eq!(config["snapshot"], false);
    assert_eq!(config["theme"], "x");
}

/// A value that reads as it is written is kept as written: a `{env:…}` inside a string is left for
/// OpenCode to put in, whatever the launch's environment holds.
#[test]
fn a_value_that_reads_as_written_keeps_its_variables_for_opencode() {
    let config = config_with_user_and_env(
        r#"{"username": "{env:NAME}", "theme": "x"}"#,
        &[("NAME", "me")],
    );
    assert_eq!(config["username"], "{env:NAME}");
}

/// The substitution is OpenCode's (1.19.6): once over the text, nothing for a variable that is not
/// set, the name as written, a value that holds a token left as it is, a token left open or with
/// no name kept.
#[test]
fn variables_are_put_in_place_as_opencode_does() {
    let env: std::collections::HashMap<String, String> = [
        ("A", "{env:B}"),
        ("B", "inner"),
        ("EMPTY", ""),
        ("Q", "\"q\""),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_string(), value.to_string()))
    .collect();
    for (text, expected) in [
        ("{env:Q}", "\"q\""),
        ("a{env:NOPE}b", "ab"),
        ("a{env:EMPTY}b", "ab"),
        ("{env:A}", "{env:B}"),
        ("{env: B }", ""),
        ("{env:B}{env:B}", "innerinner"),
        ("x {env:B", "x {env:B"),
        ("{env:}", "{env:}"),
        ("{env:}{env:B}", "{env:}inner"),
        ("no token", "no token"),
    ] {
        assert_eq!(with_variables(text, &env), expected, "{text}");
    }
}

/// A value that is no object even with its variables in place is dropped whole, as one OpenCode
/// refuses: a variable that is not set, or one whose value is not JSON there.
#[test]
fn a_value_that_is_not_json_after_substitution_is_dropped() {
    for (user, shell) in [
        (r#"{"theme": "x", "model": {env:MODEL}}"#, vec![]),
        (
            r#"{"theme": "x", "model": {env:MODEL}}"#,
            vec![("MODEL", "bare words")],
        ),
        ("{env:WHOLE}", vec![("WHOLE", "[1]")]),
    ] {
        let config = config_with_user_and_env(user, &shell);
        assert_eq!(config.get("theme"), None, "{user} {shell:?}");
    }
}

/// A launch whose working directory and home are directories of its own in the scratch directory,
/// each holding `files` (a path below it and the content).
fn fixture_with_files(in_cwd: &[(&str, &str)], in_home: &[(&str, &str)]) -> SpecFixture {
    let mut fixture = spec_fixture();
    let cwd = fixture.scratch.join("work");
    let home = fixture.scratch.join("home");
    for (dir, files) in [(&cwd, in_cwd), (&home, in_home)] {
        std::fs::create_dir_all(dir).expect("dir");
        for (name, content) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("parent");
            std::fs::write(path, content).expect("file");
        }
    }
    fixture
        .shell_env
        .insert("HOME".to_string(), home.to_string_lossy().into_owned());
    fixture.cwd = cwd;
    fixture
}

/// The configuration `fixture` launches with when the user's shell carries `user` inline.
fn config_of_user(fixture: &mut SpecFixture, user: &str) -> Value {
    fixture
        .shell_env
        .insert("OPENCODE_CONFIG_CONTENT".to_string(), user.to_string());
    config_of(&OpenCodeProvider.plan(&fixture.spec()).expect("plan"))
}

/// A value that is JSON only once OpenCode has put a file's content in place of a `{file:…}`
/// written outside a string is read with the file in place, as OpenCode reads it, and merged: the
/// path relative to the working directory, absolute, or below the home for `~/`; the content
/// trimmed, a byte order mark included.
#[test]
fn a_value_that_is_json_only_with_a_file_in_place_is_merged() {
    let mut fixture = fixture_with_files(
        &[
            ("sub/flag.txt", " false\n\n"),
            ("bom.txt", "\u{feff}true \n"),
        ],
        &[("vals/port.txt", "4096\n")],
    );
    let absolute = fixture.cwd.join("sub/flag.txt");
    for (user, key, expected) in [
        (
            r#"{"snapshot": {file:sub/flag.txt}, "theme": "x"}"#.to_string(),
            "snapshot",
            json!(false),
        ),
        (
            r#"{"snapshot": {file:./sub/../sub/flag.txt}, "theme": "x"}"#.to_string(),
            "snapshot",
            json!(false),
        ),
        (
            format!(
                r#"{{"snapshot": {{file:{}}}, "theme": "x"}}"#,
                absolute.display()
            ),
            "snapshot",
            json!(false),
        ),
        (
            r#"{"autoupdate": {file:bom.txt}, "theme": "x"}"#.to_string(),
            "autoupdate",
            json!(true),
        ),
        (
            r#"{"server": {"port": {file:~/vals/port.txt}}, "theme": "x"}"#.to_string(),
            "server",
            json!({ "port": 4096 }),
        ),
    ] {
        let config = config_of_user(&mut fixture, &user);
        assert_eq!(config[key], expected, "{user}");
        assert_eq!(config["theme"], "x", "{user}");
        assert_eq!(config["mcp"]["octoboard"]["type"], "local", "{user}");
    }
}

/// The variables go in before the files, as in OpenCode: a path can come from a variable, and so
/// can a whole token.
#[test]
fn a_file_named_through_a_variable_is_put_in_place() {
    let mut fixture = fixture_with_files(&[("flag.txt", "true")], &[]);
    fixture
        .shell_env
        .insert("FLAG_PATH".to_string(), "flag.txt".to_string());
    fixture
        .shell_env
        .insert("FLAG".to_string(), "{file:flag.txt}".to_string());
    for user in [
        r#"{"snapshot": {file:{env:FLAG_PATH}}}"#,
        r#"{"snapshot": {env:FLAG}}"#,
    ] {
        let config = config_of_user(&mut fixture, user);
        assert_eq!(config["snapshot"], true, "{user}");
    }
}

/// A `{file:…}` inside a string is left for OpenCode to put in, also in a value that needs
/// another file outside a string: the content of a file named there never reaches the launch's
/// environment. A value that reads as written is not read for files at all, so a file it names
/// that is missing does not drop it.
#[test]
fn a_file_named_inside_a_string_is_left_for_opencode() {
    let mut fixture = fixture_with_files(&[("flag.txt", "true"), ("key.txt", "sk-secret")], &[]);
    let config = config_of_user(
        &mut fixture,
        r#"{"snapshot": {file:flag.txt}, "provider": {"x": {"options": {"apiKey": "{file:key.txt}"}}}}"#,
    );
    assert_eq!(config["snapshot"], true);
    assert_eq!(
        config["provider"]["x"]["options"]["apiKey"],
        "{file:key.txt}"
    );

    let config = config_of_user(
        &mut fixture,
        r#"{"username": "{file:gone.txt}", "theme": "x"}"#,
    );
    assert_eq!(config["username"], "{file:gone.txt}");
    assert_eq!(config["theme"], "x");
}

/// The files are put in as OpenCode (1.19.6) puts them: the content escaped as the inside of a
/// string and not read for tokens again; the name as written, a space included; `~` standing for
/// the home only as `~/`; a token on a line that starts with `//`, in a comment and inside a
/// string left alone, an escaped quote not ending the string; a token with no name or left open
/// kept.
#[test]
fn files_are_put_in_place_as_opencode_does() {
    let fixture = fixture_with_files(
        &[
            ("n.txt", " 7 \n"),
            ("quoted.txt", "\"q\"\\\n"),
            ("token.txt", "{file:n.txt}"),
            (" spaced.txt", "1"),
        ],
        &[("h.txt", "8")],
    );
    let home = fixture.scratch.join("home");
    for (text, expected) in [
        ("[{file:n.txt}]", Some("[7]")),
        ("[{file:n.txt},{file:~/h.txt}]", Some("[7,8]")),
        ("{file:quoted.txt}", Some(r#"\"q\"\\"#)),
        ("{file:token.txt}", Some("{file:n.txt}")),
        ("{file: spaced.txt}", Some("1")),
        ("\"{file:n.txt}\" {file:n.txt}", Some("\"{file:n.txt}\" 7")),
        (
            r#""a\"{file:n.txt}" {file:n.txt}"#,
            Some(r#""a\"{file:n.txt}" 7"#),
        ),
        (
            "/* {file:gone.txt} */ {file:n.txt}",
            Some("/* {file:gone.txt} */ 7"),
        ),
        (
            "1 // {file:gone.txt}\n{file:n.txt}",
            Some("1 // {file:gone.txt}\n7"),
        ),
        ("  // c\r{file:gone.txt}", Some("  // c\r{file:gone.txt}")),
        ("{file:}", Some("{file:}")),
        ("x {file:n.txt", Some("x {file:n.txt")),
        ("no token", Some("no token")),
        ("{file:gone.txt}", None),
        ("{file:~h.txt}", None),
        ("{file:.}", None),
    ] {
        assert_eq!(
            with_files(text, Some(home.as_path()), &fixture.cwd).as_deref(),
            expected,
            "{text}"
        );
    }
    assert_eq!(with_files("{file:~/h.txt}", None, &fixture.cwd), None);
}

/// A value that names a file outside a string that cannot be read, or whose content is not JSON
/// there, is dropped whole, as one OpenCode refuses: a missing file, a file holding a quoted
/// string (its quotes are escaped), a file over the bound.
#[test]
fn a_value_whose_file_cannot_be_read_or_is_not_json_is_dropped() {
    let large = "1".repeat(VALUE_FILE_LIMIT + 1);
    let mut fixture = fixture_with_files(&[("quoted.txt", "\"dark\""), ("large.txt", &large)], &[]);
    for user in [
        r#"{"theme": "x", "snapshot": {file:gone.txt}}"#,
        r#"{"theme": {file:quoted.txt}, "username": "me"}"#,
        r#"{"theme": "x", "snapshot": {file:large.txt}}"#,
        r#"{"theme": "x", "snapshot": {file:~/gone.txt}}"#,
    ] {
        let config = config_of_user(&mut fixture, user);
        assert_eq!(config.get("theme"), None, "{user}");
        assert_eq!(config.get("username"), None, "{user}");
    }
}

/// JavaScript's `trim`, which OpenCode trims a file's content with, takes a byte order mark and a
/// no-break space as white space and U+0085 not.
#[test]
fn a_files_content_is_trimmed_as_javascript_trims() {
    assert_eq!(js_trimmed("\u{feff}\u{a0}\t 1\u{3000}\u{2028}\r\n"), "1");
    assert_eq!(js_trimmed("\u{85}1\u{85}"), "\u{85}1\u{85}");
    assert_eq!(js_trimmed(" \n"), "");
}

fn lenient(text: &str) -> Option<Value> {
    user_json::parse(text, CONFIG_TAKES).ok()
}

/// What OpenCode 1.19.6 refuses in its inline value, measured with `opencode debug config`, is
/// refused here: a comma that follows no value, and a byte order mark. A line comment ends at a
/// carriage return as at a line feed, as OpenCode's does.
#[test]
fn what_opencode_refuses_is_refused_and_a_comment_ends_at_a_carriage_return() {
    for text in [
        r#"{"a": [,]}"#,
        r#"{"a": {,}}"#,
        r#"{"a": ["b",,]}"#,
        "{,}",
        "\u{FEFF}{\"a\": 1}",
    ] {
        assert_eq!(lenient(text), None, "{text}");
    }
    for text in [
        "{\"a\": 1, // c\r\"b\": 2}",
        "{\"a\": 1, // c\r\n\"b\": 2}",
        "{\"a\": 1, // c\n\"b\": 2}",
    ] {
        assert_eq!(lenient(text), Some(json!({ "a": 1, "b": 2 })), "{text:?}");
    }
}

/// OpenCode reads the variable as it reads a configuration file: `//` and `/* */` comments and a
/// comma before a closing bracket or brace are allowed, and read here the same way.
#[test]
fn comments_and_trailing_commas_are_read_as_opencode_reads_them() {
    let expected = Some(json!({ "a": 1, "b": [1, 2], "c": { "d": true } }));
    for text in [
        "{\"a\": 1, // line comment\n \"b\": [1, 2,], \"c\": {\"d\": true,},}",
        "/* leading */ {\"a\": 1 /* mid */, \"b\": [1, /* in */ 2], \"c\": {\"d\": true}} // last",
        "{\"a\": 1,\n \"b\": [1, 2, // end of the list\n ],\n \"c\": {\"d\": true, /* x */ },\n}",
        "{\"a\": 1, \"b\": [1, 2], \"c\": {\"d\": true}}",
    ] {
        assert_eq!(lenient(text), expected, "{text}");
    }
}

/// What is inside a string is left alone: a comment marker, a comma before a closing bracket, an
/// escaped quote ahead of either, and OpenCode's `{env:…}` substitution.
#[test]
fn what_is_inside_a_string_is_left_alone() {
    let text = r#"{
        "url": "http://example.com/*not a comment*/", // a real one
        "slashes": "a // b",
        "block": "/* kept */",
        "comma": ",}",
        "quote": "say \" // still inside \", ]",
        "env": "{env:HOME}/bin", "file": "{file:~/.key}",
    }"#;
    assert_eq!(
        lenient(text),
        Some(json!({
            "url": "http://example.com/*not a comment*/",
            "slashes": "a // b",
            "block": "/* kept */",
            "comma": ",}",
            "quote": "say \" // still inside \", ]",
            "env": "{env:HOME}/bin",
            "file": "{file:~/.key}",
        }))
    );
}

/// A value that still does not read as an object is dropped whole, nothing of it kept: not an
/// object, broken, a substitution written outside a string, or a comment left open.
#[test]
fn a_value_that_still_does_not_parse_is_dropped_whole() {
    let plain = {
        let fixture = spec_fixture();
        let plan = OpenCodeProvider.plan(&fixture.spec()).expect("plan");
        keys_of(&config_of(&plan))
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    for user in [
        r#"{"theme": "x", "broken": }"#,
        r#"{"theme": "x", "model": {env:MODEL}}"#,
        r#"{"theme": "x" /* never closed"#,
        r#"{"theme": "x",, "other": 1}"#,
        r#"["theme"]"#,
        "// only a comment",
    ] {
        let config = config_with_user(user);
        assert_eq!(config.get("theme"), None, "{user}");
        assert_eq!(keys_of(&config), plain.as_slice(), "{user}");
    }
}

/// A user value with comments and trailing commas is merged under Octoboard's like a plain one, and
/// the plugin is given the text as the user wrote it, to hand back.
#[test]
fn a_user_value_with_comments_is_merged_and_handed_back_as_written() {
    let mut fixture = spec_fixture();
    let user = "{ // mine\n  \"theme\": \"x\", /* c */\n  \"plugin\": [\"/me/p.js\",],\n}";
    fixture
        .shell_env
        .insert("OPENCODE_CONFIG_CONTENT".to_string(), user.to_string());
    let plan = OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    let config = config_of(&plan);
    assert_eq!(config["theme"], "x");
    assert_eq!(
        paths_in(&config["plugin"]),
        ["/me/p.js".to_string(), written(&fixture, "octoboard.js")]
    );
    assert!(plugin_data(&fixture).get("user_config_content").is_none());
    assert!(plan.env.contains(&(ORIGINAL_CONFIG.into(), user.into())));
}

// What the plugin does at runtime has no harness here (no JavaScript is run by the daemon's
// tests), so the checks below read its source. They hold what is written, not what it does: a
// logic error that keeps the text (a comparison turned round, a guard on the wrong branch) passes
// them, and the replay of its output in `hooks.rs` is the only test of what it forwards. The
// rules its `config` hook writes are tested by running it, in `plugin.test.mjs` beside it, which
// `node --test` runs and the daemon's tests do not.

/// The function of the plugin's source named `name`, from its declaration to the next line that is
/// a closing brace at the declaration's own indent.
fn function_body<'a>(plugin: &'a str, declaration: &str) -> &'a str {
    let start = plugin
        .find(declaration)
        .unwrap_or_else(|| panic!("{declaration} is declared"));
    let indent = plugin[..start].rsplit('\n').next().expect("a line").len();
    let closing = format!("\n{}}}", " ".repeat(indent));
    let end = plugin[start..].find(&closing).expect("a closing brace");
    &plugin[start..start + end]
}

/// The states the plugin's source reports to the terminal: the literal of every `report("…")`.
fn reported_states(plugin: &str) -> Vec<String> {
    plugin
        .split("report(\"")
        .skip(1)
        .map(|rest| rest.split('"').next().expect("a literal").to_string())
        .collect()
}

/// What the plugin writes as a report, once for a state: its template with the state in place.
fn written_report(plugin: &str, state: &str) -> Vec<u8> {
    let template = function_body(plugin, "function report(state)");
    assert!(
        template.contains(r"`\u001b]7501;state=${state}\u001b\\`"),
        "{template}"
    );
    format!("\x1b]7501;state={state}\x1b\\").into_bytes()
}

/// Every state the plugin reports is one the terminal's parser takes, written as one OSC 7501
/// report that its scanner cuts out whole: working, idle, and blocked on a permission or a
/// question. (A source scan: it checks the states written, not when each is.)
#[test]
fn every_report_the_plugin_writes_is_one_the_terminal_reads() {
    use crate::terminal::program_status::{parse, BlockedKind, Record, Report, ReportedState};
    use crate::terminal::queries::{Piece, Query, QueryScanner};

    let states = reported_states(PLUGIN_SOURCE);
    for expected in [
        "working",
        "idle",
        "blocked:kind=permission",
        "blocked:kind=question",
    ] {
        assert!(
            states.iter().any(|state| state == expected),
            "{expected}: {states:?}"
        );
    }
    let record = |state, kind| Report::Set {
        id: None,
        record: Record {
            state,
            kind,
            progress: None,
            app: None,
            title: None,
            msg: None,
        },
    };
    for state in &states {
        let wire = written_report(PLUGIN_SOURCE, state);
        let pieces = QueryScanner::default().scan(&wire);
        let [Piece::Sequence(Query::ProgramStatusReport(body), cut)] = pieces.as_slice() else {
            panic!("{state}: {pieces:?}");
        };
        assert_eq!(cut, &wire, "{state}");
        let expected = match state.as_str() {
            "working" => record(ReportedState::Working, None),
            "idle" => record(ReportedState::Idle, None),
            "blocked:kind=permission" => {
                record(ReportedState::Blocked, Some(BlockedKind::Permission))
            }
            "blocked:kind=question" => record(ReportedState::Blocked, Some(BlockedKind::Question)),
            other => panic!("the plugin reports a state this test does not know: {other}"),
        };
        assert_eq!(parse(body), Some(expected), "{state}");
    }
}

/// The plugin writes to standard output in one place, the report, and prints nothing else there,
/// where OpenCode's own drawing goes. (A source scan: a second route to the screen by another name
/// is not found.)
#[test]
fn the_plugin_writes_to_standard_output_only_its_reports() {
    for forbidden in ["console.log", "console.info", "process.stdout", "print("] {
        assert!(!PLUGIN_SOURCE.contains(forbidden), "{forbidden}");
    }
    assert_eq!(PLUGIN_SOURCE.matches("writeSync(").count(), 1);
    assert!(function_body(PLUGIN_SOURCE, "function report(state)").contains("writeSync(1,"));
}

/// A clipped text never ends in the first half of a character outside the basic plane: a lone
/// surrogate is one the daemon's JSON reader refuses, and the payload with it. (A source scan: it
/// finds the check on the code unit at the cut, not that the cut is right.)
#[test]
fn the_plugin_does_not_clip_between_a_surrogate_pair() {
    let clip = function_body(PLUGIN_SOURCE, "function clip(text)");
    assert!(clip.contains("charCodeAt(TEXT_LIMIT - 1)"), "{clip}");
    assert!(clip.contains("0xd800") && clip.contains("0xdbff"), "{clip}");
    assert!(clip.contains("? TEXT_LIMIT - 1 : TEXT_LIMIT"), "{clip}");
}

/// An answer to a prompt or a question is forwarded only while the root session's turn is open:
/// the check comes before the forward, so an answer after the turn's end never puts a session at
/// rest back at work. (A source scan: it finds the guard ahead of the forward, not that the
/// condition is the right one.)
#[test]
fn the_plugin_forwards_a_reply_only_while_the_roots_turn_is_open() {
    let start = PLUGIN_SOURCE
        .find(r#"case "permission.replied":"#)
        .expect("the replies are handled");
    let block = &PLUGIN_SOURCE[start..];
    let block = &block[..block.find("default:").expect("the end of the switch")];
    let guard = block
        .find("!turn.open")
        .expect("a guard on the turn being open");
    let forward = block.find("forward(").expect("the reply is forwarded");
    assert!(guard < forward, "{block}");
    assert!(block[..guard].contains("turns.get(root)"), "{block}");
    assert!(block[guard..forward].contains("return"), "{block}");
}

/// A permission prompt or a question is told of from a timer of the bound's length, which is the
/// only place it is forwarded from, and its answer stops that timer: one OpenCode answers itself
/// under `--auto` is over before anyone is told the session waits. (A source scan: the replay of
/// such a launch's payloads in `hooks.rs` is the test of what is forwarded.)
#[test]
fn the_plugin_tells_of_a_prompt_only_once_it_has_stayed_open() {
    let start = PLUGIN_SOURCE
        .find(r#"case "permission.asked":"#)
        .expect("the prompts are handled");
    let end = PLUGIN_SOURCE
        .find(r#"case "permission.replied":"#)
        .expect("the replies are handled");
    let asked = &PLUGIN_SOURCE[start..end];
    let timer = asked.find("setTimeout(").expect("a timer");
    let forward = asked
        .find("forward(payload)")
        .expect("the prompt is forwarded");
    assert_eq!(asked.matches("forward(").count(), 1, "{asked}");
    assert!(timer < forward, "{asked}");
    assert!(asked[forward..].contains("}, PROMPT_BOUND_MS);"), "{asked}");
    let replied = &PLUGIN_SOURCE[end..];
    assert!(replied.contains("clearTimeout(prompt.timer)"));
}

/// The hook script's path is quoted for a Windows shell, which runs the batch file, and only there.
/// (A source scan: the quoting is found, not run on Windows.)
#[test]
fn the_plugin_quotes_the_hook_path_for_a_windows_shell() {
    assert!(PLUGIN_SOURCE.contains(r#"process.platform === "win32""#));
    assert!(PLUGIN_SOURCE.contains(r#"windows ? `"${data.hook}"` : data.hook"#));
    assert!(PLUGIN_SOURCE.contains("shell: windows"));
}

/// The shells the agent starts get the user's own configuration back, and the user's own setting of
/// the title variable where the shell set one. (A source scan.)
#[test]
fn the_plugin_hands_the_shells_own_values_back() {
    let hook = PLUGIN_SOURCE
        .find(r#""shell.env""#)
        .expect("the shell.env hook");
    let body = &PLUGIN_SOURCE[hook..];
    assert!(body.contains("output.env.OPENCODE_CONFIG_CONTENT = "));
    assert!(body.contains("process.env.OCTOBOARD_OPENCODE_ORIGINAL_CONFIG"));
    assert!(body.contains("output.env.OPENCODE_DISABLE_TERMINAL_TITLE = data.user_title_off"));
}

/// Every key of the data file the launch writes is one the plugin reads, and the plugin reads no
/// other: the two cannot drift apart unnoticed.
#[test]
fn the_plugin_reads_exactly_the_keys_the_launch_writes() {
    let fixture = spec_fixture();
    OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    let data: Value = serde_json::from_str(
        &std::fs::read_to_string(injection_dir(&fixture).join("octoboard.json")).expect("data"),
    )
    .expect("JSON");
    let written: Vec<&str> = keys_of(&data);
    for key in &written {
        assert!(PLUGIN_SOURCE.contains(&format!("data.{key}")), "{key}");
    }
    // The properties read off `data` itself: not those of another object's own `data`.
    let mut read: Vec<&str> = PLUGIN_SOURCE
        .match_indices("data.")
        .filter(|(at, _)| {
            !PLUGIN_SOURCE[..*at]
                .ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        })
        .map(|(at, _)| {
            let rest = &PLUGIN_SOURCE[at + "data.".len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            &rest[..end]
        })
        .collect();
    read.sort_unstable();
    read.dedup();
    let mut written_sorted = written.clone();
    written_sorted.sort_unstable();
    assert_eq!(read, written_sorted);
}

/// One session of OpenCode 1.19.6 at 30x40, each dialog opened in turn
/// (`testdata/opencode-captures.md`, "Lists and dialogs").
const DIALOGS_AT_30_COLUMNS: &[u8] =
    include_bytes!("../../../testdata/opencode_screen_dialogs_30x40_1.19.6.bin");
/// Where the agent list stood open in it (`Select agent` drawn from byte 60154, the theme list
/// from 69814): its title row has five blanks before `esc`, fewer than a dialog is recognised by.
const AT_AGENTS_30: usize = 63793;

/// Below 40 columns a dialog can be up unrecognised, so a message is written into it rather than
/// held: OpenCode takes it into the dialog and submits no prompt, so its plugin reports none, and
/// with the box under the dialog empty the message went elsewhere: its sender gets the
/// `message_not_delivered` error, so it may send it again, and the user the notice of that code;
/// nothing holds the queue, and no submit key is pressed again into the dialog.
#[test]
fn a_message_written_into_a_dialog_not_recognised_is_told_not_delivered() {
    use crate::messaging::delivery::{write_message, WhenBlocked};
    use crate::protocol::{Event, Session, SessionStatus};

    let (state, dir) = crate::test_support::app_state("opencode-unrecognised-dialog");
    let console = crate::test_support::console(&dir);
    state.store.insert_console(&console).unwrap();
    state
        .store
        .insert_session(&Session {
            agent: AgentId::OPENCODE,
            ..crate::state::test_support::session_record("s", SessionStatus::Idle)
        })
        .unwrap();
    let live = crate::test_support::fake_live_session("s", AgentId::OPENCODE, 30, 40, "sleep 30");
    state.register_live(live.clone());
    *state.send_wait.lock().unwrap() = Some(std::time::Duration::ZERO);
    live.on_output(&DIALOGS_AT_30_COLUMNS[..AT_AGENTS_30]);
    // Still since well after it drew the list, so the screen reads as a whole frame.
    live.settle_input();
    let drawn = live.screen_as(crate::session::screen::ScreenText::AsDrawn);
    assert!(
        drawn.contains("Select agent     esc"),
        "the agent list is open: {drawn}"
    );
    assert_eq!(
        live.look_for_dialog().current,
        None,
        "the agent list is not recognised"
    );
    // The box is drawn whole under it, and empty.
    assert_eq!(
        live.input_box_rows(&live.box_screen()),
        Some(vec![String::new()])
    );
    let mut events = state.subscribe();
    let writes = live.input_writes();

    let delivery = write_message(&state, "s", "please do the next step", WhenBlocked::Queue);

    let error = delivery.expect_err("the message is not delivered");
    let coded = error
        .downcast_ref::<crate::protocol::CodedError>()
        .expect("a coded error");
    assert_eq!(
        coded.code,
        crate::protocol::error_code::MESSAGE_NOT_DELIVERED
    );
    assert_eq!(
        live.input_writes(),
        writes + 1,
        "written once, pressed no more"
    );
    assert!(!live.unsent_waiting(), "nothing holds the queue");
    let notices: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            Event::SessionNotice { session, code, .. } if session == "s" => {
                Some(code.as_str().to_string())
            }
            _ => None,
        })
        .collect();
    assert!(
        notices.iter().any(|code| code == "message_not_delivered"),
        "{notices:?}"
    );
    assert!(
        !notices.iter().any(|code| code == "message_not_sent"),
        "{notices:?}"
    );
}

/// With a cross-project guard the plugin's data names the guard's script, the tools it is asked
/// about and its deadline; without one it names no script, so the plugin asks about nothing.
#[test]
fn the_plugin_is_handed_the_guard_and_the_tools_that_write_a_file() {
    let fixture = spec_fixture();
    let list = fixture.scratch.join("cross-project-guard.json");
    let script = fixture.scratch.join("guard-hook");
    let mut spec = fixture.spec();
    spec.guard = Some(crate::adapter::GuardHook {
        exe: "/opt/octoboard/install/octoboardd",
        list: &list,
        script: &script,
    });
    OpenCodeProvider.plan(&spec).expect("plan");
    let data = plugin_data(&fixture);
    assert_eq!(
        data["guard"].as_str(),
        Some(script.to_string_lossy().as_ref())
    );
    assert_eq!(data["guard_tools"], json!(["write", "edit", "apply_patch"]));
    assert_eq!(
        data["guard_timeout_ms"],
        json!(crate::adapter::GUARD_HOOK_TIMEOUT_SECS * 1000)
    );

    let fixture = spec_fixture();
    OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    assert_eq!(plugin_data(&fixture)["guard"], Value::Null);
}

#[cfg(windows)]
#[test]
fn the_npm_launcher_resolves_to_the_executable_beside_its_package_manifest() {
    use crate::agents::cli::install::Install;
    use crate::platform::shell::ResolvedBinary;
    use crate::protocol::CliInstallMethod;

    let prefix = crate::test_support::ScratchDir::new("opencode-npm");
    let launcher = prefix.join("opencode.cmd");
    let package = prefix.join("node_modules/opencode-ai");
    let program = package.join("bin/opencode.exe");
    std::fs::create_dir_all(program.parent().unwrap()).unwrap();
    std::fs::write(&launcher, "").unwrap();
    std::fs::write(&program, "").unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"opencode-ai","version":"1.19.6"}"#,
    )
    .unwrap();

    let resolved = OpenCodeProvider
        .native_binary(&ResolvedBinary::new(launcher))
        .unwrap()
        .expect("the npm launcher names the native program");
    assert_eq!(resolved.program, program);
    assert_eq!(
        OpenCodeProvider.installed_version(resolved.program.to_str().unwrap()),
        Some((1, 19, 6))
    );
    let install = Install::of_binary(OpenCodeProvider.cli_packaging(), &resolved);
    assert_eq!(install.method, CliInstallMethod::Npm);
    assert_eq!(install.npm_package(), Some("opencode-ai"));
}

/// The version is read from the npm package the program is in (its own, or the platform package
/// it carries the program in, as 1.19.6 installs), or from the Homebrew cellar's directory; the
/// installer's bare program, and a package of another name, tell none.
#[test]
fn an_install_tells_its_version_from_its_package_or_its_cellar() {
    let fixture = crate::test_support::ScratchDir::new("opencode-installed-version");
    let program = |relative: &str, manifest: Option<(&str, &str)>| -> PathBuf {
        let path = fixture.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "").unwrap();
        if let Some((name, version)) = manifest {
            let package = path.parent().unwrap().parent().unwrap();
            std::fs::write(
                package.join("package.json"),
                json!({ "name": name, "version": version }).to_string(),
            )
            .unwrap();
        }
        path
    };
    let version = |path: PathBuf| OpenCodeProvider.installed_version(path.to_str().unwrap());

    let npm = program(
        "lib/node_modules/opencode-ai/bin/opencode.exe",
        Some(("opencode-ai", "1.19.6")),
    );
    assert_eq!(version(npm), Some((1, 19, 6)));
    let npm_v2 = program(
        "lib/node_modules/@opencode/cli/bin/opencode.exe",
        Some(("@opencode/cli", "2.4.1")),
    );
    assert_eq!(version(npm_v2), Some((2, 4, 1)));
    let platform = program(
        "lib/node_modules/opencode-ai/node_modules/opencode-darwin-arm64/bin/opencode",
        Some(("opencode-darwin-arm64", "1.19.7")),
    );
    assert_eq!(version(platform), Some((1, 19, 7)));
    let brew = program("Cellar/opencode/2.3.2/bin/opencode", None);
    assert_eq!(version(brew), Some((2, 3, 2)));
    let revised = program("Cellar/opencode/2.3.3_1/bin/opencode", None);
    assert_eq!(version(revised), Some((2, 3, 3)));

    assert_eq!(version(program("home/.opencode/bin/opencode", None)), None);
    let other = program(
        "lib/node_modules/other/bin/opencode",
        Some(("something-else", "9.9.9")),
    );
    assert_eq!(version(other), None);
    assert_eq!(
        version(program("Cellar/other/1.0.0/bin/opencode", None)),
        None
    );
}

#[test]
fn v2_loads_a_launch_scoped_plugin_directory_and_private_server() {
    let mut fixture = spec_fixture();
    fixture.agent_version = Some((2, 4, 1));
    fixture.shell_env.insert(
        "HOME".into(),
        fixture.scratch.join("home").display().to_string(),
    );
    fixture.shell_env.insert(CONFIG_CONTENT_VAR.into(), r#"{"plugins":["theirs"],"permissions":[{"action":"edit","resource":"*","effect":"deny"}]}"#.into());
    let plan = OpenCodeProvider.plan(&fixture.spec()).expect("plan");
    let config = config_of(&plan);
    let plugins = paths_in(&config["plugins"]);
    assert_eq!(plugins[0], "theirs");
    assert!(Path::new(&plugins[1]).join("index.js").is_file());
    assert!(Path::new(&plugins[1]).join("tui.js").is_file());
    assert_eq!(config["permissions"][0]["effect"], "deny");
    assert!(config.get("plugin").is_none());
    assert_eq!(plan.args, ["--standalone"]);
    assert!(plan.private_server.is_some());
    assert_eq!(plan.adoption.unwrap()["opencode_api"], 2);
}

#[test]
fn an_unmeasured_version_is_refused_before_injection() {
    let mut fixture = spec_fixture();
    fixture.agent_version = Some((3, 0, 0));
    let Err(error) = OpenCodeProvider.plan(&fixture.spec()) else {
        panic!("unsupported CLI was admitted");
    };
    assert_eq!(
        CodedError::of(&error).code,
        crate::protocol::Code::of_provider(crate::providers::opencode::UNSUPPORTED_VERSION),
    );
    assert!(!injection_dir(&fixture).exists());
}
