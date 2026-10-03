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
use crate::protocol::{Event, Request, TermControl};
use crate::state::AppState;

/// Events queued for one control client before its writer is considered the bottleneck. Small:
/// these are state records, not terminal output, and a client this far behind is better served by
/// the fresh snapshot a lagged broadcast receiver triggers.
const OUTBOX_CAPACITY: usize = 256;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws/control", get(control_ws))
        .route("/ws/term/:session", get(term_ws))
        .route("/hook/:session", post(hook_callback))
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

    // The snapshot is unprompted and goes first: it is the only way a client that just connected
    // learns the current state.
    match snapshot(&state) {
        Ok(snapshot) => {
            if outbound.send(snapshot).await.is_err() {
                return;
            }
        }
        Err(err) => {
            tracing::error!(%err, "building the state snapshot failed");
            return;
        }
    }

    let forwarder = tokio::spawn(forward_broadcasts(
        state.clone(),
        state.subscribe(),
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
                Ok(snapshot) => snapshot,
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
            }
            if let Some(agent) = agent {
                if let Some(status) = hooks::status_from_event(agent, &event, &payload) {
                    if let Err(err) = state.apply_hook_status(&session_id, status) {
                        tracing::debug!(session = %session_id, %event, %err, "applying the hook status failed");
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
