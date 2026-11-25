//! Tests of Grok Build's quota probe: how its replies read and what its probe sends.

use super::*;
use crate::accounts::quotas::diagnostics::ProbeTrace;
use crate::accounts::quotas::probe::test_support::{
    answering_agent, probe_of, replying_agent, run_probe, unavailable_code,
};
use crate::protocol::{quota_unavailable_code as code, AgentId};
use crate::test_support::{ScratchDir, PATIENCE};
use serde_json::{json, Value};

/// Grok Build 1.0.92's billing reply right after a weekly reset: a period, and no percent.
const REPLY_PERIOD_UNUSED: &str =
    include_str!("../../../../testdata/grok_billing_reply_period_unused.json");

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
            let env = replying_agent(&dir, AgentId::GROK, &replies);
            assert_eq!(unavailable_code(probe_of(AgentId::GROK, &env)), expected);
        }
    }
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
    let env = answering_agent(&dir, AgentId::GROK, REPLY);
    let reading = run_probe(
        AgentId::GROK,
        &env,
        None,
        PATIENCE,
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
fn a_grok_period_with_nothing_used_reads_as_zero_percent() {
    // Grok Build 1.0.92, right after a weekly reset: the percent is left out, the period is there.
    let dir = ScratchDir::new("quota-grok-unused");
    let env = answering_agent(&dir, AgentId::GROK, REPLY_PERIOD_UNUSED);
    let reading = probe_of(AgentId::GROK, &env).expect("a readable reply");
    assert_eq!(reading.plan.as_deref(), Some("SuperGrok Lite"));
    assert_eq!(
        reading.windows,
        [QuotaWindow {
            kind: QuotaWindowKind::CreditPeriod,
            label: None,
            used_percent: 0.0,
            resets_at: Some(1_795_708_297_667),
            window_minutes: Some(10080),
        }]
    );
}

#[test]
fn a_grok_reply_without_a_percent_or_a_period_is_unreadable() {
    for config in [
        json!({}),
        json!({"currentPeriod": null}),
        json!({"currentPeriod": "weekly"}),
    ] {
        let reply = json!({"jsonrpc": "2.0", "id": 2, "result": {
            "config": config, "subscription_tier": "SuperGrok"}});
        assert_eq!(
            read_grok(&reply, None),
            Err(ReplyProblem::Unreadable),
            "{reply}"
        );
    }
}

#[test]
fn a_grok_percent_that_is_not_a_number_is_unreadable_even_with_a_period() {
    for percent in [json!("12.5"), json!(null), json!({"val": 1}), json!(true)] {
        let reply = json!({"jsonrpc": "2.0", "id": 2, "result": {
            "config": {
                "creditUsagePercent": percent,
                "currentPeriod": {
                    "start": "2026-11-19T15:51:37.667+00:00",
                    "end": "2026-11-26T15:51:37.667+00:00"}}}});
        assert_eq!(
            read_grok(&reply, None),
            Err(ReplyProblem::Unreadable),
            "{reply}"
        );
    }
}
