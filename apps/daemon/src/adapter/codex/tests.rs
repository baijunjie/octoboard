//! Tests of the Codex adapter's launch plan and its title and config readers.

use std::collections::BTreeSet;

use super::config::upgrade_table;
use super::*;
use crate::adapter::files::SMALL_FILE_LIMIT;
use crate::adapter::tests::spec_fixture;
use crate::records::line_file::JSON_FILE_LIMIT;

#[test]
fn verified_private_server_keeps_client_and_server_overrides() {
    let mut fixture = spec_fixture();
    fixture.agent_version = Some((0, 163, 0));
    fixture.model = Some("probe-model".to_string());
    fixture.effort = Some("high".to_string());
    let mut spec = fixture.spec();
    spec.task = Some("first task");
    let plan = CodexAdapter.plan(&spec).unwrap();
    let private = plan.codex_server.as_ref().unwrap();
    assert_eq!(overrides(&private.args), overrides(&plan.args));
    assert_eq!(private.tui_args, plan.args);
    assert_eq!(&private.tui_args[..2], ["--cd", spec.cwd.to_str().unwrap()]);
    assert_eq!(
        private.tui_args.last().map(String::as_str),
        Some("first task")
    );
    assert_eq!(private.hook_command, spec.hook_script.to_string_lossy());
    spec.resume_agent_session_id = Some("thread-to-resume");
    let resumed = CodexAdapter.plan(&spec).unwrap();
    let private = resumed.codex_server.unwrap();
    assert_eq!(private.resume_thread.as_deref(), Some("thread-to-resume"));
    assert_eq!(private.tui_args, resumed.args);
    assert_eq!(&private.tui_args[..2], ["resume", "thread-to-resume"]);
    assert_eq!(
        &private.tui_args[2..4],
        ["--cd", spec.cwd.to_str().unwrap()]
    );
    fixture.agent_version = None;
    assert!(CodexAdapter
        .plan(&fixture.spec())
        .unwrap()
        .codex_server
        .is_none());
    fixture.agent_version = Some((0, 164, 0));
    assert!(CodexAdapter
        .plan(&fixture.spec())
        .unwrap()
        .codex_server
        .is_none());
}

#[test]
fn standalone_installs_report_the_version_from_the_resolved_package() {
    let fixture = crate::test_support::ScratchDir::new("codex-installed-version");
    std::fs::create_dir_all(fixture.join("release/bin")).unwrap();
    std::fs::write(fixture.join("release/bin/codex"), "").unwrap();
    std::fs::write(
        fixture.join("release/codex-package.json"),
        r#"{"version":"0.163.0"}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(fixture.join("release/bin/codex"), fixture.join("codex")).unwrap();
    assert_eq!(
        CodexAdapter.installed_version(fixture.join("codex").to_str().unwrap()),
        Some((0, 163, 0))
    );
}

#[test]
fn terminal_titles_keep_only_the_task_between_the_spinner_and_the_project() {
    let names = ["proj".to_owned()];
    let task = |raw| CodexAdapter.task_title(raw, &names);
    for raw in [
        "proj",
        "⠧ ⠧ | proj",
        "⠙ | proj",
        "| proj",
        "[ ! ] Action Required | proj",
        "[ . ] Action Required | proj",
        "",
    ] {
        assert_eq!(task(raw), None, "{raw}");
    }
    assert_eq!(
        task("Explain pseudo terminal | proj").as_deref(),
        Some("Explain pseudo terminal")
    );
    assert_eq!(
        task("⠙ | Explain pseudo terminal | proj").as_deref(),
        Some("Explain pseudo terminal")
    );
    assert_eq!(task("Fix a | b").as_deref(), Some("Fix a | b"));
    assert_eq!(
        task("[ x ] Fix it | proj").as_deref(),
        Some("[ x ] Fix it"),
        "only the known status items are dropped"
    );
}

#[test]
fn a_project_name_codex_cut_short_is_still_the_project() {
    let names = ["f1120ca1-f304-4489-aa46-2ac04c2297d1".to_owned()];
    let task = |raw| CodexAdapter.task_title(raw, &names);
    assert_eq!(task("f1120ca1-f304-4489-aa..."), None);
    assert_eq!(task("⠙ | f1120ca1-f304-4489-aa..."), None);
    assert_eq!(
        task("Uninstall codexbar | f1120ca1-f304-4489-aa...").as_deref(),
        Some("Uninstall codexbar")
    );
    assert_eq!(
        task("Fix it | f1120ca1-xxxx...").as_deref(),
        Some("Fix it | f1120ca1-xxxx..."),
        "a cut name must begin the project's"
    );
    assert_eq!(task("Fix it | ...").as_deref(), Some("Fix it | ..."));
    let short = ["proj".to_owned()];
    assert_eq!(
        CodexAdapter.task_title("x | proj...", &short).as_deref(),
        Some("x | proj..."),
        "a name Codex keeps whole is never cut"
    );
}

fn overrides(args: &[String]) -> Vec<String> {
    args.windows(2)
        .filter(|pair| pair[0] == "-c")
        .map(|pair| pair[1].clone())
        .collect()
}

/// With no model or effort set, neither override is passed, so Codex keeps its own choice.
#[test]
fn no_model_or_effort_adds_no_overrides() {
    let fixture = spec_fixture();
    let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
    assert!(!overrides(&plan.args)
        .iter()
        .any(|override_| override_.starts_with("model")));
}

#[test]
fn a_model_and_effort_are_config_overrides_ahead_of_the_task() {
    let mut fixture = spec_fixture();
    fixture.model = Some("gpt-5".to_string());
    fixture.effort = Some("high".to_string());
    let mut spec = fixture.spec();
    spec.task = Some("do it");
    let plan = CodexAdapter.plan(&spec).expect("plan");
    let overrides = overrides(&plan.args);
    assert!(overrides.contains(&"model=\"gpt-5\"".to_string()));
    assert!(overrides.contains(&"model_reasoning_effort=\"high\"".to_string()));
    assert_eq!(plan.args.last().map(String::as_str), Some("do it"));
}

/// Project trust is Codex's own to record when its confirmation is pressed, so it is never
/// passed for one launch; the update prompt, which would come up before that confirmation, is
/// turned off for the launch.
#[test]
fn passes_no_project_trust_and_no_update_check() {
    let fixture = spec_fixture();
    let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
    let overrides = overrides(&plan.args);
    assert!(
        !overrides
            .iter()
            .any(|override_| override_.starts_with("projects")),
        "{overrides:?}"
    );
    assert!(overrides.contains(&"check_for_update_on_startup=false".to_string()));
}

#[test]
fn injects_one_hook_override_per_event_and_gets_past_the_trust_gate() {
    let fixture = spec_fixture();
    let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
    let overrides = overrides(&plan.args);
    for event in HOOK_EVENTS {
        assert_eq!(
            overrides
                .iter()
                .filter(|override_| override_.starts_with(&format!("hooks.{event}=")))
                .count(),
            1,
            "exactly one override for {event}"
        );
    }
    for override_ in overrides.iter().filter(|o| o.starts_with("hooks.")) {
        assert!(override_.contains("async=true"));
        assert!(override_.contains(&format!("timeout={HOOK_TIMEOUT_SECS}")));
    }
    // Without clearing the hook trust gate an interactive session raises a blocking review
    // modal and a headless one hangs indefinitely.
    assert!(plan
        .args
        .iter()
        .any(|arg| arg == "--dangerously-bypass-hook-trust"));
}

#[test]
fn a_resume_names_the_subcommand_first_and_still_re_injects() {
    let fixture = spec_fixture();
    let mut spec = fixture.spec();
    spec.resume_agent_session_id = Some("thread-id");
    let plan = CodexAdapter.plan(&spec).expect("plan");
    assert_eq!(plan.args[0], "resume");
    assert_eq!(plan.args[1], "thread-id");
    // Every `-c` override is per-invocation and persisted nowhere, so a resume that omits them
    // loses the hooks and the MCP server.
    let overrides = overrides(&plan.args);
    assert!(overrides.iter().any(|o| o.starts_with("hooks.")));
    assert!(overrides
        .iter()
        .any(|o| o.starts_with("mcp_servers.octoboard.command=")));
    assert!(overrides
        .iter()
        .any(|o| o.starts_with("developer_instructions=")));
}

#[test]
fn registers_the_mcp_server_as_a_command_with_its_tools_pre_approved() {
    let fixture = spec_fixture();
    let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
    let overrides = overrides(&plan.args);
    assert!(overrides.contains(&format!(
        "mcp_servers.octoboard.command={:?}",
        fixture.self_exe
    )));
    let args = overrides
        .iter()
        .find(|o| o.starts_with("mcp_servers.octoboard.args="))
        .expect("the server's arguments");
    assert!(args.contains(&fixture.session_id));
    assert!(args.contains(&fixture.mcp_token));
    assert!(overrides
        .contains(&"mcp_servers.octoboard.default_tools_approval_mode=\"approve\"".to_string()));
}

/// Codex abandons a tool call at a limit of its own, silently, and its default would end a long
/// Octoboard call first (the "Codex" section of `docs/agent-cli-reference.md` has the value);
/// the override puts it above Octoboard's forwarding limit, for this server only.
#[test]
fn the_servers_tool_timeout_sits_above_octoboards_own_call_limit() {
    let fixture = spec_fixture();
    let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
    let overrides = overrides(&plan.args);
    assert!(overrides.contains(&"mcp_servers.octoboard.tool_timeout_sec=660".to_string()));
    assert!(!overrides.iter().any(|o| o.starts_with("tool_timeout_sec")));
}

/// `instructions` would replace the system prompt outright; `developer_instructions` adds a
/// developer message and leaves the rest byte-identical.
#[test]
fn the_role_text_is_added_as_a_developer_message_not_a_replaced_system_prompt() {
    let fixture = spec_fixture();
    let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
    let overrides = overrides(&plan.args);
    assert!(overrides.iter().any(|o| o.starts_with(&format!(
        "developer_instructions={}",
        toml_string(&session_tools::role::role_description(
            fixture.role,
            fixture.owner.as_ref(),
            Agent::Codex,
            &fixture.spec().role_options(),
        ))
    ))));
    assert!(!overrides.iter().any(|o| o.starts_with("instructions=")));
}

/// Claude compatibility appends the instructions that have Codex read Claude Code's
/// configuration after the role text, naming the directory the launch's Claude Code default
/// account resolves to; off, the instructions are the role text alone.
#[test]
fn claude_compatibility_appends_the_instructions_naming_the_claude_config_dir() {
    let instructions = |plan: &crate::adapter::LaunchPlan| {
        overrides(&plan.args)
            .into_iter()
            .find(|o| o.starts_with("developer_instructions="))
            .expect("developer_instructions")
    };
    let mut fixture = spec_fixture();
    fixture.shell_env.insert(
        "CLAUDE_CONFIG_DIR".to_string(),
        "/work/claude-home".to_string(),
    );
    let off = instructions(&CodexAdapter.plan(&fixture.spec()).unwrap());
    assert!(!off.contains("Claude Code compatibility"));

    let mut spec = fixture.spec();
    spec.claude_compatibility = true;
    let on = instructions(&CodexAdapter.plan(&spec).unwrap());
    let role = toml_string(&session_tools::role::role_description(
        fixture.role,
        fixture.owner.as_ref(),
        Agent::Codex,
        &spec.role_options(),
    ));
    let role_text = role.trim_end_matches('"');
    assert!(on.starts_with(&format!("developer_instructions={role_text}")));
    assert!(on.contains("# Claude Code compatibility"));
    assert!(on.contains("/work/claude-home"));
    assert!(!on.contains("{CLAUDE_CONFIG_DIR}"));
}

/// With Claude compatibility on, Claude Code's servers reach the launch as overrides and the
/// values that may be secret as environment, never argv; a server the account's own
/// `config.toml` defines is left alone; off, nothing is added.
#[test]
fn claude_compatibility_passes_claude_codes_mcp_servers_without_secrets_on_argv() {
    let mut fixture = spec_fixture();
    let home = fixture.scratch.join("home");
    let codex_home = fixture.scratch.join("codex-home");
    std::fs::create_dir_all(&codex_home).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        codex_home.join("config.toml"),
        "[mcp_servers.mine]\ncommand = \"mine\"\n",
    )
    .unwrap();
    std::fs::write(
        home.join(".claude.json"),
        serde_json::json!({"mcpServers": {
            "remote": {"type": "http", "url": "https://x.example/mcp",
                       "headers": {"Authorization": "Bearer hunter2"}},
            "mine": {"command": "claude-mine"},
            "octoboard": {"command": "claude-octoboard"}
        }})
        .to_string(),
    )
    .unwrap();
    fixture
        .shell_env
        .insert("HOME".to_string(), home.to_string_lossy().into_owned());
    fixture.config_dir = Some(codex_home);

    let plan = CodexAdapter.plan(&fixture.spec()).unwrap();
    assert!(!overrides(&plan.args)
        .iter()
        .any(|o| o.starts_with("mcp_servers.remote")));

    let mut spec = fixture.spec();
    spec.claude_compatibility = true;
    let plan = CodexAdapter.plan(&spec).unwrap();
    let all = overrides(&plan.args);
    assert!(all.contains(&"mcp_servers.remote.url=\"https://x.example/mcp\"".to_string()));
    assert!(all
        .iter()
        .any(|o| o.starts_with("mcp_servers.remote.env_http_headers=")));
    assert!(!all.iter().any(|o| o.contains("hunter2")));
    assert!(!all
        .iter()
        .any(|o| o.ends_with("command=\"claude-mine\"")
            || o.ends_with("command=\"claude-octoboard\"")));
    assert!(plan.env.iter().any(|(name, value)| {
        name.starts_with("OCTOBOARD_CLAUDE_MCP_") && value == "Bearer hunter2"
    }));
}

/// A server whose `env` sets a variable the launch or Codex owns is skipped as a whole; an
/// ordinary one beside them still passes.
#[test]
fn claude_compatibility_skips_servers_that_set_what_the_launch_or_codex_owns() {
    let mut fixture = spec_fixture();
    let home = fixture.scratch.join("home");
    let codex_home = fixture.scratch.join("codex-home");
    std::fs::create_dir_all(&codex_home).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let server = |key: &str, value: &str| serde_json::json!({"command": "c", "env": {key: value}});
    std::fs::write(
        home.join(".claude.json"),
        serde_json::json!({"mcpServers": {
            "home": server("CODEX_HOME", "/elsewhere"),
            "gitcfg": server("GIT_CONFIG_COUNT", "0"),
            "octo": server("OCTOBOARD_X", "1"),
            "log": server("RUST_LOG", "trace"),
            "proxy": server("https_proxy", "http://p"),
            "ordinary": server("MY_KEY", "v")
        }})
        .to_string(),
    )
    .unwrap();
    fixture
        .shell_env
        .insert("HOME".to_string(), home.to_string_lossy().into_owned());
    // The shell already holds the value the server asks for, so only `Refused` skips it.
    fixture
        .shell_env
        .insert("GIT_CONFIG_COUNT".to_string(), "0".to_string());
    fixture.config_dir = Some(codex_home);

    let mut spec = fixture.spec();
    spec.claude_compatibility = true;
    let plan = CodexAdapter.plan(&spec).unwrap();
    let all = overrides(&plan.args);
    for skipped in ["home", "gitcfg", "octo", "log", "proxy"] {
        let prefix = format!("mcp_servers.{skipped}.");
        assert!(!all.iter().any(|o| o.starts_with(&prefix)), "{skipped}");
    }
    assert!(all.contains(&"mcp_servers.ordinary.command=\"c\"".to_string()));
    assert_eq!(
        plan.env
            .iter()
            .filter(|(name, _)| name == "CODEX_HOME")
            .count(),
        1
    );
    assert!(plan.env.contains(&("MY_KEY".to_string(), "v".to_string())));
}

/// A server the project's `.codex/config.toml` defines, in the working directory or up to the
/// repository root, is left alone; one that cannot be read leaves nothing to compare with.
#[test]
fn claude_compatibility_leaves_servers_the_projects_codex_config_defines() {
    let mut fixture = spec_fixture();
    let repo = fixture.scratch.join("repo");
    let cwd = repo.join("sub");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir_all(cwd.join(".codex")).unwrap();
    std::fs::create_dir_all(repo.join(".codex")).unwrap();
    std::fs::write(
        cwd.join(".codex/config.toml"),
        "[mcp_servers.near]\ncommand = \"n\"\n",
    )
    .unwrap();
    std::fs::write(
        repo.join(".codex/config.toml"),
        "[mcp_servers.root]\ncommand = \"r\"\n",
    )
    .unwrap();
    let home = fixture.scratch.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join(".claude.json"),
        serde_json::json!({"mcpServers": {
            "near": {"command": "c"}, "root": {"command": "c"}, "free": {"command": "c"}
        }})
        .to_string(),
    )
    .unwrap();
    fixture
        .shell_env
        .insert("HOME".to_string(), home.to_string_lossy().into_owned());
    fixture.cwd = cwd.clone();
    let mut spec = fixture.spec();
    spec.claude_compatibility = true;
    let all = overrides(&CodexAdapter.plan(&spec).unwrap().args);
    assert!(all.contains(&"mcp_servers.free.command=\"c\"".to_string()));
    assert!(!all.iter().any(|o| o.starts_with("mcp_servers.near.")));
    assert!(!all.iter().any(|o| o.starts_with("mcp_servers.root.")));

    std::fs::write(cwd.join(".codex/config.toml"), "= not toml").unwrap();
    let mut spec = fixture.spec();
    spec.claude_compatibility = true;
    let all = overrides(&CodexAdapter.plan(&spec).unwrap().args);
    assert!(!all.iter().any(|o| o.starts_with("mcp_servers.free.")));
}

#[test]
fn account_reviewer_overrides_are_repeated_on_resume_without_changing_the_users_config() {
    use crate::protocol::ApprovalReviewer;
    let mut fixture = spec_fixture();
    let home = fixture.scratch.join("reviewer-home");
    std::fs::create_dir_all(&home).unwrap();
    let config = home.join("config.toml");
    let original = "approvals_reviewer = \"auto_review\"\n";
    std::fs::write(&config, original).unwrap();
    fixture.config_dir = Some(home);
    for (reviewer, text, resolves) in [
        (ApprovalReviewer::User, "user", false),
        (ApprovalReviewer::AutoReview, "auto_review", true),
    ] {
        for resume in [None, Some("conversation-1")] {
            let mut spec = fixture.spec();
            spec.approval_reviewer = Some(reviewer);
            spec.resume_agent_session_id = resume;
            spec.task = Some("Initial prompt");
            let plan = CodexAdapter.plan(&spec).unwrap();
            assert_eq!(plan.resolves_approvals_itself, resolves);
            assert!(overrides(&plan.args).contains(&format!("approvals_reviewer=\"{text}\"")));
            assert!(!plan
                .args
                .iter()
                .any(|arg| arg.contains("approval_policy") || arg.contains("sandbox_mode")));
            if resume.is_none() {
                assert_eq!(plan.args.last().unwrap(), "Initial prompt");
            }
            assert_eq!(std::fs::read_to_string(&config).unwrap(), original);
        }
    }
    let plan = CodexAdapter.plan(&fixture.spec()).unwrap();
    assert!(plan.resolves_approvals_itself);
    assert!(!overrides(&plan.args)
        .iter()
        .any(|arg| arg.starts_with("approvals_reviewer=")));
}

#[test]
fn remote_resumes_pass_the_reviewer_to_the_server_only() {
    use crate::protocol::ApprovalReviewer;
    let mut fixture = spec_fixture();
    fixture.agent_version = Some((0, 163, 0));
    for reviewer in [ApprovalReviewer::User, ApprovalReviewer::AutoReview] {
        for resume in [None, Some("conversation-1")] {
            let mut spec = fixture.spec();
            spec.approval_reviewer = Some(reviewer);
            spec.resume_agent_session_id = resume;
            let plan = CodexAdapter.plan(&spec).unwrap();
            let private = plan.codex_server.as_ref().unwrap();
            assert_eq!(private.resume_reviewer, Some(reviewer));
            let server_overrides = overrides(&private.args);
            let client_overrides = overrides(&private.tui_args);
            let reviewer_override = server_overrides
                .iter()
                .find(|value| value.starts_with("approvals_reviewer="))
                .unwrap();
            if resume.is_some() {
                assert_eq!(&private.tui_args[..2], ["resume", "conversation-1"]);
                assert!(!client_overrides.contains(reviewer_override));
                assert_eq!(
                    client_overrides,
                    server_overrides
                        .iter()
                        .filter(|value| *value != reviewer_override)
                        .cloned()
                        .collect::<Vec<_>>()
                );
            } else {
                assert_eq!(private.tui_args, plan.args);
                assert!(client_overrides.contains(reviewer_override));
            }
        }
    }
}

/// A hand raised for an approval the user is never shown can never be cleared by them, so the
/// setting has to be read before the promise is made.
#[test]
fn auto_review_in_the_users_own_config_is_noticed() {
    let mut fixture = spec_fixture();
    let home = fixture.scratch.join("codex-home");
    std::fs::create_dir_all(&home).expect("codex home");
    fixture.shell_env.insert(
        "CODEX_HOME".to_string(),
        home.to_string_lossy().into_owned(),
    );

    // No configuration at all: the user gets asked, which is the safe way to be wrong.
    assert!(
        !CodexAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .resolves_approvals_itself
    );

    std::fs::write(
        home.join("config.toml"),
        "approvals_reviewer = \"auto_review\"\n",
    )
    .expect("config");
    assert!(
        CodexAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .resolves_approvals_itself
    );

    // The setting has been seen as a table as well as a plain value, so neither shape may be
    // assumed.
    std::fs::write(
        home.join("config.toml"),
        "[approvals_reviewer]\nmode = \"auto_review\"\n",
    )
    .expect("config");
    assert!(
        CodexAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .resolves_approvals_itself
    );

    std::fs::write(home.join("config.toml"), "approvals_reviewer = \"ask\"\n").expect("config");
    assert!(
        !CodexAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .resolves_approvals_itself
    );
}

/// A FIFO at `config.toml` or at the models cache counts as unreadable instead of holding the
/// launch until a writer appears, and so does either file above its size bound.
#[test]
fn a_fifo_or_an_oversized_config_counts_as_unreadable() {
    let mut fixture = spec_fixture();
    let home = fixture.scratch.join("codex-home");
    std::fs::create_dir_all(&home).expect("codex home");
    fixture.shell_env.insert(
        "CODEX_HOME".to_string(),
        home.to_string_lossy().into_owned(),
    );
    let config = home.join("config.toml");
    for path in [&config, &home.join("models_cache.json")] {
        let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    }
    let (done, finished) = std::sync::mpsc::channel();
    let fixture = std::thread::spawn(move || {
        let planned = CodexAdapter.plan(&fixture.spec()).is_ok();
        let _ = done.send(planned);
        fixture
    });
    assert_eq!(
        finished.recv_timeout(std::time::Duration::from_secs(10)),
        Ok(true),
        "the launch was held by a FIFO or did not plan"
    );
    let fixture = fixture.join().expect("planned");

    let cache = home.join("models_cache.json");
    std::fs::remove_file(&cache).expect("FIFO removed");
    let models = json!({ "models": [{ "slug": "a", "upgrade": { "model": "b" } }] });
    std::fs::write(&cache, models.to_string()).expect("cache");
    assert!(model_migrations(&home).is_some());
    let mut padded = models;
    padded["pad"] = json!("x".repeat(JSON_FILE_LIMIT as usize));
    std::fs::write(&cache, padded.to_string()).expect("cache");
    assert_eq!(model_migrations(&home), None);

    std::fs::remove_file(&config).expect("FIFO removed");
    std::fs::write(
        &config,
        format!(
            "approvals_reviewer = \"auto_review\"\n# {}\n",
            "x".repeat(SMALL_FILE_LIMIT)
        ),
    )
    .expect("config");
    assert!(matches!(read_config(&home), ConfigFile::Unreadable));
    assert!(
        !CodexAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .resolves_approvals_itself
    );
}

/// A home directory opened as the project holds the user's own Codex home, which a dotfiles
/// manager may link elsewhere: its `config.toml` is read as Codex reads it, not refused as a
/// link out of the project.
#[test]
fn a_linked_codex_home_under_a_home_opened_as_the_project_is_read() {
    let home = crate::test_support::ScratchDir::new("codex-home-as-project");
    let home = canonical_project(&home);
    let dotfiles = crate::test_support::ScratchDir::new("codex-dotfiles");
    std::fs::create_dir(dotfiles.join("codex")).expect("linked home");
    std::fs::write(
        dotfiles.join("codex/config.toml"),
        "[mcp_servers.mine]\ncommand = \"x\"\n",
    )
    .expect("config");
    std::os::unix::fs::symlink(dotfiles.join("codex"), home.join(".codex")).expect("link");
    assert_eq!(
        project_server_names(&home, &home.join(".codex")),
        Some(BTreeSet::from(["mine".to_string()]))
    );
}

/// The title items are pinned for every launch, so a project's own `terminal_title = []` cannot
/// leave the trust watch without its start signal; a resume starts in the current directory,
/// and the nudge to switch model is hidden.
#[test]
fn every_launch_pins_the_terminal_title_and_hides_the_model_nudge() {
    let fixture = spec_fixture();
    let fresh = overrides(&CodexAdapter.plan(&fixture.spec()).expect("plan").args);
    let mut spec = fixture.spec();
    spec.resume_agent_session_id = Some("thread-id");
    let resumed = overrides(&CodexAdapter.plan(&spec).expect("plan").args);
    for overrides in [&fresh, &resumed] {
        assert!(overrides.contains(
            &"tui.terminal_title=[\"activity\",\"thread-name\",\"project-name\"]".to_string()
        ));
        assert!(overrides.contains(&"notice.hide_rate_limit_model_nudge=true".to_string()));
    }
    assert!(!fresh.iter().any(|o| o.starts_with("tui.resume_cwd")));
    assert!(resumed.contains(&"tui.resume_cwd=\"current\"".to_string()));
}

const MODELS_CACHE: &str = r#"{"models":[
    {"slug":"gpt-6-sol","upgrade":null},
    {"slug":"gpt-5.6-sol","upgrade":{
        "model":"gpt-6-sol","migration_markdown":"Meet GPT-6 Sol\n"}},
    {"slug":"gpt-5.5","upgrade":{"model":"gpt-6-sol","migration_markdown":"x"}},
    {"slug":"gpt-5.5","upgrade":{"model":"repeated"}},
    {"slug":"no-upgrade-key"},
    {"slug":"odd","upgrade":{"markdown":"no target"}},
    {"upgrade":{"model":"no-slug"}}]}"#;

#[test]
fn the_migration_table_maps_every_model_that_has_an_upgrade() {
    let table = upgrade_table(MODELS_CACHE).expect("table");
    assert_eq!(
        table,
        "{\"gpt-5.6-sol\"=\"gpt-6-sol\",\"gpt-5.5\"=\"gpt-6-sol\"}"
    );
    let parsed: toml::Table = format!("v={table}").parse().expect("valid TOML");
    assert_eq!(parsed["v"]["gpt-5.5"].as_str(), Some("gpt-6-sol"));
}

#[test]
fn a_models_cache_that_says_nothing_gives_no_migration_table() {
    for cache in [
        "",
        "not json",
        "[]",
        "{}",
        r#"{"models":{}}"#,
        r#"{"models":[{"slug":"a","upgrade":null}]}"#,
    ] {
        assert_eq!(upgrade_table(cache), None, "{cache:?}");
    }
}

/// The models cache is read from the Codex home in effect, the pinned directory when there is
/// one, and nothing is passed when there is none.
#[test]
fn the_migration_override_comes_from_the_codex_home_in_effect() {
    let mut fixture = spec_fixture();
    let shell_home = fixture.scratch.join("shell-codex-home");
    let pinned = fixture.scratch.join("pinned-codex-home");
    for dir in [&shell_home, &pinned] {
        std::fs::create_dir_all(dir).expect("codex home");
    }
    fixture.shell_env.insert(
        "CODEX_HOME".to_string(),
        shell_home.to_string_lossy().into_owned(),
    );
    let migrations = |fixture: &crate::adapter::tests::SpecFixture| {
        overrides(&CodexAdapter.plan(&fixture.spec()).expect("plan").args)
            .into_iter()
            .find(|o| o.starts_with("notice.model_migrations="))
    };
    assert_eq!(migrations(&fixture), None, "no cache");

    std::fs::write(shell_home.join("models_cache.json"), MODELS_CACHE).expect("cache");
    assert_eq!(
        migrations(&fixture).as_deref(),
        Some("notice.model_migrations={\"gpt-5.6-sol\"=\"gpt-6-sol\",\"gpt-5.5\"=\"gpt-6-sol\"}")
    );

    fixture.config_dir = Some(pinned.clone());
    assert_eq!(migrations(&fixture), None, "the pinned home has its own");
    std::fs::write(
        pinned.join("models_cache.json"),
        r#"{"models":[{"slug":"a","upgrade":{"model":"b"}}]}"#,
    )
    .expect("cache");
    assert_eq!(
        migrations(&fixture).as_deref(),
        Some("notice.model_migrations={\"a\"=\"b\"}")
    );
}

/// The approval setting is read from the Codex home in effect, which is the pinned directory
/// when there is one — not the shell's or the default one.
#[test]
fn the_approval_setting_is_read_from_the_pinned_codex_home() {
    let mut fixture = spec_fixture();
    let shell_home = fixture.scratch.join("shell-codex-home");
    let pinned = fixture.scratch.join("pinned-codex-home");
    for dir in [&shell_home, &pinned] {
        std::fs::create_dir_all(dir).expect("codex home");
    }
    fixture.shell_env.insert(
        "CODEX_HOME".to_string(),
        shell_home.to_string_lossy().into_owned(),
    );
    std::fs::write(
        shell_home.join("config.toml"),
        "approvals_reviewer = \"auto_review\"\n",
    )
    .expect("config");
    let resolves = |fixture: &crate::adapter::tests::SpecFixture| {
        CodexAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .resolves_approvals_itself
    };
    assert!(resolves(&fixture));

    // The pinned directory replaces the shell's: its own (empty) configuration decides.
    fixture.config_dir = Some(pinned.clone());
    assert!(!resolves(&fixture));
    std::fs::write(
        pinned.join("config.toml"),
        "approvals_reviewer = \"auto_review\"\n",
    )
    .expect("config");
    assert!(resolves(&fixture));
}

#[test]
fn a_pinned_codex_home_is_exported_and_an_unpinned_launch_adds_nothing() {
    let mut fixture = spec_fixture();
    assert!(CodexAdapter
        .plan(&fixture.spec())
        .expect("plan")
        .env
        .is_empty());

    let dir = fixture.scratch.join("codex-alt");
    std::fs::create_dir_all(&dir).expect("config dir");
    fixture.config_dir = Some(dir.clone());
    let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
    assert_eq!(
        plan.env,
        vec![("CODEX_HOME".to_string(), dir.to_string_lossy().into_owned())]
    );
}

#[test]
fn a_pinned_codex_home_that_has_gone_refuses_resuming_a_conversation() {
    let mut fixture = spec_fixture();
    fixture.config_dir = Some(fixture.scratch.join("missing"));
    let mut spec = fixture.spec();
    spec.resume_agent_session_id = Some("agent-side-id");
    let err = CodexAdapter.plan(&spec).err().expect("refused");
    let message = err.to_string();
    assert!(message.contains("Codex"), "{message}");
    assert!(message.contains("missing"), "{message}");
    assert!(!message.contains("  "), "stray spaces in: {message}");
}

/// A fresh session, and one that never had a turn, launch into a vanished pinned directory
/// rather than being refused, and the directory is created for them: Codex itself exits on a
/// `CODEX_HOME` that does not exist.
#[test]
fn a_pinned_codex_home_that_has_gone_is_created_with_no_conversation_to_resume() {
    let mut fixture = spec_fixture();
    let dir = fixture.scratch.join("missing").join("nested");
    fixture.config_dir = Some(dir.clone());
    CodexAdapter.plan(&fixture.spec()).expect("not refused");
    assert!(dir.is_dir());
}

#[test]
fn never_pre_allocates_a_session_id() {
    let fixture = spec_fixture();
    let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
    assert!(!CodexAdapter.preallocates_session_id());
    assert!(!plan.args.iter().any(|arg| arg.contains("session-id")));
}
