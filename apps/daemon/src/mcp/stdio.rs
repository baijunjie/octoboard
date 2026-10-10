//! `octoboardd mcp` — the MCP server each adapter registers as a `command`-type server, one child
//! process per session.
//!
//! It owns no state. The tool catalogue it announces comes from [`super::tools_for`] for the role
//! and binding it was launched with, and every call is forwarded to the running daemon, which is
//! where the sessions and the store actually are. The session's identity travels in argv rather
//! than in the environment: Claude Code and Grok do export a session id to an MCP child, but Codex
//! exports nothing, and Grok's `{{session_id}}` templating does not work — argv is the one route
//! all three agree on, and the daemon is the one assigning the id anyway.
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

/// How long a tool call may take. Generous on purpose: `add_project` with a `git` source runs a
/// `git clone`, which is minutes on a large repository. The agent's own tool timeout is the one
/// that should decide when to give up, not this.
const CALL_TIMEOUT: Duration = Duration::from_secs(600);

/// Runs the server until the agent closes the connection.
pub fn run(
    session: String,
    role: Role,
    bound: bool,
    port: u16,
    token: String,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let server = OctoboardMcp {
            session: Arc::new(session),
            role,
            bound,
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
    /// Whether the session has the role of one bound to an owner, which decides the catalogue
    /// along with the role. Fixed for the session's lifetime, so a launch argument can carry it.
    bound: bool,
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
        let tools = super::tools_for(self.role, self.bound)
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
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        // A name outside this role's catalogue is a protocol-level mistake rather than a tool
        // that failed, so it does not come back as a tool result the model is invited to retry.
        if super::tool_by_name(self.role, self.bound, &request.name).is_none() {
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
        // Only a request for a console session is withdrawn when its call is cancelled. Any other
        // call is left to finish in the daemon, as before: the daemon stops serving a call whose
        // connection goes, and a `start_session` or a `report` cut off part-way would leave its
        // work half done.
        let cancelled = async {
            if request.name == WITHDRAWN_ON_CANCEL {
                context.ct.cancelled().await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        let response = match forward_until(self.port, path, body, cancelled).await {
            Forwarded::Answered(response) => response,
            Forwarded::Cancelled => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "The call was cancelled.",
                )]))
            }
        };

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

/// What became of a call forwarded to the daemon.
enum Forwarded {
    Answered(std::io::Result<Vec<u8>>),
    /// The agent cancelled the call, and the connection carrying it was closed.
    Cancelled,
}

/// The tool whose call the daemon has to learn the agent cancelled: a request for a console session
/// waits in front of the user, and is withdrawn when its call's connection to the daemon goes.
const WITHDRAWN_ON_CANCEL: &str = "request_console_session";

/// Forwards one call to the daemon and waits for its answer, unless `cancelled` completes first —
/// for a call the agent cancels (`notifications/cancelled`, which cancels the call's context). A
/// cancelled call's connection is shut down rather than left to finish: that is how the daemon
/// learns the call is gone.
async fn forward_until(
    port: u16,
    path: String,
    body: Vec<u8>,
    cancelled: impl std::future::Future<Output = ()>,
) -> Forwarded {
    tokio::pin!(cancelled);
    // The loopback client is blocking: the connect as well as the exchange, which can take minutes.
    let connecting = tokio::task::spawn_blocking(move || {
        let stream = crate::loopback::connect(port, CALL_TIMEOUT)?;
        let closer = stream.try_clone()?;
        Ok::<_, std::io::Error>((stream, closer))
    });
    let (stream, closer) = tokio::select! {
        connected = connecting => match connected {
            Ok(Ok(connected)) => connected,
            Ok(Err(err)) => return Forwarded::Answered(Err(err)),
            Err(err) => return Forwarded::Answered(Err(std::io::Error::other(err.to_string()))),
        },
        // Nothing has reached the daemon yet; a connection made after this is dropped unused.
        () = &mut cancelled => return Forwarded::Cancelled,
    };
    let exchange =
        tokio::task::spawn_blocking(move || crate::loopback::exchange(stream, port, &path, &body));
    tokio::select! {
        answered = exchange => Forwarded::Answered(answered.unwrap_or_else(|err| {
            Err(std::io::Error::other(err.to_string()))
        })),
        () = &mut cancelled => {
            let _ = closer.shutdown(std::net::Shutdown::Both);
            Forwarded::Cancelled
        }
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

    /// A call the agent cancels has its connection closed at once, which is the only way the
    /// daemon learns of the cancellation; the daemon's side here never answers.
    #[tokio::test]
    async fn a_cancelled_call_closes_its_connection_to_the_daemon() {
        use tokio::io::AsyncReadExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
        let call = tokio::spawn(forward_until(
            port,
            "/mcp/token".to_string(),
            b"{}".to_vec(),
            async {
                let _ = cancelled.await;
            },
        ));
        let (mut daemon_side, _) = listener.accept().await.unwrap();
        let mut request = vec![0u8; 1024];
        let read = daemon_side.read(&mut request).await.unwrap();
        assert!(read > 0, "the call was sent");

        cancel.send(()).unwrap();
        assert!(matches!(call.await.unwrap(), Forwarded::Cancelled));
        let mut rest = Vec::new();
        tokio::time::timeout(
            crate::test_support::PATIENCE,
            daemon_side.read_to_end(&mut rest),
        )
        .await
        .expect("the connection is closed rather than left open")
        .unwrap();
    }

    #[test]
    fn an_unreadable_answer_is_an_error_result_rather_than_a_panic() {
        let result = read_outcome(b"not json");
        assert_eq!(result.is_error, Some(true));
    }
}
