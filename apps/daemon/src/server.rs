//! The HTTP and WebSocket surface described in `apps/daemon/PROTOCOL.md`, bound to localhost only.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};

use crate::browse::budget;
use crate::browse::git::GitEnv;
use crate::browse::lane::{Frame, Lane};
use crate::coordinator::Followup;
use crate::hook_queue::Push;
use crate::hooks::{self, BackgroundTasksChange};
use crate::protocol::{
    Agent, BrowseBody, BrowseRequest, Event, Request, SessionStatus, TermControl,
    BROWSE_REQUEST_TYPES,
};
use crate::pty::PtyCore;
use crate::reporting;
use crate::rollout;
use crate::session::LiveSession;
use crate::shell_terminal::ShellProcess;
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
        .route("/git-ref/:session", post(git_ref_callback))
        .route("/context-window/:session", post(context_window_callback))
        .route("/mcp/:token", post(mcp_call))
        .with_state(state)
}

// -- /ws/control -------------------------------------------------------------

async fn control_ws(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_control(socket, state))
}

async fn handle_control(socket: WebSocket, state: Arc<AppState>) {
    // One writer, many producers. Requests are handled in their own tasks rather than inline,
    // because some of them are long: `add_project` with a `git` source runs a `git clone`, which
    // is minutes on a large repository. Handled inline, that would stall every status event for
    // every session until the clone finished — and a stall long enough to overrun the broadcast
    // channel costs the client its place in it. Replies are correlated by request id, so they do
    // not have to come back in the order they were asked.
    let (sink, mut stream) = socket.split();
    let (outbound, outbox) = mpsc::channel::<Event>(OUTBOX_CAPACITY);
    let (content_tx, content) = mpsc::channel::<Frame>(CONTENT_QUEUE_CAPACITY);
    let lane = Arc::new(Lane::new(
        state.browse_gate.clone(),
        budget::MAX_PENDING_REQUESTS,
        budget::MAX_RETAINED_PER_CONNECTION,
        outbound.clone(),
        content_tx,
    ));

    let mut writer = tokio::spawn(write_frames(sink, outbox, content, WRITE_DEADLINE));

    // Subscribed before anything is collected, so that a broadcast between collecting the state and
    // listening for changes is not lost. A repeat is harmless: the records are whole and the
    // application ignores a prompt or request it already holds.
    let broadcasts = state.subscribe();

    // The snapshot is unprompted and goes first: it is the only way a client that just connected
    // learns the current state.
    match snapshot(&state) {
        Ok(snapshot) => {
            if outbound.send(snapshot).await.is_err() {
                return;
            }
            // The snapshot cannot say that a session is waiting for the user's go-ahead or answer,
            // and the prompt or request was broadcast only once.
            for question in pending_questions(&state) {
                if outbound.send(question).await.is_err() {
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

    // The writer ends early when the client has stopped reading, or its socket failed; the
    // connection is then dropped, here, rather than left reading requests it can no longer answer.
    // A request this loop is awaiting on the way (a refusal waiting for room on the control queue)
    // returns as soon as the writer is gone, since that closes the queue.
    let mut writer_done = false;
    loop {
        let incoming = tokio::select! {
            incoming = stream.next() => incoming,
            _ = &mut writer => {
                writer_done = true;
                break;
            }
        };
        let Some(incoming) = incoming else {
            break;
        };
        match incoming {
            Ok(Message::Text(text)) => {
                match browse_request(&text) {
                    Some(Ok(request)) => {
                        submit_browse(&state, &lane, request).await;
                        continue;
                    }
                    Some(Err(detail)) => {
                        let _ = outbound.send(Event::unreadable_request(&detail)).await;
                        continue;
                    }
                    None => {}
                }
                let state = state.clone();
                let outbound = outbound.clone();
                tokio::spawn(async move {
                    let (reply, followup) = handle_request(&state, &text).await;
                    let _ = outbound.send(reply).await;
                    // After the reply is queued, never before: what the follow-up does (a launch
                    // that fails at once deletes the session just opened) is broadcast on a path
                    // of its own, and must not reach the client ahead of the reply that tells it
                    // the session exists.
                    if let Some(followup) = followup {
                        followup.start();
                    }
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
    lane.close();
    drop(lane);
    drop(outbound);
    if !writer_done {
        let _ = writer.await;
    }
}

/// Browse replies queued for one client's socket, as frames; each also holds its share of the
/// retained-bytes budget, so this only bounds how many wait, not how much.
const CONTENT_QUEUE_CAPACITY: usize = 16;

/// How long the socket may take to accept one frame. On a loopback connection a client that takes
/// no bytes for this long has stopped reading, and holding its frames — browse replies above all,
/// which keep their share of the retained bytes until written — would hold up every other
/// connection; it is disconnected instead, and reconnects to a fresh `snapshot`.
const WRITE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// The one writer of a control socket. A control event (a broadcast, a reply to an ordinary
/// request, any `error`) is always written before a browse reply that is waiting at the same
/// moment, so a queue of file bodies delays a status change by at most the one frame already being
/// written. Ends when the control queue closes, which happens once the connection is done with,
/// when a send fails, or when the socket has not taken a frame within `deadline`.
async fn write_frames<S>(
    mut sink: S,
    mut control: mpsc::Receiver<Event>,
    mut content: mpsc::Receiver<Frame>,
    deadline: std::time::Duration,
) where
    S: futures_util::Sink<Message> + Unpin,
{
    let mut content_open = true;
    loop {
        // A browse reply's frame is held until its text has been written, so its share of the
        // retained bytes is given back only then.
        let mut written = None;
        let text = tokio::select! {
            biased;
            event = control.recv() => match event {
                Some(event) => {
                    serde_json::to_string(&event).expect("protocol events always serialize")
                }
                None => break,
            },
            frame = content.recv(), if content_open => match frame {
                Some(mut frame) => {
                    let text = std::mem::take(&mut frame.text);
                    written = Some(frame);
                    text
                }
                None => {
                    content_open = false;
                    continue;
                }
            },
        };
        let sent = match tokio::time::timeout(deadline, sink.send(Message::Text(text))).await {
            Ok(result) => result.is_ok(),
            Err(_) => {
                tracing::debug!("the control client stopped reading");
                false
            }
        };
        if !sent {
            break;
        }
        drop(written);
    }
}

/// The frame as a browse request, when its `type` is one: `Some(Ok)` to serve, `Some(Err)` with
/// why its fields do not read. Anything else, malformed frames included, is `None` and left to the
/// ordinary request path and its own error reply.
fn browse_request(text: &str) -> Option<Result<BrowseRequest, String>> {
    #[derive(serde::Deserialize)]
    struct Kind<'a> {
        #[serde(rename = "type", borrow)]
        kind: Option<std::borrow::Cow<'a, str>>,
    }
    let kind = serde_json::from_str::<Kind>(text).ok()?.kind?;
    if !BROWSE_REQUEST_TYPES.contains(&kind.as_ref()) {
        return None;
    }
    Some(serde_json::from_str::<BrowseRequest>(text).map_err(|err| err.to_string()))
}

async fn submit_browse(state: &Arc<AppState>, lane: &Arc<Lane>, request: BrowseRequest) {
    let state = state.clone();
    let id = request.id.clone();
    let reservation = match request.body {
        BrowseBody::ListProjectDir { .. } | BrowseBody::SearchProjectFiles { .. } => {
            budget::LISTING_RESERVATION
        }
        BrowseBody::GetProjectSource { .. } | BrowseBody::ReadProjectFile { .. } => {
            budget::READ_RESERVATION
        }
        BrowseBody::ListProjectChanges { .. } => budget::CHANGE_LIST_RESERVATION,
        BrowseBody::ReadProjectChange { .. }
        | BrowseBody::ReadProjectComparisonChange { .. }
        | BrowseBody::ReadProjectChangeBodies { .. }
        | BrowseBody::ReadProjectComparisonChangeBodies { .. }
        | BrowseBody::ReadProjectCommitChange { .. }
        | BrowseBody::ReadProjectCommitChangeBodies { .. } => budget::DIFF_RESERVATION,
        BrowseBody::ListProjectBranches { .. } => budget::BRANCH_LIST_RESERVATION,
        BrowseBody::CompareProjectBranches { .. } => budget::COMPARISON_RESERVATION,
        BrowseBody::ListProjectCommits { .. } => budget::COMMITS_RESERVATION,
    };
    lane.submit(request.id, request.slot, reservation, move |cancel| {
        crate::browse::serve(&state, id, request.body, &GitEnv::from_shell, cancel)
    })
    .await;
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
                    // Whatever prompt or request the client lagged past is asked again; see the
                    // initial snapshot.
                    for question in pending_questions(&state) {
                        if outbound.send(question).await.is_err() {
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

/// What the daemon is waiting on the user for, to put to a client again after a `snapshot`: the
/// trust prompts, then the session requests, oldest first.
fn pending_questions(state: &AppState) -> Vec<Event> {
    let mut questions = crate::trust::pending_prompts(state);
    questions.extend(crate::session_request::pending_events(state));
    questions
}

/// Parses and runs one request, and returns what goes back on the asking socket, with what the
/// request still has to do once that has been sent.
async fn handle_request(state: &Arc<AppState>, text: &str) -> (Event, Option<Followup>) {
    let request: Request = match serde_json::from_str(text) {
        Ok(request) => request,
        Err(err) => return (Event::unreadable_request(&err.to_string()), None),
    };
    let id = request.id.clone();
    match crate::coordinator::handle_deferred(state, request.id, request.body).await {
        Ok((Some(event), followup)) => (event, followup),
        Ok((None, followup)) => (Event::Ack { id }, followup),
        Err(err) => (Event::from_error(id, &err), None),
    }
}

fn snapshot(state: &Arc<AppState>) -> anyhow::Result<Event> {
    Ok(Event::Snapshot {
        hosts: state.store.list_hosts()?,
        consoles: state.store.list_consoles()?,
        projects: state.store.list_projects()?,
        sessions: state
            .store
            .list_sessions()?
            .into_iter()
            .map(|session| state.wire_session(session))
            .collect(),
        trusted_directories: state.store.trusted_directories()?,
        settings: state.store.get_settings()?,
        git_statuses: state.git_statuses(),
        agent_availability: state.agent_availability(),
        account_quotas: state.account_quotas.list(),
        account_logins: state.account_logins.running(),
        account_sharing: crate::shared_config::states(state)?,
        home_dir: crate::paths::known_home_dir().map(|home| home.to_string_lossy().into_owned()),
        shell_terminals: state.shell_terminals.list(),
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

/// What a terminal socket is attached to: an agent session's process, or a shell terminal's.
enum Terminal {
    Session(Arc<LiveSession>),
    Shell(Arc<ShellProcess>),
}

impl Terminal {
    fn pty(&self) -> &PtyCore {
        match self {
            Terminal::Session(session) => session,
            Terminal::Shell(shell) => shell,
        }
    }

    fn write_input(&self, data: &[u8]) -> Result<(), crate::ptyio::PartialWrite> {
        match self {
            Terminal::Session(session) => session.write_input(data),
            Terminal::Shell(shell) => shell.write(data),
        }
    }
}

async fn handle_term(mut socket: WebSocket, state: Arc<AppState>, id: String) {
    let terminal = if let Some(session) = state.live_session(&id) {
        Terminal::Session(session)
    } else if let Some(shell) = state.shell_terminals.process(&id) {
        Terminal::Shell(shell)
    } else {
        // Not running: what its last process printed, if that was kept, and the close. Read-only:
        // there is nothing to write input into.
        let saved = {
            let state = state.clone();
            let session_id = id.clone();
            tokio::task::spawn_blocking(move || state.saved_output(&session_id))
        };
        if let Ok(Some(output)) = saved.await {
            if !output.is_empty() && socket.send(Message::Binary(output)).await.is_err() {
                return;
            }
        }
        let _ = socket.send(Message::Close(None)).await;
        return;
    };
    let pty = terminal.pty();

    // Archiving is announced before the process finishes its graceful exit. Its frozen screen
    // is already available then, so a read-only client never replays the shutdown renderer.
    if let Some(output) = pty.frozen_output() {
        if !output.is_empty() && socket.send(Message::Binary(output)).await.is_err() {
            return;
        }
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    // Snapshot and subscription happen under one lock inside `attach`, so the replay cannot miss a
    // byte or repeat one even while the process is producing output right now. A shell that has
    // exited answers with its replay alone, read-only, as a dormant session does.
    let (replay, mut live) = pty.attach();
    if !replay.is_empty() && socket.send(Message::Binary(replay)).await.is_err() {
        return;
    }
    if live.is_closed() {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Binary(data))) => {
                        if let Err(err) = terminal.write_input(&data) {
                            tracing::debug!(terminal = %id, %err, "writing terminal input failed");
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<TermControl>(&text) {
                            Ok(TermControl::Resize { cols, rows }) => {
                                if let Err(err) = pty.resize(cols, rows) {
                                    tracing::debug!(terminal = %id, %err, "resize failed");
                                }
                            }
                            Err(err) => tracing::debug!(terminal = %id, %err, "unreadable terminal control frame"),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        tracing::debug!(terminal = %id, %err, "terminal socket error");
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
                    // The daemon dropped this client (it stopped draining), the session ended,
                    // or the shell it followed exited or was closed.
                    None => break,
                }
            }
        }
    }
}

// -- /hook/:session ----------------------------------------------------------

/// The hook callback. It answers `200` with an empty body at once whatever happens, and applies the
/// event afterwards on the session's own queue, in the order the events arrived. Every one of the
/// three agents renders a failing hook to the user, and the daemon being busy or confused is not
/// something to show them; for Claude Code's own HTTP hook, which is synchronous, an answer that
/// waited would also hold the turn, and one that never came (an unreachable daemon, the hook's
/// timeout) shows a non-blocking error notice in the transcript. An empty `200` is read there as
/// "no decision", so the event cannot steer the turn either.
async fn hook_callback(
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    match state.hook_queues.push(&session_id, body) {
        Push::StartWorker => {
            // Applying an event is blocking work on the store, so the drain runs on the blocking
            // pool; the tasks `apply_hook` starts find the runtime from there.
            tokio::task::spawn_blocking(move || drain_hook_queue(&state, &session_id));
        }
        Push::Queued => {}
        Push::Coalesced => {
            tracing::warn!(session = %session_id, "the hook queue is full; the last event in line was replaced")
        }
    }
    axum::http::StatusCode::OK
}

/// Applies a session's queued hook events one after another until none is left. Applying one is
/// ordinary blocking work on the store, as it was when the route did it before answering.
fn drain_hook_queue(state: &Arc<AppState>, session_id: &str) {
    /// Releases the queue if applying an event panics, so the session's later events start a worker
    /// of their own instead of waiting on one that is gone.
    struct Abandon<'a> {
        state: &'a AppState,
        session_id: &'a str,
        drained: bool,
    }
    impl Drop for Abandon<'_> {
        fn drop(&mut self) {
            if !self.drained {
                self.state.hook_queues.abandon(self.session_id);
            }
        }
    }
    let mut guard = Abandon {
        state,
        session_id,
        drained: false,
    };
    while let Some(body) = state.hook_queues.next(session_id) {
        apply_hook(state, session_id, &body);
        if !state.hook_queues.complete(session_id) {
            break;
        }
    }
    guard.drained = true;
}

/// The agent a hook's session runs, live or recorded.
fn hook_agent(state: &Arc<AppState>, session_id: &str) -> Option<Agent> {
    state
        .live_session(session_id)
        .map(|live| live.agent)
        .or_else(|| {
            state
                .store
                .get_session(session_id)
                .ok()
                .flatten()
                .map(|session| session.agent)
        })
}

/// Applies one hook event to its session. The event name comes from the payload, because the hook
/// script is argument-free.
fn apply_hook(state: &Arc<AppState>, session_id: &str, body: &[u8]) {
    let payload: serde_json::Value =
        serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);

    // Marked before anything of this event can release a queue: the first hook ends the trust
    // hold below, and its drain must not write into an error's prompt. A subagent's failure is
    // its own business, never the session's.
    let agent = hook_agent(state, session_id);
    if let (Some(agent), Some(event)) = (agent, hooks::event_name(&payload)) {
        if !hooks::is_subagent(Some(agent), &payload)
            && hooks::stopped_on_agent_error(agent, &event, &payload)
        {
            if let Some(live) = state.live_session(session_id) {
                live.mark_agent_error();
                // Nothing else drains the queue once the hold has run out.
                let state = state.clone();
                let id = session_id.to_string();
                tokio::spawn(async move {
                    tokio::time::sleep(crate::session::AGENT_ERROR_SETTLE).await;
                    if let Ok(session) = state.session_record(&id) {
                        state.spawn_flush_outbox(&id, state.agent_status(&session));
                    }
                });
            }
        }
    }

    // Claude Code and Grok run no hook before their trust confirmation has been answered, so the
    // first one means that screen is gone — and anything on the terminal that looks like it is not
    // it. Codex runs none around its own, but its first hook still comes only once the session is
    // past it. A Grok screen the person answered in the terminal has its trust entry carried over
    // from here (`trust::on_hook`).
    if let Some(live) = state.live_session(session_id) {
        crate::trust::on_hook(state, &live);
    }

    // A subagent's events are the agent's internal business and carry the subagent's own ids; the
    // only thing read from them is the background work they leave running, which never changes
    // the session's status.
    if hooks::is_subagent(agent, &payload) {
        let change = hooks::event_name(&payload)
            .zip(agent)
            .and_then(|(event, agent)| hooks::background_tasks_change(agent, &event, &payload));
        if let Some(change) = change {
            state.apply_background_tasks_change(session_id, change);
        }
        return;
    }

    if let Some(agent_session_id) = hooks::agent_session_id(&payload) {
        if let Err(err) = state.set_agent_session_id(session_id, &agent_session_id) {
            tracing::debug!(session = %session_id, %err, "recording the agent's session id failed");
        }
    }

    crate::agent_records::on_hook(state, session_id, &payload);

    match hooks::event_name(&payload) {
        Some(event) => {
            let mut prompt_sent = false;
            if hooks::starts_conversation(&event) {
                if let Err(err) = state.mark_conversation_started(session_id) {
                    tracing::debug!(session = %session_id, %err, "recording the session's first turn failed");
                }
                let prompt = agent.and_then(|agent| hooks::submitted_prompt(agent, &payload));
                // A background task's completion arrives as a prompt the agent sends itself, which
                // says nothing about what is in its input box.
                prompt_sent = !prompt.is_some_and(hooks::is_task_notification);
                // The previous turn's watch goes before the new turn opens: a watch still current
                // when it opens could find the previous turn's failure (Codex) or interrupt (Claude
                // Code) and close this one. Grok has no watch.
                if matches!(agent, Some(Agent::Codex | Agent::Claude)) {
                    state.end_record_watch(session_id);
                }
                state.turn_started(session_id, prompt);
            }
            if let Some(agent) = agent {
                let believable = read_turn_boundary(state, session_id, agent, &event, &payload);
                if believable {
                    // A whole new list goes in before the status, so the record the status
                    // change publishes already carries it. A completion notification goes in
                    // after: it arrives as a prompt while the session is still idle, and trimming
                    // the list first would publish a plain idle record before the turn starts.
                    let mut finished = None;
                    match hooks::background_tasks_change(agent, &event, &payload) {
                        Some(BackgroundTasksChange::Finished(ids)) => finished = Some(ids),
                        Some(change) => state.apply_background_tasks_change(session_id, change),
                        None => {}
                    }
                    if let Some(status) = hooks::status_from_event(agent, &event, &payload) {
                        let status = suppress_auto_review_hook(state, session_id, status);
                        // Read before the status lands: a Claude Code watch is armed on a move
                        // into a status, not on more of the same.
                        let previous = (agent == Agent::Claude)
                            .then(|| state.session_record(session_id).ok())
                            .flatten()
                            .map(|session| state.agent_status(&session));
                        if let Err(err) = state.apply_hook_status(session_id, status) {
                            tracing::debug!(session = %session_id, %event, %err, "applying the hook status failed");
                        }
                        arm_record_watches(
                            state, session_id, agent, &event, previous, status, &payload,
                        );
                        // Counted after the status it brings and before the drain that status
                        // releases: that drain lets a queue held for an unsent message go on, and
                        // the agent reads as working by the time the outbox sees the count move.
                        if prompt_sent {
                            if let Some(live) = state.live_session(session_id) {
                                live.note_prompt_sent();
                            }
                        }
                        // Releasing a message queued while the session could not take one writes
                        // into its PTY, which blocks; it must not sit on this response, where the
                        // adapters' few-second hook timeout would surface as agent-visible noise.
                        state.spawn_flush_outbox(session_id, status);
                    }
                    if let Some(ids) = finished {
                        state.apply_background_tasks_change(
                            session_id,
                            BackgroundTasksChange::Finished(ids),
                        );
                    }
                }
            } else {
                tracing::debug!(session = %session_id, %event, "hook callback for an unknown session");
            }
        }
        None => tracing::debug!(session = %session_id, "hook payload carried no event name"),
    }
}

/// Starts watching the agent's own session record for what its hooks will not report after this
/// event, which reported `status` and found the agent at `previous` (read for Claude Code alone).
/// Claude Code reports neither a declined prompt nor an interrupted turn, so its transcript is
/// watched (`crate::transcript` says when); a Codex turn that ends in an error fires nothing after
/// its `UserPromptSubmit`, so its rollout is. Each watch is spawned, not awaited: it is expected to
/// far outlive this response, which the adapters' few-second hook timeout would not allow.
fn arm_record_watches(
    state: &Arc<AppState>,
    session_id: &str,
    agent: Agent,
    event: &str,
    previous: Option<SessionStatus>,
    status: SessionStatus,
    payload: &serde_json::Value,
) {
    let transcript_path = payload
        .get("transcript_path")
        .and_then(serde_json::Value::as_str);
    match agent {
        Agent::Claude => transcript::watch_after_hook(
            state,
            session_id,
            event,
            previous,
            status,
            transcript_path,
        ),
        Agent::Codex if event == "UserPromptSubmit" => rollout::watch_for_failure(
            state,
            session_id,
            transcript_path,
            payload.get("turn_id").and_then(serde_json::Value::as_str),
        ),
        _ => {}
    }
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
    agent: Agent,
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
        // A session with its hand up has not finished its turn — it is waiting for a person (the
        // agent's own hand: a session request's hand says nothing of its turn). Grok's
        // clock-attributed end would otherwise lower the hand and report "it said nothing" for any
        // prompt left unanswered longer than the backstop delay.
        Some(turn)
            if turn.backstop
                && state
                    .store
                    .get_session(session_id)
                    .ok()
                    .flatten()
                    .is_some_and(|session| {
                        state.agent_status(&session) == SessionStatus::WaitingUser
                    }) =>
        {
            false
        }
        Some(turn) => match reporting::close_turn_and_report(state, session_id, turn) {
            // Talking about a turn that is already over, so nothing it says about the session
            // holds.
            TurnClose::NotThisTurn => false,
            TurnClose::NoTurn | TurnClose::Reported | TurnClose::OwesReport => true,
        },
        None => {
            // Anything that is not a turn boundary is the session still working on its turn, which
            // is what keeps a backstop from being attributed to it.
            state.touch_turn(session_id);
            true
        }
    }
}

/// Automatic reviews do not establish that a person is being asked to act.
///
/// With Codex's `approvals_reviewer` set to auto review, the permission hook fires, Codex resolves
/// the request itself. Visible action prompts override this hook status independently.
fn suppress_auto_review_hook(
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

// -- /git-ref/:session -------------------------------------------------------

/// A session's git reporting the branches it created (`octoboardd git-ref`). Recorded on the
/// blocking pool, since it reads the repository's files and writes the store; the reply carries
/// nothing, and the process that posted it exits whatever it is.
async fn git_ref_callback(
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let Ok(report) = serde_json::from_slice::<crate::branch_origin::RefReport>(&body) else {
        return axum::http::StatusCode::BAD_REQUEST;
    };
    tokio::task::spawn_blocking(move || {
        crate::branch_origin::ref_created(&state, &session_id, &report);
    });
    axum::http::StatusCode::OK
}

// -- /context-window/:session -----------------------------------------------

/// A Claude Code session's status line reporting the session's context window (`octoboardd
/// status-line`). Held for the record collector, which stores it with the session's context; the
/// reply carries nothing, and the reporter ignores it.
async fn context_window_callback(
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
    let mut answer =
        match crate::mcp::exec::call(&state, &session_id, &request.tool, &arguments).await {
            Ok(result) => serde_json::json!({ "ok": true, "result": result }),
            // The whole context chain, because this text is the only account the model gets of
            // why its call did not work.
            Err(err) => serde_json::json!({ "ok": false, "error": format!("{err:#}") }),
        };
    // Whether the session has a bound session's tools as it stands after the call, which its
    // binding changing while it runs can have changed: the child announces the catalogue this
    // names, and tells the agent when it differs from the one it announced.
    if let Ok(session) = state.session_record(&session_id) {
        answer["bound"] = serde_json::json!(session.has_bound_role());
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
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;

    use super::*;
    use crate::outbox::Drain;
    use crate::state::WriteHold;
    use crate::test_support::{app_state, PATIENCE};

    /// A sink that records what it is sent, or — `stalled` — never takes anything.
    struct RecordingSink {
        sent: Arc<std::sync::Mutex<Vec<String>>>,
        stalled: bool,
    }

    impl futures_util::Sink<Message> for RecordingSink {
        type Error = axum::Error;

        fn poll_ready(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            if self.stalled {
                std::task::Poll::Pending
            } else {
                std::task::Poll::Ready(Ok(()))
            }
        }

        fn start_send(self: std::pin::Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            if let Message::Text(text) = item {
                self.sent.lock().unwrap().push(text);
            }
            Ok(())
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn the_writer_sends_a_waiting_control_event_before_waiting_browse_replies() {
        let (control_tx, control) = mpsc::channel(8);
        let (content_tx, content) = mpsc::channel(8);
        for n in 0..3 {
            content_tx
                .send(Frame::standalone(&format!("reply-{n}")))
                .await
                .unwrap();
        }
        control_tx
            .send(Event::Ack {
                id: Some("control".into()),
            })
            .await
            .unwrap();
        drop((control_tx, content_tx));
        let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = RecordingSink {
            sent: sent.clone(),
            stalled: false,
        };
        write_frames(sink, control, content, PATIENCE).await;
        let sent = sent.lock().unwrap();
        assert!(sent[0].contains("\"control\""), "{sent:?}");
    }

    #[tokio::test]
    async fn the_writer_gives_up_on_a_client_that_stops_reading() {
        let (control_tx, control) = mpsc::channel::<Event>(8);
        let (_content_tx, content) = mpsc::channel::<Frame>(8);
        control_tx.send(Event::Ack { id: None }).await.unwrap();
        let sink = RecordingSink {
            sent: Default::default(),
            stalled: true,
        };
        let deadline = std::time::Duration::from_millis(50);
        tokio::time::timeout(PATIENCE, write_frames(sink, control, content, deadline))
            .await
            .expect("the writer ends at its deadline while the control queue is still open");
    }

    /// Serves the real router on an ephemeral loopback port and returns the port.
    async fn serve(name: &str) -> u16 {
        serve_with_output_dir(name).await.0
    }

    /// `serve`, also handing back the directory the daemon keeps saved session output in.
    async fn serve_with_output_dir(name: &str) -> (u16, std::path::PathBuf) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        let (state, dir) = app_state(&format!("server-{name}"));
        let output_dir = dir.join("output");
        tokio::spawn(async move {
            let _dir = dir;
            axum::serve(listener, router(state)).await
        });
        (port, output_dir)
    }

    /// Opens a WebSocket on `path` and reads every frame the daemon sends until its close, as
    /// `(opcode, payload)`. Frames from a server are never masked.
    fn websocket_frames(port: u16, path: &str) -> Vec<(u8, Vec<u8>)> {
        use std::io::Read;

        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: Upgrade\r\n\
             Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
        )
        .expect("write");
        stream
            .set_read_timeout(Some(PATIENCE))
            .expect("a read timeout");
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).expect("read");
        assert!(line.contains(" 101 "), "{line}");
        while line != "\r\n" {
            line.clear();
            reader.read_line(&mut line).expect("read");
        }
        let mut frames = Vec::new();
        loop {
            let mut head = [0u8; 2];
            reader.read_exact(&mut head).expect("a frame");
            let len = match head[1] & 0x7f {
                126 => {
                    let mut len = [0u8; 2];
                    reader.read_exact(&mut len).expect("a length");
                    u16::from_be_bytes(len) as usize
                }
                127 => {
                    let mut len = [0u8; 8];
                    reader.read_exact(&mut len).expect("a length");
                    u64::from_be_bytes(len) as usize
                }
                len => len as usize,
            };
            let mut payload = vec![0u8; len];
            reader.read_exact(&mut payload).expect("a payload");
            let opcode = head[0] & 0x0f;
            frames.push((opcode, payload));
            if opcode == 0x8 {
                return frames;
            }
        }
    }

    /// A session with no process replays what its last one printed and closes, and one with
    /// nothing kept closes straight away, as before there was anything to keep.
    #[tokio::test]
    async fn the_terminal_of_a_session_with_no_process_replays_its_saved_output_and_closes() {
        let (port, output_dir) = serve_with_output_dir("saved-output").await;
        std::fs::create_dir_all(&output_dir).unwrap();
        std::fs::write(output_dir.join("kept"), b"\x1b[1mlast words\x1b[0m").unwrap();

        tokio::task::spawn_blocking(move || {
            assert_eq!(
                websocket_frames(port, "/ws/term/kept"),
                [
                    (0x2, b"\x1b[1mlast words\x1b[0m".to_vec()),
                    (0x8, Vec::new())
                ]
            );
            assert_eq!(
                websocket_frames(port, "/ws/term/nothing-kept"),
                [(0x8, Vec::new())]
            );
        })
        .await
        .expect("the checks ran");
    }

    #[tokio::test]
    async fn a_terminating_session_replays_its_frozen_screen_without_shutdown_output() {
        let (state, dir) = app_state("server-frozen-output");
        let live = crate::test_support::fake_live_session(
            "frozen",
            crate::protocol::Agent::Grok,
            80,
            24,
            "sleep 30",
        );
        live.on_output(b"\x1b[?1049h\x1b[Hbefore archive");
        live.freeze_output();
        let expected = live.frozen_output().unwrap();
        live.on_output(b"\x1b[?1049l\x1b[2J\x1b[Hshutdown cleanup");
        state.register_live(live.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move { axum::serve(listener, router(state)).await });

        tokio::task::spawn_blocking(move || {
            assert_eq!(
                websocket_frames(port, "/ws/term/frozen"),
                [(0x2, expected), (0x8, Vec::new())]
            );
        })
        .await
        .unwrap();
        server.abort();
        let _ = server.await;
        drop((live, dir));
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
                "POST /git-ref/s",
                "POST /context-window/s",
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
                    "POST /hook/s HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{}\
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
                crate::loopback::post(port, &format!("/context-window/{session}"), body, PATIENCE)
                    .expect("answered")
            };
            assert!(post("s", br#"{"window":200000}"#).is_empty());
            for body in [&b"{}"[..], br#"{"window":0}"#, b"200000"] {
                post("t", body);
            }
            let host = format!("Host: 127.0.0.1:{port}");
            assert_eq!(status_of(port, "POST /context-window/t", &[&host]), 400);
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

    /// A completion notification trims the list once the session is working: trimmed while it is
    /// still idle, the record it publishes would show a plain idle between the finished work and
    /// the turn it started.
    #[tokio::test]
    async fn a_completion_notification_does_not_publish_a_plain_idle_before_the_turn_starts() {
        let (state, dir) = app_state("server-notification-order");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Idle,
            ))
            .unwrap();
        let stop =
            br#"{"hook_event_name": "Stop", "background_tasks": [{"id": "b1", "type": "shell"}]}"#;
        apply_hook(&state, "s", stop);
        assert_eq!(state.background_tasks_of("s").len(), 1);
        let mut events = state.subscribe();

        let notified = br#"{"hook_event_name": "UserPromptSubmit", "prompt": "<task-notification><task-id>b1</task-id></task-notification>"}"#;
        apply_hook(&state, "s", notified);

        let mut published = Vec::new();
        while let Ok(Event::SessionUpserted { session }) = events.try_recv() {
            published.push((session.status, session.background_tasks.len()));
        }
        assert!(
            !published.contains(&(SessionStatus::Idle, 0)),
            "{published:?}"
        );
        assert_eq!(published.last(), Some(&(SessionStatus::Working, 0)));
        assert!(state.background_tasks_of("s").is_empty());
    }

    /// A Grok session that stopped on an error holds what is queued for it, and a prompt sent ends
    /// the hold; the other agents' failures hold nothing.
    #[tokio::test]
    async fn only_a_grok_error_holds_writes_and_a_prompt_ends_the_hold() {
        for (agent, error, held) in [
            (Agent::Grok, r#"{"hookEventName": "StopFailure"}"#, true),
            (
                Agent::Grok,
                r#"{"hook_event_name": "Notification", "notification_type": "agent_error"}"#,
                true,
            ),
            (
                Agent::Grok,
                r#"{"hook_event_name": "Notification", "notification_type": "idle_prompt"}"#,
                false,
            ),
            (
                Agent::Claude,
                r#"{"hook_event_name": "StopFailure"}"#,
                false,
            ),
            (Agent::Codex, r#"{"hook_event_name": "StopFailure"}"#, false),
        ] {
            let (state, dir) = app_state("server-agent-error");
            state
                .store
                .insert_console(&crate::test_support::console(&dir))
                .unwrap();
            state
                .store
                .insert_session(&crate::test_support::console_session(
                    "s",
                    SessionStatus::Idle,
                ))
                .unwrap();
            let live = crate::test_support::fake_live_session("s", agent, 80, 24, "sleep 30");
            state.register_live(live.clone());

            apply_hook(&state, "s", error.as_bytes());
            let hold = state.write_hold("s");
            assert_eq!(
                hold,
                held.then_some(WriteHold::AgentError),
                "{agent:?} {error}"
            );
            // The error alone raises no hand; the session stays idle.
            assert_eq!(
                state.agent_status(&state.session_record("s").unwrap()),
                SessionStatus::Idle
            );
            if !held {
                continue;
            }

            state.queue_message("s", "go on");
            assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Pending);
            assert_eq!(state.queued_count("s"), 1);

            apply_hook(&state, "s", br#"{"hook_event_name": "UserPromptSubmit"}"#);
            assert_eq!(state.write_hold("s"), None);
            let deadline = std::time::Instant::now() + PATIENCE;
            while state.queued_count("s") != 0 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the prompt must release the queue"
                );
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    }

    /// An error that leaves no dialog on screen holds the queue only briefly: once the hold has run
    /// out, the queue goes in with no prompt sent.
    #[tokio::test]
    async fn an_agent_error_hold_runs_out_and_releases_the_queue() {
        let (state, dir) = app_state("server-agent-error-settle");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Idle,
            ))
            .unwrap();
        let live = crate::test_support::fake_live_session("s", Agent::Grok, 80, 24, "sleep 30");
        state.register_live(live.clone());

        apply_hook(&state, "s", br#"{"hookEventName": "StopFailure"}"#);
        state.queue_message("s", "go on");
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Pending);

        let deadline = std::time::Instant::now() + crate::session::AGENT_ERROR_SETTLE + PATIENCE;
        while state.queued_count("s") != 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "the queue must go in once the hold has run out"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(state.write_hold("s"), None);
    }

    /// A Claude Code session at `status` with a live stand-in, and the path its transcript is at,
    /// for the interrupt-watch tests below. The stand-in is returned to keep it alive.
    fn claude_session_with_transcript(
        name: &str,
        status: SessionStatus,
    ) -> (
        Arc<AppState>,
        crate::test_support::ScratchDir,
        crate::test_support::StandIn,
        std::path::PathBuf,
    ) {
        let (state, dir) = app_state(name);
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session("s", status))
            .unwrap();
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let path = dir.join("transcript.jsonl");
        std::fs::write(&path, "").unwrap();
        (state, dir, live, path)
    }

    fn claude_hook(event: &str, transcript: &std::path::Path) -> Vec<u8> {
        serde_json::json!({
            "hook_event_name": event,
            "prompt": "go",
            "tool_name": "Bash",
            "transcript_path": transcript,
        })
        .to_string()
        .into_bytes()
    }

    /// Whether the session reaches `status` within `timeout`.
    async fn reaches_within(
        state: &Arc<AppState>,
        status: SessionStatus,
        timeout: std::time::Duration,
    ) -> bool {
        let state = state.clone();
        crate::test_support::within(timeout, move || {
            state.store.get_session("s").unwrap().unwrap().status == status
        })
        .await
    }

    async fn reaches(state: &Arc<AppState>, status: SessionStatus) -> bool {
        reaches_within(state, status, PATIENCE).await
    }

    /// An idle Claude Code session whose outbox checks that what it writes is sent, with its output
    /// read so a test can see what reached it.
    fn checked_claude_session(
        name: &str,
    ) -> (
        Arc<AppState>,
        crate::test_support::ScratchDir,
        crate::test_support::StandIn,
        std::path::PathBuf,
    ) {
        let (state, dir, live, path) = claude_session_with_transcript(name, SessionStatus::Idle);
        crate::session::spawn_reader_thread((*live).clone(), 8 * 1024);
        *state.send_wait.lock().unwrap() = Some(std::time::Duration::from_millis(300));
        (state, dir, live, path)
    }

    /// [`checked_claude_session`] with room for a slow machine to read the write and apply the
    /// hook: a wait that succeeds ends as soon as the hook lands, so a long one costs nothing.
    fn patient_claude_session(
        name: &str,
    ) -> (
        Arc<AppState>,
        crate::test_support::ScratchDir,
        crate::test_support::StandIn,
        std::path::PathBuf,
    ) {
        let session = checked_claude_session(name);
        *session.0.send_wait.lock().unwrap() = Some(PATIENCE);
        session
    }

    /// A message the agent does not send is reported unsent to its sender and announced to the
    /// user, and one sent after it waits — unwritten — until the agent reports a prompt sent.
    #[tokio::test]
    async fn a_message_left_unsent_holds_what_follows_until_a_prompt_is_sent() {
        let (state, _dir, live, path) = checked_claude_session("server-unsent");
        let mut events = state.subscribe();

        let first = crate::reporting::write_message(
            &state,
            "s",
            "first message",
            crate::reporting::WhenBlocked::Queue,
        )
        .unwrap();
        assert_eq!(first, crate::reporting::Delivery::Unsent);
        let mut announced = false;
        while let Ok(event) = events.try_recv() {
            announced |= matches!(event, Event::SessionNotice { ref session, ref code, .. }
                if session == "s" && code == crate::protocol::notice_code::MESSAGE_NOT_SENT);
        }
        assert!(announced);

        let second = crate::reporting::write_message(
            &state,
            "s",
            "second message",
            crate::reporting::WhenBlocked::Queue,
        )
        .unwrap();
        assert_eq!(second, crate::reporting::Delivery::Queued);
        assert_eq!(
            live.input_writes(),
            1,
            "nothing is written behind the unsent message"
        );

        apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
        let heard = crate::test_support::output_containing(&live, "second message");
        assert!(heard.contains("second message"), "{heard}");
    }

    /// A background task's completion arrives as a prompt the agent sends itself, which leaves
    /// whatever waits in its input box there, so it does not lift the hold.
    #[tokio::test]
    async fn a_task_notification_does_not_lift_the_hold() {
        let (state, _dir, live, path) = checked_claude_session("server-unsent-notification");
        let first = crate::reporting::write_message(
            &state,
            "s",
            "first message",
            crate::reporting::WhenBlocked::Queue,
        )
        .unwrap();
        assert_eq!(first, crate::reporting::Delivery::Unsent);

        let notification = serde_json::json!({
            "hook_event_name": "UserPromptSubmit",
            "prompt": "<task-notification><task-id>b1</task-id></task-notification>",
            "transcript_path": path,
        });
        apply_hook(&state, "s", notification.to_string().as_bytes());
        let second = crate::reporting::write_message(
            &state,
            "s",
            "second message",
            crate::reporting::WhenBlocked::Queue,
        )
        .unwrap();
        assert_eq!(second, crate::reporting::Delivery::Queued);
        assert_eq!(live.input_writes(), 1);
    }

    /// A message the agent reports sending in time is delivered, as before.
    #[tokio::test]
    async fn a_message_the_agent_sends_is_delivered() {
        let (state, _dir, live, path) = patient_claude_session("server-sent");
        let hook_state = state.clone();
        let runtime = tokio::runtime::Handle::current();
        let sender = std::thread::spawn(move || {
            let _runtime = runtime.enter();
            crate::test_support::output_containing(&live, "a message");
            apply_hook(&hook_state, "s", &claude_hook("UserPromptSubmit", &path));
            live
        });
        let delivery = crate::reporting::write_message(
            &state,
            "s",
            "a message",
            crate::reporting::WhenBlocked::Queue,
        )
        .unwrap();
        let _live = sender.join().unwrap();
        assert_eq!(delivery, crate::reporting::Delivery::Written);
    }

    /// A write that lands mid-turn is queued by the agent and sent only when its turn ends, so it
    /// is not waited on.
    #[tokio::test]
    async fn a_message_written_mid_turn_is_not_waited_on() {
        let (state, _dir, _live, _path) = checked_claude_session("server-mid-turn");
        state
            .apply_hook_status("s", SessionStatus::Working)
            .unwrap();
        let delivery = crate::reporting::write_message(
            &state,
            "s",
            "a message",
            crate::reporting::WhenBlocked::Queue,
        )
        .unwrap();
        assert_eq!(delivery, crate::reporting::Delivery::Written);
    }

    /// A Claude Code turn the user interrupts fires no hook; its prompt arms the transcript watch
    /// that puts the session back at its prompt.
    #[tokio::test]
    async fn an_interrupted_claude_turn_goes_back_to_its_prompt() {
        let (state, _dir, _live, path) =
            claude_session_with_transcript("server-interrupt", SessionStatus::Idle);
        apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
        apply_hook(&state, "s", &claude_hook("PreToolUse", &path));

        std::fs::write(&path, crate::test_support::fresh_interrupt()).unwrap();

        assert!(reaches(&state, SessionStatus::Idle).await);
    }

    /// The hand coming down on an answered prompt is a move back into work, which arms the watch
    /// again: the one armed for the raised hand was looking for a decline.
    #[tokio::test]
    async fn a_claude_turn_interrupted_after_an_answered_prompt_goes_back_to_its_prompt() {
        let (state, _dir, _live, path) =
            claude_session_with_transcript("server-interrupt-after-hand", SessionStatus::Working);
        apply_hook(&state, "s", &claude_hook("PermissionRequest", &path));
        apply_hook(&state, "s", &claude_hook("PostToolUse", &path));

        std::fs::write(&path, crate::test_support::fresh_interrupt()).unwrap();

        assert!(reaches(&state, SessionStatus::Idle).await);
    }

    /// An interrupt the watch has not read yet when the next prompt arrives belongs to the turn
    /// before, and must not end the new one; the new turn's own interrupt still does, though the
    /// session never left `Working` between the two.
    #[tokio::test]
    async fn only_the_next_turns_own_interrupt_ends_it() {
        let (state, _dir, _live, path) =
            claude_session_with_transcript("server-interrupt-next-turn", SessionStatus::Idle);
        apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
        std::fs::write(&path, crate::test_support::fresh_interrupt()).unwrap();
        // Well inside the first watch's first poll, a second apart.
        apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));

        assert!(
            !reaches_within(&state, SessionStatus::Idle, crate::transcript::TEST_TIMEOUT).await,
            "the next turn must keep working"
        );

        std::fs::write(&path, crate::test_support::fresh_interrupt().repeat(2)).unwrap();
        assert!(reaches(&state, SessionStatus::Idle).await);
    }

    /// A helper agent's tool events carry an `agent_id` and no `agent_type`: they are not the
    /// session's work, so an idle session stays idle.
    #[tokio::test]
    async fn a_helper_agents_tool_events_leave_an_idle_session_idle() {
        let (state, dir) = app_state("server-helper-agent");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Idle,
            ))
            .unwrap();
        for event in ["PreToolUse", "PostToolUse", "PostToolBatch"] {
            let body = format!(
                r#"{{"hook_event_name": "{event}", "agent_id": "h1", "tool_name": "Read"}}"#
            );
            apply_hook(&state, "s", body.as_bytes());
        }
        let session = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(session.status, SessionStatus::Idle);
    }

    /// Subagent events keep the task list while the session stays idle; the handoff between two
    /// subagents never publishes it empty, and the end of the last one does once it has stood.
    #[tokio::test(start_paused = true)]
    async fn subagent_events_keep_the_tasks_of_an_idle_session() {
        let (state, dir) = app_state("server-subagent-tasks");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Idle,
            ))
            .unwrap();
        let mut events = state.subscribe();
        let hook = |body: &str| apply_hook(&state, "s", body.as_bytes());
        let published = |events: &mut tokio::sync::broadcast::Receiver<Event>| {
            let mut out = Vec::new();
            while let Ok(Event::SessionUpserted { session }) = events.try_recv() {
                out.push((session.status, session.background_tasks.len()));
            }
            out
        };

        hook(
            r#"{"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "general-purpose"}"#,
        );
        hook(
            r#"{"hook_event_name": "SubagentStart", "agent_id": "b", "agent_type": "general-purpose"}"#,
        );
        assert_eq!(
            published(&mut events),
            [(SessionStatus::Idle, 1), (SessionStatus::Idle, 2)]
        );
        // The helper agent is not work, and its stop says nothing.
        hook(r#"{"hook_event_name": "SubagentStop", "agent_id": "h", "background_tasks": []}"#);
        let stop_a = r#"{"hook_event_name": "SubagentStop", "agent_id": "a", "agent_type": "general-purpose", "background_tasks": [{"id": "a", "type": "subagent", "description": "probe A"}, {"id": "b", "type": "subagent", "description": "probe B"}]}"#;
        hook(stop_a);
        assert_eq!(published(&mut events), [(SessionStatus::Idle, 1)]);

        // b stops, a starts again in the same instant: no empty list in between.
        hook(
            r#"{"hook_event_name": "SubagentStop", "agent_id": "b", "agent_type": "general-purpose", "background_tasks": [{"id": "b", "type": "subagent"}]}"#,
        );
        hook(
            r#"{"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "general-purpose"}"#,
        );
        tokio::time::sleep(crate::state::EMPTY_TASKS_DELAY * 2).await;
        let seen = published(&mut events);
        assert!(!seen.contains(&(SessionStatus::Idle, 0)), "{seen:?}");
        let tasks = state.background_tasks_of("s");
        // b had stopped with nothing else running, so only a is left.
        assert_eq!(tasks.len(), 1);
        // a was named by an earlier stop, and comes back with its description.
        let a = tasks.iter().find(|task| task.id == "a").unwrap();
        assert_eq!(a.description.as_deref(), Some("probe A"));

        // A `Stop` within the wait takes the emptying back: its own list is the truth.
        hook(
            r#"{"hook_event_name": "SubagentStop", "agent_id": "a", "agent_type": "general-purpose", "background_tasks": [{"id": "a", "type": "subagent"}]}"#,
        );
        hook(r#"{"hook_event_name": "Stop", "background_tasks": [{"id": "S", "type": "shell"}]}"#);
        tokio::time::sleep(crate::state::EMPTY_TASKS_DELAY * 2).await;
        let tasks = state.background_tasks_of("s");
        assert_eq!(
            tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["S"]
        );

        // The last subagent stops with nothing else running: the list is emptied once the stop has
        // stood.
        hook(r#"{"hook_event_name": "Stop", "background_tasks": []}"#);
        hook(
            r#"{"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "general-purpose"}"#,
        );
        hook(
            r#"{"hook_event_name": "SubagentStop", "agent_id": "a", "agent_type": "general-purpose", "background_tasks": [{"id": "a", "type": "subagent"}]}"#,
        );
        assert!(!state.background_tasks_of("s").is_empty());
        tokio::time::sleep(crate::state::EMPTY_TASKS_DELAY * 2).await;
        assert!(state.background_tasks_of("s").is_empty());
        assert_eq!(
            published(&mut events).last(),
            Some(&(SessionStatus::Idle, 0))
        );
        assert_eq!(
            state.store.get_session("s").unwrap().unwrap().status,
            SessionStatus::Idle
        );
    }

    /// Inside a turn a subagent's start and stop leave the list alone: the turn's own `Stop` lists
    /// everything, and a turn cancelled without one would leave a foreground subagent behind.
    #[tokio::test(start_paused = true)]
    async fn subagent_events_inside_a_turn_leave_the_tasks_alone() {
        let (state, dir) = app_state("server-subagent-in-turn");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Working,
            ))
            .unwrap();
        apply_hook(
            &state,
            "s",
            br#"{"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "general-purpose"}"#,
        );
        assert!(state.background_tasks_of("s").is_empty());
        apply_hook(
            &state,
            "s",
            br#"{"hook_event_name": "SubagentStop", "agent_id": "a", "agent_type": "general-purpose", "background_tasks": [{"id": "a", "type": "subagent"}, {"id": "b", "type": "shell"}]}"#,
        );
        assert!(state.background_tasks_of("s").is_empty());
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

    /// A client drives a shell terminal end to end over the real sockets: it creates one over the
    /// control socket, types into it and resizes it over the terminal socket, finds its scrollback
    /// on attaching again, and closes it, which lets the terminal socket go.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_shell_terminal_is_driven_over_the_control_and_terminal_sockets() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-shell-terminal");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        let project = crate::test_support::ScratchDir::new("server-shell-terminal-project");
        crate::test_support::insert_project(&state, &project);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = state.clone();
        let server = tokio::spawn(async move { axum::serve(listener, router(served)).await });

        let (control, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws/control"))
                .await
                .unwrap();
        let (mut requests, mut events) = control.split();
        type Socket = tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >;
        async fn next_event(
            events: &mut futures_util::stream::SplitStream<Socket>,
        ) -> serde_json::Value {
            let frame = tokio::time::timeout(PATIENCE, events.next())
                .await
                .expect("an event in time")
                .unwrap()
                .unwrap();
            let Frame::Text(text) = frame else {
                panic!("a control frame that is not text: {frame:?}");
            };
            serde_json::from_str(&text).unwrap()
        }
        let snapshot = next_event(&mut events).await;
        assert_eq!(snapshot["type"], "snapshot");
        assert_eq!(snapshot["shell_terminals"], serde_json::json!([]));

        let create = r#"{"type":"create_shell_terminal","id":"create","project":"p1"}"#;
        requests.send(Frame::Text(create.into())).await.unwrap();
        let terminal = loop {
            let event = next_event(&mut events).await;
            if event["type"] == "shell_terminal_created" && event["id"] == "create" {
                break event["shell_terminal"]["id"].as_str().unwrap().to_string();
            }
        };

        let term_url = format!("ws://127.0.0.1:{port}/ws/term/{terminal}");
        let (mut term, _) = tokio_tungstenite::connect_async(&term_url).await.unwrap();
        async fn read_until(term: &mut Socket, needle: &str) -> String {
            let mut seen = String::new();
            while !seen.contains(needle) {
                let frame = tokio::time::timeout(PATIENCE, term.next())
                    .await
                    .unwrap_or_else(|_| panic!("{needle:?} never arrived in {seen:?}"));
                match frame {
                    Some(Ok(Frame::Binary(bytes))) => {
                        seen.push_str(&String::from_utf8_lossy(&bytes))
                    }
                    other => panic!(
                        "the terminal socket ended before {needle:?}: {other:?} after {seen:?}"
                    ),
                }
            }
            seen
        }
        term.send(Frame::Binary(b"echo $((6*7))\r".to_vec()))
            .await
            .unwrap();
        read_until(&mut term, "42").await;
        term.send(Frame::Text(
            r#"{"type":"resize","cols":100,"rows":40}"#.into(),
        ))
        .await
        .unwrap();
        term.send(Frame::Binary(b"stty size\r".to_vec()))
            .await
            .unwrap();
        read_until(&mut term, "40 100").await;
        drop(term);

        let (mut again, _) = tokio_tungstenite::connect_async(&term_url).await.unwrap();
        let replay = read_until(&mut again, "40 100").await;
        assert!(replay.contains("42"), "{replay}");

        let close = format!(
            r#"{{"type":"close_shell_terminal","id":"close","shell_terminal":"{terminal}"}}"#
        );
        requests.send(Frame::Text(close)).await.unwrap();
        let mut deleted = false;
        loop {
            let event = next_event(&mut events).await;
            deleted |=
                event["type"] == "shell_terminal_deleted" && event["shell_terminal"] == terminal;
            if event["id"] == "close" {
                assert_eq!(event["type"], "ack", "{event}");
                break;
            }
        }
        assert!(deleted, "the deletion was not broadcast before the reply");
        let ended = tokio::time::timeout(PATIENCE, async {
            while let Some(Ok(frame)) = again.next().await {
                if matches!(frame, Frame::Close(_)) {
                    break;
                }
            }
        })
        .await;
        assert!(ended.is_ok(), "the terminal socket outlived its terminal");
        server.abort();
        drop(dir);
    }
}
