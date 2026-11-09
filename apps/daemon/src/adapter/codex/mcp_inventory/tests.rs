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

/// What the first conversation's `replies` say with no server turned off.
fn read_replies(
    replies: &Replies,
) -> Result<(Vec<Installed>, Vec<LocalMcpServer>), McpInventoryUnavailable> {
    interpret(replies, &HashSet::new())
}

/// A signed-in first conversation: `installed` and `servers` are the `apps` and `data` of the two
/// listings.
fn signed_in(installed: Value, servers: Value) -> Replies {
    Replies::from([
        (
            ACCOUNT,
            json!({"id": 2, "result": {"account": {"type": "chatgpt", "email": "a@b.c"},
                "requiresOpenaiAuth": true}}),
        ),
        (
            APP_INSTALLED,
            json!({"id": 3, "result": {"apps": installed}}),
        ),
        (
            SERVER_STATUS,
            json!({"id": 4, "result": {"data": servers, "nextCursor": null}}),
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
    let result = read_replies(&replies_of(SIGNED_OUT));
    assert_eq!(
        result,
        Err(unavailable(McpInventoryFailure::NotSignedIn, None))
    );
}

#[test]
fn a_connected_stdio_server_and_one_needing_authentication_are_read_from_the_recording() {
    let reply: Value = serde_json::from_str(CONNECTED_NEEDS_AUTH).unwrap();
    let replies = signed_in(json!([]), reply["result"]["data"].clone());
    let (connectors, servers) = read_replies(&replies).unwrap();
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
            // No runtime status, but tools listed: it started.
            runtime: McpRuntimeState::Running,
            runtime_message: None,
            tool_count: Some(2),
            oauth_sign_in: false,
        }
    );
}

#[test]
fn nothing_configured_is_two_empty_lists() {
    let replies = signed_in(json!([]), json!([]));
    assert_eq!(read_replies(&replies), Ok((vec![], vec![])));
}

#[test]
fn every_installed_app_is_a_connector_in_its_order_named_and_attributed_by_app_read() {
    let installed = installed(Some(&json!({"id": 3, "result": {"apps": [
        {"id": "b_app", "runtimeName": "B runtime", "enabled": true, "callable": true},
        {"id": "a_app", "runtimeName": "A runtime", "enabled": false, "callable": false},
        {"id": "c_app", "runtimeName": "C runtime", "enabled": true, "callable": false},
        {"id": "d_app", "enabled": true},
        {"runtimeName": "no id", "enabled": true, "callable": true}
    ]}})));
    let details = Replies::from([
        (
            APP_READ,
            json!({"id": 10, "result": {"apps": [
                {"id": "a_app", "name": "A", "pluginDisplayNames": ["Team Tools", "Kit"]},
                {"id": "b_app", "name": "B", "pluginDisplayNames": []}
            ], "missingAppIds": ["c_app"]}}),
        ),
        (
            APP_READ + 1,
            json!({"id": 11, "error": {"code": -32603, "message": "failed to read app metadata"}}),
        ),
    ]);
    let summary: Vec<_> = connectors(&installed, &details)
        .into_iter()
        .map(|c| (c.id, c.name, c.state, c.plugins))
        .collect();
    let strings = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
    assert_eq!(
        summary,
        [
            (
                "b_app".into(),
                "B".into(),
                ConnectorState::Connected,
                vec![]
            ),
            (
                "a_app".into(),
                "A".into(),
                ConnectorState::NotConnected,
                strings(&["Team Tools", "Kit"])
            ),
            (
                "c_app".into(),
                "C runtime".into(),
                ConnectorState::Unknown,
                vec![]
            ),
            (
                "d_app".into(),
                "d_app".into(),
                ConnectorState::Unknown,
                vec![]
            ),
        ]
    );
}

#[test]
fn app_reads_are_batched_by_a_hundred() {
    let ids: Vec<String> = (0..150).map(|n| format!("app{n}")).collect();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let requests = reads(&ids);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["id"], 10);
    assert_eq!(requests[0]["method"], "app/read");
    assert_eq!(
        requests[0]["params"]["appIds"].as_array().unwrap().len(),
        100
    );
    assert_eq!(requests[0]["params"]["includeTools"], false);
    assert_eq!(requests[1]["id"], 11);
    assert_eq!(requests[1]["params"]["appIds"][49], "app149");
}

#[test]
fn servers_map_origin_plugin_sign_in_and_runtime() {
    let replies = signed_in(
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
    let (_, servers) = read_replies(&replies).unwrap();
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
fn turned_off_servers_are_left_out_and_one_with_no_status_runs_by_its_tools() {
    let replies = signed_in(
        json!([]),
        json!([
            server_of("configured_off", json!({})),
            server_of("reported_off", json!({"runtimeStatus": "disabled"})),
            server_of("quiet", json!({"tools": {"a": {}}})),
            server_of("silent", json!({})),
            server_of("running", json!({"runtimeStatus": "connected"})),
        ]),
    );
    let disabled = HashSet::from(["configured_off".to_string()]);
    let (_, servers) = interpret(&replies, &disabled).unwrap();
    let states: Vec<_> = servers
        .iter()
        .map(|server| (server.name.as_str(), server.runtime))
        .collect();
    assert_eq!(
        states,
        [
            ("quiet", McpRuntimeState::Running),
            ("silent", McpRuntimeState::Unknown),
            ("running", McpRuntimeState::Running)
        ]
    );
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
        json!([
            server_of("remote", json!({"authStatus": "oAuth"})),
            server_of("token", json!({"authStatus": "bearerToken"})),
        ]),
    );
    let (_, servers) = read_replies(&replies).unwrap();
    let offered: Vec<_> = servers.iter().map(|s| s.oauth_sign_in).collect();
    assert_eq!(offered, [true, false]);
}

#[test]
fn an_app_listing_an_older_codex_does_not_know_leaves_the_connectors_empty() {
    let mut replies = signed_in(json!([]), json!([server_of("s", json!({}))]));
    replies.insert(
        APP_INSTALLED,
        json!({"id": 3, "error": {"code": -32600,
            "message": "Invalid request: unknown variant `app/installed`"}}),
    );
    let (installed, servers) = read_replies(&replies).unwrap();
    assert!(installed.is_empty());
    assert_eq!(servers.len(), 1);
}

#[test]
fn a_refused_server_listing_fails_the_reading_with_its_words() {
    let mut replies = signed_in(json!([]), json!([]));
    replies.insert(
        SERVER_STATUS,
        json!({"id": 4, "error": {"code": -32603, "message": "config is invalid"}}),
    );
    assert_eq!(
        read_replies(&replies),
        Err(unavailable(
            McpInventoryFailure::Failed,
            Some("config is invalid".to_string())
        ))
    );
}

/// A fake Codex in `dir` that answers `mcp list` with `configured` (recording its arguments and
/// the directory variable in `listed`) and, as an app server, records its arguments, the
/// directory variable and each line it is given (both conversations' lines in one file), and
/// answers the server listing with `replies` and an `app/read` with [`READ_REPLY`], one line each,
/// printed in order once the request comes.
fn answering_codex_with(dir: &Path, configured: &str, replies: &[&str]) -> HashMap<String, String> {
    std::fs::write(dir.join("replies"), replies.join("\n") + "\n").expect("replies");
    std::fs::write(dir.join("read"), format!("{READ_REPLY}\n")).expect("read");
    std::fs::write(dir.join("configured"), configured).expect("configured");
    let var = crate::providers::codex::LAYOUT.config_dir_var;
    fake_agent(
        dir,
        AgentId::CODEX.descriptor().binary,
        &format!(
            "if [ \"$1\" = mcp ]; then echo \"$* ${var}\" > '{d}/listed'; \
             cat '{d}/configured'; exit 0; fi\n\
             echo \"$*\" > '{d}/args'; echo \"${var}\" > '{d}/dir'\n\
             while read line; do echo \"$line\" >> '{d}/input'\n\
             case \"$line\" in *'mcpServerStatus/list'*) echo 'not a reply'; \
             cat '{d}/replies';; *'app/read'*) cat '{d}/read';; esac\n\
             done",
            d = dir.display(),
        ),
    )
}

/// [`answering_codex_with`] where `codex mcp list` turns nothing off.
fn answering_codex(dir: &Path, replies: &[&str]) -> HashMap<String, String> {
    answering_codex_with(dir, "[]", replies)
}

fn read_in(
    env: &HashMap<String, String>,
    account: Option<&Path>,
    refresh: bool,
) -> Result<Listing, McpInventoryUnavailable> {
    CodexMcpInventory.read(env, account, refresh, PATIENCE, &AtomicBool::new(false))
}

const SIGNED_IN_REPLIES: [&str; 3] = [
    r#"{"id":2,"result":{"account":{"type":"chatgpt"},"requiresOpenaiAuth":true}}"#,
    r#"{"id":3,"result":{"apps":[{"id":"gmail","runtimeName":"gmail","enabled":true,"callable":true}]}}"#,
    r#"{"id":4,"result":{"data":[{"name":"okserver","runtimeStatus":"connected","tools":{"t":{}},"toolsError":null,"authStatus":"unsupported"}]}}"#,
];

/// The `app/read` answer of the fake Codex.
const READ_REPLY: &str = r#"{"id":10,"result":{"apps":[{"id":"gmail","name":"Gmail","pluginDisplayNames":["Mail Kit"]}],"missingAppIds":[]}}"#;

#[test]
fn the_conversations_ask_for_the_installed_apps_then_read_them() {
    let dir = ScratchDir::new("mcp-inventory-codex");
    let env = answering_codex(&dir, &SIGNED_IN_REPLIES);
    let account = dir.join("account");
    std::fs::create_dir(&account).unwrap();
    let (connectors, servers) = read_in(&env, Some(&account), true).unwrap();
    assert_eq!(
        connectors,
        [PlatformConnector {
            id: "gmail".to_string(),
            name: "Gmail".to_string(),
            state: ConnectorState::Connected,
            plugins: vec!["Mail Kit".to_string()],
        }]
    );
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
    assert_eq!(lines[0], app_server::initialize());
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "account/read",
            "app/installed",
            "mcpServerStatus/list",
            "initialize",
            "initialized",
            "app/read"
        ]
    );
    assert_eq!(lines[3]["params"], json!({"forceRefresh": true}));
    assert_eq!(
        lines[4]["params"],
        json!({"detail": "toolsAndAuthOnly", "limit": 1000})
    );
    assert_eq!(
        lines[7]["params"],
        json!({"appIds": ["gmail"], "includeTools": false})
    );
}

/// A server listing of 9 MiB, the size the tool schemas of an account with many Apps reach, is
/// within the output limit.
#[test]
fn a_nine_mebibyte_server_listing_is_read() {
    let dir = ScratchDir::new("mcp-inventory-codex-large");
    let schema = "x".repeat(9 * 1024 * 1024);
    let status = json!({"id": 4, "result": {"data": [{"name": "big", "runtimeStatus": "connected",
        "toolsError": null, "authStatus": "unsupported",
        "tools": {"a": {"description": schema}, "b": {}}}]}})
    .to_string();
    let replies = [SIGNED_IN_REPLIES[0], SIGNED_IN_REPLIES[1], status.as_str()];
    let env = answering_codex(&dir, &replies);
    let (connectors, servers) = read_in(&env, None, false).unwrap();
    assert_eq!(connectors.len(), 1);
    assert_eq!(servers[0].name, "big");
    assert_eq!(servers[0].tool_count, Some(2));
}

#[test]
fn a_cached_reading_does_not_ask_for_a_refetch() {
    let dir = ScratchDir::new("mcp-inventory-codex-cached");
    let env = answering_codex(&dir, &SIGNED_IN_REPLIES);
    read_in(&env, None, false).unwrap();
    let input = std::fs::read_to_string(dir.join("input")).unwrap();
    assert!(input.contains(r#""forceRefresh":false"#), "{input}");
}

#[test]
fn an_account_with_no_installed_app_is_not_read_twice() {
    let dir = ScratchDir::new("mcp-inventory-codex-no-apps");
    let replies = [
        SIGNED_IN_REPLIES[0],
        r#"{"id":3,"result":{"apps":[]}}"#,
        SIGNED_IN_REPLIES[2],
    ];
    let env = answering_codex(&dir, &replies);
    let (connectors, _) = read_in(&env, None, false).unwrap();
    assert!(connectors.is_empty());
    let input = std::fs::read_to_string(dir.join("input")).unwrap();
    assert!(!input.contains("app/read"), "{input}");
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

const TWO_SERVERS: [&str; 3] = [
    SIGNED_IN_REPLIES[0],
    SIGNED_IN_REPLIES[1],
    r#"{"id":4,"result":{"data":[{"name":"on","tools":{},"authStatus":"unsupported"},{"name":"off","tools":{},"authStatus":"unsupported"}]}}"#,
];

#[test]
fn a_server_codex_lists_as_turned_off_is_left_out() {
    let dir = ScratchDir::new("mcp-inventory-codex-disabled");
    let configured = r#"[{"name":"on","enabled":true,"transport":{"type":"stdio","env":{"K":"v"}}},
        {"name":"off","enabled":false,"disabled_reason":null}]"#;
    let env = answering_codex_with(&dir, configured, &TWO_SERVERS);
    let account = dir.join("account");
    std::fs::create_dir(&account).unwrap();
    let (_, servers) = read_in(&env, Some(&account), false).unwrap();
    let names: Vec<_> = servers.iter().map(|server| server.name.as_str()).collect();
    assert_eq!(names, ["on"]);
    let listed = std::fs::read_to_string(dir.join("listed")).unwrap();
    assert!(listed.starts_with("mcp list --json "), "{listed}");
    assert!(
        listed.contains(super::super::MCP_FILE_CREDENTIALS_STORE),
        "{listed}"
    );
    assert!(
        listed.trim_end().ends_with(&account.display().to_string()),
        "{listed}"
    );
}

#[test]
fn a_listing_of_servers_that_cannot_be_read_turns_none_off() {
    let dir = ScratchDir::new("mcp-inventory-codex-unlisted");
    let env = answering_codex_with(&dir, "error: unknown flag --json", &TWO_SERVERS);
    let (_, servers) = read_in(&env, None, false).unwrap();
    assert_eq!(servers.len(), 2);
    let listed = std::fs::read_to_string(dir.join("listed")).unwrap();
    assert!(!listed.contains("mcp_oauth_credentials_store"), "{listed}");
}

/// A fake Codex whose `mcp list` is `listing` and whose app server answers the first conversation
/// with [`SIGNED_IN_REPLIES`] and the second, the `app/read`, with `on_read`.
fn codex_failing_to_read_apps(dir: &Path, listing: &str, on_read: &str) -> HashMap<String, String> {
    std::fs::write(dir.join("replies"), SIGNED_IN_REPLIES.join("\n") + "\n").expect("replies");
    fake_agent(
        dir,
        AgentId::CODEX.descriptor().binary,
        &format!(
            "if [ \"$1\" = mcp ]; then {listing}; fi\n\
             while read line; do\n\
             case \"$line\" in *'mcpServerStatus/list'*) cat '{d}/replies';; \
             *'app/read'*) {on_read};; esac\n\
             done",
            d = dir.display(),
        ),
    )
}

#[test]
fn an_app_read_that_gets_no_answer_leaves_the_apps_their_runtime_names() {
    for (name, on_read, timeout) in [
        ("exits", "exit 1", PATIENCE),
        ("silent", ":", Duration::from_millis(2500)),
    ] {
        let dir = ScratchDir::new("mcp-inventory-codex-read-unanswered");
        let env = codex_failing_to_read_apps(&dir, "echo '[]'; exit 0", on_read);
        let (connectors, servers) = CodexMcpInventory
            .read(&env, None, false, timeout, &AtomicBool::new(false))
            .unwrap_or_else(|failure| panic!("{name}: {failure:?}"));
        assert_eq!(
            connectors,
            [PlatformConnector {
                id: "gmail".to_string(),
                name: "gmail".to_string(),
                state: ConnectorState::Connected,
                plugins: vec![],
            }],
            "{name}"
        );
        assert_eq!(servers.len(), 1, "{name}");
    }
}

#[test]
fn a_cancelled_app_read_still_fails_the_reading() {
    let dir = ScratchDir::new("mcp-inventory-codex-read-cancelled");
    let cancel = AtomicBool::new(false);
    // The fake's `app/read` never answers; setting the flag from a thread stops the reading.
    let env = codex_failing_to_read_apps(&dir, "echo '[]'; exit 0", "sleep 30");
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(1500));
            cancel.store(true, Ordering::Relaxed);
        });
        CodexMcpInventory.read(&env, None, false, Duration::from_secs(20), &cancel)
    });
    assert!(result.is_err());
}

#[test]
fn a_hanging_server_listing_still_lets_the_reading_proceed_with_every_server() {
    let dir = ScratchDir::new("mcp-inventory-codex-list-hangs");
    let env = codex_failing_to_read_apps(&dir, "while :; do sleep 1; done", "exit 1");
    let started = Instant::now();
    let (_, servers) = CodexMcpInventory
        .read(
            &env,
            None,
            false,
            Duration::from_secs(8),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(servers.len(), 1);
    assert!(started.elapsed() < Duration::from_secs(6));
}
