//! The HTTP routes the daemon's own helper processes and the agents' hooks call: the hook events
//! (queued here, applied by `crate::hooks::apply`), the branches a session's git created, the
//! revert gate's question, the context window a status line reports, and the MCP tool calls.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use bytes::Bytes;

use crate::hooks;
use crate::hooks::queue::Push;
use crate::state::AppState;

/// The hook callback. It answers `200` with an empty body at once whatever happens, and applies the
/// event afterwards on the session's own queue, in the order the events arrived. Every one of the
/// three agents renders a failing hook to the user, and the daemon being busy or confused is not
/// something to show them; for Claude Code's own HTTP hook, which is synchronous, an answer that
/// waited would also hold the turn, and one that never came (an unreachable daemon, the hook's
/// timeout) shows a non-blocking error notice in the transcript. An empty `200` is read there as
/// "no decision", so the event cannot steer the turn either.
pub(super) async fn hook_callback(
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    match state.hook_queues.push(&session_id, body) {
        Push::StartWorker => {
            // Applying an event is blocking work on the store, so the drain runs on the blocking
            // pool; the tasks `apply_hook` starts find the runtime from there.
            tokio::task::spawn_blocking(move || {
                hooks::apply::drain_hook_queue(&state, &session_id)
            });
        }
        Push::Queued => {}
        Push::Coalesced => {
            tracing::warn!(session = %session_id, "the hook queue is full; the last event in line was replaced")
        }
    }
    axum::http::StatusCode::OK
}

/// A session's git reporting the branches it created and whether it moved refs a branch header
/// reads (`octoboardd git-ref`). Handled on the blocking pool, since it reads the repository's
/// files and writes the store; a move schedules a re-read of the repository's projects
/// (`crate::project_git::ref_moves`). The reply carries nothing, and the process that posted it
/// exits whatever it is.
pub(super) async fn git_ref_callback(
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let Ok(report) = serde_json::from_slice::<crate::branch_origin::RefReport>(&body) else {
        return axum::http::StatusCode::BAD_REQUEST;
    };
    tokio::task::spawn_blocking(move || {
        if report.moved {
            if let Some(repository) =
                crate::branch_origin::reported_repository(&report.cwd, report.git_dir.as_deref())
            {
                crate::project_git::ref_moves::moved(&state, repository);
            }
        }
        if !report.refs.is_empty() {
            crate::branch_origin::ref_created(&state, &session_id, &report);
        }
    });
    axum::http::StatusCode::OK
}

/// A session's git asking whether the revert gate applies to the update or push it is running a
/// hook for, and what the gate is to know (`octoboardd gate-hook`). The reply is the gate's inputs,
/// or `null` to skip it. Answered on the blocking pool, since it reads the store and runs `git`;
/// the hook is waiting on the answer, so nothing else is done here.
pub(super) async fn revert_gate_callback(
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> axum::response::Response {
    let Ok(request) =
        serde_json::from_slice::<crate::session_git_hooks::gate_inputs::GateRequest>(&body)
    else {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    };
    let inputs = tokio::task::spawn_blocking(move || {
        crate::session_git_hooks::gate_inputs::inputs(&state, &session_id, &request)
    })
    .await
    .ok()
    .flatten();
    axum::Json(inputs).into_response()
}

/// A Claude Code session's status line reporting the session's context window (`octoboardd
/// status-line`). Held for the record collector, which stores it with the session's context; the
/// reply carries nothing, and the reporter ignores it.
pub(super) async fn context_window_callback(
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    #[derive(serde::Deserialize)]
    struct Report {
        window: u64,
    }
    match serde_json::from_slice::<Report>(&body) {
        Ok(Report { window }) if window > 0 => {
            state.context_windows.report(&session_id, window);
            axum::http::StatusCode::OK
        }
        _ => axum::http::StatusCode::BAD_REQUEST,
    }
}

/// One MCP tool call, forwarded here by the session's own MCP server. The token is what says which
/// session is calling: the child never names its own session, so one that rewrote its arguments
/// still cannot act on another.
pub(super) async fn mcp_call(
    Path(token): Path<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let Some(session_id) = state.session_for_mcp_token(&token) else {
        // No token, no session — and no detail either: an unrecognised token is either a stale
        // child or something that has no business here.
        return axum::Json(serde_json::json!({
            "ok": false,
            "error": "this session's Octoboard connection is no longer valid",
        }));
    };

    let request: McpCall = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(err) => {
            return axum::Json(serde_json::json!({
                "ok": false,
                "error": format!("unreadable tool call: {err}"),
            }))
        }
    };

    let arguments = match request.arguments {
        serde_json::Value::Object(arguments) => arguments,
        _ => serde_json::Map::new(),
    };
    let mut answer =
        match crate::mcp::dispatch::call(&state, &session_id, &request.tool, &arguments).await {
            Ok(result) => serde_json::json!({ "ok": true, "result": result }),
            // The whole context chain, because this text is the only account the model gets of
            // why its call did not work.
            Err(err) => serde_json::json!({ "ok": false, "error": format!("{err:#}") }),
        };
    // Whether the session has a worker's tools as it stands after the call, which its binding
    // changing while it runs can have changed: the child announces the catalogue this names, and
    // tells the agent when it differs from the one it announced.
    if let Ok(session) = state.session_record(&session_id) {
        answer["worker"] = serde_json::json!(session.is_worker());
    }
    axum::Json(answer)
}

#[derive(serde::Deserialize)]
struct McpCall {
    tool: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use crate::server::router;
    use crate::server::test_support::{keyed, serve, status_of};
    use crate::test_support::{app_state, PATIENCE, TEST_KEY};
    use std::io::Write;
    use std::net::TcpStream;

    /// What Claude Code's own `"type": "http"` hook sends: the event's JSON as a POST with no
    /// `Origin`. It must get an empty `200`, which Claude Code reads as "no decision", and a page
    /// posting the same thing from a browser must still be refused.
    #[tokio::test]
    async fn a_claude_http_hook_post_is_answered_with_an_empty_200() {
        let port = serve("http-hook").await;
        tokio::task::spawn_blocking(move || {
            let payload = br#"{"hook_event_name":"Notification","notification_type":"idle_prompt","session_id":"agent-1"}"#;
            let post = |origin: Option<&str>| {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                write!(
                    stream,
                    "POST /hook/s?key={TEST_KEY} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{}\
                     Content-Type: application/json\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n",
                    origin.map(|o| format!("Origin: {o}\r\n")).unwrap_or_default(),
                    payload.len()
                )
                .expect("write head");
                stream.write_all(payload).expect("write body");
                let mut response = String::new();
                std::io::Read::read_to_string(&mut stream, &mut response).expect("read");
                response
            };
            let accepted = post(None);
            assert!(accepted.starts_with("HTTP/1.1 200"), "{accepted}");
            assert!(accepted.ends_with("\r\n\r\n"), "{accepted}");
            assert!(post(Some("https://evil.example")).starts_with("HTTP/1.1 403"));
        })
        .await
        .expect("the checks ran");
    }

    /// What `octoboardd status-line` posts is held for the record collector; a body without a
    /// usable window is refused and holds nothing.
    #[tokio::test]
    async fn a_reported_context_window_is_held_for_the_collector() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        let (state, dir) = app_state("server-context-window");
        let served = state.clone();
        tokio::spawn(async move {
            let _dir = dir;
            axum::serve(listener, router(served)).await
        });
        tokio::task::spawn_blocking(move || {
            let post = |session: &str, body: &[u8]| {
                crate::loopback::post(
                    port,
                    TEST_KEY,
                    &format!("/context-window/{session}"),
                    body,
                    PATIENCE,
                )
            };
            assert!(post("s", br#"{"window":200000}"#)
                .expect("answered")
                .is_empty());
            for body in [&b"{}"[..], br#"{"window":0}"#, b"200000"] {
                let err = post("t", body).expect_err("an unusable report is refused");
                assert_eq!(crate::loopback::error_status(&err), Some(400));
            }
            let host = format!("Host: 127.0.0.1:{port}");
            assert_eq!(
                status_of(port, &keyed("POST /context-window/t"), &[&host]),
                400
            );
        })
        .await
        .expect("the checks ran");
        assert_eq!(
            state
                .context_windows
                .retain(&std::collections::HashSet::from(["s", "t"])),
            std::collections::HashMap::from([("s".to_owned(), 200_000)])
        );
    }
}
