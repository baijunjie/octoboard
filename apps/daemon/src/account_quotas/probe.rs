//! Asking an agent's own CLI for an account's quota. Each probe is a short conversation over
//! lines on the CLI's stdin and stdout (`crate::subprocess::run_exchange`), measured in
//! `docs/agent-cli-reference.md` ("Plan quota"):
//!
//! - **Claude Code** is `claude -p` in stream-JSON mode, given an `initialize` control request
//!   and then `get_usage`. The reply's `limits[]` lists the windows with their kind; the older
//!   `five_hour`, `seven_day` and `model_scoped` fields are read only when `limits[]` yields none.
//! - **Codex** is `codex app-server`, given `initialize`, `initialized` and
//!   `account/rateLimits/read`: a primary and a secondary window and the plan.
//! - **Grok Build** is `grok agent --no-leader stdio`, given ACP `initialize` and `_x.ai/billing`:
//!   one credit pool over a billing period and the subscription tier.
//!
//! The agents mark these calls experimental, so a reply that cannot be read is an
//! [`QuotaUnavailable`] for that account and nothing else. A probe runs in the login-shell
//! snapshot with the agent's configuration-directory variable pointed at the account's directory,
//! and with the CLI told to load as little of the user's setup as it can without losing its
//! sign-in.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use crate::adapter;
use crate::protocol::{
    quota_unavailable_code as code, Agent, QuotaUnavailable, QuotaWindow, QuotaWindowKind,
};
use crate::rfc3339::parse_rfc3339_millis;
use crate::subprocess::{run_exchange, Bounds, ExchangeFailure};

/// How long a probe may take. The CLIs answer in one to three seconds; this leaves room for a
/// slow start or a refreshed login while keeping a hung CLI from holding a slot for long.
pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// The most a probe may print. Claude Code's reply to `initialize` lists every command and runs
/// to hundreds of kilobytes.
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

/// How much of a probe's stderr is kept for the failure that reports it.
const STDERR_LIMIT: usize = 4096;

const SESSION_MINUTES: u32 = 5 * 60;
const WEEK_MINUTES: u32 = 7 * 24 * 60;

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
}

/// One agent's side of the conversation: what to run, what to write, which line is the reply,
/// and how to read it.
struct Conversation {
    args: &'static [&'static str],
    input: Vec<String>,
    is_reply: fn(&Value) -> bool,
    read: fn(&Value) -> Result<Reading, ReplyProblem>,
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
            read: read_claude,
        },
        Agent::Codex => Conversation {
            // The features off are the ones that reach out on start: the plugin catalogue sync
            // (which runs `git ls-remote` over SSH), apps, and the user's hooks. Set as
            // configuration rather than `--disable`, which refuses a feature name the installed
            // Codex does not know, where an unknown configuration key is ignored.
            args: &[
                "app-server",
                "-c",
                "features.plugins=false",
                "-c",
                "features.remote_plugin=false",
                "-c",
                "features.apps=false",
                "-c",
                "features.hooks=false",
            ],
            input: lines(vec![
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {"clientInfo": {"name": "octoboard",
                        "version": env!("CARGO_PKG_VERSION")}}}),
                json!({"jsonrpc": "2.0", "method": "initialized"}),
                json!({"jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read"}),
            ]),
            is_reply: is_second_response,
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
            read: read_grok,
        },
    }
}

/// The JSON-RPC response to the request with id 2, as opposed to a notification or a request the
/// server makes of its own.
fn is_second_response(value: &Value) -> bool {
    value.get("id").and_then(Value::as_i64) == Some(2) && value.get("method").is_none()
}

/// Runs the probe for `agent` and reads its reply. Blocking.
pub(super) fn probe(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
) -> Result<Reading, QuotaUnavailable> {
    let conversation = conversation(agent);
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
    };
    let is_reply = conversation.is_reply;
    let reply = run_exchange(&mut command, &conversation.input, &bounds, |line| {
        serde_json::from_str::<Value>(line).is_ok_and(|value| is_reply(&value))
    })
    .map_err(|failure| match failure {
        ExchangeFailure::TimedOut(_) => QuotaUnavailable::new(
            code::TIMED_OUT,
            format!("{} did not report its quota in time", agent.label()),
            &[("agent", agent.label())],
        ),
        other => failed(agent, &other.to_string()),
    })?;
    let value: Value = serde_json::from_str(&reply).map_err(|_| unreadable(agent))?;
    (conversation.read)(&value).map_err(|problem| match problem {
        ReplyProblem::Unreadable => unreadable(agent),
        ReplyProblem::NoWindows => QuotaUnavailable::new(
            code::NO_WINDOWS,
            format!("{} reports no quota for this account", agent.label()),
            &[("agent", agent.label())],
        ),
        ReplyProblem::Error(detail) => failed(agent, &detail),
    })
}

fn failed(agent: Agent, detail: &str) -> QuotaUnavailable {
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

fn read_claude(reply: &Value) -> Result<Reading, ReplyProblem> {
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

/// A JSON-RPC error response's message, if the reply is one.
fn rpc_error(reply: &Value) -> Option<ReplyProblem> {
    let error = reply.get("error")?;
    let detail = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("the request was refused");
    Some(ReplyProblem::Error(detail.to_string()))
}

fn read_codex(reply: &Value) -> Result<Reading, ReplyProblem> {
    if let Some(problem) = rpc_error(reply) {
        return Err(problem);
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

fn read_grok(reply: &Value) -> Result<Reading, ReplyProblem> {
    if let Some(problem) = rpc_error(reply) {
        return Err(problem);
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

    fn unavailable_code(result: Result<Reading, QuotaUnavailable>) -> String {
        result.expect_err("an unavailable quota").code
    }

    #[test]
    fn claude_reports_its_session_weekly_and_model_windows_from_limits() {
        let dir = ScratchDir::new("quota-claude");
        let env = answering_agent(&dir, Agent::Claude, CLAUDE_REPLY);
        let reading = probe(Agent::Claude, &env, None, crate::test_support::PATIENCE)
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
        let reading = read_claude(&reply).expect("readable");
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
        assert_eq!(read_claude(&none), Err(ReplyProblem::NoWindows));
        let refused = json!({"type": "control_response", "response": {
            "subtype": "error", "request_id": "r2", "error": "Not logged in"}});
        assert_eq!(
            read_claude(&refused),
            Err(ReplyProblem::Error("Not logged in".to_string()))
        );
    }

    #[test]
    fn codex_reports_its_primary_and_secondary_windows_and_plan() {
        let dir = ScratchDir::new("quota-codex");
        let env = answering_agent(&dir, Agent::Codex, CODEX_REPLY);
        let reading = probe(Agent::Codex, &env, None, crate::test_support::PATIENCE)
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
        assert_eq!(
            methods,
            ["initialize", "initialized", "account/rateLimits/read"]
        );
    }

    #[test]
    fn a_codex_window_is_what_its_length_says_and_a_missing_length_falls_back_to_position() {
        let reply = json!({"id": 2, "result": {"rateLimits": {
            "primary": {"usedPercent": 10, "windowDurationMins": 10080},
            "secondary": {"usedPercent": 20},
            "planType": "free"}}});
        let reading = read_codex(&reply).expect("readable");
        assert_eq!(reading.windows[0].kind, QuotaWindowKind::Weekly);
        assert_eq!(reading.windows[1].kind, QuotaWindowKind::Weekly);
        assert_eq!(reading.windows[1].window_minutes, None);
        assert_eq!(
            read_codex(&json!({"id": 2, "error": {"code": -32600, "message": "not signed in"}})),
            Err(ReplyProblem::Error("not signed in".to_string()))
        );
    }

    #[test]
    fn grok_reports_its_credit_pool_over_the_current_period_and_tier() {
        let dir = ScratchDir::new("quota-grok");
        let env = answering_agent(&dir, Agent::Grok, GROK_REPLY);
        let reading = probe(Agent::Grok, &env, None, crate::test_support::PATIENCE)
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
        let reading = read_grok(&reply).expect("readable");
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
            probe(
                agent,
                &env,
                Some(&account_dir),
                crate::test_support::PATIENCE,
            )
            .expect("a readable reply");
            assert_eq!(
                std::fs::read_to_string(dir.join("dir")).unwrap().trim(),
                account_dir.to_string_lossy(),
                "{agent:?}"
            );
            // The default account is left to the shell, which here exports nothing.
            probe(agent, &env, None, crate::test_support::PATIENCE).expect("a readable reply");
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
                unavailable_code(probe(agent, &env, None, crate::test_support::PATIENCE)),
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
                let result = probe(agent, &env, None, SHORT);
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
            let unavailable =
                probe(agent, &env, None, crate::test_support::PATIENCE).expect_err("no reply");
            assert_eq!(unavailable.code, code::COMMAND_FAILED, "{agent:?}");
            assert!(
                unavailable.params["detail"].ends_with("not logged in"),
                "{unavailable:?}"
            );
        }
    }

    #[test]
    fn a_missing_binary_is_reported_as_such() {
        let dir = ScratchDir::new("quota-missing");
        let env = HashMap::from([("PATH".to_string(), dir.to_string_lossy().into_owned())]);
        assert_eq!(
            unavailable_code(probe(Agent::Codex, &env, None, SHORT)),
            code::BINARY_NOT_FOUND
        );
    }
}
