//! `obd-proto mcp` — a stdio MCP server, one process per session, launched by the agent CLI
//! itself via `--mcp-config`. Implements just enough of MCP for Claude Code to connect:
//! `initialize`, `notifications/initialized`, `tools/list`, `tools/call`. Every `tools/call` is
//! forwarded to `POST /mcp/:session/:tool` on the daemon; the daemon's JSON response becomes the
//! tool result. This is sufficient to prove identity (`session`) and role travel correctly from
//! launch argv (G5) — it does not implement MCP resources, prompts, or any transport but stdio.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::http_client;
use crate::protocol::Role;

pub struct McpArgs {
    pub session: String,
    pub role: Role,
    pub daemon: String,
}

/// A JSON-RPC error, carrying the standard code rather than always `-32601` ("method not
/// found") regardless of what actually went wrong.
struct RpcError {
    code: i32,
    message: String,
}

impl RpcError {
    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
        }
    }

    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method `{method}` is not implemented by this prototype MCP server"),
        }
    }
}

/// One tool's advertised shape: `tools/list` needs name + description + a JSON Schema for
/// arguments; the schemas below are intentionally minimal (every field a free-form string) since
/// their validation is Claude Code's job, not this bridge's.
///
/// This is the single source of truth for which tools a role has: `server.rs`'s `/mcp/:session/:tool`
/// authorization check derives its allow-list from this same list rather than keeping its own, so
/// a tool added here cannot be advertised without also being authorized (and vice versa) — that
/// mismatch used to be possible and meant the MCP server could advertise a tool the daemon then
/// rejected with 403.
pub(crate) fn tools_for_role(role: Role) -> Vec<Value> {
    let string_prop = |desc: &str| json!({ "type": "string", "description": desc });
    match role {
        Role::Hub => vec![
            json!({
                "name": "start_session",
                "description": "Start a worker session in a project and hand it a task.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": string_prop("Project name or path"),
                        "task": string_prop("Task brief for the new session"),
                        "agent": string_prop("Optional agent override (claude|grok)"),
                    },
                    "required": ["project", "task"]
                }
            }),
            json!({
                "name": "show_page",
                "description": "Push a page of HTML to the report panel.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "html": string_prop("HTML content to display") },
                    "required": ["html"]
                }
            }),
        ],
        Role::Worker => vec![
            json!({
                "name": "report",
                "description": "Report progress or completion back to the hub.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "summary": string_prop("Natural-language summary of the work done"),
                        "outcome": string_prop("done|failed|needs_decision"),
                    },
                    "required": ["summary", "outcome"]
                }
            }),
            json!({
                "name": "ask_user",
                "description": "Ask the human user a question directly (not through the hub).",
                "inputSchema": {
                    "type": "object",
                    "properties": { "question": string_prop("Question for the user") },
                    "required": ["question"]
                }
            }),
        ],
    }
}

pub fn run(args: McpArgs) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => {
                tracing::warn!(%err, "mcp: malformed JSON-RPC line, ignoring");
                continue;
            }
        };

        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let id = request.get("id").cloned();

        // Notifications (no `id`) never get a response, per JSON-RPC.
        let Some(id) = id else {
            continue;
        };

        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "obd-proto", "version": env!("CARGO_PKG_VERSION") }
            })),
            "tools/list" => Ok(json!({ "tools": tools_for_role(args.role) })),
            "tools/call" => handle_tools_call(&args, &request),
            other => Err(RpcError::method_not_found(other)),
        };

        let response = match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(err) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": err.code, "message": err.message }
            }),
        };
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }

    Ok(())
}

fn handle_tools_call(args: &McpArgs, request: &Value) -> Result<Value, RpcError> {
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let tool = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params("tools/call missing `params.name`"))?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let url = format!("{}/mcp/{}/{}", args.daemon, args.session, tool);
    let (ok, body) = http_client::post_json(&url, &arguments)
        .map_err(|err| RpcError::internal(format!("forwarding tool call to daemon: {err}")))?;

    Ok(json!({
        "content": [{ "type": "text", "text": body.to_string() }],
        "isError": !ok
    }))
}
