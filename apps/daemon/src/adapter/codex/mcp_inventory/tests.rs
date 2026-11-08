use super::*;
use crate::test_support::{fake_agent, ScratchDir, PATIENCE};

const SIGNED_OUT: &str = include_str!("../../../../testdata/codex_mcp_signed_out_replies.jsonl");
const CONNECTED_NEEDS_AUTH: &str =
    include_str!("../../../../testdata/codex_mcp_status_connected_needsauth.json");

/// The replies of a recorded conversation, by id, as the reading collects them.
fn replies_of(lines: &str) -> Replies {
    lines
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter_map(|value| Some((value.get("id")?.as_i64()?, value)))
        .filter(|(id, _)| *id >= ACCOUNT)
        .collect()
}

/// A signed-in conversation: `apps`, `installed` and `servers` are the `data`, `apps` and `data`
/// of the three listings.
fn signed_in(apps: Value, installed: Value, servers: Value) -> Replies {
    Replies::from([
        (
            ACCOUNT,
            json!({"id": 2, "result": {"account": {"type": "chatgpt", "email": "a@b.c"},
                "requiresOpenaiAuth": true}}),
        ),
        (
            APP_LIST,
            json!({"id": 3, "result": {"data": apps, "nextCursor": null}}),
        ),
        (
            APP_INSTALLED,
            json!({"id": 4, "result": {"apps": installed}}),
        ),
        (
            SERVER_STATUS,
            json!({"id": 5, "result": {"data": servers, "nextCursor": null}}),
        ),
    ])
}

fn server_of(name: &str, fields: Value) -> Value {
    let mut server = json!({"name": name, "runtimeStatus": null, "pluginId": null,
        "httpOrigin": null, "tools": {}, "toolsError": null, "authStatus": "unsupported"});
    for (key, value) in fields.as_object().unwrap() {
        server[key] = value.clone();
    }
    server
}

#[test]
fn a_signed_out_account_is_not_signed_in_whatever_its_servers_say() {
    let result = interpret(&replies_of(SIGNED_OUT));
    assert_eq!(
        result,
        Err(unavailable(McpInventoryFailure::NotSignedIn, None))
    );
}

#[test]
fn a_connected_stdio_server_and_one_needing_authentication_are_read_from_the_recording() {
    let reply: Value = serde_json::from_str(CONNECTED_NEEDS_AUTH).unwrap();
    let replies = signed_in(json!([]), json!([]), reply["result"]["data"].clone());
    let (connectors, servers) = interpret(&replies).unwrap();
    assert!(connectors.is_empty());
    assert_eq!(servers.len(), 2);
    assert_eq!(servers[0].name, "needsauth");
    assert_eq!(servers[0].origin.as_deref(), Some("http://127.0.0.1:47811"));
    // No runtime status and an error: failed, with its message; an unknown auth status is unknown.
    assert_eq!(servers[0].runtime, McpRuntimeState::Failed);
    assert!(servers[0]
        .runtime_message
        .as_deref()
        .unwrap()
        .contains("Auth required"));
    assert_eq!(servers[0].sign_in, McpSignInState::Unknown);
    assert_eq!(servers[0].tool_count, None);
    assert_eq!(
        servers[1],
        LocalMcpServer {
            name: "okserver".to_string(),
            origin: None,
            plugin: None,
            sign_in: McpSignInState::NotNeeded,
            runtime: McpRuntimeState::Running,
            runtime_message: None,
            tool_count: Some(2),
            oauth_sign_in: false,
        }
    );
}

#[test]
fn nothing_configured_is_two_empty_lists() {
    let replies = signed_in(json!([]), json!([]), json!([]));
    assert_eq!(interpret(&replies), Ok((vec![], vec![])));
}

#[test]
fn only_the_apps_worth_listing_are_connectors() {
    let replies = signed_in(
        json!([
            {"id": "gmail", "name": "Gmail", "isAccessible": true, "isEnabled": true,
                "pluginDisplayNames": []},
            {"id": "slack", "name": "Slack", "isAccessible": false,
                "pluginDisplayNames": ["Team Tools"]},
            {"id": "notion", "name": "Notion", "isAccessible": false, "pluginDisplayNames": []},
            {"id": "linear", "name": "Linear", "isAccessible": false, "pluginDisplayNames": []},
            {"id": "odd", "pluginDisplayNames": ["P"]},
            {"name": "no id", "isAccessible": true}
        ]),
        json!([{"id": "linear", "runtimeName": "linear", "enabled": true, "callable": true}]),
        json!([]),
    );
    let (connectors, _) = interpret(&replies).unwrap();
    let summary: Vec<_> = connectors
        .iter()
        .map(|c| (c.id.as_str(), c.name.as_str(), c.state, c.plugins.clone()))
        .collect();
    assert_eq!(
        summary,
        [
            ("gmail", "Gmail", ConnectorState::Connected, vec![]),
            (
                "slack",
                "Slack",
                ConnectorState::NotConnected,
                vec!["Team Tools".to_string()]
            ),
            ("linear", "Linear", ConnectorState::NotConnected, vec![]),
            ("odd", "odd", ConnectorState::Unknown, vec!["P".to_string()]),
        ]
    );
}

#[test]
fn servers_map_origin_plugin_sign_in_and_runtime() {
    let replies = signed_in(
        json!([]),
        json!([]),
        json!([
            server_of("codex_apps", json!({"runtimeStatus": "connected"})),
            server_of(
                "remote",
                json!({"runtimeStatus": "connected", "httpOrigin": "https://r.example",
                    "authStatus": "oAuth", "tools": {"a": {}, "b": {}, "c": {}}})
            ),
            server_of(
                "token",
                json!({"authStatus": "bearerToken", "runtimeStatus": "connected"})
            ),
            server_of(
                "loggedout",
                json!({"authStatus": "notLoggedIn",
                "runtimeStatus": "failed", "toolsError": "boom"})
            ),
            server_of(
                "fromplugin",
                json!({"pluginId": "team@market",
                "runtimeStatus": "connected"})
            ),
            server_of(
                "auth",
                json!({"runtimeStatus": "authenticationRequired",
                "authStatus": "unknown"})
            ),
            server_of("cancelled", json!({"runtimeStatus": "cancelled"})),
            server_of("starting", json!({"runtimeStatus": "starting"})),
            server_of(
                "future",
                json!({"runtimeStatus": "hibernating",
                "authStatus": "somethingNew"})
            ),
        ]),
    );
    let (_, servers) = interpret(&replies).unwrap();
    assert!(servers.iter().all(|server| server.name != "codex_apps"));
    let by = |name: &str| servers.iter().find(|server| server.name == name).unwrap();
    let remote = by("remote");
    assert_eq!(remote.origin.as_deref(), Some("https://r.example"));
    assert_eq!(remote.sign_in, McpSignInState::SignedIn);
    assert_eq!(remote.tool_count, Some(3));
    assert_eq!(by("token").sign_in, McpSignInState::SignedIn);
    let loggedout = by("loggedout");
    assert_eq!(loggedout.sign_in, McpSignInState::NeedsSignIn);
    assert_eq!(loggedout.runtime, McpRuntimeState::Failed);
    assert_eq!(loggedout.runtime_message.as_deref(), Some("boom"));
    assert_eq!(loggedout.tool_count, None);
    assert_eq!(by("fromplugin").plugin.as_deref(), Some("team@market"));
    let auth = by("auth");
    assert_eq!(auth.sign_in, McpSignInState::NeedsSignIn);
    assert_eq!(auth.runtime, McpRuntimeState::Unknown);
    assert_eq!(by("cancelled").runtime, McpRuntimeState::Failed);
    assert_eq!(by("starting").runtime, McpRuntimeState::Unknown);
    let future = by("future");
    assert_eq!(future.sign_in, McpSignInState::Unknown);
    assert_eq!(future.runtime, McpRuntimeState::Unknown);
}

#[test]
fn only_a_server_with_or_awaiting_an_oauth_token_offers_signing_in() {
    for (auth, offered) in [
        (Some("oAuth"), true),
        (Some("notLoggedIn"), true),
        (Some("bearerToken"), false),
        (Some("unsupported"), false),
        (Some("somethingNew"), false),
        (None, false),
    ] {
        assert_eq!(oauth_sign_in(auth), offered, "{auth:?}");
    }
    let replies = signed_in(
        json!([]),
        json!([]),
        json!([
            server_of("remote", json!({"authStatus": "oAuth"})),
            server_of("token", json!({"authStatus": "bearerToken"})),
        ]),
    );
    let (_, servers) = interpret(&replies).unwrap();
    let offered: Vec<_> = servers.iter().map(|s| s.oauth_sign_in).collect();
    assert_eq!(offered, [true, false]);
}

#[test]
fn an_app_listing_an_older_codex_does_not_know_leaves_the_connectors_empty() {
    let mut replies = signed_in(json!([]), json!([]), json!([server_of("s", json!({}))]));
    let unknown = json!({"id": 3, "error": {"code": -32600,
        "message": "Invalid request: unknown variant `app/list`, expected one of `initialize`"}});
    replies.insert(APP_LIST, unknown.clone());
    replies.insert(APP_INSTALLED, json!({"id": 4, "error": unknown["error"]}));
    let (connectors, servers) = interpret(&replies).unwrap();
    assert!(connectors.is_empty());
    assert_eq!(servers.len(), 1);
}

#[test]
fn a_refused_server_listing_fails_the_reading_with_its_words() {
    let mut replies = signed_in(json!([]), json!([]), json!([]));
    replies.insert(
        SERVER_STATUS,
        json!({"id": 5, "error": {"code": -32603, "message": "config is invalid"}}),
    );
    assert_eq!(
        interpret(&replies),
        Err(unavailable(
            McpInventoryFailure::Failed,
            Some("config is invalid".to_string())
        ))
    );
}

/// A fake Codex in `dir` that records its arguments, the directory variable and each line it is
/// given, and answers the four requests with `replies`, one line each.
fn answering_codex(dir: &Path, replies: &[&str]) -> HashMap<String, String> {
    std::fs::write(dir.join("replies"), replies.join("\n") + "\n").expect("replies");
    let var = crate::providers::codex::LAYOUT.config_dir_var;
    fake_agent(
        dir,
        AgentId::CODEX.descriptor().binary,
        &format!(
            "echo \"$*\" > '{d}/args'; echo \"${var}\" > '{d}/dir'\n\
             while read line; do echo \"$line\" >> '{d}/input'\n\
             case \"$line\" in *'mcpServerStatus/list'*) echo 'not a reply'; \
             cat '{d}/replies';; esac\n\
             done",
            d = dir.display(),
        ),
    )
}

fn read_in(
    env: &HashMap<String, String>,
    account: Option<&Path>,
    refresh: bool,
) -> Result<Listing, McpInventoryUnavailable> {
    CodexMcpInventory.read(env, account, refresh, PATIENCE, &AtomicBool::new(false))
}

const SIGNED_IN_REPLIES: [&str; 4] = [
    r#"{"id":2,"result":{"account":{"type":"chatgpt"},"requiresOpenaiAuth":true}}"#,
    r#"{"id":3,"result":{"data":[{"id":"gmail","name":"Gmail","isAccessible":true,"pluginDisplayNames":[]}],"nextCursor":null}}"#,
    r#"{"id":4,"result":{"apps":[]}}"#,
    r#"{"id":5,"result":{"data":[{"name":"okserver","runtimeStatus":"connected","tools":{"t":{}},"toolsError":null,"authStatus":"unsupported"}]}}"#,
];

#[test]
fn the_conversation_asks_for_the_four_things_and_reads_the_answers() {
    let dir = ScratchDir::new("mcp-inventory-codex");
    let env = answering_codex(&dir, &SIGNED_IN_REPLIES);
    let account = dir.join("account");
    std::fs::create_dir(&account).unwrap();
    let (connectors, servers) = read_in(&env, Some(&account), true).unwrap();
    assert_eq!(connectors.len(), 1);
    assert_eq!(servers[0].name, "okserver");
    assert_eq!(servers[0].tool_count, Some(1));
    assert_eq!(
        std::fs::read_to_string(dir.join("dir")).unwrap().trim(),
        account.display().to_string()
    );
    let lines: Vec<Value> = std::fs::read_to_string(dir.join("input"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let methods: Vec<_> = lines
        .iter()
        .map(|line| line["method"].as_str().unwrap())
        .collect();
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "account/read",
            "app/list",
            "app/installed",
            "mcpServerStatus/list"
        ]
    );
    assert_eq!(
        lines[3]["params"],
        json!({"limit": 1000, "forceRefetch": true})
    );
    assert_eq!(lines[4]["params"], json!({"forceRefresh": true}));
    assert_eq!(
        lines[5]["params"],
        json!({"detail": "toolsAndAuthOnly", "limit": 1000})
    );
}

#[test]
fn a_cached_reading_does_not_ask_for_a_refetch() {
    let dir = ScratchDir::new("mcp-inventory-codex-cached");
    let env = answering_codex(&dir, &SIGNED_IN_REPLIES);
    read_in(&env, None, false).unwrap();
    let input = std::fs::read_to_string(dir.join("input")).unwrap();
    assert!(input.contains(r#""forceRefetch":false"#), "{input}");
    assert!(input.contains(r#""forceRefresh":false"#), "{input}");
}

#[test]
fn apps_and_plugins_stay_on_and_only_a_non_default_account_gets_its_own_credential_store() {
    for (account, store) in [(true, true), (false, false)] {
        let dir = ScratchDir::new("mcp-inventory-codex-args");
        let env = answering_codex(&dir, &SIGNED_IN_REPLIES);
        let home = dir.join("account");
        std::fs::create_dir(&home).unwrap();
        read_in(&env, account.then_some(home.as_path()), false).unwrap();
        let args = std::fs::read_to_string(dir.join("args")).unwrap();
        assert!(args.starts_with("app-server "), "{args}");
        assert!(!args.contains("features.apps"), "{args}");
        assert!(!args.contains("features.plugins"), "{args}");
        assert!(args.contains("features.remote_plugin=false"), "{args}");
        assert!(args.contains("features.hooks=false"), "{args}");
        assert_eq!(
            args.contains(super::super::MCP_FILE_CREDENTIALS_STORE),
            store,
            "{args}"
        );
    }
}

#[test]
fn a_missing_binary_a_silent_codex_and_a_failing_one_are_told_apart() {
    let dir = ScratchDir::new("mcp-inventory-codex-failures");
    let none = HashMap::from([("PATH".to_string(), dir.display().to_string())]);
    let missing = read_in(&none, None, false).unwrap_err();
    assert_eq!(missing.reason, McpInventoryFailure::CliMissing);

    let silent = fake_agent(
        &dir,
        AgentId::CODEX.descriptor().binary,
        "while read line; do :; done",
    );
    let result = CodexMcpInventory.read(
        &silent,
        None,
        false,
        Duration::from_millis(700),
        &AtomicBool::new(false),
    );
    assert_eq!(result.unwrap_err().reason, McpInventoryFailure::TimedOut);

    let failing = fake_agent(
        &dir,
        AgentId::CODEX.descriptor().binary,
        "echo broke >&2; exit 3",
    );
    let failed = read_in(&failing, None, false).unwrap_err();
    assert_eq!(failed.reason, McpInventoryFailure::Failed);
    assert!(failed.detail.unwrap().contains("broke"));
}
