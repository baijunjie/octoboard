//! The control socket (`/ws/control`): the hello and the state snapshot a client opens with, the
//! broadcasts forwarded after them, the requests it sends and the one writer that puts all of them
//! on the wire.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};

use crate::browse::budget;
use crate::browse::lane::{Frame, Lane};
use crate::coordinator::Followup;
use crate::protocol::{BrowseRequest, Event, Request, BROWSE_REQUEST_TYPES};
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
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_control(socket, state, Heartbeat::STANDARD))
}

async fn handle_control(socket: WebSocket, state: Arc<AppState>, heartbeat: Heartbeat) {
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
    let reservation = budget::reservation_for(&request.body);
    lane.submit(request.id, request.slot, reservation, move |cancel| {
        crate::browse::serve(
            &state,
            id,
            request.body,
            &state
                .host_tools
                .isolated_git_per_request(request.asked_by_person),
            cancel,
        )
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
        home_dir: crate::paths::known_home_dir().map(|home| home.to_string_lossy().into_owned()),
        host: crate::platform::HostFacts::current().into(),
        keep_awake_held: state.keep_awake_held(),
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
        let served = state.clone();
        let app = axum::Router::new().route(
            "/ws/control",
            axum::routing::get(move |ws: WebSocketUpgrade| {
                let state = served.clone();
                async move { ws.on_upgrade(move |socket| handle_control(socket, state, heartbeat)) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await });
        (port, state, dir)
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
}
