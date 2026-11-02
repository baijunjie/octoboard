use super::*;
use crate::adapter;
use crate::test_support::{fake_agent, until_the_shell_ran, ScratchDir};

const CLAUDE_REPLY: &str = include_str!("../../../../testdata/claude_get_usage_reply.json");
const CODEX_REPLY: &str = include_str!("../../../../testdata/codex_rate_limits_reply.json");
const GROK_REPLY: &str = include_str!("../../../../testdata/grok_billing_reply.json");

/// Long enough for a fake agent to start on a loaded machine, short enough to wait out.
const SHORT: Duration = Duration::from_millis(700);

fn reply_of(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => CLAUDE_REPLY,
        Agent::Codex => CODEX_REPLY,
        Agent::Grok => GROK_REPLY,
    }
}

/// What the request the reply answers is recognised by in the fake's input.
fn request_marker(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "get_usage",
        Agent::Codex => "account/rateLimits/read",
        Agent::Grok => "_x.ai/billing",
    }
}

/// A fake CLI named after `agent`'s binary in `dir`, running `body`; the environment it is
/// found through is returned.
fn fake(dir: &Path, agent: Agent, body: &str) -> HashMap<String, String> {
    fake_agent(dir, adapter::adapter_for(agent).binary(), body)
}

/// A fake that records its arguments, the directory variable and each line it is given under
/// `dir`, and answers the request `agent`'s probe ends with by printing `reply` after a line
/// that is not the reply.
fn answering_agent(dir: &Path, agent: Agent, reply: &str) -> HashMap<String, String> {
    std::fs::write(dir.join("reply"), reply).expect("reply");
    let var = crate::agents::layout::layout(agent).config_dir_var;
    fake(
        dir,
        agent,
        &format!(
            "echo \"$*\" > '{d}/args'; echo \"${var}\" > '{d}/dir'\n\
             while read line; do echo \"$line\" >> '{d}/input'\n\
             case \"$line\" in *'{marker}'*) echo 'not the reply'; cat '{d}/reply';; esac\n\
             done",
            d = dir.display(),
            marker = request_marker(agent),
        ),
    )
}

fn unavailable_code(result: Result<Reading, QuotaUnavailable>) -> crate::protocol::Code {
    result.expect_err("an unavailable quota").code
}

#[test]
fn claude_reports_its_session_weekly_and_model_windows_from_limits() {
    let dir = ScratchDir::new("quota-claude");
    let env = answering_agent(&dir, Agent::Claude, CLAUDE_REPLY);
    let reading = probe(
        Agent::Claude,
        &env,
        None,
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
    .expect("a readable reply");
    assert_eq!(reading.plan.as_deref(), Some("max"));
    let kinds: Vec<_> = reading
        .windows
        .iter()
        .map(|window| (window.kind, window.label.as_deref(), window.used_percent))
        .collect();
    assert_eq!(
        kinds,
        [
            (QuotaWindowKind::Session, None, 27.0),
            (QuotaWindowKind::Weekly, None, 7.0),
            (QuotaWindowKind::WeeklyModel, Some("Fable"), 0.0),
        ]
    );
    assert_eq!(reading.windows[0].resets_at, Some(1_792_258_799_899));
    assert_eq!(reading.windows[1].resets_at, Some(1_792_843_199_899));
    assert_eq!(reading.windows[0].window_minutes, Some(300));
    // The framing the CLI was measured to need.
    assert_eq!(
        std::fs::read_to_string(dir.join("args")).unwrap().trim(),
        "-p --input-format stream-json --output-format stream-json --verbose \
         --no-session-persistence --strict-mcp-config --mcp-config {\"mcpServers\":{}}"
    );
    let input = std::fs::read_to_string(dir.join("input")).unwrap();
    let lines: Vec<Value> = input
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines[0]["request"]["subtype"], "initialize");
    assert_eq!(lines[0]["request_id"], "r1");
    assert_eq!(lines[1]["request"]["subtype"], "get_usage");
    assert_eq!(lines[1]["request_id"], "r2");
}

#[test]
fn claude_falls_back_to_the_older_fields_when_limits_gives_nothing() {
    let reply = json!({"type": "control_response", "response": {
        "subtype": "success", "request_id": "r2", "response": {
            "subscription_type": "pro", "rate_limits_available": true,
            "rate_limits": {
                "five_hour": {"utilization": 40, "resets_at": "2026-10-17T17:39:59+00:00"},
                "seven_day": {"utilization": 12, "resets_at": null},
                "seven_day_opus": null,
                "limits": [{"kind": "something_new", "percent": 99}],
                "model_scoped": [
                    {"display_name": "Fable", "utilization": 3,
                     "resets_at": "2026-10-24T12:00:00Z"},
                    {"display_name": " ", "utilization": 50}]}}}});
    let reading = read_claude(&reply, None).expect("readable");
    let summary: Vec<_> = reading
        .windows
        .iter()
        .map(|window| (window.kind, window.label.as_deref(), window.used_percent))
        .collect();
    assert_eq!(
        summary,
        [
            (QuotaWindowKind::Session, None, 40.0),
            (QuotaWindowKind::Weekly, None, 12.0),
            (QuotaWindowKind::WeeklyModel, Some("Fable"), 3.0),
        ]
    );
    assert_eq!(reading.windows[1].resets_at, None);
}

#[test]
fn claude_without_a_subscription_has_no_windows_and_an_error_reply_is_a_failure() {
    let none = json!({"type": "control_response", "response": {
        "subtype": "success", "request_id": "r2", "response": {
            "subscription_type": null, "rate_limits_available": false,
            "rate_limits": null}}});
    assert_eq!(read_claude(&none, None), Err(ReplyProblem::NoWindows));
    let refused = json!({"type": "control_response", "response": {
        "subtype": "error", "request_id": "r2", "error": "Not logged in"}});
    assert_eq!(
        read_claude(&refused, None),
        Err(ReplyProblem::Error("Not logged in".to_string()))
    );
}

/// A fake agent that prints, for each input line containing a marker, the reply beside it, in
/// the order the markers are listed here.
fn replying_agent(dir: &Path, agent: Agent, replies: &[(&str, &Value)]) -> HashMap<String, String> {
    let arms: String = replies
        .iter()
        .map(|(marker, reply)| format!("*'{marker}'*) echo '{reply}';;\n"))
        .collect();
    fake(
        dir,
        agent,
        &format!("while read line; do case \"$line\" in\n{arms}esac; done"),
    )
}

fn probe_of(agent: Agent, env: &HashMap<String, String>) -> Result<Reading, QuotaUnavailable> {
    probe(
        agent,
        env,
        None,
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
}

#[test]
fn claude_is_signed_out_only_with_no_token_key_subscription_or_other_provider() {
    // Measured against Claude Code 2.1.311 in scratch homes.
    let account = |text: &str| serde_json::from_str::<Value>(text).unwrap();
    for (text, signed_out) in [
        (r#"{"tokenSource":"none","apiProvider":"firstParty"}"#, true),
        (r#"{"tokenSource":"none"}"#, true),
        // An API key, from the environment, the settings' env or `/login`.
        (
            r#"{"tokenSource":"none","apiKeySource":"ANTHROPIC_API_KEY","apiProvider":"firstParty"}"#,
            false,
        ),
        (r#"{"apiKeySource":"ANTHROPIC_API_KEY"}"#, false),
        // A cloud provider names itself and has no token source.
        (r#"{"apiProvider":"bedrock"}"#, false),
        (r#"{"apiProvider":"vertex"}"#, false),
        (r#"{"apiProvider":"foundry"}"#, false),
        (r#"{"tokenSource":"none","apiProvider":"bedrock"}"#, false),
        // A subscription login.
        (
            r#"{"tokenSource":"claude.ai","apiKeySource":"ANTHROPIC_API_KEY","apiProvider":"firstParty"}"#,
            false,
        ),
        (
            r#"{"tokenSource":"none","subscriptionType":"max","apiProvider":"firstParty"}"#,
            false,
        ),
    ] {
        assert_eq!(
            claude_signed_out(Some(&account(text))),
            signed_out,
            "{text}"
        );
    }
    assert!(!claude_signed_out(None));
}

#[test]
fn claude_whose_initialize_reply_holds_no_credential_is_not_signed_in() {
    let dir = ScratchDir::new("quota-claude-signed-out");
    let initialize = |account: &str| {
        json!({"type": "control_response", "response": {"subtype": "success",
            "request_id": "r1", "response": {"account": serde_json::from_str::<Value>(account).unwrap()}}})
    };
    // An account without a subscription answers the usage request the same way, so only the
    // `initialize` reply tells the two apart.
    let no_limits = json!({"type": "control_response", "response": {
        "subtype": "success", "request_id": "r2", "response": {
            "subscription_type": null, "rate_limits_available": false, "rate_limits": null}}});
    for (account, expected) in [
        (
            r#"{"tokenSource":"none","apiProvider":"firstParty"}"#,
            code::NOT_SIGNED_IN,
        ),
        (
            r#"{"tokenSource":"none","apiKeySource":"ANTHROPIC_API_KEY","apiProvider":"firstParty"}"#,
            code::NO_WINDOWS,
        ),
        (r#"{"apiProvider":"bedrock"}"#, code::NO_WINDOWS),
    ] {
        let init = initialize(account);
        let env = replying_agent(
            &dir,
            Agent::Claude,
            &[("initialize", &init), ("get_usage", &no_limits)],
        );
        assert_eq!(
            unavailable_code(probe_of(Agent::Claude, &env)),
            expected,
            "{account}"
        );
    }
}

#[test]
fn claude_with_a_subscription_reads_its_windows_whatever_its_account_says() {
    let dir = ScratchDir::new("quota-claude-subscription");
    let init = json!({"type": "control_response", "response": {"subtype": "success",
        "request_id": "r1", "response": {"account": {"tokenSource": "claude.ai",
            "subscriptionType": "max", "apiProvider": "firstParty"}}}});
    let usage: Value = serde_json::from_str(CLAUDE_REPLY).unwrap();
    let env = replying_agent(
        &dir,
        Agent::Claude,
        &[("initialize", &init), ("get_usage", &usage)],
    );
    assert!(probe_of(Agent::Claude, &env).is_ok());
}

/// The replies to `account/read` Codex 0.163.0 gave in scratch homes.
fn codex_account(account: Value, requires_openai_auth: bool) -> Value {
    json!({"id": 3, "result": {"account": account,
        "requiresOpenaiAuth": requires_openai_auth, "workspaceRouting": null}})
}

#[test]
fn codex_refusing_its_quota_is_signed_out_only_with_no_account_and_openai_auth_required() {
    for message in [
        "codex account authentication required to read rate limits",
        "chatgpt authentication required to read rate limits",
    ] {
        let refusal = json!({"id": 2, "error": {"code": -32600, "message": message}});
        let read = |account: Option<Value>| read_codex(&refusal, account.as_ref());
        // Nothing signed in.
        assert_eq!(
            read(Some(codex_account(Value::Null, true))),
            Err(ReplyProblem::NotSignedIn)
        );
        // An API key, Amazon Bedrock, and custom or local providers: no quota to read.
        for other in [
            codex_account(json!({"type": "apiKey"}), true),
            codex_account(
                json!({"type": "amazonBedrock", "usesCodexManagedCredentials": false}),
                false,
            ),
            codex_account(Value::Null, false),
        ] {
            assert_eq!(read(Some(other)), Err(ReplyProblem::NoWindows));
        }
        // `account/read` failing or missing leaves it unknown, which is a failed reading.
        for unknown in [
            Some(json!({"id": 3, "error": {"code": -32603, "message": "unauthorized (401)"}})),
            Some(json!({"id": 3, "result": {}})),
            None,
        ] {
            assert!(matches!(read(unknown), Err(ReplyProblem::Error(_))));
        }
    }
    // Any other error is still just an error.
    let other = json!({"id": 2, "error": {"code": -32603,
        "message": "failed to fetch codex rate limits: 401 Unauthorized"}});
    assert!(matches!(
        read_codex(&other, Some(&codex_account(Value::Null, true))),
        Err(ReplyProblem::Error(_))
    ));
}

#[test]
fn codex_probe_waits_for_account_read_only_when_its_quota_is_refused() {
    let refusal = json!({"id": 2, "error": {"code": -32600,
        "message": "codex account authentication required to read rate limits"}});
    let dir = ScratchDir::new("quota-codex-signed-out");
    for (account, expected) in [
        (codex_account(Value::Null, true), code::NOT_SIGNED_IN),
        (
            codex_account(json!({"type": "apiKey"}), true),
            code::NO_WINDOWS,
        ),
        (codex_account(Value::Null, false), code::NO_WINDOWS),
    ] {
        let env = replying_agent(
            &dir,
            Agent::Codex,
            &[
                ("account/rateLimits/read", &refusal),
                ("account/read", &account),
            ],
        );
        assert_eq!(unavailable_code(probe_of(Agent::Codex, &env)), expected);
    }
    // A reading that arrives is returned without `account/read` ever answering.
    let readings: Value = serde_json::from_str(CODEX_REPLY).unwrap();
    let env = replying_agent(
        &dir,
        Agent::Codex,
        &[("account/rateLimits/read", &readings)],
    );
    assert!(probe_of(Agent::Codex, &env).is_ok());
}

#[test]
fn grok_refusing_its_billing_is_signed_out_only_without_another_default_sign_in() {
    let refusal = json!({"jsonrpc": "2.0", "id": 2, "error": {"code": -32000,
        "message": "Authentication required",
        "data": "Authentication required to fetch billing data"}});
    // `_meta` of Grok Build 1.0.70's `initialize` reply.
    let signed_out = json!({"defaultAuthMethodId": null});
    let api_key = json!({"defaultAuthMethodId": "xai.api_key"});
    assert_eq!(
        read_grok(&refusal, Some(&signed_out)),
        Err(ReplyProblem::NotSignedIn)
    );
    assert_eq!(
        read_grok(&refusal, Some(&api_key)),
        Err(ReplyProblem::NoWindows)
    );
    // An `initialize` reply that cannot be read leaves it unknown.
    for unknown in [None, Some(Value::Null), Some(json!({}))] {
        assert!(matches!(
            read_grok(&refusal, unknown.as_ref()),
            Err(ReplyProblem::Error(_))
        ));
    }
    let expired = json!({"jsonrpc": "2.0", "id": 2, "error": {"code": -32603,
        "message": "Internal error",
        "data": "Billing service error: Invalid or expired credentials (auth_kind=bearer, \
                 x_xai_token_auth=xai-grok-cli, upstream=PermissionDenied, \
                 reason=no auth context)"}});
    assert_eq!(read_grok(&expired, None), Err(ReplyProblem::NotSignedIn));
}

#[test]
fn grok_probe_tells_a_refused_account_by_its_initialize_reply_in_either_order() {
    let refusal = json!({"jsonrpc": "2.0", "id": 2, "error": {"code": -32000,
        "message": "Authentication required",
        "data": "Authentication required to fetch billing data"}});
    let initialize = |default: Value| {
        json!({"jsonrpc": "2.0", "id": 1, "result": {"protocolVersion": 1,
            "_meta": {"defaultAuthMethodId": default}}})
    };
    let dir = ScratchDir::new("quota-grok-refused");
    for (default, expected) in [
        (Value::Null, code::NOT_SIGNED_IN),
        (json!("xai.api_key"), code::NO_WINDOWS),
    ] {
        let init = initialize(default);
        // Grok Build has been seen to refuse billing before it answers `initialize`, and the
        // other way round.
        for replies in [
            [("_x.ai/billing", &refusal), ("initialize", &init)],
            [("initialize", &init), ("_x.ai/billing", &refusal)],
        ] {
            let env = replying_agent(&dir, Agent::Grok, &replies);
            assert_eq!(unavailable_code(probe_of(Agent::Grok, &env)), expected);
        }
    }
}

#[test]
fn codex_reports_its_primary_and_secondary_windows_and_plan() {
    let dir = ScratchDir::new("quota-codex");
    let env = answering_agent(&dir, Agent::Codex, CODEX_REPLY);
    let reading = probe(
        Agent::Codex,
        &env,
        None,
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
    .expect("a readable reply");
    assert_eq!(reading.plan.as_deref(), Some("plus"));
    assert_eq!(
        reading.windows,
        [
            QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 15.0,
                resets_at: Some(1_792_261_723_000),
                window_minutes: Some(300),
            },
            QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent: 46.0,
                resets_at: Some(1_792_653_318_000),
                window_minutes: Some(10080),
            },
        ]
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("args")).unwrap().trim(),
        "app-server -c features.plugins=false -c features.remote_plugin=false -c features.apps=false \
         -c features.hooks=false"
    );
    let input = std::fs::read_to_string(dir.join("input")).unwrap();
    let methods: Vec<String> = input
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .map(|line| line["method"].as_str().unwrap().to_string())
        .collect();
    // The run waits for the fake to exit once its input has ended, so every request is logged.
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "account/rateLimits/read",
            "account/read"
        ]
    );
}

/// A Codex reply holding the quota of `CODEX_REPLY` and `credits` as its
/// `rateLimitResetCredits`.
fn codex_reply_with_credits(credits: Option<Value>) -> Value {
    let mut reply: Value = serde_json::from_str(CODEX_REPLY).unwrap();
    if let Some(credits) = credits {
        reply["result"]["rateLimitResetCredits"] = credits;
    }
    reply
}

#[test]
fn codex_reports_the_reset_credits_it_can_use_in_the_order_they_would_be_used() {
    let reply = codex_reply_with_credits(Some(json!({
        "availableCount": 3,
        "credits": [
            {"id": "none", "resetType": "codexRateLimits", "status": "available",
             "grantedAt": 1_791_413_100, "expiresAt": null, "title": null,
             "description": null},
            {"id": "used", "resetType": "codexRateLimits", "status": "redeemed",
             "grantedAt": 1_791_413_100, "expiresAt": 1_794_005_000, "title": "Used",
             "description": null},
            {"id": "late", "resetType": "codexRateLimits", "status": "available",
             "grantedAt": 1_791_413_200, "expiresAt": 1_794_005_200, "title": "Late",
             "description": "Later"},
            {"id": "soon", "resetType": "codexRateLimits", "status": "available",
             "grantedAt": 1_791_413_156, "expiresAt": 1_794_005_156,
             "title": " Full reset (Weekly + 5 hr) ", "description": ""},
        ],
    })));
    let credits = read_codex(&reply, None).unwrap().reset_credits.unwrap();
    assert_eq!(credits.available_count, 3);
    let listed = credits.credits.unwrap();
    assert_eq!(
        listed.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["soon", "late", "none"]
    );
    assert_eq!(
        listed[0],
        ResetCredit {
            id: "soon".to_string(),
            title: Some("Full reset (Weekly + 5 hr)".to_string()),
            description: None,
            granted_at: Some(1_791_413_156_000),
            expires_at: Some(1_794_005_156_000),
        }
    );
    assert_eq!(listed[2].expires_at, None);
}

#[test]
fn a_reset_credit_keeps_its_id_as_given_and_survives_a_missing_grant_time() {
    let reply = codex_reply_with_credits(Some(json!({
        "availableCount": 3,
        "credits": [
            {"id": " spaced ", "status": "available", "grantedAt": "soon",
             "expiresAt": 1_794_005_156},
            {"id": "", "status": "available", "grantedAt": 1, "expiresAt": 2},
            {"id": "ungranted", "status": "available", "expiresAt": 1_794_005_100},
        ],
    })));
    let listed = read_codex(&reply, None)
        .unwrap()
        .reset_credits
        .unwrap()
        .credits
        .unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|c| (c.id.as_str(), c.granted_at))
            .collect::<Vec<_>>(),
        [("ungranted", None), (" spaced ", None)]
    );
}

#[test]
fn codex_reset_credits_with_only_a_count_keep_the_count() {
    let reply = codex_reply_with_credits(Some(json!({"availableCount": 2, "credits": null})));
    let credits = read_codex(&reply, None).unwrap().reset_credits.unwrap();
    assert_eq!((credits.available_count, credits.credits), (2, None));
}

#[test]
fn no_reset_credits_and_unreadable_ones_never_fail_the_reading() {
    for credits in [
        None,
        Some(Value::Null),
        Some(json!({"availableCount": 0, "credits": []})),
        Some(json!({"availableCount": "one"})),
        Some(json!("credits")),
        Some(json!({"availableCount": -1})),
    ] {
        let reading = read_codex(&codex_reply_with_credits(credits.clone()), None)
            .unwrap_or_else(|problem| panic!("{credits:?}: {problem:?}"));
        assert_eq!(reading.reset_credits, None, "{credits:?}");
        assert_eq!(reading.windows.len(), 2);
    }
}

#[test]
fn a_codex_window_is_what_its_length_says_and_a_missing_length_falls_back_to_position() {
    let reply = json!({"id": 2, "result": {"rateLimits": {
        "primary": {"usedPercent": 10, "windowDurationMins": 10080},
        "secondary": {"usedPercent": 20},
        "planType": "free"}}});
    let reading = read_codex(&reply, None).expect("readable");
    assert_eq!(reading.windows[0].kind, QuotaWindowKind::Weekly);
    assert_eq!(reading.windows[1].kind, QuotaWindowKind::Weekly);
    assert_eq!(reading.windows[1].window_minutes, None);
    assert_eq!(
        read_codex(
            &json!({"id": 2, "error": {"code": -32600, "message": "not signed in"}}),
            None
        ),
        Err(ReplyProblem::Error("not signed in".to_string()))
    );
}

#[test]
fn an_error_reply_carries_its_data_into_the_detail() {
    assert_eq!(
        read_grok(
            &json!({"id": 2, "error": {"code": -32603, "message": "Internal error",
            "data": "Billing service error: upstream unavailable"}}),
            None
        ),
        Err(ReplyProblem::Error(
            "Internal error: Billing service error: upstream unavailable".to_string()
        ))
    );
    assert_eq!(
        read_grok(
            &json!({"id": 2, "error": {"message": "Internal error",
            "data": "Internal error"}}),
            None
        ),
        Err(ReplyProblem::Error("Internal error".to_string()))
    );
}

#[test]
fn grok_reports_its_credit_pool_over_the_current_period_and_tier() {
    let dir = ScratchDir::new("quota-grok");
    let env = answering_agent(&dir, Agent::Grok, GROK_REPLY);
    let reading = probe(
        Agent::Grok,
        &env,
        None,
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
    .expect("a readable reply");
    assert_eq!(reading.plan.as_deref(), Some("SuperGrok"));
    assert_eq!(
        reading.windows,
        [QuotaWindow {
            kind: QuotaWindowKind::CreditPeriod,
            label: None,
            used_percent: 100.0,
            resets_at: Some(1_792_459_835_740),
            window_minutes: Some(10080),
        }]
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("args")).unwrap().trim(),
        "agent --no-leader stdio"
    );
    let input = std::fs::read_to_string(dir.join("input")).unwrap();
    let methods: Vec<String> = input
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .map(|line| line["method"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(methods, ["initialize", "_x.ai/billing"]);
}

#[test]
fn a_grok_pool_without_a_period_still_reads() {
    let reply = json!({"jsonrpc": "2.0", "id": 2, "result": {
        "config": {"creditUsagePercent": 12.5}}});
    let reading = read_grok(&reply, None).expect("readable");
    assert_eq!(reading.plan, None);
    assert_eq!(reading.windows[0].resets_at, None);
    assert_eq!(reading.windows[0].window_minutes, None);
}

#[test]
fn an_account_directory_is_exported_under_the_agents_own_variable() {
    for agent in Agent::ALL {
        let dir = ScratchDir::new("quota-dir");
        let env = answering_agent(&dir, agent, reply_of(agent));
        let account_dir = dir.join("account");
        std::fs::create_dir(&account_dir).unwrap();
        probe(
            agent,
            &env,
            Some(&account_dir),
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
            &mut ProbeTrace::default(),
        )
        .expect("a readable reply");
        assert_eq!(
            std::fs::read_to_string(dir.join("dir")).unwrap().trim(),
            account_dir.to_string_lossy(),
            "{agent:?}"
        );
        // The default account is left to the shell, which here exports nothing.
        probe(
            agent,
            &env,
            None,
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
            &mut ProbeTrace::default(),
        )
        .expect("a readable reply");
        assert_eq!(
            std::fs::read_to_string(dir.join("dir")).unwrap().trim(),
            "",
            "{agent:?}"
        );
    }
}

#[test]
fn a_reply_that_is_not_in_a_known_shape_is_unreadable() {
    let replies = [
        (
            Agent::Claude,
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"r2","response":{}}}"#,
        ),
        (Agent::Codex, r#"{"id":2,"result":{}}"#),
        (Agent::Grok, r#"{"jsonrpc":"2.0","id":2,"result":{}}"#),
    ];
    for (agent, reply) in replies {
        let dir = ScratchDir::new("quota-unreadable");
        let env = answering_agent(&dir, agent, &format!("{reply}\n"));
        assert_eq!(
            unavailable_code(probe(
                agent,
                &env,
                None,
                crate::test_support::PATIENCE,
                &AtomicBool::new(false),
                &mut ProbeTrace::default()
            )),
            code::UNREADABLE_REPLY,
            "{agent:?}"
        );
    }
}

#[test]
fn a_cli_that_never_replies_times_out() {
    for agent in Agent::ALL {
        // Answers nothing, as a CLI waiting on a login would; the marker shows it started,
        // since a fake killed before it ran proves nothing.
        let got = until_the_shell_ran(|| {
            let dir = ScratchDir::new("quota-timeout");
            let env = fake(
                &dir,
                agent,
                &format!(
                    "echo started > '{}/started'; cat > /dev/null",
                    dir.display()
                ),
            );
            let result = probe(
                agent,
                &env,
                None,
                SHORT,
                &AtomicBool::new(false),
                &mut ProbeTrace::default(),
            );
            dir.join("started")
                .exists()
                .then(|| unavailable_code(result))
        });
        assert_eq!(got, code::TIMED_OUT, "{agent:?}");
    }
}

#[test]
fn a_cli_that_exits_without_replying_fails_with_what_it_wrote() {
    for agent in Agent::ALL {
        let dir = ScratchDir::new("quota-exit");
        let env = fake(&dir, agent, "echo 'not logged in' >&2; exit 1");
        let unavailable = probe(
            agent,
            &env,
            None,
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
            &mut ProbeTrace::default(),
        )
        .expect_err("no reply");
        assert_eq!(unavailable.code, code::COMMAND_FAILED, "{agent:?}");
        assert!(
            unavailable.params["detail"].ends_with("not logged in"),
            "{unavailable:?}"
        );
    }
}

/// A read admitted after the account's directory was moved (a removal held it back) runs
/// nothing, which would create the directory again, and reports it missing.
#[test]
fn a_read_admitted_after_the_directory_was_moved_runs_nothing() {
    let dir = ScratchDir::new("quota-probe-moved");
    let ran = dir.join("ran");
    let env = fake(&dir, Agent::Codex, &format!("touch '{}'", ran.display()));
    let account = dir.join("account");
    let unavailable = probe(
        Agent::Codex,
        &env,
        Some(&account),
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
    .expect_err("no reading");
    assert_eq!(unavailable.code, code::CONFIG_DIR_MISSING);
    assert!(!ran.exists(), "the CLI was not run");
    assert!(!account.exists());
}

#[test]
fn codex_refusing_a_home_that_does_not_exist_is_a_missing_config_directory() {
    let detail = r#"ended without replying (exit status: 1): Error: CODEX_HOME points to "/h/.codex", but that path does not exist"#;
    assert_eq!(missing_codex_home(Agent::Codex, detail), Some("/h/.codex"));
    assert_eq!(missing_codex_home(Agent::Claude, detail), None);
    assert_eq!(missing_codex_home(Agent::Codex, "not logged in"), None);

    let dir = ScratchDir::new("quota-codex-home");
    let env = fake(
        &dir,
        Agent::Codex,
        "echo 'Error loading configuration: CODEX_HOME points to \"/h/.codex\", but that path does not exist' >&2; exit 1",
    );
    let unavailable = probe(
        Agent::Codex,
        &env,
        None,
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
    .expect_err("refused");
    assert_eq!(unavailable.code, code::CONFIG_DIR_MISSING);
    assert_eq!(unavailable.params["dir"], "/h/.codex");
}

#[test]
fn a_missing_binary_is_reported_as_such() {
    let dir = ScratchDir::new("quota-missing");
    let env = HashMap::from([("PATH".to_string(), dir.to_string_lossy().into_owned())]);
    assert_eq!(
        unavailable_code(probe(
            Agent::Codex,
            &env,
            None,
            SHORT,
            &AtomicBool::new(false),
            &mut ProbeTrace::default()
        )),
        code::BINARY_NOT_FOUND
    );
}

#[test]
fn a_program_that_ignores_the_end_of_its_input_is_killed_and_the_trace_says_how_long_after() {
    let dir = ScratchDir::new("quota-ignores-eof");
    std::fs::write(dir.join("reply"), CLAUDE_REPLY).expect("reply");
    let env = fake(
        &dir,
        Agent::Claude,
        &format!(
            "read a; read b; cat '{d}/reply'; exec /bin/sleep 300",
            d = dir.display()
        ),
    );
    let mut trace = ProbeTrace::default();
    let reading = probe(
        Agent::Claude,
        &env,
        None,
        Duration::from_millis(3500),
        &AtomicBool::new(false),
        &mut trace,
    );
    assert!(reading.is_ok(), "{reading:?}");
    let stopped = trace.stopped.expect("the program was stopped");
    assert!(
        stopped.starts_with("stopped ")
            && stopped.ends_with(" after its reply: it did not exit in time"),
        "{stopped}"
    );
    assert!(trace.pid.is_some());
}

#[test]
fn an_error_line_besides_the_reply_is_kept_redacted_in_the_notes() {
    let dir = ScratchDir::new("quota-error-line");
    std::fs::write(dir.join("reply"), CLAUDE_REPLY).expect("reply");
    let env = fake(
        &dir,
        Agent::Claude,
        &format!(
            "while read line; do case \"$line\" in *get_usage*) \
             echo '{{\"type\":\"error\",\"message\":\"refresh failed for me@example.com\"}}'; \
             echo 'a plain line'; cat '{d}/reply';; esac; done",
            d = dir.display()
        ),
    );
    let mut trace = ProbeTrace::default();
    probe(
        Agent::Claude,
        &env,
        None,
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut trace,
    )
    .expect("a readable reply");
    assert_eq!(trace.notes.len(), 1, "{:?}", trace.notes);
    assert!(trace.notes[0].contains("refresh failed"));
    assert!(
        !trace.notes[0].contains("me@example.com"),
        "{:?}",
        trace.notes
    );
    assert_eq!(trace.stopped, None);
}

#[test]
fn the_signed_out_reason_of_claude_names_the_fields_that_decided_it() {
    let dir = ScratchDir::new("quota-claude-reason");
    let init = json!({"type": "control_response", "response": {"subtype": "success",
        "request_id": "r1", "response": {"account":
            {"tokenSource": "none", "apiProvider": "firstParty",
             "email": "me@example.com"}}}});
    let usage = json!({"type": "control_response", "response": {"subtype": "error",
        "request_id": "r2", "error": "no credentials for me@example.com"}});
    let env = replying_agent(
        &dir,
        Agent::Claude,
        &[("initialize", &init), ("get_usage", &usage)],
    );
    let mut trace = ProbeTrace::default();
    let result = probe(
        Agent::Claude,
        &env,
        None,
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut trace,
    );
    assert_eq!(unavailable_code(result), code::NOT_SIGNED_IN);
    let reason = trace.reason.expect("a reason");
    assert!(
        reason.contains("tokenSource none, apiProvider firstParty, apiKeySource absent"),
        "{reason}"
    );
    assert!(reason.contains("the usage request was refused"), "{reason}");
    assert!(!reason.contains("me@example.com"), "{reason}");
}
