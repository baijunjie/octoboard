//! The control socket (`/ws/control`): the hello and the state snapshot a client opens with, the
//! broadcasts forwarded after them, the requests it sends and the one writer that puts all of them
//! on the wire.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};

use crate::browse::budget;
use crate::browse::lane::{Frame, Lane};
use crate::coordinator::Followup;
use crate::protocol::{BrowseRequest, Event, Request, RequestBody, BROWSE_REQUEST_TYPES};
use crate::server::access::Admission;
use crate::session::pasted_image::upload::Uploads;
use crate::state::{AppState, Stamp};

/// Events queued for one control client before its writer is considered the bottleneck. Small:
/// these are state records, not terminal output, and a client this far behind is better served by
/// the fresh snapshot a lagged broadcast receiver triggers.
const OUTBOX_CAPACITY: usize = 256;

/// The control socket's heartbeat (see "The heartbeat" in `apps/daemon/PROTOCOL.md`). The daemon
/// pings every `interval` and counts any frame the client sends, a WebSocket pong included, as a
/// sign of life; a client silent for `timeout` is taken as gone. A connection can die with no close
/// on either side (a network path dropping, a machine sleeping), and without this the daemon would
/// hold such a client's queues and its place in the broadcast channel for as long as TCP takes to
/// notice, which with nothing to send is never.
#[derive(Clone, Copy, Debug)]
struct Heartbeat {
    interval: std::time::Duration,
    timeout: std::time::Duration,
}

impl Heartbeat {
    /// Three missed intervals before the timeout, so one late pong is not a lost connection. A
    /// browser answers pings itself, even while its page's timers are held back.
    const STANDARD: Heartbeat = Heartbeat {
        interval: std::time::Duration::from_secs(15),
        timeout: std::time::Duration::from_secs(45),
    };
}

pub(super) async fn control_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Extension(admission): Extension<Admission>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_control(socket, state, Heartbeat::STANDARD, admission))
}

/// Serves one control client until it goes, its heartbeat times out, or the owner key that
/// admitted it (`admission`) is rotated away, which drops it at once, writer and all: a page
/// holding a key the owner has revoked gets nothing more.
async fn handle_control(
    socket: WebSocket,
    state: Arc<AppState>,
    heartbeat: Heartbeat,
    mut admission: Admission,
) {
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

    // This connection's pasted image uploads, dropped with it, and with them their parts.
    let uploads = Arc::new(Uploads::new(state.pasted_image_uploads.clone()));

    let mut writer = tokio::spawn(write_frames(
        sink,
        outbox,
        content,
        WRITE_DEADLINE,
        heartbeat.interval,
    ));

    // Subscribed before anything is collected, so that a broadcast between collecting the state and
    // listening for changes is not lost. A repeat is harmless: the records are whole and the
    // application ignores a prompt or request it already holds.
    let broadcasts = state.subscribe();

    // The hello is unprompted and goes first, so that a client learns which protocol generation
    // it is talking to before it has to read anything of that generation. The snapshot follows:
    // it is the only way a client that just connected learns the current state. A lag recovery
    // resends the snapshot only; it is the same connection.
    let hello = Event::Hello {
        generation: crate::protocol::GENERATION,
        daemon_version: crate::protocol::DAEMON_VERSION.to_string(),
    };
    if outbound.send(hello).await.is_err() {
        return;
    }
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
    // returns as soon as the writer is gone, since that closes the queue. A client that sends
    // nothing within the heartbeat's timeout is dropped as well, writer and all: its socket is
    // presumed dead, so nothing still queued for it would arrive, and a write into it could hang
    // until the write deadline.
    let mut writer_done = false;
    loop {
        let incoming = tokio::select! {
            incoming = tokio::time::timeout(heartbeat.timeout, stream.next()) => match incoming {
                Ok(incoming) => incoming,
                Err(_) => {
                    tracing::debug!("the control client's heartbeat timed out");
                    writer.abort();
                    writer_done = true;
                    break;
                }
            },
            _ = &mut writer => {
                writer_done = true;
                break;
            }
            () = admission.revoked() => {
                tracing::info!("a control client's owner key was rotated; it is disconnected");
                writer.abort();
                writer_done = true;
                break;
            }
        };
        let Some(incoming) = incoming else {
            break;
        };
        match incoming {
            Ok(Message::Text(text)) => {
                let kind = frame_type(&text);
                match browse_request(&text, kind.as_deref()) {
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
                match upload_request(&text, kind.as_deref()) {
                    Some(UploadFrame::Serve(request)) => {
                        serve_upload(&state, &uploads, &outbound, *request).await;
                        continue;
                    }
                    // Answered as unreadable by the ordinary path below; the upload it names is
                    // discarded, as on any refusal of one of its requests.
                    Some(UploadFrame::Unreadable(upload)) => uploads.delete(&upload),
                    None => {}
                }
                let state = state.clone();
                let outbound = outbound.clone();
                let by_owner_key = admission.by_owner_key();
                tokio::spawn(async move {
                    let (reply, followup) = answered(request_id(&text), async move {
                        handle_request(&state, &text, by_owner_key).await
                    })
                    .await;
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
            // Control is text-only; binary frames and pings are not ours to interpret. They, and
            // the pongs answering the heartbeat, still count as signs of life.
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
/// written. A heartbeat ping goes out every `ping_every`, between frames, ahead of both. Ends when
/// the control queue closes, which happens once the connection is done with, when a send fails, or
/// when the socket has not taken a frame within `deadline`.
async fn write_frames<S>(
    mut sink: S,
    mut control: mpsc::Receiver<Event>,
    mut content: mpsc::Receiver<Frame>,
    deadline: std::time::Duration,
    ping_every: std::time::Duration,
) where
    S: futures_util::Sink<Message> + Unpin,
{
    let mut content_open = true;
    let mut pings = tokio::time::interval_at(tokio::time::Instant::now() + ping_every, ping_every);
    pings.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        // A browse reply's frame is held until its text has been written, so its share of the
        // retained bytes is given back only then.
        let mut written = None;
        // The ping comes first: a full control queue, from a busy daemon or a slow client, must
        // not hold it back, or a client that only answers pings would be dropped as silent. It is
        // due once an interval and tiny, so it delays a control event by little.
        let message = tokio::select! {
            biased;
            _ = pings.tick() => Message::Ping(Vec::new()),
            event = control.recv() => match event {
                Some(event) => Message::Text(
                    serde_json::to_string(&event).expect("protocol events always serialize"),
                ),
                None => break,
            },
            frame = content.recv(), if content_open => match frame {
                Some(mut frame) => {
                    let text = std::mem::take(&mut frame.text);
                    written = Some(frame);
                    Message::Text(text)
                }
                None => {
                    content_open = false;
                    continue;
                }
            },
        };
        let sent = match tokio::time::timeout(deadline, sink.send(message)).await {
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

/// The `type` of a frame that is a JSON object with a string one.
fn frame_type(text: &str) -> Option<std::borrow::Cow<'_, str>> {
    #[derive(serde::Deserialize)]
    struct Kind<'a> {
        #[serde(rename = "type", borrow)]
        kind: Option<std::borrow::Cow<'a, str>>,
    }
    serde_json::from_str::<Kind>(text).ok()?.kind
}

/// The frame as a browse request, when its `type` (`kind`) is one: `Some(Ok)` to serve,
/// `Some(Err)` with why its fields do not read. Anything else, malformed frames included, is
/// `None` and left to the ordinary request path and its own error reply.
fn browse_request(text: &str, kind: Option<&str>) -> Option<Result<BrowseRequest, String>> {
    if !BROWSE_REQUEST_TYPES.contains(&kind?) {
        return None;
    }
    Some(serde_json::from_str::<BrowseRequest>(text).map_err(|err| err.to_string()))
}

async fn submit_browse(state: &Arc<AppState>, lane: &Arc<Lane>, request: BrowseRequest) {
    let state = state.clone();
    let id = request.id.clone();
    let reservation = budget::reservation_for(&request.body);
    let part_bytes = match request.body {
        // Already one part of a file; it is never held.
        crate::protocol::BrowseBody::ReadFileRange { .. } => None,
        _ => request.part_bytes.map(budget::reply_part_bytes),
    };
    let body = match request.body {
        crate::protocol::BrowseBody::ReadReplyPart {
            reply,
            offset,
            length,
        } => {
            lane.read_part(request.id, reply, offset, length).await;
            return;
        }
        crate::protocol::BrowseBody::SearchProjectContent(search) => {
            lane.submit_stream(
                request.id,
                request.slot,
                reservation,
                move |cancel, stream| {
                    crate::browse::search_content(
                        &state,
                        search,
                        &state
                            .host_tools
                            .isolated_git_per_request(request.asked_by_person),
                        cancel,
                        stream,
                    )
                },
            )
            .await;
            return;
        }
        body => body,
    };
    lane.submit_paged(
        request.id,
        request.slot,
        part_bytes,
        reservation,
        move |cancel| {
            crate::browse::serve(
                &state,
                id,
                body,
                &state
                    .host_tools
                    .isolated_git_per_request(request.asked_by_person),
                cancel,
            )
        },
    )
    .await;
}

/// The `type`s of the requests about a pasted image upload, which the connection serves itself:
/// it holds the uploads' parts.
const UPLOAD_REQUEST_TYPES: &[&str] = &[
    "create_pasted_image_upload",
    "write_pasted_image_part",
    "delete_pasted_image_upload",
    "save_pasted_image",
];

/// What [`upload_request`] makes of a frame.
enum UploadFrame {
    /// A request this connection serves itself.
    Serve(Box<Request>),
    /// A frame of an upload request type whose fields do not read, naming this upload: it is
    /// discarded, and the frame answered by the ordinary request path.
    Unreadable(String),
}

/// The frame as a request about one of this connection's uploads, when its `type` (`kind`) is
/// one: `None` for any other frame, `save_pasted_image` with no `upload` included, which the
/// ordinary request path serves. A frame of one of these types whose fields do not read is left
/// to that path too, which answers it, with the upload it names, if any, to discard.
fn upload_request(text: &str, kind: Option<&str>) -> Option<UploadFrame> {
    /// The `upload` of a frame, read without the rest of its fields: a single-frame save's
    /// `data` is skipped, not copied.
    #[derive(serde::Deserialize)]
    struct Named {
        #[serde(default)]
        upload: Option<String>,
    }
    let kind = kind?;
    if !UPLOAD_REQUEST_TYPES.contains(&kind) {
        return None;
    }
    let upload = serde_json::from_str::<Named>(text)
        .ok()
        .and_then(|named| named.upload);
    if kind == "save_pasted_image" && upload.is_none() {
        return None;
    }
    match serde_json::from_str::<Request>(text) {
        Ok(request) => Some(UploadFrame::Serve(Box::new(request))),
        Err(_) => upload.map(UploadFrame::Unreadable),
    }
}

/// Serves a request about one of this connection's uploads, in the order the frames came, so
/// that a client may send the parts of an upload without waiting for each `ack`. Only the save
/// leaves this loop, for the file write, once it has taken the upload's bytes.
async fn serve_upload(
    state: &Arc<AppState>,
    uploads: &Arc<Uploads>,
    outbound: &mpsc::Sender<Event>,
    request: Request,
) {
    let id = request.id;
    let answer = match request.body {
        // Off this loop, since it reads the store: no part can name the upload before the client
        // has its handle from the reply, so the frames after it lose nothing by not waiting.
        RequestBody::CreatePastedImageUpload { session, size } => {
            let state = state.clone();
            let uploads = uploads.clone();
            let outbound = outbound.clone();
            tokio::spawn(async move {
                let reply = match uploads.create(&state, &session, size) {
                    Ok(upload) => Event::PastedImageUploadCreated { id, upload },
                    Err(err) => Event::from_error(id, &err),
                };
                let _ = outbound.send(reply).await;
            });
            return;
        }
        RequestBody::WritePastedImagePart {
            upload,
            offset,
            data,
        } => uploads
            .write(&upload, offset, &data)
            .map(|()| Event::Ack { id: id.clone() }),
        RequestBody::DeletePastedImageUpload { upload } => {
            uploads.delete(&upload);
            Ok(Event::Ack { id: id.clone() })
        }
        RequestBody::SavePastedImage {
            session,
            data,
            upload: Some(upload),
        } => {
            if data.is_some() {
                uploads.delete(&upload);
                Err(anyhow::anyhow!(
                    "save_pasted_image takes exactly one of data and upload"
                ))
            } else {
                match uploads.take(&upload, &session) {
                    Ok(taken) => {
                        let state = state.clone();
                        let outbound = outbound.clone();
                        tokio::spawn(async move {
                            let saved = tokio::task::spawn_blocking(move || {
                                // The bytes stay counted against the uploads' budget until
                                // written.
                                crate::session::pasted_image::save_bytes(
                                    &state,
                                    &session,
                                    &taken.bytes,
                                )
                            })
                            .await
                            .map_err(anyhow::Error::from)
                            .and_then(|saved| saved);
                            let reply = match saved {
                                Ok(path) => Event::PastedImageSaved {
                                    id,
                                    path: path.to_string_lossy().into_owned(),
                                },
                                Err(err) => Event::from_error(id, &err),
                            };
                            let _ = outbound.send(reply).await;
                        });
                        return;
                    }
                    Err(err) => Err(err),
                }
            }
        }
        _ => unreachable!("only upload requests are served here"),
    };
    let reply = answer.unwrap_or_else(|err| Event::from_error(id, &err));
    let _ = outbound.send(reply).await;
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

/// Parses and runs one request, refusing one that changes remote access over a connection the owner
/// key opened (`by_owner_key`), and returns what goes back on the asking socket, with what the
/// request still has to do once that has been sent.
async fn handle_request(
    state: &Arc<AppState>,
    text: &str,
    by_owner_key: bool,
) -> (Event, Option<Followup>) {
    let request: Request = match serde_json::from_str(text) {
        Ok(request) => request,
        Err(err) => return (Event::unreadable_request(&err.to_string()), None),
    };
    let id = request.id.clone();
    if by_owner_key && request.body.changes_remote_access() {
        let refusal = crate::coordinator::remote_access::refused_to_browser();
        return (Event::from_error(id, &refusal), None);
    }
    match crate::coordinator::handle_deferred(state, request.id, request.body).await {
        Ok((Some(event), followup)) => (event, followup),
        Ok((None, followup)) => (Event::Ack { id }, followup),
        Err(err) => (Event::from_error(id, &err), None),
    }
}

/// The `id` a request text carries, if it is a JSON object with a string one.
fn request_id(text: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Frame<'a> {
        #[serde(borrow)]
        id: Option<std::borrow::Cow<'a, str>>,
    }
    let frame = serde_json::from_str::<Frame>(text).ok()?;
    frame.id.map(std::borrow::Cow::into_owned)
}

/// What `handling` ends with, run as a task of its own so that a panic in it ends there: the
/// request `id` is then answered with an internal error, never left for the client to wait on.
async fn answered(
    id: Option<String>,
    handling: impl std::future::Future<Output = (Event, Option<Followup>)> + Send + 'static,
) -> (Event, Option<Followup>) {
    match tokio::spawn(handling).await {
        Ok(handled) => handled,
        Err(err) => {
            tracing::error!(%err, request = ?id, "a request handler stopped unexpectedly");
            let failure = anyhow::anyhow!("the daemon failed while handling the request");
            (Event::from_error(id, &failure), None)
        }
    }
}

fn snapshot(state: &Arc<AppState>) -> anyhow::Result<Event> {
    let mut consoles = state.store.list_consoles()?;
    for console in &mut consoles {
        state.field_versions.stamp_console(console, Stamp::Snapshot);
    }
    let mut projects = state.store.list_projects()?;
    for project in &mut projects {
        state.field_versions.stamp_project(project, Stamp::Snapshot);
    }
    let mut settings = state.store.get_settings()?;
    state
        .field_versions
        .stamp_settings(&mut settings, Stamp::Snapshot);
    Ok(Event::Snapshot {
        hosts: state.store.list_hosts()?,
        consoles,
        projects,
        sessions: state
            .store
            .list_sessions()?
            .into_iter()
            .map(|session| state.wire_session(session))
            .collect(),
        trusted_directories: state.store.trusted_directories()?,
        settings,
        providers: crate::protocol::registered_providers(),
        git_statuses: state.git_statuses(),
        agent_availability: state.agent_availability(),
        account_quotas: state.account_quotas.list(),
        account_logins: state.account_logins.running(),
        account_sharing: state.account_sharing_states()?,
        agent_cli_statuses: state.agent_clis.statuses(),
        daemon_update: state.daemon_update.status().map(Box::new),
        home_dir: crate::paths::known_home_dir().map(|home| home.to_string_lossy().into_owned()),
        host: crate::platform::HostFacts::current().into(),
        keep_awake_held: state.keep_awake_held(),
        shell_terminals: state.shell_terminals.list(),
        remote_access: state.remote_access.info(state),
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
        write_frames(sink, control, content, PATIENCE, PATIENCE).await;
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
        tokio::time::timeout(
            PATIENCE,
            write_frames(sink, control, content, deadline, PATIENCE),
        )
        .await
        .expect("the writer ends at its deadline while the control queue is still open");
    }

    /// A heartbeat short enough for a test to sit through.
    const QUICK: Heartbeat = Heartbeat {
        interval: std::time::Duration::from_millis(50),
        timeout: std::time::Duration::from_millis(300),
    };

    /// Serves the control socket alone, with `heartbeat`, on a loopback port; the access checks are
    /// not what these tests are about.
    async fn serve_control(
        heartbeat: Heartbeat,
    ) -> (u16, Arc<AppState>, crate::test_support::ScratchDir) {
        let (state, dir) = crate::test_support::app_state("control-heartbeat");
        let port = serve_control_of(state.clone(), heartbeat).await;
        (port, state, dir)
    }

    /// [`serve_control`] for a state the test made.
    async fn serve_control_of(state: Arc<AppState>, heartbeat: Heartbeat) -> u16 {
        let served = state.clone();
        let app = axum::Router::new().route(
            "/ws/control",
            axum::routing::get(move |ws: WebSocketUpgrade| {
                let state = served.clone();
                async move {
                    ws.on_upgrade(move |socket| {
                        handle_control(socket, state, heartbeat, Admission::default())
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await });
        port
    }

    /// Waits until `state` has `count` broadcast listeners.
    async fn wait_for_subscribers(state: &AppState, count: usize) {
        tokio::time::timeout(PATIENCE, async {
            while state.subscriber_count() != count {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{} listeners, not {count}", state.subscriber_count()));
    }

    #[tokio::test]
    async fn a_client_that_falls_silent_is_dropped_within_the_heartbeat_timeout() {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

        let (port, state, _dir) = serve_control(QUICK).await;
        let baseline = state.subscriber_count();
        // Taken before connecting, so it is earlier than the daemon starts timing the silence.
        let connected = tokio::time::Instant::now();
        // A raw client that completes the handshake and then sends nothing at all, not even the
        // pongs a WebSocket library answers pings with: its traffic is silently gone.
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream
            .write_all(
                format!(
                    "GET /ws/control HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: Upgrade\r\n\
                     Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n\
                     Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.contains(" 101 "), "{line}");
        while line != "\r\n" {
            line.clear();
            reader.read_line(&mut line).await.unwrap();
        }
        wait_for_subscribers(&state, baseline + 1).await;

        // Frames until the daemon closes the connection, by opcode.
        let mut opcodes = Vec::new();
        tokio::time::timeout(PATIENCE, async {
            loop {
                let mut head = [0u8; 2];
                if reader.read_exact(&mut head).await.is_err() {
                    return;
                }
                let len = match head[1] & 0x7f {
                    126 => reader.read_u16().await.unwrap() as usize,
                    127 => reader.read_u64().await.unwrap() as usize,
                    len => len as usize,
                };
                let mut payload = vec![0u8; len];
                reader.read_exact(&mut payload).await.unwrap();
                opcodes.push(head[0] & 0x0f);
            }
        })
        .await
        .expect("the daemon drops the silent connection");
        let elapsed = connected.elapsed();

        assert_eq!(
            opcodes[..2],
            [0x1, 0x1],
            "the hello and the snapshot first: {opcodes:?}"
        );
        assert!(
            opcodes.contains(&0x9),
            "pinged while it lasted: {opcodes:?}"
        );
        assert!(
            elapsed >= QUICK.timeout,
            "dropped after {elapsed:?}, before the timeout"
        );
        // What the connection held is given back: its place among the broadcast's listeners.
        wait_for_subscribers(&state, baseline).await;
    }

    #[tokio::test]
    async fn a_client_that_answers_pings_stays_connected_and_its_ping_is_acknowledged() {
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (port, state, _dir) = serve_control(QUICK).await;
        let baseline = state.subscriber_count();
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws/control"))
                .await
                .unwrap();
        // Reading is what makes the library answer pings; this reads, and answers, for three
        // timeouts' worth of time.
        let mut pings = 0;
        let quiet_until = tokio::time::Instant::now() + QUICK.timeout * 3;
        while let Ok(frame) = tokio::time::timeout_at(quiet_until, socket.next()).await {
            match frame.expect("the connection stays open").unwrap() {
                Frame::Ping(_) => pings += 1,
                Frame::Close(_) => panic!("closed while answering pings"),
                _ => {}
            }
        }
        assert!(pings >= 3, "{pings} pings");
        assert_eq!(state.subscriber_count(), baseline + 1);

        socket
            .send(Frame::Text(r#"{"type":"ping","id":"beat"}"#.into()))
            .await
            .unwrap();
        let reply = loop {
            match tokio::time::timeout(PATIENCE, socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
            {
                Frame::Text(text) => {
                    break serde_json::from_str::<serde_json::Value>(&text).unwrap()
                }
                _ => continue,
            }
        };
        assert_eq!(reply, serde_json::json!({"type": "ack", "id": "beat"}));
    }

    #[tokio::test]
    async fn a_request_handler_that_panics_is_answered_with_an_internal_error() {
        assert_eq!(
            request_id(r#"{"id":"r1","type":"x"}"#).as_deref(),
            Some("r1")
        );
        assert_eq!(request_id("not json"), None);
        let (reply, followup) = answered(Some("r1".to_string()), async {
            panic!("a handler bug");
        })
        .await;
        let reply = serde_json::to_value(reply).unwrap();
        assert_eq!(reply["type"], "error");
        assert_eq!(reply["id"], "r1");
        assert_eq!(reply["code"], "internal_error");
        assert!(followup.is_none());
    }

    #[tokio::test]
    async fn a_request_that_is_handled_is_answered_as_handled_whatever_happened_to_others() {
        // One request's handler panics; the one beside it, handled as usual, still has its own answer.
        let (panicked, handled) = tokio::join!(
            answered(Some("bad".to_string()), async {
                panic!("a handler bug");
            }),
            answered(Some("good".to_string()), async {
                (
                    Event::Ack {
                        id: Some("good".to_string()),
                    },
                    None,
                )
            }),
        );
        let panicked = serde_json::to_value(panicked.0).unwrap();
        assert_eq!(
            (panicked["type"].as_str(), panicked["id"].as_str()),
            (Some("error"), Some("bad"))
        );
        assert_eq!(
            serde_json::to_value(handled.0).unwrap(),
            serde_json::json!({"type": "ack", "id": "good"})
        );
    }

    #[tokio::test]
    async fn a_request_with_no_id_whose_handler_panics_is_still_answered_with_an_internal_error() {
        assert_eq!(request_id(r#"{"type":"x"}"#), None);
        assert_eq!(request_id(r#"{"id":7,"type":"x"}"#), None);
        let (reply, _) = answered(None, async {
            panic!("a handler bug");
        })
        .await;
        let reply = serde_json::to_value(reply).unwrap();
        assert_eq!(reply["type"], "error");
        assert_eq!(reply["code"], "internal_error");
        assert!(reply["id"].is_null());
    }

    #[test]
    fn the_snapshot_stamps_each_record_with_its_current_versions() {
        let (state, _dir) =
            crate::test_support::console_with_project_and_accounts("snapshot-versions");
        let stamped = |state: &Arc<AppState>| match snapshot(state).unwrap() {
            Event::Snapshot {
                consoles,
                projects,
                settings,
                ..
            } => (consoles, projects, settings),
            other => panic!("not a snapshot: {other:?}"),
        };
        let (consoles, projects, settings) = stamped(&state);
        assert!(consoles[0].versions.contains_key("launch.claude.model"));
        assert!(projects[0].versions.contains_key("git_workflow.delivery"));
        assert!(settings.versions.contains_key("auto_pull_repositories"));
        assert_eq!(settings.accounts.len(), 3);
        assert!(settings
            .accounts
            .iter()
            .all(|account| account.versions.contains_key("config_dir")));
        // Reading advances nothing: the same snapshot again carries the same versions.
        let (again, _, _) = stamped(&state);
        assert_eq!(again[0].versions, consoles[0].versions);
    }

    /// The snapshot says where remote access stands only for a daemon that follows the desktop
    /// application: the field is there as null for any other, and a state, off until the user
    /// turns it on, for that one.
    #[test]
    fn the_snapshot_carries_remote_access_only_where_the_daemon_offers_it() {
        use crate::coordinator::remote_access::{WayIn, FORWARD_PORT};
        use crate::protocol::RemoteAccessState;

        let (state, dir) = crate::test_support::app_state("snapshot-remote-access");
        let remote_access = |state: &Arc<AppState>| match snapshot(state).unwrap() {
            Event::Snapshot { remote_access, .. } => remote_access,
            other => panic!("not a snapshot: {other:?}"),
        };
        assert_eq!(remote_access(&state), None);
        let json = serde_json::to_value(snapshot(&state).unwrap()).unwrap();
        assert!(
            json.as_object().unwrap().contains_key("remote_access"),
            "the field is always there"
        );
        assert!(json["remote_access"].is_null());

        state.set_follows_application();
        state
            .remote_access
            .begin_with(&state, dir.to_path_buf(), true, WayIn::Port);
        let offered = remote_access(&state).expect("offered to the application's daemon");
        assert_eq!(
            (offered.allowed, offered.state),
            (false, RemoteAccessState::Off)
        );
        let json = serde_json::to_value(snapshot(&state).unwrap()).unwrap();
        assert_eq!(json["remote_access"]["state"], "off");
        assert_eq!(json["remote_access"]["allowed"], false);
        assert_eq!(json["remote_access"]["browser_port"], FORWARD_PORT);
        assert!(json["remote_access"]["address"].is_null());
        drop(state.remote_access.close());
    }

    type Socket = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    /// A control client that sends requests as JSON and takes the replies by request id,
    /// keeping whatever else arrives (the hello, the snapshot, broadcasts) out of the way.
    struct Client {
        socket: Socket,
        spare: Vec<serde_json::Value>,
    }

    impl Client {
        async fn connect(port: u16) -> Self {
            let (socket, _) =
                tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws/control"))
                    .await
                    .unwrap();
            Self {
                socket,
                spare: Vec::new(),
            }
        }

        async fn send(&mut self, request: serde_json::Value) {
            use tokio_tungstenite::tungstenite::Message as Wire;
            self.socket
                .send(Wire::Text(request.to_string()))
                .await
                .unwrap();
        }

        /// The reply to request `id`, however many other frames come first.
        async fn reply(&mut self, id: &str) -> serde_json::Value {
            use tokio_tungstenite::tungstenite::Message as Wire;
            if let Some(at) = self.spare.iter().position(|v| v["id"] == id) {
                return self.spare.remove(at);
            }
            loop {
                let frame = tokio::time::timeout(PATIENCE, self.socket.next())
                    .await
                    .unwrap_or_else(|_| panic!("no reply to {id}"))
                    .expect("the connection stays open")
                    .unwrap();
                if let Wire::Text(text) = frame {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if value["id"] == id {
                        return value;
                    }
                    self.spare.push(value);
                }
            }
        }

        async fn ask(&mut self, id: &str, request: serde_json::Value) -> serde_json::Value {
            self.send(request).await;
            self.reply(id).await
        }
    }

    /// A state with a running stand-in session `id` whose scratch directory is under `run`.
    struct Running {
        state: Arc<AppState>,
        id: &'static str,
        run: crate::test_support::ScratchDir,
        /// The stand-in process, which ends before the directories it lives in go.
        _live: Box<dyn std::any::Any>,
        _dir: crate::test_support::ScratchDir,
    }

    fn running_session() -> Running {
        use crate::protocol::{AgentId, SessionStatus};
        let id = "control-upload-session";
        let (state, dir) = crate::test_support::app_state("control-upload");
        crate::state::test_support::insert_record(&state, &dir, id, SessionStatus::Working);
        let run = crate::test_support::ScratchDir::new("control-upload-run");
        let scratch = run.join(id);
        std::fs::create_dir_all(&scratch).unwrap();
        let live =
            crate::test_support::fake_live_session_in(id, AgentId::CLAUDE, "sleep 30", scratch);
        state.register_live(live.clone());
        Running {
            state,
            id,
            run,
            _live: Box::new(live),
            _dir: dir,
        }
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

    /// A PNG of `size` bytes whose bytes are not all alike, so a part out of place shows.
    fn png_of(size: usize) -> Vec<u8> {
        let mut bytes = PNG.to_vec();
        bytes.extend((0..size - PNG.len()).map(|n| (n % 251) as u8));
        bytes
    }

    fn b64(bytes: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn pasted_in(run: &crate::test_support::ScratchDir, id: &str) -> std::path::PathBuf {
        run.join(id).join("pasted")
    }

    async fn created(client: &mut Client, id: &str, session: &str, size: usize) -> String {
        let reply = client
            .ask(
                id,
                serde_json::json!({"id": id, "type": "create_pasted_image_upload",
                    "session": session, "size": size}),
            )
            .await;
        assert_eq!(reply["type"], "pasted_image_upload_created", "{reply}");
        reply["upload"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn an_image_uploaded_in_parts_sent_without_waiting_for_acks_is_saved_as_the_file_of_its_bytes(
    ) {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        let bytes = png_of(3000);
        let upload = created(&mut client, "create", r.id, bytes.len()).await;

        // The parts and the save go out back to back: they are served in the order they came.
        for (n, part) in bytes.chunks(1000).enumerate() {
            client
                .send(
                    serde_json::json!({"id": format!("part-{n}"), "type": "write_pasted_image_part",
                    "upload": upload, "offset": n * 1000, "data": b64(part)}),
                )
                .await;
        }
        client
            .send(
                serde_json::json!({"id": "save", "type": "save_pasted_image",
                "session": r.id, "upload": upload}),
            )
            .await;
        for n in 0..3 {
            let ack = client.reply(&format!("part-{n}")).await;
            assert_eq!(ack["type"], "ack", "{ack}");
        }
        let saved = client.reply("save").await;
        assert_eq!(saved["type"], "pasted_image_saved", "{saved}");
        let path = std::path::PathBuf::from(saved["path"].as_str().unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(path.parent().unwrap(), pasted_in(&r.run, r.id));
        assert_eq!(path.extension().unwrap(), "png");

        // Saved, the upload is gone.
        let again = client
            .ask(
                "again",
                serde_json::json!({"id": "again", "type": "save_pasted_image",
                    "session": r.id, "upload": upload}),
            )
            .await;
        assert_eq!(again["code"], "upload_unavailable", "{again}");
        assert_eq!(again["params"]["upload"], upload.as_str());
    }

    #[tokio::test]
    async fn parts_that_do_not_add_up_are_refused_at_the_save_and_leave_no_file() {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        let bytes = png_of(3000);
        let upload = created(&mut client, "create", r.id, bytes.len()).await;
        for (n, part) in bytes.chunks(1000).take(2).enumerate() {
            let ack = client
                .ask(
                    &format!("part-{n}"),
                    serde_json::json!({"id": format!("part-{n}"), "type": "write_pasted_image_part",
                        "upload": upload, "offset": n * 1000, "data": b64(part)}),
                )
                .await;
            assert_eq!(ack["type"], "ack");
        }
        let refused = client
            .ask(
                "save",
                serde_json::json!({"id": "save", "type": "save_pasted_image",
                    "session": r.id, "upload": upload}),
            )
            .await;
        assert_eq!(refused["type"], "error", "{refused}");
        assert_eq!(refused["code"], "upload_parts_mismatch");
        assert_eq!(refused["params"]["expected_offset"], "2000");
        assert_eq!(refused["params"]["size"], "3000");
        assert!(!pasted_in(&r.run, r.id).exists());

        let again = client
            .ask(
                "again",
                serde_json::json!({"id": "again", "type": "write_pasted_image_part",
                    "upload": upload, "offset": 2000, "data": b64(&bytes[2000..])}),
            )
            .await;
        assert_eq!(
            again["code"], "upload_unavailable",
            "the refusal discarded it: {again}"
        );
    }

    #[tokio::test]
    async fn a_part_out_of_place_is_refused_and_discards_the_upload() {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        let bytes = png_of(2000);
        let upload = created(&mut client, "create", r.id, bytes.len()).await;
        let refused = client
            .ask(
                "skip",
                serde_json::json!({"id": "skip", "type": "write_pasted_image_part",
                    "upload": upload, "offset": 1000, "data": b64(&bytes[1000..])}),
            )
            .await;
        assert_eq!(refused["code"], "upload_parts_mismatch", "{refused}");
        assert_eq!(refused["params"]["expected_offset"], "0");
        let save = client
            .ask(
                "save",
                serde_json::json!({"id": "save", "type": "save_pasted_image",
                    "session": r.id, "upload": upload}),
            )
            .await;
        assert_eq!(save["code"], "upload_unavailable", "{save}");
        assert!(!pasted_in(&r.run, r.id).exists());
    }

    #[tokio::test]
    async fn a_deleted_upload_is_acknowledged_also_when_gone_and_cannot_be_saved() {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        let upload = created(&mut client, "create", r.id, 100).await;
        for id in ["delete", "delete-again"] {
            let reply = client
                .ask(
                    id,
                    serde_json::json!({"id": id, "type": "delete_pasted_image_upload",
                        "upload": upload}),
                )
                .await;
            assert_eq!(reply["type"], "ack", "{reply}");
        }
        let save = client
            .ask(
                "save",
                serde_json::json!({"id": "save", "type": "save_pasted_image",
                    "session": r.id, "upload": upload}),
            )
            .await;
        assert_eq!(save["code"], "upload_unavailable", "{save}");
    }

    #[tokio::test]
    async fn the_single_frame_save_is_still_served_and_a_save_naming_neither_or_both_is_refused() {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        let bytes = png_of(500);
        let saved = client
            .ask(
                "whole",
                serde_json::json!({"id": "whole", "type": "save_pasted_image",
                    "session": r.id, "data": b64(&bytes)}),
            )
            .await;
        assert_eq!(saved["type"], "pasted_image_saved", "{saved}");
        let path = std::path::PathBuf::from(saved["path"].as_str().unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);

        let upload = created(&mut client, "create", r.id, bytes.len()).await;
        let neither = client
            .ask(
                "neither",
                serde_json::json!({"id": "neither", "type": "save_pasted_image", "session": r.id}),
            )
            .await;
        assert_eq!(neither["type"], "error", "{neither}");
        let both = client
            .ask(
                "both",
                serde_json::json!({"id": "both", "type": "save_pasted_image",
                    "session": r.id, "data": b64(&bytes), "upload": upload}),
            )
            .await;
        assert_eq!(both["type"], "error", "{both}");
        let files = std::fs::read_dir(pasted_in(&r.run, r.id)).unwrap().count();
        assert_eq!(files, 1, "only the single-frame save wrote a file");
    }

    #[tokio::test]
    async fn a_third_upload_on_one_connection_is_refused_and_another_connection_has_its_own_two() {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        created(&mut client, "one", r.id, 10).await;
        created(&mut client, "two", r.id, 10).await;
        let third = client
            .ask(
                "three",
                serde_json::json!({"id": "three", "type": "create_pasted_image_upload",
                    "session": r.id, "size": 10}),
            )
            .await;
        assert_eq!(third["code"], "limit_exceeded", "{third}");
        assert_eq!(third["params"]["limit"], "image_uploads");
        assert_eq!(third["params"]["max"], "2");

        let mut other = Client::connect(port).await;
        created(&mut other, "other", r.id, 10).await;
    }

    #[tokio::test]
    async fn an_upload_for_an_unknown_session_is_refused_as_such() {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        let reply = client
            .ask(
                "create",
                serde_json::json!({"id": "create", "type": "create_pasted_image_upload",
                    "session": "no-such-session", "size": 10}),
            )
            .await;
        assert_eq!(reply["code"], "unknown_session", "{reply}");
    }

    #[tokio::test]
    async fn a_connection_that_closes_gives_back_the_bytes_its_uploads_held() {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let budget = r.state.pasted_image_uploads.clone();
        let full = budget.available();
        let mut client = Client::connect(port).await;
        created(&mut client, "create", r.id, 4096).await;
        assert_eq!(budget.available(), full - 4096);
        drop(client);
        let given_back = crate::test_support::within(PATIENCE, {
            let budget = budget.clone();
            move || budget.available() == full
        })
        .await;
        assert!(given_back, "the closed connection still holds its upload");
    }

    /// A project of one large text file, served over a control connection.
    struct Browsed {
        fixture: crate::browse::tests::Fixture,
        _dir: crate::test_support::ScratchDir,
    }

    fn browsed(file: &str) -> Browsed {
        let fixture = crate::browse::tests::Fixture::new();
        let dir = crate::test_support::ScratchDir::new("control-parts");
        std::fs::write(dir.join("big.txt"), file).unwrap();
        fixture.project("parts-project", &dir);
        Browsed { fixture, _dir: dir }
    }

    fn read_file(id: &str, part_bytes: Option<u64>) -> serde_json::Value {
        let mut request = serde_json::json!({"id": id, "type": "read_project_file",
            "project": "parts-project", "path": "big.txt"});
        if let Some(part_bytes) = part_bytes {
            request["part_bytes"] = part_bytes.into();
        }
        request
    }

    fn lines(count: usize) -> String {
        (0..count)
            .map(|n| format!("line {n} of the file\n"))
            .collect()
    }

    #[tokio::test]
    async fn a_browse_reply_read_in_parts_joins_to_the_reply_sent_whole() {
        use base64::Engine;
        let b = browsed(&lines(8000));
        let port = serve_control_of(b.fixture.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;

        // A part size below the floor is raised to it.
        let first = client.ask("read", read_file("read", Some(1))).await;
        assert_eq!(first["type"], "reply_part", "{first}");
        let handle = first["reply"].as_str().unwrap().to_string();
        let total = first["total"].as_u64().unwrap();
        let decode = |part: &serde_json::Value| {
            base64::engine::general_purpose::STANDARD
                .decode(part["data"].as_str().unwrap())
                .unwrap()
        };
        let mut joined = decode(&first);
        assert_eq!(joined.len(), 16 * 1024);
        assert_eq!(first["offset"], 0);
        assert!(total > 3 * 16 * 1024, "several parts: {total}");

        let mut round = 0;
        while (joined.len() as u64) < total {
            let id = format!("part-{round}");
            let part = client
                .ask(
                    &id,
                    serde_json::json!({"id": id, "type": "read_reply_part", "reply": handle,
                        "offset": joined.len()}),
                )
                .await;
            assert_eq!(part["type"], "reply_part", "{part}");
            assert_eq!(part["offset"], joined.len());
            assert_eq!(part["total"], total);
            let data = decode(&part);
            assert!(!data.is_empty());
            joined.extend(data);
            round += 1;
        }
        assert_eq!(joined.len() as u64, total);

        // The reply the same request gets with no part size, byte for byte.
        let whole = client.ask("read", read_file("read", None)).await;
        assert_eq!(whole["type"], "project_file", "{whole}");
        let reassembled: serde_json::Value = serde_json::from_slice(&joined).unwrap();
        assert_eq!(reassembled, whole);

        // Its last part released it.
        let gone = client
            .ask(
                "late",
                serde_json::json!({"id": "late", "type": "read_reply_part", "reply": handle,
                    "offset": 0}),
            )
            .await;
        assert_eq!(gone["code"], "reply_part_unavailable", "{gone}");
        assert_eq!(gone["params"]["reply"], handle.as_str());
    }

    #[tokio::test]
    async fn a_reply_that_fits_the_part_size_comes_whole_and_a_newer_read_in_its_slot_supersedes_the_held_one(
    ) {
        let b = browsed(&lines(8000));
        let port = serve_control_of(b.fixture.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        let whole = client
            .ask("small", read_file("small", Some(4 * 1024 * 1024)))
            .await;
        assert_eq!(
            whole["type"], "project_file",
            "a part size above the file's reply: {whole}"
        );

        let mut held = read_file("held", Some(1));
        held["slot"] = "viewer".into();
        let first = client.ask("held", held).await;
        assert_eq!(first["type"], "reply_part", "{first}");
        let handle = first["reply"].as_str().unwrap().to_string();
        let mut newer = read_file("newer", None);
        newer["slot"] = "viewer".into();
        let newer = client.ask("newer", newer).await;
        assert_eq!(newer["type"], "project_file", "{newer}");
        let late = client
            .ask(
                "late",
                serde_json::json!({"id": "late", "type": "read_reply_part", "reply": handle,
                    "offset": 16 * 1024}),
            )
            .await;
        assert_eq!(late["code"], "request_superseded", "{late}");
    }

    #[tokio::test]
    async fn a_part_of_a_reply_this_connection_does_not_hold_is_unavailable() {
        let b = browsed(&lines(8000));
        let port = serve_control_of(b.fixture.state.clone(), Heartbeat::STANDARD).await;
        let mut held = Client::connect(port).await;
        let first = held.ask("read", read_file("read", Some(1))).await;
        let handle = first["reply"].as_str().unwrap().to_string();
        // Another connection has no such reply, whatever the handle names on the first.
        let mut other = Client::connect(port).await;
        let reply = other
            .ask(
                "steal",
                serde_json::json!({"id": "steal", "type": "read_reply_part", "reply": handle,
                    "offset": 0}),
            )
            .await;
        assert_eq!(reply["code"], "reply_part_unavailable", "{reply}");
    }

    impl Client {
        /// The reply carrying error `code`, for a reply that has no id of its own to look for.
        async fn reply_with_code(&mut self, code: &str) -> serde_json::Value {
            use tokio_tungstenite::tungstenite::Message as Wire;
            if let Some(at) = self.spare.iter().position(|v| v["code"] == code) {
                return self.spare.remove(at);
            }
            loop {
                let frame = tokio::time::timeout(PATIENCE, self.socket.next())
                    .await
                    .unwrap_or_else(|_| panic!("no {code} reply"))
                    .expect("the connection stays open")
                    .unwrap();
                if let Wire::Text(text) = frame {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if value["code"] == code {
                        return value;
                    }
                    self.spare.push(value);
                }
            }
        }
    }

    #[tokio::test]
    async fn an_upload_request_whose_fields_do_not_read_is_unreadable_and_discards_its_upload() {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        let bytes = png_of(2000);
        let upload = created(&mut client, "create", r.id, bytes.len()).await;
        client
            .send(
                serde_json::json!({"id": "ok", "type": "write_pasted_image_part",
                "upload": upload, "offset": 0, "data": b64(&bytes[..1000])}),
            )
            .await;
        assert_eq!(client.reply("ok").await["type"], "ack");

        // A valid upload handle, an offset that is not a number.
        client
            .send(
                serde_json::json!({"id": "bad", "type": "write_pasted_image_part",
                "upload": upload, "offset": "1000", "data": b64(&bytes[1000..])}),
            )
            .await;
        let unreadable = client.reply_with_code("unreadable_request").await;
        assert_eq!(unreadable["type"], "error", "{unreadable}");

        let after = client
            .ask(
                "after",
                serde_json::json!({"id": "after", "type": "write_pasted_image_part",
                    "upload": upload, "offset": 1000, "data": b64(&bytes[1000..])}),
            )
            .await;
        assert_eq!(after["code"], "upload_unavailable", "{after}");
        assert!(!pasted_in(&r.run, r.id).exists());
    }

    #[tokio::test]
    async fn an_unreadable_upload_request_that_names_no_upload_is_only_unreadable() {
        let r = running_session();
        let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
        let mut client = Client::connect(port).await;
        let upload = created(&mut client, "create", r.id, 100).await;
        // No upload to name, and one that is not text: nothing to discard.
        client
            .send(serde_json::json!({"id": "a", "type": "write_pasted_image_part", "offset": 0}))
            .await;
        client.reply_with_code("unreadable_request").await;
        client
            .send(
                serde_json::json!({"id": "b", "type": "write_pasted_image_part",
                "upload": 5, "offset": 0, "data": ""}),
            )
            .await;
        client.reply_with_code("unreadable_request").await;
        // The other upload is untouched.
        let ack = client
            .ask(
                "write",
                serde_json::json!({"id": "write", "type": "write_pasted_image_part",
                    "upload": upload, "offset": 0, "data": b64(&png_of(100))}),
            )
            .await;
        assert_eq!(ack["type"], "ack", "{ack}");
    }
}
