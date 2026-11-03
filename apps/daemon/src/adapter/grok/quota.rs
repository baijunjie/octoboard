//! Grok Build's quota: `grok agent --no-leader stdio`, given ACP `initialize` and `_x.ai/billing`:
//! one credit pool over a billing period and the subscription tier. Its quota request is also
//! refused with an error whose `data` says its credentials are invalid or expired once a stored
//! sign-in has expired. Measured in `docs/agent-cli-reference.md` ("Plan quota", "Starting a
//! quota window").

use serde_json::{json, Value};

use crate::accounts::quotas::diagnostics::redact;
use crate::accounts::quotas::kick::{
    strings, KickCommand, KICK_PROMPT, KICK_SYSTEM_PROMPT, REPLY_TOKEN_CAP,
};
use crate::accounts::quotas::probe::{
    is_second_response, reading, refusal_problem, QuotaProbe, Reading, ReplyProblem,
};
use crate::agents::json_rpc;
use crate::protocol::{AgentId, QuotaWindow, QuotaWindowKind};
use crate::time::parse_rfc3339_millis;

pub(super) struct GrokQuota;

impl QuotaProbe for GrokQuota {
    fn args(&self) -> &'static [&'static str] {
        &["agent", "--no-leader", "stdio"]
    }

    fn input(&self) -> Vec<String> {
        [
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": 1}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "_x.ai/billing", "params": {}}),
        ]
        .iter()
        .map(Value::to_string)
        .collect()
    }

    fn is_reply(&self, value: &Value) -> bool {
        is_second_response(value)
    }

    /// Only the field that says which sign-in methods the CLI would use by default.
    fn companion(&self, value: &Value) -> Option<Value> {
        json_rpc::is_reply_to(value, 1).then(|| {
            value
                .pointer("/result/_meta")
                .filter(|meta| meta.is_object())
                .cloned()
                .unwrap_or(Value::Null)
        })
    }

    fn needs_companion(&self, reply: &Value) -> bool {
        is_grok_auth_refusal(reply)
    }

    fn read(&self, reply: &Value, companion: Option<&Value>) -> Result<Reading, ReplyProblem> {
        read_grok(reply, companion)
    }

    fn signed_out_reason(&self, reply: Option<&Value>, _companion: Option<&Value>) -> String {
        match reply.and_then(json_rpc::error_detail) {
            Some(detail) => format!("the billing request was refused: {}", redact(&detail)),
            None => "the billing request was refused".to_string(),
        }
    }
}

/// Grok Build's built-in tools by id, all of which a kick takes out. `--tools ""` takes none out.
const GROK_TOOLS: &str = "run_terminal_cmd,read_file,search_replace,list_dir,grep,kill_task,\
    todo_write,get_task_output,wait_tasks,task,scheduler_create,scheduler_delete,scheduler_list,\
    monitor,search_tool,use_tool,workflow,enter_plan_mode,exit_plan_mode,ask_user_question,\
    send_feedback,image_gen,image_edit,image_to_video,reference_to_video,write,web_search,\
    web_fetch,AgentId";

/// The request that starts Grok Build's usage windows: its cheapest model at its lowest reasoning
/// effort, run without a terminal. It can neither leave its session out nor the MCP servers and
/// hooks of its own configuration, but it can leave out what it scans of Claude Code's.
pub(super) fn kick_command() -> KickCommand {
    KickCommand {
        args: strings(&[
            "-p",
            KICK_PROMPT,
            "-m",
            "grok-4.7",
            "--effort",
            "low",
            "--system-prompt-override",
            KICK_SYSTEM_PROMPT,
            "--no-subagents",
            "--disable-web-search",
            "--no-plan",
            "--max-turns",
            "1",
            "--disallowed-tools",
            GROK_TOOLS,
        ]),
        env: vec![
            (
                "GROK_CONFIG",
                format!(r#"{{"models":{{"max_completion_tokens":{REPLY_TOKEN_CAP}}}}}"#),
            ),
            ("GROK_CLAUDE_SKILLS_ENABLED", "0".to_string()),
            ("GROK_CLAUDE_MCPS_ENABLED", "0".to_string()),
            ("GROK_CLAUDE_AGENTS_ENABLED", "0".to_string()),
            ("GROK_CLAUDE_RULES_ENABLED", "0".to_string()),
            ("GROK_CLAUDE_HOOKS_ENABLED", "0".to_string()),
            ("GROK_TURN_SUMMARY", "0".to_string()),
            ("GROK_SESSION_RECAP", "0".to_string()),
        ],
    }
}

/// Grok Build's refusal of the billing request of an account with no credentials (code -32000,
/// "Authentication required"). Like Codex's it does not say whether another way of signing in is
/// in use; the `initialize` reply does.
fn is_grok_auth_refusal(reply: &Value) -> bool {
    json_rpc::error_code(reply) == Some(-32000)
        && json_rpc::error_detail(reply)
            .is_some_and(|detail| detail.to_lowercase().contains("authentication required"))
}

pub(crate) fn read_grok(
    reply: &Value,
    initialize_meta: Option<&Value>,
) -> Result<Reading, ReplyProblem> {
    if is_grok_auth_refusal(reply) {
        // Signed out where no sign-in method other than grok.com is the default; an API key in
        // the environment or the configuration is offered as the default instead.
        let signed_out = initialize_meta
            .and_then(|meta| meta.get("defaultAuthMethodId"))
            .map(Value::is_null);
        return Err(refusal_problem(
            signed_out,
            AgentId::GROK.descriptor().label,
        ));
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
