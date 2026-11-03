//! Claude Code's quota: `claude -p` in stream-JSON mode, given an `initialize` control request and
//! then `get_usage`. The reply's `limits[]` lists the windows with their kind; the older
//! `five_hour`, `seven_day` and `model_scoped` fields are read only when `limits[]` yields none.
//! Measured in `docs/agent-cli-reference.md` ("Plan quota", "Starting a quota window").

use serde_json::{json, Value};

use crate::accounts::quotas::diagnostics::{redact, truncate, LINE_LIMIT};
use crate::accounts::quotas::kick::{
    strings, KickCommand, KICK_PROMPT, KICK_SYSTEM_PROMPT, REPLY_TOKEN_CAP,
};
use crate::accounts::quotas::probe::{
    reading, QuotaProbe, Reading, ReplyProblem, SESSION_MINUTES, WEEK_MINUTES,
};
use crate::protocol::{QuotaWindow, QuotaWindowKind};
use crate::time::parse_rfc3339_millis;

pub(super) struct ClaudeQuota;

impl QuotaProbe for ClaudeQuota {
    fn args(&self) -> &'static [&'static str] {
        &[
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--no-session-persistence",
            // No MCP servers, so a probe starts none of the user's. Settings are left to load
            // as a session of the account would load them: they can carry the sign-in's
            // helper, environment, proxy and forced login, and a probe must read the same
            // identity the account's sessions use. The cost is that the user's plugins sync
            // and hooks are in play (see `docs/agent-cli-reference.md`, "Plan quota").
            "--strict-mcp-config",
            "--mcp-config",
            r#"{"mcpServers":{}}"#,
        ]
    }

    fn input(&self) -> Vec<String> {
        [
            json!({"type": "control_request", "request_id": "r1",
                "request": {"subtype": "initialize"}}),
            json!({"type": "control_request", "request_id": "r2",
                "request": {"subtype": "get_usage", "skip_behaviors": true}}),
        ]
        .iter()
        .map(Value::to_string)
        .collect()
    }

    fn is_reply(&self, value: &Value) -> bool {
        value.get("type").and_then(Value::as_str) == Some("control_response")
            && value
                .pointer("/response/request_id")
                .and_then(Value::as_str)
                == Some("r2")
    }

    fn companion(&self, value: &Value) -> Option<Value> {
        (value.get("type").and_then(Value::as_str) == Some("control_response"))
            .then(|| value.pointer("/response/response/account").cloned())
            .flatten()
    }

    fn read(&self, reply: &Value, companion: Option<&Value>) -> Result<Reading, ReplyProblem> {
        read_claude(reply, companion)
    }

    fn signed_out_without_reply(&self, companion: Option<&Value>) -> bool {
        claude_signed_out(companion)
    }

    /// The fields of `initialize`'s account that decide it, with the usage request's error beside
    /// them.
    fn signed_out_reason(&self, reply: Option<&Value>, companion: Option<&Value>) -> String {
        let field = |name: &str| {
            companion
                .and_then(|account| account.get(name))
                .map(|value| match value.as_str() {
                    Some(text) => truncate(&redact(text), 40),
                    None if value.is_null() => "none".to_string(),
                    None => "set".to_string(),
                })
                .unwrap_or_else(|| "absent".to_string())
        };
        let mut reason = format!(
            "initialize reported tokenSource {}, apiProvider {}, apiKeySource {}, \
             subscriptionType {}",
            field("tokenSource"),
            field("apiProvider"),
            field("apiKeySource"),
            field("subscriptionType")
        );
        if let Some(error) = reply.and_then(claude_response_error) {
            reason.push_str(&format!(
                "; the usage request was refused: {}",
                truncate(&redact(&error), LINE_LIMIT)
            ));
        }
        reason
    }
}

/// The request that starts Claude Code's usage windows: its cheapest model at its lowest reasoning
/// effort, run without a terminal, with no session kept.
pub(super) fn kick_command() -> KickCommand {
    KickCommand {
        args: strings(&[
            "-p",
            KICK_PROMPT,
            "--model",
            "haiku",
            "--effort",
            "low",
            "--no-session-persistence",
            "--system-prompt",
            KICK_SYSTEM_PROMPT,
            "--tools",
            "",
            "--disable-slash-commands",
            "--strict-mcp-config",
            "--mcp-config",
            r#"{"mcpServers":{}}"#,
            // Settings merge over the user's. No hooks (the user's and the plugins') and the
            // default output style, whose text would otherwise ride along in the request.
            "--settings",
            r#"{"disableAllHooks":true,"outputStyle":"default"}"#,
        ]),
        env: vec![
            ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", REPLY_TOKEN_CAP.to_string()),
            // No CLAUDE.md of the user's, which the request would carry as context.
            ("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1".to_string()),
        ],
    }
}

/// The error text of a Claude Code reply to a control request that answered with an error.
fn claude_response_error(reply: &Value) -> Option<String> {
    reply
        .pointer("/response/error")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Whether `account`, the `account` of Claude Code's answer to `initialize`, says it holds no
/// credential of any kind. Measured (2.1.311): an empty configuration directory answers
/// `{"tokenSource":"none","apiProvider":"firstParty"}`, but an API key (from the environment, the
/// settings' `env` or a key `/login` stored) also leaves `tokenSource` at `none` and adds
/// `apiKeySource`, a subscription login names its `tokenSource` and a cloud provider (Bedrock,
/// Vertex, Foundry) names itself as `apiProvider` without any `tokenSource`. So the account is
/// signed out only where the first-party provider is in use and none of those is there.
pub(crate) fn claude_signed_out(account: Option<&Value>) -> bool {
    let Some(account) = account else {
        return false;
    };
    let text = |field: &str| account.get(field).and_then(Value::as_str);
    matches!(text("apiProvider"), None | Some("firstParty"))
        && text("tokenSource") == Some("none")
        && text("apiKeySource").is_none_or(str::is_empty)
        && account.get("subscriptionType").is_none_or(Value::is_null)
}

pub(crate) fn read_claude(reply: &Value, account: Option<&Value>) -> Result<Reading, ReplyProblem> {
    // The usage reply of an account without a subscription is the same as a signed-out one's.
    if claude_signed_out(account) {
        return Err(ReplyProblem::NotSignedIn);
    }
    let response = reply.get("response").ok_or(ReplyProblem::Unreadable)?;
    if response.get("subtype").and_then(Value::as_str) == Some("error") {
        let detail = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the usage request was refused");
        return Err(ReplyProblem::Error(detail.to_string()));
    }
    let body = response.get("response").ok_or(ReplyProblem::Unreadable)?;
    let limits = match body.get("rate_limits") {
        Some(limits) if limits.is_object() => limits,
        // An account without a subscription has no limits to report.
        Some(Value::Null) => return Err(ReplyProblem::NoWindows),
        _ => return Err(ReplyProblem::Unreadable),
    };
    let mut windows: Vec<QuotaWindow> = limits
        .get("limits")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().filter_map(claude_limit).collect())
        .unwrap_or_default();
    if windows.is_empty() {
        windows = claude_fallback(limits);
    }
    let none_is_plain = body.get("rate_limits_available").and_then(Value::as_bool) == Some(false);
    reading(body.get("subscription_type"), windows, none_is_plain)
}

/// One entry of `limits[]`: the kinds Octoboard shows, and nothing else.
fn claude_limit(limit: &Value) -> Option<QuotaWindow> {
    let (kind, label, minutes) = match limit.get("kind").and_then(Value::as_str)? {
        "session" => (QuotaWindowKind::Session, None, SESSION_MINUTES),
        "weekly_all" => (QuotaWindowKind::Weekly, None, WEEK_MINUTES),
        "weekly_scoped" => {
            let name = limit
                .pointer("/scope/model/display_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())?;
            (
                QuotaWindowKind::WeeklyModel,
                Some(name.to_string()),
                WEEK_MINUTES,
            )
        }
        _ => return None,
    };
    Some(QuotaWindow {
        kind,
        label,
        used_percent: limit.get("percent").and_then(Value::as_f64)?,
        resets_at: limit
            .get("resets_at")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_millis),
        window_minutes: Some(minutes),
    })
}

/// The older fields, for a reply whose `limits[]` gave nothing: `five_hour`, `seven_day` and the
/// entries of `model_scoped`.
fn claude_fallback(limits: &Value) -> Vec<QuotaWindow> {
    let window = |value: &Value, kind, label, minutes| {
        Some(QuotaWindow {
            kind,
            label,
            used_percent: value.get("utilization").and_then(Value::as_f64)?,
            resets_at: value
                .get("resets_at")
                .and_then(Value::as_str)
                .and_then(parse_rfc3339_millis),
            window_minutes: Some(minutes),
        })
    };
    let mut windows = Vec::new();
    for (field, kind, minutes) in [
        ("five_hour", QuotaWindowKind::Session, SESSION_MINUTES),
        ("seven_day", QuotaWindowKind::Weekly, WEEK_MINUTES),
    ] {
        windows.extend(
            limits
                .get(field)
                .and_then(|value| window(value, kind, None, minutes)),
        );
    }
    if let Some(scoped) = limits.get("model_scoped").and_then(Value::as_array) {
        windows.extend(scoped.iter().filter_map(|entry| {
            let name = entry
                .get("display_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())?;
            window(
                entry,
                QuotaWindowKind::WeeklyModel,
                Some(name.to_string()),
                WEEK_MINUTES,
            )
        }));
    }
    windows
}
