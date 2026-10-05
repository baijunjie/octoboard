//! The HTTP and WebSocket surface described in `daemon/PROTOCOL.md`, bound to localhost only.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};

use crate::hooks;
use crate::protocol::{Agent, Event, Request, SessionStatus, TermControl};
use crate::reporting;
use crate::state::{AppState, TurnClose};
use crate::transcript;

/// Events queued for one control client before its writer is considered the bottleneck. Small:
/// these are state records, not terminal output, and a client this far behind is better served by
/// the fresh snapshot a lagged broadcast receiver triggers.
const OUTBOX_CAPACITY: usize = 256;

pub fn router(state: Arc<AppState>) -> Router {
    // The guard wraps the finished router, fallback included, so a route added to `routes` later
    // cannot end up outside it. A refused request never reaches a handler or a WebSocket upgrade.
    routes(state).layer(axum::middleware::from_fn(crate::access::guard))
}

fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws/control", get(control_ws))
        .route("/ws/term/:session", get(term_ws))
        .route("/hook/:session", post(hook_callback))
        .route("/mcp/:token", post(mcp_call))
        .with_state(state)
}

// -- /ws/control -------------------------------------------------------------

async fn control_ws(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_control(socket, state))
}

async fn handle_control(socket: WebSocket, state: Arc<AppState>) {
    // One writer, many producers. Requests are handled in their own tasks rather than inline,
    // because some of them are long: `add_project` with a GitHub source runs a `git clone`, which
    // is minutes on a large repository. Handled inline, that would stall every status event for
    // every session until the clone finished — and a stall long enough to overrun the broadcast
    // channel costs the client its place in it. Replies are correlated by request id, so they do
    // not have to come back in the order they were asked.
    let (mut sink, mut stream) = socket.split();
    let (outbound, mut outbox) = mpsc::channel::<Event>(OUTBOX_CAPACITY);

    let writer = tokio::spawn(async move {
        while let Some(event) = outbox.recv().await {
            let text = serde_json::to_string(&event).expect("protocol events always serialize");
            if sink.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    });

    // Subscribed before anything is collected, so that a broadcast between collecting the state and
    // listening for changes is not lost. A repeat is harmless: the records are whole and the
    // application ignores a prompt it already holds.
    let broadcasts = state.subscribe();

    // The snapshot is unprompted and goes first: it is the only way a client that just connected
    // learns the current state.
    match snapshot(&state) {
        Ok(snapshot) => {
            if outbound.send(snapshot).await.is_err() {
                return;
            }
            // The snapshot cannot say that a session is waiting for the user's go-ahead, and the
            // prompt was broadcast only once.
            for prompt in crate::trust::pending_prompts(&state) {
                if outbound.send(prompt).await.is_err() {
                    return;
                }
            }
        }
        Err(err) => {
            tracing::error!(%err, "building the state snapshot failed");
            return;
        }
    }

    let forwarder = tokio::spawn(forward_broadcasts(
        state.clone(),
        broadcasts,
        outbound.clone(),
    ));

    while let Some(incoming) = stream.next().await {
        match incoming {
            Ok(Message::Text(text)) => {
                let state = state.clone();
                let outbound = outbound.clone();
                tokio::spawn(async move {
                    let reply = handle_request(&state, &text).await;
                    let _ = outbound.send(reply).await;
                });
            }
            Ok(Message::Close(_)) => break,
            // Control is text-only; binary frames and pings are not ours to interpret.
            Ok(_) => {}
            Err(err) => {
                tracing::debug!(%err, "control socket error");
                break;
            }
        }
    }

    forwarder.abort();
    drop(outbound);
    let _ = writer.await;
}

/// Forwards state broadcasts to one client. A client that fell behind cannot repair itself from
/// the stream, so it is sent a fresh snapshot instead.
async fn forward_broadcasts(
    state: Arc<AppState>,
    mut events: broadcast::Receiver<Event>,
    outbound: mpsc::Sender<Event>,
) {
    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            Err(broadcast::error::RecvError::Lagged(_)) => match snapshot(&state) {
                Ok(snapshot) => {
                    if outbound.send(snapshot).await.is_err() {
                        return;
                    }
                    // Whatever prompt the client lagged past is asked again; see the initial
                    // snapshot.
                    for prompt in crate::trust::pending_prompts(&state) {
                        if outbound.send(prompt).await.is_err() {
                            return;
                        }
                    }
                    continue;
                }
                Err(err) => {
                    tracing::error!(%err, "rebuilding the state snapshot failed");
                    return;
                }
            },
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if outbound.send(event).await.is_err() {
            return;
        }
    }
}

/// Parses and runs one request, and returns what goes back on the asking socket.
async fn handle_request(state: &Arc<AppState>, text: &str) -> Event {
    let request: Request = match serde_json::from_str(text) {
        Ok(request) => request,
        Err(err) => {
            return Event::Error {
                id: None,
                code: None,
                message: format!("unreadable request: {err}"),
            }
        }
    };
    let id = request.id.clone();
    match crate::coordinator::handle(state, request.id, request.body).await {
        Ok(Some(event)) => event,
        Ok(None) => Event::Ack { id },
        // The message is the user's only account of what went wrong, so the whole context chain
        // goes through rather than just the outermost error.
        Err(err) => Event::Error {
            id,
            code: err
                .downcast_ref::<crate::protocol::CodedError>()
                .map(|coded| coded.code.to_string()),
            message: format!("{err:#}"),
        },
    }
}

fn snapshot(state: &Arc<AppState>) -> anyhow::Result<Event> {
    Ok(Event::Snapshot {
        hosts: state.store.list_hosts()?,
        consoles: state.store.list_consoles()?,
        projects: state.store.list_projects()?,
        sessions: state.store.list_sessions()?,
        trusted_directories: state.store.trusted_directories()?,
    })
}

// -- /ws/term/:session -------------------------------------------------------

async fn term_ws(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_term(socket, state, session_id))
}

async fn handle_term(mut socket: WebSocket, state: Arc<AppState>, session_id: String) {
    let Some(session) = state.live_session(&session_id) else {
        // Not running: the UI shows the stored status and offers to resume.
        let _ = socket.send(Message::Close(None)).await;
        return;
    };

    // Snapshot and subscription happen under one lock inside `attach`, so the replay cannot miss a
    // byte or repeat one even while the agent is producing output right now.
    let (replay, mut live) = session.attach();
    if !replay.is_empty() && socket.send(Message::Binary(replay)).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Binary(data))) => {
                        if let Err(err) = session.write_input(&data) {
                            tracing::debug!(session = %session_id, %err, "writing terminal input failed");
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<TermControl>(&text) {
                            Ok(TermControl::Resize { cols, rows }) => {
                                if let Err(err) = session.resize(cols, rows) {
                                    tracing::debug!(session = %session_id, %err, "resize failed");
                                }
                            }
                            Err(err) => tracing::debug!(session = %session_id, %err, "unreadable terminal control frame"),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        tracing::debug!(session = %session_id, %err, "terminal socket error");
                        break;
                    }
                }
            }
            output = live.recv() => {
                match output {
                    Some(bytes) => {
                        if socket.send(Message::Binary(bytes.to_vec())).await.is_err() {
                            break;
                        }
                    }
                    // The daemon dropped this client (it stopped draining) or the session ended.
                    None => break,
                }
            }
        }
    }
}

// -- /hook/:session ----------------------------------------------------------

/// The hook callback. It answers `200` immediately whatever happens: every one of the three agents
/// renders a failing hook to the user, and the daemon being busy or confused is not something to
/// show them. The event name comes from the payload, because the hook script is argument-free.
async fn hook_callback(
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let payload: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);

    // Claude Code runs no hook before its workspace-trust screen has been answered, so the first
    // one means that screen is gone — and anything on the terminal that looks like it is not it.
    if let Some(live) = state.live_session(&session_id) {
        live.trust.mark_hook_seen();
    }

    // A subagent's events are the agent's internal business and carry the subagent's own ids.
    if hooks::is_subagent(&payload) {
        return axum::http::StatusCode::OK;
    }

    if let Some(agent_session_id) = hooks::agent_session_id(&payload) {
        if let Err(err) = state.set_agent_session_id(&session_id, &agent_session_id) {
            tracing::debug!(session = %session_id, %err, "recording the agent's session id failed");
        }
    }

    match hooks::event_name(&payload) {
        Some(event) => {
            let agent = state
                .live_session(&session_id)
                .map(|live| live.agent)
                .or_else(|| {
                    state
                        .store
                        .get_session(&session_id)
                        .ok()
                        .flatten()
                        .map(|session| session.agent)
                });
            if hooks::starts_conversation(&event) {
                if let Err(err) = state.mark_conversation_started(&session_id) {
                    tracing::debug!(session = %session_id, %err, "recording the session's first turn failed");
                }
                state.turn_started(&session_id);
            }
            if let Some(agent) = agent {
                let believable = read_turn_boundary(&state, &session_id, agent, &event, &payload);
                if believable {
                    if let Some(status) = hooks::status_from_event(agent, &event, &payload) {
                        let status = suppress_unanswerable_hand(&state, &session_id, status);
                        if let Err(err) = state.apply_hook_status(&session_id, status) {
                            tracing::debug!(session = %session_id, %event, %err, "applying the hook status failed");
                        }
                        // Claude Code emits no hook event at all for a rejected permission prompt
                        // or a declined `AskUserQuestion` — the transcript is the only trace either
                        // leaves. Spawned, not awaited: the watch is expected to far outlive this
                        // response, which the adapters' few-second hook timeout would not allow.
                        if agent == Agent::Claude && status == SessionStatus::WaitingUser {
                            transcript::watch_for_rejection(
                                &state,
                                &session_id,
                                payload
                                    .get("transcript_path")
                                    .and_then(serde_json::Value::as_str),
                            );
                        }
                        // Releasing a message queued while the session could not take one writes
                        // into its PTY, which blocks; it must not sit on this response, where the
                        // adapters' few-second hook timeout would surface as agent-visible noise.
                        state.spawn_flush_outbox(&session_id, status);
                    }
                }
            } else {
                tracing::debug!(session = %session_id, %event, "hook callback for an unknown session");
            }
        }
        None => tracing::debug!(session = %session_id, "hook payload carried no event name"),
    }

    axum::http::StatusCode::OK
}

/// Acts on what this event says about the session's current turn, and says whether anything else it
/// claims about the session may be believed.
///
/// A turn boundary is read before the status is: Grok's one id-less turn-end signal also fires after
/// a turn that already ended, and taking its "idle" at face value would put a session that is
/// working back to idle.
fn read_turn_boundary(
    state: &Arc<AppState>,
    session_id: &str,
    agent: crate::protocol::Agent,
    event: &str,
    payload: &serde_json::Value,
) -> bool {
    if hooks::turn_cancelled(agent, event) {
        // No report is owed — the user did it and knows — but the turn still has to be closed, or a
        // later clock-attributed end would find it open and invent one.
        state.abandon_turn(session_id);
        return true;
    }
    match hooks::turn_end(agent, event, payload) {
        // A session with its hand up has not finished its turn — it is waiting for a person. Grok's
        // clock-attributed end would otherwise lower the hand and report "it said nothing" for any
        // prompt left unanswered longer than the backstop delay.
        Some(turn)
            if turn.backstop
                && state
                    .store
                    .get_session(session_id)
                    .ok()
                    .flatten()
                    .is_some_and(|session| session.status == SessionStatus::WaitingUser) =>
        {
            false
        }
        Some(turn) => match state.close_turn(session_id, turn.backstop) {
            // Talking about a turn that is already over, so nothing it says about the session holds.
            TurnClose::NotThisTurn => false,
            TurnClose::NoTurn | TurnClose::Reported => true,
            TurnClose::OwesReport => {
                let state = state.clone();
                let session_id = session_id.to_string();
                // Delivery writes into the hub's PTY, which blocks, so it goes off this response.
                tokio::task::spawn_blocking(move || {
                    reporting::synthesise_report(&state, &session_id, turn)
                });
                true
            }
        },
        None => {
            // Anything that is not a turn boundary is the session still working on its turn, which
            // is what keeps a backstop from being attributed to it.
            state.touch_turn(session_id);
            true
        }
    }
}

/// Turns a raised hand nobody can answer back into plain work.
///
/// With Codex's `approvals_reviewer` set to auto review, the permission hook fires, Codex resolves
/// the request itself, no modal is ever shown and the tool proceeds. Raising a hand there would
/// ask the user to answer something they never see, and nothing would ever clear it.
fn suppress_unanswerable_hand(
    state: &Arc<AppState>,
    session_id: &str,
    status: crate::protocol::SessionStatus,
) -> crate::protocol::SessionStatus {
    use crate::protocol::SessionStatus;
    if status != SessionStatus::WaitingUser {
        return status;
    }
    match state.live_session(session_id) {
        Some(live) if live.resolves_approvals_itself => SessionStatus::Working,
        _ => status,
    }
}

// -- /mcp/:token -------------------------------------------------------------

/// One MCP tool call, forwarded here by the session's own MCP server. The token is what says which
/// session is calling: the child never names its own session, so one that rewrote its arguments
/// still cannot act on another.
async fn mcp_call(
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
    axum::Json(
        match crate::mcp::exec::call(&state, &session_id, &request.tool, &arguments).await {
            Ok(result) => serde_json::json!({ "ok": true, "result": result }),
            // The whole context chain, because this text is the only account the model gets of
            // why its call did not work.
            Err(err) => serde_json::json!({ "ok": false, "error": format!("{err:#}") }),
        },
    )
}

#[derive(serde::Deserialize)]
struct McpCall {
    tool: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;

    use super::*;
    use crate::state::tests::app_state;

    /// Serves the real router on an ephemeral loopback port and returns the port.
    async fn serve(name: &str) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        let state = Arc::new(app_state(name));
        tokio::spawn(async move { axum::serve(listener, router(state)).await });
        port
    }

    /// Sends one request with exactly the headers given (plus a body length) and returns the
    /// response's status code, read from the status line alone so an upgraded connection does not
    /// have to close first.
    fn status_of(port: u16, request_line: &str, headers: &[&str]) -> u16 {
        let mut request = format!("{request_line} HTTP/1.1\r\n");
        for header in headers {
            request.push_str(header);
            request.push_str("\r\n");
        }
        request.push_str("Content-Length: 2\r\n\r\n{}");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream.write_all(request.as_bytes()).expect("write");
        let mut status_line = String::new();
        BufReader::new(stream)
            .read_line(&mut status_line)
            .expect("read");
        status_line
            .split_whitespace()
            .nth(1)
            .and_then(|status| status.parse().ok())
            .expect("a status line")
    }

    /// A refused request must not reach a handler, and an accepted one must: `/mcp/:token`
    /// answers 200 to a token it does not know, which no other layer would, while the upgrade
    /// routes answer 400 to a handshake that is not one — still not the 403 of the guard.
    #[tokio::test]
    async fn every_route_is_behind_the_origin_guard() {
        let port = serve("guard").await;
        let host = format!("Host: 127.0.0.1:{port}");
        let tests = tokio::task::spawn_blocking(move || {
            let routes = [
                "GET /ws/control",
                "GET /ws/term/s",
                "POST /hook/s",
                "POST /mcp/x",
            ];
            for route in routes {
                let forbidden = |headers: &[&str]| status_of(port, route, headers) == 403;
                assert!(
                    forbidden(&[&host, "Origin: https://evil.example"]),
                    "{route}"
                );
                assert!(forbidden(&[&host, "Origin: null"]), "{route}");
                assert!(
                    forbidden(&["Host: evil.example", "Origin: http://evil.example"]),
                    "{route}"
                );
                assert!(forbidden(&["Host: evil.example"]), "{route}");
                assert!(forbidden(&[]), "{route}");
                assert!(!forbidden(&[&host]), "{route}");
                assert!(
                    !forbidden(&[&host, "Origin: http://localhost:5174"]),
                    "{route}"
                );
                assert!(!forbidden(&[&host, "Origin: tauri://localhost"]), "{route}");
            }
            assert_eq!(status_of(port, "POST /mcp/x", &[&host]), 200);
        });
        tests.await.expect("the checks ran");
    }

    /// The handshake a browser sends: a refusal must come before the upgrade, and an allowed
    /// origin, or none, must get through it.
    #[tokio::test]
    async fn a_websocket_handshake_is_refused_or_upgraded_by_origin() {
        let port = serve("handshake").await;
        let host = format!("Host: 127.0.0.1:{port}");
        tokio::task::spawn_blocking(move || {
            let handshake = |origin: Option<&str>| {
                let mut headers = vec![
                    host.as_str(),
                    "Connection: Upgrade",
                    "Upgrade: websocket",
                    "Sec-WebSocket-Version: 13",
                    "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
                ];
                headers.extend(origin);
                status_of(port, "GET /ws/control", &headers)
            };
            assert_eq!(handshake(Some("Origin: https://evil.example")), 403);
            assert_eq!(handshake(Some("Origin: tauri://localhost")), 101);
            assert_eq!(handshake(None), 101);
        })
        .await
        .expect("the checks ran");
    }

    /// A path no route serves is still behind the guard: hostile callers learn nothing from it.
    #[tokio::test]
    async fn the_fallback_is_behind_the_origin_guard() {
        let port = serve("fallback").await;
        let host = format!("Host: 127.0.0.1:{port}");
        tokio::task::spawn_blocking(move || {
            let hostile = status_of(port, "GET /nope", &[&host, "Origin: https://evil.example"]);
            assert_eq!(hostile, 403);
            assert_eq!(status_of(port, "GET /nope", &[&host]), 404);
        })
        .await
        .expect("the checks ran");
    }
}
