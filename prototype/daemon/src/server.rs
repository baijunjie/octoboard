//! Axum HTTP/WebSocket server implementing `prototype/PROTOCOL.md` verbatim.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use uuid::Uuid;

use crate::protocol::{Agent, ClientToControl, ControlToClient, Role, SessionState, StatusSource};
use crate::state::AppState;
use crate::term;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws/control", get(control_ws))
        .route("/ws/term/:session", get(term_ws))
        .route("/hook/:session/:event", post(hook_callback))
        .route("/mcp/:session/:tool", post(mcp_tool))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// /ws/control
// ---------------------------------------------------------------------------

async fn control_ws(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_control_ws(socket, state))
}

async fn handle_control_ws(mut socket: WebSocket, state: Arc<AppState>) {
    let mut events = state.control_tx.subscribe();
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        // Deliberately broadcast rather than answered on this socket alone:
                        // request-scoped replies (this `Error`, and `list_sessions`'s replay
                        // below) go out to every connected client, not just the one that asked.
                        // Harmless with the single UI client this prototype ever runs against,
                        // and simpler than threading the requester's socket through
                        // `handle_control_message` — not worth fixing for a harness milestone 01
                        // deletes.
                        if let Err(err) = handle_control_message(&state, &text).await {
                            state.broadcast(ControlToClient::Error { message: err.to_string() });
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {} // control is text-only; ignore binary/ping/pong
                    Some(Err(err)) => {
                        tracing::warn!(%err, "control websocket error");
                        break;
                    }
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        let text = serde_json::to_string(&event).expect("ControlToClient always serializes");
                        if socket.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn handle_control_message(state: &Arc<AppState>, text: &str) -> anyhow::Result<()> {
    let msg: ClientToControl = serde_json::from_str(text)?;
    match msg {
        ClientToControl::StartSession {
            agent,
            cwd,
            role,
            task,
        } => {
            start_session(state, agent, role, &cwd, task)?;
        }
        ClientToControl::SendMessage { session, text } => {
            let Some(session_handle) = state.get_session(&session) else {
                anyhow::bail!("unknown session {session}");
            };
            term::queue_or_send(state, &session_handle, &text)?;
        }
        ClientToControl::KillSession { session } => {
            let Some(session_handle) = state.get_session(&session) else {
                anyhow::bail!("unknown session {session}");
            };
            session_handle.kill()?;
            // Deliberately does not unregister the pid or emit `status: exited` here: the
            // process is only *requested* to die at this point, not yet known to be gone. The
            // exit watcher (`term::spawn_exit_watcher`) owns both — it observes the actual exit
            // via `try_wait` and is the only place that transition is emitted, so a client never
            // sees `exited` twice.
        }
        ClientToControl::ListSessions => {
            let sessions = state.sessions.read().expect("sessions lock poisoned");
            for session in sessions.values() {
                // Re-announce each known session before its status, the same two frames a client
                // would have seen from `start_session` — a client that reloads has no other way
                // to rebuild its session list, since `status` alone carries neither `agent` nor
                // `pid`. "Known" includes exited ones: `state.sessions` is never pruned, so this
                // re-announces every session ever started for the daemon's lifetime, not just
                // the live ones. Also broadcast rather than answered on the requester's own
                // socket — see the comment in `handle_control_ws`.
                let agent_session_id = session
                    .agent_session_id
                    .lock()
                    .expect("agent_session_id mutex poisoned")
                    .clone();
                state.broadcast(ControlToClient::SessionStarted {
                    session: session.id,
                    agent: session.agent,
                    pid: session.pid,
                    agent_session_id,
                });
                let current = *session.status.lock().expect("status mutex poisoned");
                state.broadcast(ControlToClient::Status {
                    session: session.id,
                    state: current,
                    source: StatusSource::Process,
                    raw: serde_json::json!({ "note": "list_sessions snapshot" }),
                });
            }
        }
    }
    Ok(())
}

fn start_session(
    state: &Arc<AppState>,
    agent: Agent,
    role: Role,
    cwd: &str,
    task: Option<String>,
) -> anyhow::Result<()> {
    // `portable_pty` falls back to the home directory when `cwd` does not name an existing
    // directory, which would otherwise silently start a write-capable agent in `$HOME` with no
    // error anywhere. Reject it here instead; the `Err` propagates out through
    // `handle_control_message` and becomes a `ControlToClient::Error`.
    if !std::path::Path::new(cwd).is_dir() {
        anyhow::bail!("cwd `{cwd}` is not an existing directory");
    }

    let session_id = Uuid::new_v4();
    let session = term::spawn_session(state, session_id, agent, role, cwd, task)?;
    let pid = session.pid;
    state
        .sessions
        .write()
        .expect("sessions lock poisoned")
        .insert(session_id, session);
    state.broadcast(ControlToClient::SessionStarted {
        session: session_id,
        agent,
        pid,
        agent_session_id: None,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// /ws/term/:session
// ---------------------------------------------------------------------------

async fn term_ws(
    ws: WebSocketUpgrade,
    Path(session_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_term_ws(socket, state, session_id))
}

async fn handle_term_ws(mut socket: WebSocket, state: Arc<AppState>, session_id: Uuid) {
    let Some(session) = state.get_session(&session_id) else {
        let _ = socket.send(Message::Close(None)).await;
        return;
    };

    // Replay boundary (T3): snapshot + subscribe happen atomically inside `attach_term`, so
    // sending the snapshot first and then forwarding the receiver cannot lose or duplicate a
    // byte even if the PTY is actively producing output during this attach.
    let (snapshot, mut live) = session.attach_term();
    if !snapshot.is_empty() && socket.send(Message::Binary(snapshot)).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Binary(data))) => {
                        if let Err(err) = session.write_input(&data) {
                            tracing::warn!(%err, session = %session_id, "writing PTY input failed");
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(crate::protocol::TermControl::Resize { cols, rows }) = serde_json::from_str(&text) {
                            if let Err(err) = session.resize(cols, rows) {
                                tracing::warn!(%err, session = %session_id, "resize failed");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        tracing::warn!(%err, "term websocket error");
                        break;
                    }
                }
            }
            data = live.recv() => {
                match data {
                    Ok(bytes) => {
                        if socket.send(Message::Binary(bytes.to_vec())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Prototype-quality limitation: a lagged client has to reconnect to
                        // resync rather than get a gapless replay. See
                        // `state::TERM_BROADCAST_CAPACITY`'s doc comment for why this channel
                        // lags in the first place (T2) — the PTY reader outruns it, and
                        // backpressure, not buffer depth, is the real fix milestone 01 owes.
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// /hook/:session/:event
// ---------------------------------------------------------------------------

async fn hook_callback(
    Path((session_id, event)): Path<(Uuid, String)>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    // Always 200 immediately, no matter what: PROTOCOL.md requires the hook callback to never
    // make the agent hang or error out (G4). Anything that goes wrong here is a daemon-side log,
    // not a signal fed back to the hook's `curl`.
    if let Some(session) = state.get_session(&session_id) {
        let raw: serde_json::Value =
            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
        if let Some(id) = raw.get("session_id").and_then(|v| v.as_str()) {
            // `agent_session_id` starts at `None` (see `start_session`, which always announces
            // it as such) and is set here the first time a hook payload carries it. Re-announce
            // the session at that point — otherwise this field is dead state: nothing else ever
            // reads it, and every client would be stuck believing resume is unavailable for the
            // session's whole life.
            let became_known = {
                let mut guard = session
                    .agent_session_id
                    .lock()
                    .expect("agent_session_id mutex poisoned");
                let became_known = guard.is_none();
                *guard = Some(id.to_string());
                became_known
            };
            if became_known {
                state.broadcast(ControlToClient::SessionStarted {
                    session: session.id,
                    agent: session.agent,
                    pid: session.pid,
                    agent_session_id: Some(id.to_string()),
                });
            }
        }
        let mapped = map_claude_hook_status(&event);
        state.set_status(&session, mapped, StatusSource::Hook, raw);
    } else {
        tracing::warn!(session = %session_id, %event, "hook callback for unknown session");
    }
    axum::http::StatusCode::OK
}

/// Maps a Claude Code hook event name to the `status` states in PROTOCOL.md.
///
/// Prototype-only in one respect: it dispatches on the event name alone, while `Notification`
/// carries the state in its payload's `notification_type`. Of those types only `idle_prompt` was
/// ever observed firing, which is why it maps to `Idle` here; `permission_prompt` never fired, so
/// a pending prompt is taken from `PermissionRequest` instead. A real implementation should
/// dispatch on the payload rather than assume one type.
///
/// Three gaps the mapping accepts, all settled: a `PreToolUse` carrying `AskUserQuestion` is a question to the user but
/// maps to `Working` here, for the same reason — the tool name is in the payload, not the event name; a plain-text question at turn end is
/// indistinguishable from an ordinary `Stop`, and a user interrupt fires nothing at all, so a
/// session interrupted mid-tool stays `Working` here until something else moves it.
fn map_claude_hook_status(event: &str) -> SessionState {
    match event {
        "SessionStart" => SessionState::Idle,
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => SessionState::Working,
        "PermissionRequest" => SessionState::WaitingUser,
        "Notification" => SessionState::Idle,
        "Stop" | "StopFailure" => SessionState::Idle,
        "SessionEnd" => SessionState::Exited,
        _ => SessionState::Working,
    }
}

// ---------------------------------------------------------------------------
// /mcp/:session/:tool
// ---------------------------------------------------------------------------

/// Tool sets per role, from PROTOCOL.md. The daemon only checks membership here; it does not
/// implement the tools' actual orchestration behaviour (listing projects, archiving sessions,
/// ...) — that is milestone 01's job. This bridge exists to prove that a session's identity and
/// role travel correctly from launch argv through the stdio MCP server to this endpoint (G5).
///
/// Names only — derived from `mcp_stdio::tools_for_role`, which is the advertised list including
/// schemas, so this authorization check and `tools/list`'s advertisement can never drift apart.
fn tool_names_for_role(role: Role) -> Vec<String> {
    crate::mcp_stdio::tools_for_role(role)
        .iter()
        .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect()
}

async fn mcp_tool(
    Path((session_id, tool)): Path<(Uuid, String)>,
    State(state): State<Arc<AppState>>,
    Json(args): Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(session) = state.get_session(&session_id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("unknown session {session_id}") })),
        );
    };

    if !tool_names_for_role(session.role).contains(&tool) {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": format!("tool `{tool}` is not available to role `{:?}`", session.role)
            })),
        );
    }

    state.broadcast(ControlToClient::ToolCall {
        session: session_id,
        tool: tool.clone(),
        args: args.clone(),
    });

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({ "received": true, "tool": tool, "args": args })),
    )
}
