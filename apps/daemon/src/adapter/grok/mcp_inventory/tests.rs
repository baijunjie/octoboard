use super::*;
use crate::test_support::{fake_agent, ScratchDir, PATIENCE};

const LIST: &str = include_str!("../../../../testdata/grok_mcp_list.json");
const DOCTOR: &str = include_str!("../../../../testdata/grok_mcp_doctor.json");

fn parsed() -> Vec<LocalMcpServer> {
    servers_of(
        serde_json::from_str(LIST).unwrap(),
        serde_json::from_str(DOCTOR).ok(),
    )
}

fn server<'a>(servers: &'a [LocalMcpServer], name: &str) -> &'a LocalMcpServer {
    servers.iter().find(|server| server.name == name).unwrap()
}

#[test]
fn the_recorded_listing_and_report_read_running_failed_and_needing_sign_in_without_the_disabled() {
    let servers = parsed();
    let names: Vec<_> = servers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "stdio-missing",
            "remote-local",
            "remote-hdr",
            "stdio-env",
            "okstdio",
            "oauthish",
            "false-cmd"
        ]
    );
    assert!(servers
        .iter()
        .all(|s| !s.oauth_sign_in && s.plugin.is_none()));

    // A healthy command: running, with the tools it offers.
    assert_eq!(
        *server(&servers, "okstdio"),
        LocalMcpServer {
            name: "okstdio".to_string(),
            origin: None,
            plugin: None,
            sign_in: McpSignInState::NotNeeded,
            runtime: McpRuntimeState::Running,
            runtime_message: None,
            tool_count: Some(2),
            oauth_sign_in: false,
        }
    );
    // A command that does not exist, and one that exits at once.
    let missing = server(&servers, "stdio-missing");
    assert_eq!(missing.runtime, McpRuntimeState::Failed);
    assert_eq!(
        missing.runtime_message.as_deref(),
        Some("command not found: /nonexistent/mcp-server")
    );
    assert_eq!(
        server(&servers, "false-cmd").runtime_message.as_deref(),
        Some("handshake failed: connection closed: initialize response")
    );
    // A remote server that is down: the reason is the transport's, without its library prefix.
    let down = server(&servers, "remote-local");
    assert_eq!(down.origin.as_deref(), Some("http://127.0.0.1:9/mcp"));
    assert_eq!(down.sign_in, McpSignInState::Unknown);
    assert_eq!(down.runtime, McpRuntimeState::Failed);
    assert!(down
        .runtime_message
        .as_deref()
        .unwrap()
        .starts_with("handshake failed: Client error: error sending request for url"));
    // A remote server that wants authentication.
    let oauth = server(&servers, "oauthish");
    assert_eq!(oauth.sign_in, McpSignInState::NeedsSignIn);
    assert_eq!(oauth.runtime, McpRuntimeState::Unknown);
    assert_eq!(oauth.runtime_message, None);
    // A server turned off in the configuration is not read.
    assert!(!servers.iter().any(|s| s.name == "stdio-off"));
}

#[test]
fn nothing_the_listing_carries_beyond_what_is_read_reaches_a_server() {
    let servers = parsed();
    let shown = format!("{servers:?}");
    for secret in ["not-a-real-token", "not-a-real-key", "Authorization"] {
        assert!(!shown.contains(secret), "{secret}");
    }
}

#[test]
fn a_server_only_the_report_has_is_added_and_an_unreadable_report_leaves_states_unknown() {
    let report: Report = serde_json::from_str(
        r#"{"servers": [
            {"name": "fromclaude", "transport": "stdio", "target": "/c/one --x", "healthy": true,
             "checks": [{"label": "1 tool discovered", "passed": true}]},
            {"name": "hosted", "transport": "sse", "target": "https://h.example/sse",
             "healthy": false, "checks": [{"label": "handshake failed", "passed": false}]}
        ]}"#,
    )
    .unwrap();
    let servers = servers_of(vec![], Some(report));
    assert_eq!(servers.len(), 2);
    assert_eq!(servers[0].origin, None);
    assert_eq!(servers[0].tool_count, Some(1));
    assert_eq!(servers[1].origin.as_deref(), Some("https://h.example/sse"));
    assert_eq!(
        servers[1].runtime_message.as_deref(),
        Some("handshake failed")
    );

    let unread = servers_of(serde_json::from_str(LIST).unwrap(), None);
    assert_eq!(unread.len(), 7);
    assert!(unread.iter().all(|s| s.runtime == McpRuntimeState::Unknown));
}

/// A fake Grok Build in `dir` that records each call's arguments and the home variable, and
/// answers `mcp list --json` and `mcp doctor --json` with `list` and `doctor`; `doctor` exits 1
/// as the real one does when a server fails.
fn answering_grok(dir: &Path, list: &str, doctor: &str) -> HashMap<String, String> {
    std::fs::write(dir.join("list"), list).unwrap();
    std::fs::write(dir.join("doctor"), doctor).unwrap();
    let var = crate::providers::grok::LAYOUT.config_dir_var;
    fake_agent(
        dir,
        AgentId::GROK.descriptor().binary,
        &format!(
            "echo \"$*\" >> '{d}/args'; echo \"${{{var}-unset}}\" >> '{d}/dir'\n\
             case \"$2\" in\n\
             list) cat '{d}/list';;\n\
             doctor) cat '{d}/doctor'; exit 1;;\n\
             esac",
            d = dir.display(),
        ),
    )
}

fn read(
    env: &HashMap<String, String>,
    account: Option<&Path>,
) -> Result<Listing, McpInventoryUnavailable> {
    GrokMcpInventory.read(env, account, false, PATIENCE, &AtomicBool::new(false))
}

#[test]
fn the_reading_asks_list_then_doctor_under_the_accounts_own_home_only() {
    let var = crate::providers::grok::LAYOUT.config_dir_var;

    let dir = ScratchDir::new("mcp-inventory-grok-account");
    let env = answering_grok(&dir, LIST, DOCTOR);
    let account = dir.join("account");
    std::fs::create_dir(&account).unwrap();
    let (connectors, servers) = read(&env, Some(&account)).unwrap();
    assert!(connectors.is_empty());
    assert_eq!(servers.len(), 7);
    assert_eq!(
        server(&servers, "okstdio").runtime,
        McpRuntimeState::Running
    );
    let args = std::fs::read_to_string(dir.join("args")).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        ["mcp list --json", "mcp doctor --json"]
    );
    let dirs = std::fs::read_to_string(dir.join("dir")).unwrap();
    assert!(
        dirs.lines()
            .all(|line| line == account.display().to_string()),
        "{var}: {dirs}"
    );

    let dir = ScratchDir::new("mcp-inventory-grok-default");
    let env = answering_grok(&dir, "[]", r#"{"servers": []}"#);
    assert_eq!(read(&env, None), Ok((vec![], vec![])));
    let dirs = std::fs::read_to_string(dir.join("dir")).unwrap();
    assert!(dirs.lines().all(|line| line == "unset"), "{dirs}");
}

#[test]
fn a_missing_binary_a_slow_doctor_and_a_failing_list_are_told_apart() {
    let dir = ScratchDir::new("mcp-inventory-grok-failures");
    let none = HashMap::from([("PATH".to_string(), dir.display().to_string())]);
    assert_eq!(
        read(&none, None).unwrap_err().reason,
        McpInventoryFailure::CliMissing
    );

    let hanging = fake_agent(
        &dir,
        AgentId::GROK.descriptor().binary,
        "case \"$2\" in list) echo '[{\"name\": \"a\"}]';; *) while :; do sleep 1; done;; esac",
    );
    // `list` answers and `doctor` does not: the whole reading is timed out, with no servers.
    let result = GrokMcpInventory.read(
        &hanging,
        None,
        false,
        Duration::from_millis(1500),
        &AtomicBool::new(false),
    );
    assert_eq!(
        result,
        Err(unavailable(McpInventoryFailure::TimedOut, None))
    );

    let failing = fake_agent(
        &dir,
        AgentId::GROK.descriptor().binary,
        "echo 'broke' >&2; exit 2",
    );
    let failed = read(&failing, None).unwrap_err();
    assert_eq!(failed.reason, McpInventoryFailure::Failed);
    assert_eq!(failed.detail.as_deref(), Some("broke"));

    let garbled = fake_agent(&dir, AgentId::GROK.descriptor().binary, "echo 'not json'");
    assert_eq!(
        read(&garbled, None).unwrap_err().reason,
        McpInventoryFailure::Failed
    );
}

#[test]
fn a_doctor_that_prints_nothing_leaves_the_listed_servers_in_an_unknown_state() {
    let dir = ScratchDir::new("mcp-inventory-grok-no-report");
    let env = answering_grok(&dir, LIST, "");
    let (_, servers) = read(&env, None).unwrap();
    assert_eq!(servers.len(), 7);
    assert!(servers
        .iter()
        .all(|s| s.runtime == McpRuntimeState::Unknown));
}

#[test]
fn an_entry_of_the_report_that_cannot_be_read_does_not_void_the_others() {
    let report: Report = serde_json::from_str(
        r#"{"servers": [
            {"name": "okstdio", "transport": "stdio", "target": "/c/one", "healthy": true,
             "checks": [{"label": "2 tools discovered", "passed": true}]},
            {"transport": "http"},
            "not an entry",
            {"name": "bare", "healthy": true}
        ]}"#,
    )
    .unwrap();
    let servers = servers_of(vec![], Some(report));
    let names: Vec<_> = servers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["okstdio", "bare"]);
    assert_eq!(servers[0].runtime, McpRuntimeState::Running);
    // An entry with no transport is not taken for a remote server.
    assert_eq!(servers[1].origin, None);
    assert_eq!(servers[1].sign_in, McpSignInState::NotNeeded);
}
