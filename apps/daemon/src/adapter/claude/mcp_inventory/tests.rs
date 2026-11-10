use super::*;
use crate::test_support::{fake_agent, ScratchDir, PATIENCE};

const MIXED: &str = include_str!("../../../../testdata/claude_mcp_list_mixed.txt");
const NONE_CONFIGURED: &str = "No MCP servers configured. Use `claude mcp add` to add a server.\n";
const SIGNED_OUT_STATUS: &str = r#"{
  "loggedIn": false,
  "authMethod": "none",
  "apiProvider": "firstParty",
  "analyticsDisabled": false,
  "projectsDirectory": "/x/claudecfg/projects",
  "configDirectory": "/x/claudecfg",
  "loginExpiresAt": null
}"#;

fn server<'a>(servers: &'a [LocalMcpServer], name: &str) -> &'a LocalMcpServer {
    servers.iter().find(|server| server.name == name).unwrap()
}

#[test]
fn the_recorded_mixed_listing_reads_connected_failed_and_needing_authentication() {
    let (connectors, servers) = parse_list(MIXED);
    assert!(connectors.is_empty());
    let names: Vec<_> = servers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["okserver", "broken", "needsauth", "sse1"]);

    assert_eq!(
        servers[0],
        LocalMcpServer {
            name: "okserver".to_string(),
            origin: None,
            plugin: None,
            sign_in: McpSignInState::NotNeeded,
            runtime: McpRuntimeState::Running,
            runtime_message: None,
            tool_count: None,
            oauth_sign_in: false,
        }
    );
    assert!(servers.iter().all(|server| !server.oauth_sign_in));
    // A command with two spaces before the dash, failing with a reason.
    assert_eq!(servers[1].origin, None);
    assert_eq!(servers[1].sign_in, McpSignInState::NotNeeded);
    assert_eq!(servers[1].runtime, McpRuntimeState::Failed);
    assert_eq!(
        servers[1].runtime_message.as_deref(),
        Some("CONNECTION_CLOSED: Connection closed")
    );
    // A remote server that wants a sign-in.
    assert_eq!(
        servers[2].origin.as_deref(),
        Some("http://127.0.0.1:47811/mcp")
    );
    assert_eq!(servers[2].sign_in, McpSignInState::NeedsSignIn);
    assert_eq!(servers[2].runtime, McpRuntimeState::Unknown);
    // A remote server that failed: its sign-in is not known, its reason is kept whole.
    assert_eq!(
        servers[3].origin.as_deref(),
        Some("http://127.0.0.1:47811/sse")
    );
    assert_eq!(servers[3].sign_in, McpSignInState::Unknown);
    assert_eq!(servers[3].runtime, McpRuntimeState::Failed);
    assert!(servers[3]
        .runtime_message
        .as_deref()
        .unwrap()
        .ends_with("(or origin)"));
}

#[test]
fn nothing_configured_is_two_empty_lists() {
    assert_eq!(parse_list(NONE_CONFIGURED), (vec![], vec![]));
    assert_eq!(parse_list(""), (vec![], vec![]));
}

#[test]
fn claude_ai_lines_are_connectors_and_plugin_names_name_their_plugin() {
    let (connectors, servers) = parse_list(
        "Checking MCP server health…\n\
         claude.ai Gmail: https://gmail.mcp.claude.com/mcp - \u{2714} Connected\n\
         claude.ai Google Calendar: https://gcal.mcp.claude.com/mcp - ! Needs authentication\n\
         claude.ai Odd: https://odd.example/mcp - \u{2718} Failed to connect\n\
         plugin:github:github: https://api.example/mcp/ (HTTP) - \u{2714} Connected\n\
         plugin:tools:local: node /x/server.js --flag - dash - \u{2718} Failed to connect \u{2014} spawn - failed\n",
    );
    let summary: Vec<_> = connectors
        .iter()
        .map(|c| (c.id.as_str(), c.name.as_str(), c.state))
        .collect();
    assert_eq!(
        summary,
        [
            ("claude.ai Gmail", "Gmail", ConnectorState::Connected),
            (
                "claude.ai Google Calendar",
                "Google Calendar",
                ConnectorState::NotConnected
            ),
            ("claude.ai Odd", "Odd", ConnectorState::Unknown),
        ]
    );
    assert!(connectors.iter().all(|c| c.plugins.is_empty()));
    assert_eq!(servers.len(), 2);
    let github = server(&servers, "plugin:github:github");
    assert_eq!(github.plugin.as_deref(), Some("github"));
    assert_eq!(github.origin.as_deref(), Some("https://api.example/mcp/"));
    assert_eq!(github.sign_in, McpSignInState::Unknown);
    // A dash inside the command and inside the reason do not move the status.
    let local = server(&servers, "plugin:tools:local");
    assert_eq!(local.plugin.as_deref(), Some("tools"));
    assert_eq!(local.origin, None);
    assert_eq!(local.runtime, McpRuntimeState::Failed);
    assert_eq!(local.runtime_message.as_deref(), Some("spawn - failed"));
}

#[test]
fn a_status_this_reading_does_not_know_is_a_server_in_an_unknown_state() {
    let (_, servers) =
        parse_list("odd: https://x.example/mcp (HTTP) - ? Sleeping\nnot a server line\n");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].origin.as_deref(), Some("https://x.example/mcp"));
    assert_eq!(servers[0].sign_in, McpSignInState::Unknown);
    assert_eq!(servers[0].runtime, McpRuntimeState::Unknown);
}

/// A fake Claude Code in `dir` that records each call's arguments and the configuration
/// directory variable, and answers `auth status` and `mcp list` with `status` and `list`.
fn answering_claude(dir: &Path, status: &str, list: &str) -> HashMap<String, String> {
    std::fs::write(dir.join("status"), status).unwrap();
    std::fs::write(dir.join("list"), list).unwrap();
    let var = crate::providers::claude::LAYOUT.config_dir_var;
    fake_agent(
        dir,
        AgentId::CLAUDE.descriptor().binary,
        &format!(
            "echo \"$*\" >> '{d}/args'; echo \"${{{var}-unset}}\" >> '{d}/dir'\n\
             case \"$1 $2\" in\n\
             'auth status') cat '{d}/status'; exit 1;;\n\
             'mcp list') cat '{d}/list';;\n\
             esac",
            d = dir.display(),
        ),
    )
}

fn read(
    env: &HashMap<String, String>,
    account: Option<&Path>,
) -> Result<Listing, McpInventoryUnavailable> {
    ClaudeMcpInventory.read(env, account, false, PATIENCE, &AtomicBool::new(false))
}

#[test]
fn a_signed_out_account_is_not_signed_in_and_its_servers_are_not_listed() {
    let dir = ScratchDir::new("mcp-inventory-claude-signed-out");
    let env = answering_claude(&dir, SIGNED_OUT_STATUS, MIXED);
    assert_eq!(
        read(&env, None),
        Err(unavailable(McpInventoryFailure::NotSignedIn, None))
    );
    let args = std::fs::read_to_string(dir.join("args")).unwrap();
    assert_eq!(args.trim(), "auth status");
}

#[test]
fn the_reading_asks_status_then_list_under_the_accounts_own_directory_only() {
    let status = r#"{"loggedIn": true, "authMethod": "claude.ai"}"#;
    let var = crate::providers::claude::LAYOUT.config_dir_var;

    let dir = ScratchDir::new("mcp-inventory-claude-account");
    let env = answering_claude(&dir, status, MIXED);
    let account = dir.join("account");
    std::fs::create_dir(&account).unwrap();
    let (_, servers) = read(&env, Some(&account)).unwrap();
    assert_eq!(servers.len(), 4);
    let args = std::fs::read_to_string(dir.join("args")).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        ["auth status", "mcp list"]
    );
    let dirs = std::fs::read_to_string(dir.join("dir")).unwrap();
    assert!(
        dirs.lines()
            .all(|line| line == account.display().to_string()),
        "{var}: {dirs}"
    );

    let dir = ScratchDir::new("mcp-inventory-claude-default");
    let env = answering_claude(&dir, status, NONE_CONFIGURED);
    assert_eq!(read(&env, None), Ok((vec![], vec![])));
    let dirs = std::fs::read_to_string(dir.join("dir")).unwrap();
    assert!(dirs.lines().all(|line| line == "unset"), "{dirs}");
}

#[test]
fn a_missing_binary_a_slow_list_and_a_failing_one_are_told_apart() {
    let dir = ScratchDir::new("mcp-inventory-claude-failures");
    let none = HashMap::from([("PATH".to_string(), dir.display().to_string())]);
    assert_eq!(
        read(&none, None).unwrap_err().reason,
        McpInventoryFailure::CliMissing
    );

    let status = r#"{"loggedIn": true}"#;
    let hanging = fake_agent(
        &dir,
        AgentId::CLAUDE.descriptor().binary,
        &format!("case \"$1\" in auth) echo '{status}';; *) while :; do sleep 1; done;; esac"),
    );
    let result = ClaudeMcpInventory.read(
        &hanging,
        None,
        false,
        Duration::from_millis(1500),
        &AtomicBool::new(false),
    );
    assert_eq!(result.unwrap_err().reason, McpInventoryFailure::TimedOut);

    let failing = fake_agent(
        &dir,
        AgentId::CLAUDE.descriptor().binary,
        &format!("case \"$1\" in auth) echo '{status}';; *) echo 'broke' >&2; exit 2;; esac"),
    );
    let failed = read(&failing, None).unwrap_err();
    assert_eq!(failed.reason, McpInventoryFailure::Failed);
    assert_eq!(failed.detail.as_deref(), Some("broke"));
}

#[cfg(unix)]
#[test]
fn status_and_list_share_one_reading_budget() {
    use std::os::unix::fs::OpenOptionsExt;

    let budget = Duration::from_secs(3);
    let step_wait = budget.mul_f64(2.0 / 3.0).as_secs();
    let result = crate::test_support::until_the_shell_ran(|| {
        let dir = ScratchDir::new("mcp-inventory-claude-shared-budget");
        let fifo = dir.join("hold");
        crate::test_support::mkfifo(&fifo);
        // An open writer keeps the built-in timed read from seeing EOF. No subprocess starts
        // inside either step's wait, where a host's executable scan could consume its budget.
        let _writer = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&fifo)
            .unwrap();
        let env = fake_agent(
            &dir,
            AgentId::CLAUDE.descriptor().binary,
            &format!(
                "test -n \"$BASH_VERSION\" || exec /bin/bash \"$0\" \"$@\"\n\
                 case \"$1\" in\n\
                 auth) read -t {step_wait} ignored < '{d}/hold'; echo '{{\"loggedIn\":true}}';;\n\
                 mcp) echo started > '{d}/second'; read -t {step_wait} ignored < '{d}/hold'; echo 'No MCP servers configured.';;\n\
                 esac",
                d = dir.display(),
            ),
        );
        let result = ClaudeMcpInventory.read(&env, None, false, budget, &AtomicBool::new(false));
        // Retry when the host delayed a start until the deadline: the second step must have
        // reached its own wait for this attempt to exercise the shared budget.
        dir.join("second").exists().then_some(result)
    });
    assert_eq!(result.unwrap_err().reason, McpInventoryFailure::TimedOut);
}

#[test]
fn the_listing_waits_at_most_the_server_timeout_unless_the_shell_asks_for_less() {
    let shell = |value: Option<&str>| {
        let mut env = HashMap::from([("PATH".to_string(), "/usr/bin".to_string())]);
        if let Some(value) = value {
            env.insert("MCP_TIMEOUT".to_string(), value.to_string());
        }
        env
    };
    let capped = LIST_SERVER_TIMEOUT_MS.to_string();
    let cases = [
        (None, capped.as_str()),
        // A shorter positive wait of the shell's own is kept.
        (Some("3000"), "3000"),
        (Some("1"), "1"),
        // A longer one, zero (no wait at all would list every server as failed) and one that is no number are replaced.
        (Some("600000"), capped.as_str()),
        (Some("0"), capped.as_str()),
        (Some("soon"), capped.as_str()),
        (Some(""), capped.as_str()),
        (Some("-5"), capped.as_str()),
    ];
    for (held, expected) in cases {
        let env = shell(held);
        let adjusted = with_list_server_timeout(&env);
        assert_eq!(
            adjusted.get("MCP_TIMEOUT").map(String::as_str),
            Some(expected),
            "shell MCP_TIMEOUT {held:?}"
        );
        // Nothing else of the environment changes, and the shell's own is left as it was.
        assert_eq!(adjusted.get("PATH"), env.get("PATH"));
        assert_eq!(adjusted.len(), 2);
        assert_eq!(env.get("MCP_TIMEOUT").map(String::as_str), held);
    }
}

#[test]
fn only_the_listing_runs_with_the_server_timeout_and_auth_status_keeps_the_shell_environment() {
    let status = r#"{"loggedIn": true, "authMethod": "claude.ai"}"#;
    // `None` is a shell with no `MCP_TIMEOUT`.
    for (held, status_saw, list_saw) in [
        (
            None,
            "unset".to_string(),
            LIST_SERVER_TIMEOUT_MS.to_string(),
        ),
        (Some("3000"), "3000".to_string(), "3000".to_string()),
        (
            Some("600000"),
            "600000".to_string(),
            LIST_SERVER_TIMEOUT_MS.to_string(),
        ),
    ] {
        let dir = ScratchDir::new("mcp-inventory-claude-server-timeout");
        std::fs::write(dir.join("status"), status).unwrap();
        let mut env = fake_agent(
            &dir,
            AgentId::CLAUDE.descriptor().binary,
            &format!(
                "echo \"$1 $2 ${{MCP_TIMEOUT-unset}}\" >> '{d}/seen'\n\
                 case \"$1 $2\" in\n\
                 'auth status') cat '{d}/status'; exit 1;;\n\
                 'mcp list') echo 'No MCP servers configured.';;\n\
                 esac",
                d = dir.display(),
            ),
        );
        if let Some(held) = held {
            env.insert("MCP_TIMEOUT".to_string(), held.to_string());
        }
        assert_eq!(read(&env, None), Ok((vec![], vec![])));
        let seen = std::fs::read_to_string(dir.join("seen")).unwrap();
        assert_eq!(
            seen.lines().collect::<Vec<_>>(),
            [
                format!("auth status {status_saw}"),
                format!("mcp list {list_saw}")
            ],
            "shell MCP_TIMEOUT {held:?}"
        );
    }
}
