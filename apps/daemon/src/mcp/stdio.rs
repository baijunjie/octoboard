//! `octoboardd mcp` — the MCP server each adapter registers as a `command`-type server, one child
//! process per session.
//!
//! It owns no state. The tool catalogue it announces comes from [`super::tools_for`] for the role
//! it was launched with, and every call is forwarded to the running daemon, which is where the
//! sessions and the store actually are. The session's identity travels in argv rather than in the
//! environment: Claude Code and Grok do export a session id to an MCP child, but Codex exports
//! nothing, and Grok's `{{session_id}}` templating does not work — argv is the one route all three
//! agree on, and the daemon is the one assigning the id anyway.
//!
//! The token authenticates the child to the daemon, so the MCP surface is not reachable by any
//! local process that happens to guess the port.
//!
//! **Stdout belongs to the MCP protocol alone.** Nothing else may be written to it, which is why
//! this mode installs no tracing subscriber and reports its own failures on stderr.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParam, ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Value};

use crate::protocol::Role;

/// How long a tool call may take. Generous on purpose: `add_project` with a GitHub source runs a
/// `git clone`, which is minutes on a large repository. The agent's own tool timeout is the one
/// that should decide when to give up, not this.
const CALL_TIMEOUT: Duration = Duration::from_secs(600);

/// Runs the server until the agent closes the connection.
pub fn run(session: String, role: Role, port: u16, token: String) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let server = OctoboardMcp {
            session: Arc::new(session),
            role,
            port,
            token: Arc::new(token),
        };
        let running = server.serve(rmcp::transport::stdio()).await?;
        running.waiting().await?;
        Ok(())
    })
}

#[derive(Clone)]
struct OctoboardMcp {
    /// Named only in what the model is told when the daemon cannot be reached. The daemon
    /// resolves the session from the token, so a child that rewrote this argument still cannot act
    /// on a session other than its own.
    session: Arc<String>,
    role: Role,
    port: u16,
    token: Arc<String>,
}

impl ServerHandler for OctoboardMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: super::SERVER_KEY.to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("Octoboard".to_string()),
                icons: None,
                website_url: None,
            },
            instructions: None,
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = super::tools_for(self.role)
            .iter()
            .map(|tool| Tool {
                name: Cow::Borrowed(tool.name),
                title: None,
                description: Some(Cow::Borrowed(tool.description)),
                input_schema: Arc::new(
                    (tool.schema)()
                        .as_object()
                        .cloned()
                        .expect("every tool schema is a JSON object"),
                ),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            })
            .collect();
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        // A name outside this role's catalogue is a protocol-level mistake rather than a tool
        // that failed, so it does not come back as a tool result the model is invited to retry.
        if super::tool_by_name(self.role, &request.name).is_none() {
            return Err(ErrorData::invalid_params(
                format!("`{}` is not a tool this session can call", request.name),
                None,
            ));
        }

        let body = serde_json::to_vec(&json!({
            "tool": request.name,
            "arguments": Value::Object(request.arguments.unwrap_or_default()),
        }))
        .map_err(|err| ErrorData::internal_error(err.to_string(), None))?;
        let path = format!("/mcp/{}", self.token);
        let port = self.port;

        // The loopback client is blocking, and a tool call can take minutes.
        let response = tokio::task::spawn_blocking(move || {
            crate::loopback::post(port, &path, &body, CALL_TIMEOUT)
        })
        .await
        .map_err(|err| ErrorData::internal_error(err.to_string(), None))?;

        Ok(match response {
            Ok(body) => read_outcome(&body),
            // Octoboard itself being unreachable is the model's problem to work around, not a
            // protocol error: it should be able to say so to the user rather than have the call
            // fail opaquely.
            Err(err) => CallToolResult::error(vec![Content::text(format!(
                "Octoboard could not be reached for session {}: {err}",
                self.session
            ))]),
        })
    }
}

/// Turns the daemon's answer into a tool result. A refusal comes back as an error *result* rather
/// than a protocol error, so the model reads the reason and can act on it — "this session is
/// waiting for the user" is advice, not a malfunction.
fn read_outcome(body: &[u8]) -> CallToolResult {
    let outcome: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(err) => {
            return CallToolResult::error(vec![Content::text(format!(
                "Octoboard returned something unreadable: {err}"
            ))])
        }
    };
    if outcome.get("ok").and_then(Value::as_bool) == Some(true) {
        match outcome.get("result") {
            Some(result) if result.is_object() => CallToolResult::structured(result.clone()),
            // A tool with nothing to return still has to say it worked.
            _ => CallToolResult::success(vec![Content::text("Done.")]),
        }
    } else {
        let message = outcome
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Octoboard refused the call without saying why");
        CallToolResult::error(vec![Content::text(message.to_string())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_structured_result_comes_through_as_structured_content() {
        let result = read_outcome(br#"{"ok":true,"result":{"session":"s-1"}}"#);
        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.structured_content.unwrap()["session"], "s-1");
    }

    /// A refusal has to reach the model as a readable reason it can act on, not as a protocol
    /// failure: "this session is waiting for the user" is the console session's cue to leave it
    /// alone.
    #[test]
    fn a_refusal_becomes_an_error_result_carrying_the_reason() {
        let result = read_outcome(br#"{"ok":false,"error":"this session is waiting for you"}"#);
        assert_eq!(result.is_error, Some(true));
        assert!(format!("{:?}", result.content).contains("waiting for you"));
    }

    #[test]
    fn an_unreadable_answer_is_an_error_result_rather_than_a_panic() {
        let result = read_outcome(b"not json");
        assert_eq!(result.is_error, Some(true));
    }
}
