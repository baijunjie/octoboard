//! Asking an agent's own CLI for an account's quota. Each probe is a short conversation over
//! lines on the CLI's stdin and stdout (`crate::subprocess::run_exchange`), measured in
//! `docs/agent-cli-reference.md` ("Plan quota"):
//!
//! - **Claude Code** is `claude -p` in stream-JSON mode, given an `initialize` control request
//!   and then `get_usage`. The reply's `limits[]` lists the windows with their kind; the older
//!   `five_hour`, `seven_day` and `model_scoped` fields are read only when `limits[]` yields none.
//! - **Codex** is `codex app-server`, given `initialize`, `initialized`, `account/rateLimits/read`
//!   and `account/read`: a primary and a secondary window and the plan, and who is signed in.
//! - **Grok Build** is `grok agent --no-leader stdio`, given ACP `initialize` and `_x.ai/billing`:
//!   one credit pool over a billing period and the subscription tier.
//!
//! The agents mark these calls experimental, so a reply that cannot be read is an
//! [`QuotaUnavailable`] for that account and nothing else. An account that is not signed in is
//! told from each agent's own structured signal and reported as its own state rather than as a
//! failure, and only where the agent's own reply rules out every other way of being signed in: an
//! API key, a cloud provider or a custom provider also leaves a quota request refused or empty
//! (see [`claude_signed_out`], [`read_codex`] and [`read_grok`]). Grok Build's quota request is
//! also refused with an error whose `data` says its credentials are invalid or expired once a
//! stored sign-in has expired. A probe runs in the login-shell snapshot with the agent's
//! configuration-directory variable pointed at the account's directory, and with the CLI told to
//! load as little of the user's setup as it can without losing its sign-in.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::{json, Value};

use crate::adapter;
use crate::protocol::{
    quota_unavailable_code as code, Agent, QuotaUnavailable, QuotaWindow, QuotaWindowKind,
};
use crate::rfc3339::parse_rfc3339_millis;
use crate::subprocess::{run_exchange, Bounds, ExchangeFailure, Reach};

/// How long a probe may take. The CLIs answer in one to three seconds on an idle machine, but a
/// Claude Code start has been measured at 17 seconds under heavy load (several test suites running
/// at once); this leaves room for that and a refreshed login while keeping a hung CLI from holding
/// a slot for long.
pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// The most a probe may print. Claude Code's reply to `initialize` lists every command and runs
/// to hundreds of kilobytes.
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

/// How much of a probe's or a kick's stderr is kept for the failure that reports it.
pub(super) const STDERR_LIMIT: usize = 4096;

const SESSION_MINUTES: u32 = 5 * 60;
const WEEK_MINUTES: u32 = 7 * 24 * 60;

/// The Codex configuration overrides that keep it from reaching out on start: the plugin catalogue
/// sync (which runs `git ls-remote` over SSH), apps, and the user's hooks. Set as configuration
/// rather than `--disable`, which refuses a feature name the installed Codex does not know, where
/// an unknown configuration key is ignored.
pub(super) const CODEX_QUIET_FEATURES: &[&str] = &[
    "-c",
    "features.plugins=false",
    "-c",
    "features.remote_plugin=false",
    "-c",
    "features.apps=false",
    "-c",
    "features.hooks=false",
];

/// How Codex's app server is started, here and for signing an account in.
pub(crate) const CODEX_APP_SERVER_ARGS: &[&str] = &APP_SERVER_ARGS;

const APP_SERVER_ARGS: [&str; 1 + CODEX_QUIET_FEATURES.len()] = {
    let mut args = ["app-server"; 1 + CODEX_QUIET_FEATURES.len()];
    let mut i = 0;
    while i < CODEX_QUIET_FEATURES.len() {
        args[i + 1] = CODEX_QUIET_FEATURES[i];
        i += 1;
    }
    args
};

/// What a readable reply holds.
#[derive(Debug, PartialEq)]
pub(super) struct Reading {
    pub plan: Option<String>,
    pub windows: Vec<QuotaWindow>,
}

/// Why a reply that arrived gives no reading.
#[derive(Debug, PartialEq)]
enum ReplyProblem {
    /// Not in a shape this code knows.
    Unreadable,
    /// In a shape it knows, saying the account has no quota windows.
    NoWindows,
    /// The CLI answered the request with an error.
    Error(String),
    /// The CLI answered that the account is not signed in.
    NotSignedIn,
}

/// One agent's side of the conversation: what to run, what to write, which line is the reply,
/// and how to read it.
struct Conversation {
    args: &'static [&'static str],
    input: Vec<String>,
    is_reply: fn(&Value) -> bool,
    /// The part of another reply that [`Conversation::read`] needs along with the reply: who is
    /// signed in, as each agent says it.
    companion: fn(&Value) -> Option<Value>,
    /// Whether the exchange must hold the companion before the reply can be read, which is decided
    /// by the reply, so that a reading that needs no companion is not held up for one.
    needs_companion: fn(&Value) -> bool,
    read: fn(&Value, Option<&Value>) -> Result<Reading, ReplyProblem>,
}

fn conversation(agent: Agent) -> Conversation {
    let lines = |values: Vec<Value>| values.iter().map(Value::to_string).collect();
    match agent {
        Agent::Claude => Conversation {
            args: &[
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
            ],
            input: lines(vec![
                json!({"type": "control_request", "request_id": "r1",
                    "request": {"subtype": "initialize"}}),
                json!({"type": "control_request", "request_id": "r2",
                    "request": {"subtype": "get_usage", "skip_behaviors": true}}),
            ]),
            is_reply: |value| {
                value.get("type").and_then(Value::as_str) == Some("control_response")
                    && value
                        .pointer("/response/request_id")
                        .and_then(Value::as_str)
                        == Some("r2")
            },
            companion: |value| {
                (value.get("type").and_then(Value::as_str) == Some("control_response"))
                    .then(|| value.pointer("/response/response/account").cloned())
                    .flatten()
            },
            needs_companion: |_| false,
            read: read_claude,
        },
        Agent::Codex => Conversation {
            args: CODEX_APP_SERVER_ARGS,
            input: lines(vec![
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {"clientInfo": {"name": "octoboard",
                        "version": env!("CARGO_PKG_VERSION")}}}),
                json!({"jsonrpc": "2.0", "method": "initialized"}),
                json!({"jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read"}),
                json!({"jsonrpc": "2.0", "id": 3, "method": "account/read", "params": {}}),
            ]),
            is_reply: is_second_response,
            companion: |value| is_response(value, 3).then(|| value.clone()),
            needs_companion: |reply| is_codex_auth_refusal(reply),
            read: read_codex,
        },
        Agent::Grok => Conversation {
            args: &["agent", "--no-leader", "stdio"],
            input: lines(vec![
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {"protocolVersion": 1}}),
                json!({"jsonrpc": "2.0", "id": 2, "method": "_x.ai/billing", "params": {}}),
            ]),
            is_reply: is_second_response,
            // Only the field that says which sign-in methods the CLI would use by default.
            companion: |value| {
                is_response(value, 1).then(|| {
                    value
                        .pointer("/result/_meta")
                        .filter(|meta| meta.is_object())
                        .cloned()
                        .unwrap_or(Value::Null)
                })
            },
            needs_companion: |reply| is_grok_auth_refusal(reply),
            read: read_grok,
        },
    }
}

/// The JSON-RPC response to the request with id 2, as opposed to a notification or a request the
/// server makes of its own.
fn is_second_response(value: &Value) -> bool {
    is_response(value, 2)
}

/// The JSON-RPC response to the request with `id`.
fn is_response(value: &Value, id: i64) -> bool {
    value.get("id").and_then(Value::as_i64) == Some(id) && value.get("method").is_none()
}

/// Runs the probe for `agent` and reads its reply. Blocking; setting `cancel` stops it, which
/// reports as a failure.
pub(super) fn probe(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<Reading, QuotaUnavailable> {
    let conversation = conversation(agent);
    // The account's directory must not change sharing under the CLI while it runs.
    let _reading = crate::shared_config::admitted_reading(config_dir)
        .map_err(|missing| config_dir_missing(agent, &missing.0.to_string_lossy()))?;
    let mut command = adapter::account_command(agent, shell_env, config_dir, conversation.args)
        .map_err(|binary| {
            QuotaUnavailable::new(
                code::BINARY_NOT_FOUND,
                format!("`{binary}` was not found on PATH in the shell environment"),
                &[("binary", binary)],
            )
        })?;
    let bounds = Bounds {
        timeout,
        stdout_limit: OUTPUT_LIMIT,
        stderr_limit: STDERR_LIMIT,
        reach: Reach::Session,
    };
    let (is_reply, companion_of, needs_companion) = (
        conversation.is_reply,
        conversation.companion,
        conversation.needs_companion,
    );
    // The reply that is read, and the part of another reply that says who is signed in: the two
    // arrive in either order.
    let reply: RefCell<Option<Value>> = RefCell::new(None);
    let companion: RefCell<Option<Value>> = RefCell::new(None);
    run_exchange(&mut command, &conversation.input, &bounds, cancel, |line| {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        if let Some(part) = companion_of(&value) {
            *companion.borrow_mut() = Some(part);
        }
        if is_reply(&value) {
            *reply.borrow_mut() = Some(value);
        }
        reply
            .borrow()
            .as_ref()
            .is_some_and(|reply| !needs_companion(reply) || companion.borrow().is_some())
    })
    .map_err(|failure| match failure {
        // Claude Code's sign-in state is in a reply other than the one that is read.
        _ if agent == Agent::Claude && claude_signed_out(companion.borrow().as_ref()) => {
            not_signed_in(agent)
        }
        ExchangeFailure::TimedOut(_) => QuotaUnavailable::new(
            code::TIMED_OUT,
            format!("{} did not report its quota in time", agent.label()),
            &[("agent", agent.label())],
        ),
        other => {
            let detail = other.to_string();
            match missing_codex_home(agent, &detail) {
                Some(dir) => config_dir_missing(agent, dir),
                None => failed(agent, &detail),
            }
        }
    })?;
    let value = reply.take().ok_or_else(|| unreadable(agent))?;
    let companion = companion.take();
    (conversation.read)(&value, companion.as_ref()).map_err(|problem| match problem {
        ReplyProblem::NotSignedIn => not_signed_in(agent),
        ReplyProblem::Unreadable => unreadable(agent),
        ReplyProblem::NoWindows => QuotaUnavailable::new(
            code::NO_WINDOWS,
            format!("{} reports no quota for this account", agent.label()),
            &[("agent", agent.label())],
        ),
        ReplyProblem::Error(detail) => failed(agent, &detail),
    })
}

pub(super) fn config_dir_missing(agent: Agent, dir: &str) -> QuotaUnavailable {
    QuotaUnavailable::new(
        code::CONFIG_DIR_MISSING,
        format!(
            "{}'s configuration directory {dir} does not exist yet",
            agent.label()
        ),
        &[("agent", agent.label()), ("dir", dir)],
    )
}

/// The directory in Codex's refusal of a `CODEX_HOME` that does not exist (`CODEX_HOME points to
/// "<dir>", but that path does not exist`), which is how the default account's missing directory
/// shows, since nothing is looked for before its probe is run.
fn missing_codex_home(agent: Agent, detail: &str) -> Option<&str> {
    if agent != Agent::Codex {
        return None;
    }
    let rest = detail.split_once("CODEX_HOME points to \"")?.1;
    let (dir, after) = rest.split_once('"')?;
    after.contains("does not exist").then_some(dir)
}

fn not_signed_in(agent: Agent) -> QuotaUnavailable {
    QuotaUnavailable::new(
        code::NOT_SIGNED_IN,
        format!("{} is not signed in for this account", agent.label()),
        &[("agent", agent.label())],
    )
}

/// Whether `account`, the `account` of Claude Code's answer to `initialize`, says it holds no
/// credential of any kind. Measured (2.1.311): an empty configuration directory answers
/// `{"tokenSource":"none","apiProvider":"firstParty"}`, but an API key (from the environment, the
/// settings' `env` or a key `/login` stored) also leaves `tokenSource` at `none` and adds
/// `apiKeySource`, a subscription login names its `tokenSource` and a cloud provider (Bedrock,
/// Vertex, Foundry) names itself as `apiProvider` without any `tokenSource`. So the account is
/// signed out only where the first-party provider is in use and none of those is there.
fn claude_signed_out(account: Option<&Value>) -> bool {
    let Some(account) = account else {
        return false;
    };
    let text = |field: &str| account.get(field).and_then(Value::as_str);
    matches!(text("apiProvider"), None | Some("firstParty"))
        && text("tokenSource") == Some("none")
        && text("apiKeySource").is_none_or(str::is_empty)
        && account.get("subscriptionType").is_none_or(Value::is_null)
}

pub(super) fn failed(agent: Agent, detail: &str) -> QuotaUnavailable {
    QuotaUnavailable::new(
        code::COMMAND_FAILED,
        format!("reading {}'s quota failed: {detail}", agent.label()),
        &[("agent", agent.label()), ("detail", detail)],
    )
}

fn unreadable(agent: Agent) -> QuotaUnavailable {
    QuotaUnavailable::new(
        code::UNREADABLE_REPLY,
        format!(
            "{} answered with a quota that could not be read",
            agent.label()
        ),
        &[("agent", agent.label())],
    )
}

/// The reading in a reply with `windows`, or the problem when there is none: no window at all is
/// `NoWindows` where the reply says plainly that the account has no limits, else `Unreadable`.
fn reading(
    plan: Option<&Value>,
    windows: Vec<QuotaWindow>,
    none_is_plain: bool,
) -> Result<Reading, ReplyProblem> {
    if windows.is_empty() {
        return Err(if none_is_plain {
            ReplyProblem::NoWindows
        } else {
            ReplyProblem::Unreadable
        });
    }
    Ok(Reading {
        plan: plan
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|plan| !plan.is_empty())
            .map(str::to_string),
        windows,
    })
}

fn read_claude(reply: &Value, account: Option<&Value>) -> Result<Reading, ReplyProblem> {
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

/// A JSON-RPC error response's detail, if the reply is one: its message joined to its `data`,
/// since Grok Build refuses an account whose stored credentials have expired with a bare "Internal
/// error" (code -32603) and the cause only in `data` ("Billing service error: Invalid or expired
/// credentials (...)").
fn rpc_error_detail(reply: &Value) -> Option<String> {
    let error = reply.get("error")?;
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty());
    let data = error
        .get("data")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|data| !data.is_empty() && Some(*data) != message);
    Some(match (message, data) {
        (Some(message), Some(data)) => format!("{message}: {data}"),
        (Some(text), None) | (None, Some(text)) => text.to_string(),
        (None, None) => "the request was refused".to_string(),
    })
}

/// Codex refuses `account/rateLimits/read` ("codex account authentication required to read rate
/// limits", or "chatgpt authentication required ..." for a ChatGPT login) both for a home that is
/// signed in to nothing and for one that holds an API key or uses another provider, which have no
/// quota to read. The code alone is generic, so it is the message that marks the refusal; which of
/// the cases it is comes from `account/read`.
fn is_codex_auth_refusal(reply: &Value) -> bool {
    rpc_error_detail(reply).is_some_and(|detail| {
        detail
            .to_lowercase()
            .contains("authentication required to read rate limits")
    })
}

/// Grok Build's refusal of the billing request of an account with no credentials (code -32000,
/// "Authentication required"). Like Codex's it does not say whether another way of signing in is
/// in use; the `initialize` reply does.
fn is_grok_auth_refusal(reply: &Value) -> bool {
    reply.pointer("/error/code").and_then(Value::as_i64) == Some(-32000)
        && rpc_error_detail(reply)
            .is_some_and(|detail| detail.to_lowercase().contains("authentication required"))
}

/// Who a refused quota request leaves the account as, told from `signed_out`: `Some(true)` when
/// the agent says nothing is signed in, `Some(false)` when it says something else is, and `None`
/// when what it says cannot be read.
fn refusal_problem(signed_out: Option<bool>, agent_name: &str) -> ReplyProblem {
    match signed_out {
        Some(true) => ReplyProblem::NotSignedIn,
        Some(false) => ReplyProblem::NoWindows,
        None => ReplyProblem::Error(format!(
            "{agent_name} refused the quota request and did not say whether it is signed in"
        )),
    }
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

fn read_codex(reply: &Value, account_reply: Option<&Value>) -> Result<Reading, ReplyProblem> {
    if is_codex_auth_refusal(reply) {
        return Err(refusal_problem(codex_signed_out(account_reply), "Codex"));
    }
    if let Some(detail) = rpc_error_detail(reply) {
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
    reading(limits.get("planType"), windows, false)
}

fn read_grok(reply: &Value, initialize_meta: Option<&Value>) -> Result<Reading, ReplyProblem> {
    if is_grok_auth_refusal(reply) {
        // Signed out where no sign-in method other than grok.com is the default; an API key in
        // the environment or the configuration is offered as the default instead.
        let signed_out = initialize_meta
            .and_then(|meta| meta.get("defaultAuthMethodId"))
            .map(Value::is_null);
        return Err(refusal_problem(signed_out, "Grok Build"));
    }
    if let Some(detail) = rpc_error_detail(reply) {
        return Err(
            if detail
                .to_lowercase()
                .contains("invalid or expired credentials")
            {
                ReplyProblem::NotSignedIn
            } else {
                ReplyProblem::Error(detail)
            },
        );
    }
    let result = reply.get("result").ok_or(ReplyProblem::Unreadable)?;
    let config = result.get("config").ok_or(ReplyProblem::Unreadable)?;
    let period = config.get("currentPeriod");
    let bound = |field: &str| {
        period?
            .get(field)
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_millis)
    };
    let (start, end) = (bound("start"), bound("end"));
    let window = QuotaWindow {
        kind: QuotaWindowKind::CreditPeriod,
        label: None,
        used_percent: config
            .get("creditUsagePercent")
            .and_then(Value::as_f64)
            .ok_or(ReplyProblem::Unreadable)?,
        resets_at: end,
        window_minutes: start
            .zip(end)
            .and_then(|(start, end)| u32::try_from((end - start) / 60_000).ok())
            .filter(|minutes| *minutes > 0),
    };
    reading(result.get("subscription_tier"), vec![window], false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, until_the_shell_ran, ScratchDir};

    const CLAUDE_REPLY: &str = include_str!("../../testdata/claude_get_usage_reply.json");
    const CODEX_REPLY: &str = include_str!("../../testdata/codex_rate_limits_reply.json");
    const GROK_REPLY: &str = include_str!("../../testdata/grok_billing_reply.json");

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
        let var = adapter::config_dir_var(agent);
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
    fn replying_agent(
        dir: &Path,
        agent: Agent,
        replies: &[(&str, &Value)],
    ) -> HashMap<String, String> {
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
        // The reply ends the run, so the fake may stop before it has logged the last request.
        assert_eq!(
            methods[..3],
            ["initialize", "initialized", "account/rateLimits/read"]
        );
        assert!(methods.get(3).is_none_or(|method| method == "account/read"));
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
                    &AtomicBool::new(false)
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
                let result = probe(agent, &env, None, SHORT, &AtomicBool::new(false));
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
                &AtomicBool::new(false)
            )),
            code::BINARY_NOT_FOUND
        );
    }
}
