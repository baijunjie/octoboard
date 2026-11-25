//! Tests of Codex's quota probe: how its replies read and what its probe sends.

use super::*;
use crate::accounts::quotas::diagnostics::ProbeTrace;
use crate::accounts::quotas::probe::test_support::{
    answering_agent, fake, probe_of, replying_agent, run_probe, unavailable_code,
};
use crate::adapter::provider;
use crate::protocol::{quota_unavailable_code as code, AgentId};
use crate::test_support::{ScratchDir, PATIENCE};
use serde_json::{json, Value};

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
            AgentId::CODEX,
            &[
                ("account/rateLimits/read", &refusal),
                ("account/read", &account),
            ],
        );
        assert_eq!(unavailable_code(probe_of(AgentId::CODEX, &env)), expected);
    }
    // A reading that arrives is returned without `account/read` ever answering.
    let readings: Value = serde_json::from_str(REPLY).unwrap();
    let env = replying_agent(
        &dir,
        AgentId::CODEX,
        &[("account/rateLimits/read", &readings)],
    );
    assert!(probe_of(AgentId::CODEX, &env).is_ok());
}

#[test]
fn codex_reports_its_primary_and_secondary_windows_and_plan() {
    let dir = ScratchDir::new("quota-codex");
    let env = answering_agent(&dir, AgentId::CODEX, REPLY);
    let reading = run_probe(
        AgentId::CODEX,
        &env,
        None,
        PATIENCE,
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

/// A Codex reply holding the quota of `REPLY` and `credits` as its
/// `rateLimitResetCredits`.
fn codex_reply_with_credits(credits: Option<Value>) -> Value {
    let mut reply: Value = serde_json::from_str(REPLY).unwrap();
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
fn codex_refusing_a_home_that_does_not_exist_is_a_missing_config_directory() {
    let detail = r#"ended without replying (exit status: 1): Error: CODEX_HOME points to "/h/.codex", but that path does not exist"#;
    let missing_home = |agent: AgentId, detail| {
        let probe = provider(agent).accounts().unwrap().quota_probe().unwrap();
        probe.missing_config_dir(detail).map(str::to_string)
    };
    assert_eq!(
        missing_home(AgentId::CODEX, detail).as_deref(),
        Some("/h/.codex")
    );
    assert_eq!(missing_home(AgentId::CLAUDE, detail), None);
    assert_eq!(missing_home(AgentId::CODEX, "not logged in"), None);

    let dir = ScratchDir::new("quota-codex-home");
    let env = fake(
        &dir,
        AgentId::CODEX,
        "echo 'Error loading configuration: CODEX_HOME points to \"/h/.codex\", but that path does not exist' >&2; exit 1",
    );
    let unavailable = run_probe(
        AgentId::CODEX,
        &env,
        None,
        PATIENCE,
        &mut ProbeTrace::default(),
    )
    .expect_err("refused");
    assert_eq!(unavailable.code, code::CONFIG_DIR_MISSING);
    assert_eq!(unavailable.params["dir"], "/h/.codex");
}
