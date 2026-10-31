//! The control socket (`/ws/control`): the state snapshot a client opens with, the broadcasts
//! forwarded after it, the requests it sends and the one writer that puts all of them on the wire.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};

use crate::browse::budget;
use crate::browse::lane::{Frame, Lane};
use crate::coordinator::Followup;
use crate::git::isolated::IsolatedGit;
use crate::protocol::{BrowseBody, BrowseRequest, Event, Request, BROWSE_REQUEST_TYPES};
use crate::state::AppState;

/// Events queued for one control client before its writer is considered the bottleneck. Small:
/// these are state records, not terminal output, and a client this far behind is better served by
/// the fresh snapshot a lagged broadcast receiver triggers.
const OUTBOX_CAPACITY: usize = 256;

pub(super) async fn control_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
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
        }
        Err(err) => {
            tracing::error!(%err, "building the state snapshot failed");
            return;
        }
    }

    // The snapshot cannot say that a session is waiting for the user's go-ahead or answer, and the
    // prompt or request was broadcast only once, so they are asked again. Deciding which prompts
    // to repeat looks at the file system (`trust::pending_prompts`), so it runs on the forwarder's
    // task rather than here, where it would hold up this client's first requests; the broadcasts
    // follow it there, so one that supersedes a repeated prompt still arrives after it.
    let forwarder = {
        let state = state.clone();
        let outbound = outbound.clone();
        tokio::spawn(async move {
            for question in pending_questions(&state).await {
                if outbound.send(question).await.is_err() {
                    return;
                }
            }
            forward_broadcasts(state, broadcasts, outbound).await;
        })
    };

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
        BrowseBody::ListProjectDir { .. } => budget::LISTING_RESERVATION,
        BrowseBody::SearchProjectFiles { .. } => budget::SEARCH_RESERVATION,
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
        crate::browse::serve(&state, id, request.body, &IsolatedGit::from_shell, cancel)
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
                    for question in pending_questions(&state).await {
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
async fn pending_questions(state: &AppState) -> Vec<Event> {
    let mut questions = crate::session::trust::pending_prompts(state).await;
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
        account_sharing: crate::accounts::shared_config::states(state)?,
        home_dir: crate::paths::known_home_dir().map(|home| home.to_string_lossy().into_owned()),
        shell_terminals: state.shell_terminals.list(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::PATIENCE;

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
}
