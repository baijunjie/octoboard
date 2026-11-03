//! Asking an agent's own CLI for an account's quota. Each probe is a short conversation over
//! lines on the CLI's stdin and stdout (`crate::processes::subprocess::run_exchange`), measured in
//! `docs/agent-cli-reference.md` ("Plan quota"):
//!
//! - **Claude Code** is `claude -p` in stream-JSON mode, given an `initialize` control request
//!   and then `get_usage`. The reply's `limits[]` lists the windows with their kind; the older
//!   `five_hour`, `seven_day` and `model_scoped` fields are read only when `limits[]` yields none.
//! - **Codex** is `codex app-server`, given `initialize`, `initialized`, `account/rateLimits/read`
//!   and `account/read`: a primary and a secondary window and the plan, who is signed in, and the
//!   rate limit reset credits that can be used.
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

use super::diagnostics::{redact, stderr_tail, truncate, ProbeTrace, LINE_LIMIT, MAX_NOTES};
use crate::adapter::account_cli::{self, NotRun};
use crate::agents::{codex_app_server, json_rpc};
use crate::processes::subprocess::{
    run_exchange_traced, AfterReply, Bounds, ExchangeFailure, ExchangeTrace, AGENT_REPLY_GRACE,
};
use crate::protocol::{
    quota_unavailable_code as code, Agent, QuotaUnavailable, QuotaWindow, QuotaWindowKind,
    ResetCredit, ResetCredits,
};
use crate::time::parse_rfc3339_millis;

/// How long a probe may take. The CLIs answer in one to three seconds on an idle machine, but a
/// Claude Code start has been measured at 17 seconds under heavy load (several test suites running
/// at once); this leaves room for that and a refreshed login while keeping a hung CLI from holding
/// a slot for long.
pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// The most a probe may print. Claude Code's reply to `initialize` lists every command and runs
/// to hundreds of kilobytes.
pub(super) const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

const SESSION_MINUTES: u32 = 5 * 60;
const WEEK_MINUTES: u32 = 7 * 24 * 60;

/// What a readable reply holds.
#[derive(Debug, PartialEq)]
pub(super) struct Reading {
    pub plan: Option<String>,
    pub windows: Vec<QuotaWindow>,
    /// Codex's rate limit reset credits that can be used; `None` for the other agents and for
    /// an account with none.
    pub reset_credits: Option<ResetCredits>,
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
            args: codex_app_server::CODEX_APP_SERVER_ARGS,
            input: lines(vec![
                codex_app_server::initialize(),
                codex_app_server::initialized(),
                json!({"jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read"}),
                json!({"jsonrpc": "2.0", "id": 3, "method": "account/read", "params": {}}),
            ]),
            is_reply: is_second_response,
            companion: |value| json_rpc::is_reply_to(value, 3).then(|| value.clone()),
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
                json_rpc::is_reply_to(value, 1).then(|| {
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
    json_rpc::is_reply_to(value, 2)
}

/// Runs the probe for `agent` and reads its reply. Blocking; setting `cancel` stops it, which
/// reports as a failure. Notes in `trace` what is worth logging of the run: the process's pid,
/// how long it ran, its stderr, whether Octoboard had to end it, what the agent said that made
/// the account read as not signed in, and any error-like line it printed besides the reply.
pub(super) fn probe(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
    cancel: &AtomicBool,
    trace: &mut ProbeTrace,
) -> Result<Reading, QuotaUnavailable> {
    let conversation = conversation(agent);
    let mut run = account_cli::admitted_command(agent, shell_env, config_dir, conversation.args)
        .map_err(|refused| match refused {
            NotRun::DirMissing(dir) => config_dir_missing(agent, &dir.to_string_lossy()),
            NotRun::BinaryNotFound(binary) => QuotaUnavailable::new(
                code::BINARY_NOT_FOUND,
                refused.detail(),
                &[("binary", binary)],
            ),
        })?;
    let bounds = Bounds::agent_query(timeout, OUTPUT_LIMIT);
    let (is_reply, companion_of, needs_companion) = (
        conversation.is_reply,
        conversation.companion,
        conversation.needs_companion,
    );
    // The reply that is read, and the part of another reply that says who is signed in: the two
    // arrive in either order.
    let reply: RefCell<Option<Value>> = RefCell::new(None);
    let companion: RefCell<Option<Value>> = RefCell::new(None);
    let notes: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let mut exchange = ExchangeTrace::default();
    let result = run_exchange_traced(
        &mut run.command,
        &conversation.input,
        &bounds,
        cancel,
        AGENT_REPLY_GRACE,
        &mut exchange,
        |line| {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                return false;
            };
            if let Some(part) = companion_of(&value) {
                *companion.borrow_mut() = Some(part);
            }
            if is_reply(&value) {
                *reply.borrow_mut() = Some(value);
            } else if is_error_like(&value) && notes.borrow().len() < MAX_NOTES {
                notes.borrow_mut().push(truncate(&redact(line), LINE_LIMIT));
            }
            reply
                .borrow()
                .as_ref()
                .is_some_and(|reply| !needs_companion(reply) || companion.borrow().is_some())
        },
    );
    trace.pid = exchange.pid;
    trace.elapsed = exchange.elapsed;
    trace.stderr_tail = stderr_tail(&exchange.stderr);
    trace.notes = notes.take();
    match &result {
        Err(failure) => trace.stopped = stopped_by_us(failure, exchange.pid.is_some()),
        // A program that exits by itself after its reply is the normal end of a reading.
        Ok(_) => {
            if let Some(AfterReply::Killed { after, why }) = &exchange.after_reply {
                trace.stopped = Some(format!("stopped {after:?} after its reply: {why}"));
            }
        }
    }
    result.map_err(|failure| match failure {
        // Claude Code's sign-in state is in a reply other than the one that is read.
        _ if agent == Agent::Claude && claude_signed_out(companion.borrow().as_ref()) => {
            trace.reason = Some(signed_out_reason(agent, None, companion.borrow().as_ref()));
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
        ReplyProblem::NotSignedIn => {
            trace.reason = Some(signed_out_reason(agent, Some(&value), companion.as_ref()));
            not_signed_in(agent)
        }
        ReplyProblem::Unreadable => unreadable(agent),
        ReplyProblem::NoWindows => QuotaUnavailable::new(
            code::NO_WINDOWS,
            format!("{} reports no quota for this account", agent.label()),
            &[("agent", agent.label())],
        ),
        ReplyProblem::Error(detail) => failed(agent, &detail),
    })
}

/// Why Octoboard ended the process before it ended itself, from how the exchange failed;
/// `started` is whether there was a process at all. A child that closed its output by itself was
/// not ended by Octoboard. A process that had to be killed after its reply is told from the
/// run's `after_reply`, not from a failure.
fn stopped_by_us(failure: &ExchangeFailure, started: bool) -> Option<String> {
    match failure {
        ExchangeFailure::TimedOut(timeout) => Some(format!("no reply within {timeout:?}")),
        ExchangeFailure::StdoutExceeded(limit) => Some(format!("output went past {limit} bytes")),
        ExchangeFailure::Cancelled => Some("the daemon is stopping".to_string()),
        ExchangeFailure::Failed(_) if started => Some("its output could not be read".to_string()),
        ExchangeFailure::Failed(_) | ExchangeFailure::ClosedWithoutReply { .. } => None,
    }
}

/// Whether a line other than the reply is an error or sign-in status message from the agent. Only
/// what the agent plainly marks as such counts: a `type` of `auth_status` or `error`, or an
/// `error` field.
fn is_error_like(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("auth_status" | "error")
    ) || value.get("error").is_some_and(|error| !error.is_null())
}

/// What the agent said that makes the account read as not signed in, in words fit for the log
/// (see `diagnostics::redact`): for Claude Code the fields of `initialize`'s account that decide
/// it with the usage request's error beside them, for the others the refusal of the quota
/// request.
fn signed_out_reason(agent: Agent, reply: Option<&Value>, account: Option<&Value>) -> String {
    let field = |name: &str| {
        account
            .and_then(|account| account.get(name))
            .map(|value| match value.as_str() {
                Some(text) => truncate(&redact(text), 40),
                None if value.is_null() => "none".to_string(),
                None => "set".to_string(),
            })
            .unwrap_or_else(|| "absent".to_string())
    };
    match agent {
        Agent::Claude => {
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
        Agent::Codex => "the rate limits request was refused for want of a sign-in and \
            account/read reports no account"
            .to_string(),
        Agent::Grok => match reply.and_then(json_rpc::error_detail) {
            Some(detail) => format!("the billing request was refused: {}", redact(&detail)),
            None => "the billing request was refused".to_string(),
        },
    }
}

/// The error text of a Claude Code reply to a control request that answered with an error.
fn claude_response_error(reply: &Value) -> Option<String> {
    reply
        .pointer("/response/error")
        .and_then(Value::as_str)
        .map(str::to_string)
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
        plan: crate::text::trimmed_or_none(plan.and_then(Value::as_str)),
        windows,
        reset_credits: None,
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

/// Grok Build's refusal of the billing request of an account with no credentials (code -32000,
/// "Authentication required"). Like Codex's it does not say whether another way of signing in is
/// in use; the `initialize` reply does.
fn is_grok_auth_refusal(reply: &Value) -> bool {
    json_rpc::error_code(reply) == Some(-32000)
        && json_rpc::error_detail(reply)
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

fn read_grok(reply: &Value, initialize_meta: Option<&Value>) -> Result<Reading, ReplyProblem> {
    if is_grok_auth_refusal(reply) {
        // Signed out where no sign-in method other than grok.com is the default; an API key in
        // the environment or the configuration is offered as the default instead.
        let signed_out = initialize_meta
            .and_then(|meta| meta.get("defaultAuthMethodId"))
            .map(Value::is_null);
        return Err(refusal_problem(signed_out, "Grok Build"));
    }
    if let Some(detail) = json_rpc::error_detail(reply) {
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
mod tests;
