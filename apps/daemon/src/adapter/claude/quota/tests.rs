//! Tests of Claude Code's quota probe: how its replies read and what its probe sends.

use super::*;
use crate::accounts::quotas::diagnostics::ProbeTrace;
use crate::accounts::quotas::probe::test_support::{
    answering_agent, probe_of, replying_agent, run_probe, unavailable_code,
};
use crate::protocol::{quota_unavailable_code as code, AgentId};
use crate::test_support::{ScratchDir, PATIENCE};
use serde_json::{json, Value};

#[test]
fn claude_reports_its_session_weekly_and_model_windows_from_limits() {
    let dir = ScratchDir::new("quota-claude");
    let env = answering_agent(&dir, AgentId::CLAUDE, REPLY);
    let reading = run_probe(
        AgentId::CLAUDE,
        &env,
        None,
        PATIENCE,
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
            AgentId::CLAUDE,
            &[("initialize", &init), ("get_usage", &no_limits)],
        );
        assert_eq!(
            unavailable_code(probe_of(AgentId::CLAUDE, &env)),
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
    let usage: Value = serde_json::from_str(REPLY).unwrap();
    let env = replying_agent(
        &dir,
        AgentId::CLAUDE,
        &[("initialize", &init), ("get_usage", &usage)],
    );
    assert!(probe_of(AgentId::CLAUDE, &env).is_ok());
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
        AgentId::CLAUDE,
        &[("initialize", &init), ("get_usage", &usage)],
    );
    let mut trace = ProbeTrace::default();
    let result = run_probe(AgentId::CLAUDE, &env, None, PATIENCE, &mut trace);
    assert_eq!(unavailable_code(result), code::NOT_SIGNED_IN);
    let reason = trace.reason.expect("a reason");
    assert!(
        reason.contains("tokenSource none, apiProvider firstParty, apiKeySource absent"),
        "{reason}"
    );
    assert!(reason.contains("the usage request was refused"), "{reason}");
    assert!(!reason.contains("me@example.com"), "{reason}");
}
