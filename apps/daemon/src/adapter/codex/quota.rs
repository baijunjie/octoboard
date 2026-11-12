//! Codex's quota: `codex app-server`, given `initialize`, `initialized`, `account/rateLimits/read`
//! and `account/read`: a primary and a secondary window and the plan, who is signed in, and the
//! rate limit reset credits that can be used. Measured in `docs/agent-cli-reference.md` ("Plan
//! quota", "Starting a quota window").

use serde_json::{json, Value};

use super::app_server::{self, CODEX_QUIET_FEATURES};
use crate::accounts::quotas::kick::{strings, KickCommand, KICK_PROMPT, KICK_SYSTEM_PROMPT};
use crate::accounts::quotas::probe::{
    is_second_response, reading, refusal_problem, QuotaProbe, Reading, ReplyProblem, WEEK_MINUTES,
};
use crate::agents::json_rpc;
use crate::protocol::{AgentId, QuotaWindow, QuotaWindowKind, ResetCredit, ResetCredits};

pub(super) struct CodexQuota;

impl QuotaProbe for CodexQuota {
    fn args(&self) -> &'static [&'static str] {
        app_server::CODEX_APP_SERVER_ARGS
    }

    fn input(&self) -> Vec<String> {
        [
            app_server::initialize(),
            app_server::initialized(),
            json!({"jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read"}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "account/read", "params": {}}),
        ]
        .iter()
        .map(Value::to_string)
        .collect()
    }

    fn is_reply(&self, value: &Value) -> bool {
        is_second_response(value)
    }

    fn companion(&self, value: &Value) -> Option<Value> {
        json_rpc::is_reply_to(value, 3).then(|| value.clone())
    }

    fn needs_companion(&self, reply: &Value) -> bool {
        is_codex_auth_refusal(reply)
    }

    fn read(&self, reply: &Value, companion: Option<&Value>) -> Result<Reading, ReplyProblem> {
        read_codex(reply, companion)
    }

    /// The directory in Codex's refusal of a `CODEX_HOME` that does not exist (`CODEX_HOME points
    /// to "<dir>", but that path does not exist`).
    fn missing_config_dir<'a>(&self, detail: &'a str) -> Option<&'a str> {
        let rest = detail.split_once("CODEX_HOME points to \"")?.1;
        let (dir, after) = rest.split_once('"')?;
        after.contains("does not exist").then_some(dir)
    }

    fn signed_out_reason(&self, _reply: Option<&Value>, _companion: Option<&Value>) -> String {
        "the rate limits request was refused for want of a sign-in and \
         account/read reports no account"
            .to_string()
    }
}

/// What Codex is told on top of [`CODEX_QUIET_FEATURES`] to leave its tools out of the request.
/// Measured against an API key, only `shell_tool`, `goals` and `view_image` shrink it. The rest
/// (`unified_exec`, `multi_agent`, `sleep_tool` and `image_generation`) are kept off as a
/// precaution, as are the browser, computer, tool-suggestion and skill-search ones, for the tools
/// a ChatGPT sign-in may add.
const CODEX_TOOL_FEATURES: &[&str] = &[
    "-c",
    "features.shell_tool=false",
    "-c",
    "features.unified_exec=false",
    "-c",
    "features.sleep_tool=false",
    "-c",
    "features.view_image=false",
    "-c",
    "features.image_generation=false",
    "-c",
    "features.goals=false",
    "-c",
    "features.multi_agent=false",
    "-c",
    "features.browser_use=false",
    "-c",
    "features.browser_use_external=false",
    "-c",
    "features.in_app_browser=false",
    "-c",
    "features.computer_use=false",
    "-c",
    "features.tool_suggest=false",
    "-c",
    "features.skill_search=false",
];

/// The request that starts Codex's usage windows: its cheapest model at its lowest reasoning
/// effort, run without a terminal, with no session kept.
pub(super) fn kick_command() -> KickCommand {
    let mut args = strings(&[
        "exec",
        "--ephemeral",
        "--skip-git-repo-check",
        // Not the user's `config.toml`, whose MCP servers, `notify` program and plugins
        // would all start: a `-c` override merges into those tables rather than emptying
        // them. The sign-in is read from `CODEX_HOME` all the same.
        "--ignore-user-config",
        "--ignore-rules",
        "-m",
        "gpt-6-luna",
        "-c",
        r#"model_reasoning_effort="low""#,
        "-c",
        &format!(r#"instructions="{KICK_SYSTEM_PROMPT}""#),
        "-c",
        "include_permissions_instructions=false",
        "-c",
        "include_apps_instructions=false",
        "-c",
        "include_environment_context=false",
        "-c",
        "include_collaboration_mode_instructions=false",
        "-c",
        r#"web_search="disabled""#,
    ]);
    args.extend(strings(CODEX_QUIET_FEATURES));
    args.extend(strings(CODEX_TOOL_FEATURES));
    args.push(KICK_PROMPT.to_string());
    KickCommand {
        args,
        env: Vec::new(),
    }
}

/// Codex refuses `account/rateLimits/read` ("codex account authentication required to read rate
/// limits", or "chatgpt authentication required ..." for a ChatGPT login) both for a home that is
/// signed in to nothing and for one that holds an API key or uses another provider, which have no
/// quota to read. The code alone is generic, so it is the message that marks the refusal; which of
/// the cases it is comes from `account/read`.
fn is_codex_auth_refusal(reply: &Value) -> bool {
    json_rpc::error_detail(reply).is_some_and(|detail| {
        detail
            .to_lowercase()
            .contains("authentication required to read rate limits")
    })
}

/// Codex's `account/read` reply as "signed out" (`account: null` with `requiresOpenaiAuth: true`),
/// "signed in some other way" (an account such as `{"type":"apiKey"}`, or a provider that needs no
/// OpenAI login), or unreadable, which includes an error reply.
fn codex_signed_out(account_reply: Option<&Value>) -> Option<bool> {
    let result = account_reply?.get("result")?;
    let account = result.get("account")?;
    let requires_auth = result.get("requiresOpenaiAuth").and_then(Value::as_bool)?;
    Some(account.is_null() && requires_auth)
}

pub(crate) fn read_codex(
    reply: &Value,
    account_reply: Option<&Value>,
) -> Result<Reading, ReplyProblem> {
    if is_codex_auth_refusal(reply) {
        return Err(refusal_problem(
            codex_signed_out(account_reply),
            AgentId::CODEX.descriptor().label,
        ));
    }
    if let Some(detail) = json_rpc::error_detail(reply) {
        return Err(ReplyProblem::Error(detail));
    }
    let limits = reply
        .pointer("/result/rateLimits")
        .filter(|limits| limits.is_object())
        .ok_or(ReplyProblem::Unreadable)?;
    let windows = ["primary", "secondary"]
        .iter()
        .enumerate()
        .filter_map(|(position, field)| {
            let window = limits.get(field)?;
            let minutes = window
                .get("windowDurationMins")
                .and_then(Value::as_u64)
                .and_then(|minutes| u32::try_from(minutes).ok());
            // The length says what a window is; where it is not given, the position does.
            let kind = match minutes {
                Some(minutes) if minutes >= WEEK_MINUTES => QuotaWindowKind::Weekly,
                Some(_) => QuotaWindowKind::Session,
                None if position == 0 => QuotaWindowKind::Session,
                None => QuotaWindowKind::Weekly,
            };
            Some(QuotaWindow {
                kind,
                label: None,
                used_percent: window.get("usedPercent").and_then(Value::as_f64)?,
                resets_at: window
                    .get("resetsAt")
                    .and_then(Value::as_i64)
                    .map(|seconds| seconds.saturating_mul(1000)),
                window_minutes: minutes,
            })
        })
        .collect();
    let mut reading = reading(limits.get("planType"), windows, false)?;
    reading.reset_credits = reply
        .pointer("/result/rateLimitResetCredits")
        .and_then(codex_reset_credits);
    Ok(reading)
}

/// The usable credits in Codex's `rateLimitResetCredits` (`availableCount` and `credits`, which
/// is null when only the count is known), or `None` for a count of zero or a field in a shape
/// this code does not know: the credits are an addition to the reading, never a reason to fail
/// it. The listed credits are the ones whose `status` is `available`, in the order they would be
/// used (see [`reset_credit_order`]).
fn codex_reset_credits(summary: &Value) -> Option<ResetCredits> {
    let available_count = summary
        .get("availableCount")
        .and_then(Value::as_u64)
        .and_then(|count| u32::try_from(count).ok())
        .filter(|count| *count > 0)?;
    let credits = summary
        .get("credits")
        .and_then(Value::as_array)
        .map(|list| {
            let mut credits: Vec<ResetCredit> =
                list.iter().filter_map(codex_reset_credit).collect();
            credits.sort_by_key(reset_credit_order);
            credits
        });
    Some(ResetCredits {
        available_count,
        credits,
    })
}

/// One entry of Codex's `credits[]`, when it is a credit that can be used.
fn codex_reset_credit(credit: &Value) -> Option<ResetCredit> {
    if credit.get("status").and_then(Value::as_str) != Some("available") {
        return None;
    }
    let text =
        |field: &str| crate::text::trimmed_or_none(credit.get(field).and_then(Value::as_str));
    let millis = |field: &str| {
        credit
            .get(field)
            .and_then(Value::as_i64)
            .map(|seconds| seconds.saturating_mul(1000))
    };
    // The id goes back to Codex as it came, so it is not trimmed.
    let id = credit
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())?;
    Some(ResetCredit {
        id: id.to_string(),
        title: text("title"),
        description: text("description"),
        granted_at: millis("grantedAt"),
        expires_at: millis("expiresAt"),
    })
}

/// Where a credit stands in the order they are used: the one that expires first, a credit with no
/// expiry last.
fn reset_credit_order(credit: &ResetCredit) -> (bool, i64) {
    (credit.expires_at.is_none(), credit.expires_at.unwrap_or(0))
}
