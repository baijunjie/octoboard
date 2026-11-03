//! The terminal socket (`/ws/term/:session`): the replay a client attaches with, the live output
//! after it, and the writer that puts what the client types into the process's PTY without
//! stalling the output.

use std::collections::VecDeque;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;

use crate::protocol::{error_code, CodedError, Event, TermControl, TermEvent};
use crate::session::LiveSession;
use crate::state::AppState;
use crate::terminal::fan::{Answering, Attached};
use crate::terminal::input_class::is_terminal_protocol;
use crate::terminal::pty::PtyCore;
use crate::terminal::shell::ShellProcess;

pub(super) async fn term_ws(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_term(socket, state, session_id))
}

/// What a terminal socket is attached to: an agent session's process, or a shell terminal's.
#[derive(Clone)]
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

    fn write_input(&self, data: &[u8]) -> Result<(), crate::terminal::ptyio::PartialWrite> {
        match self {
            Terminal::Session(session) => session.write_terminal_input(data),
            Terminal::Shell(shell) => shell.write(data),
        }
    }
    fn write_reply(&self, data: &[u8]) -> Result<(), crate::terminal::ptyio::PartialWrite> {
        match self {
            Terminal::Session(session) => session.write_terminal_reply(data),
            Terminal::Shell(shell) => shell.write(data),
        }
    }
}

/// Retains whole user frames when a draft transaction temporarily stops accepting keys.
/// Query replies bypass this queue while the socket is readable. Beyond the connection's byte
/// budget, socket backpressure bounds memory until the CLI accepts more input.
#[derive(Default)]
struct DeferredInput {
    frames: VecDeque<Vec<u8>>,
    bytes: usize,
    replies: VecDeque<Vec<u8>>,
    /// A user frame that was not only the terminal's own reports failed to be written, or a write
    /// task failed with its frame unknown.
    typing_lost: bool,
}

#[derive(Debug)]
enum InputFrame {
    User(Vec<u8>),
    Reply(Vec<u8>),
}

impl InputFrame {
    fn len(&self) -> usize {
        match self {
            Self::User(data) | Self::Reply(data) => data.len(),
        }
    }
}

impl DeferredInput {
    fn push(&mut self, data: Vec<u8>) {
        self.enqueue(InputFrame::User(data));
    }

    fn push_reply(&mut self, data: Vec<u8>) {
        self.enqueue(InputFrame::Reply(data));
    }

    fn enqueue(&mut self, frame: InputFrame) {
        let (data, queue) = match frame {
            InputFrame::User(data) => (data, &mut self.frames),
            InputFrame::Reply(data) => (data, &mut self.replies),
        };
        if data.is_empty() {
            return;
        }
        self.bytes += data.len();
        // Small websocket frames are consecutive bytes to the terminal. Coalescing bounds queue
        // overhead too, including replies waiting behind an atomic paste write.
        if let Some(last) = queue.back_mut() {
            if last.len().saturating_add(data.len()) <= 64 * 1024 {
                last.extend_from_slice(&data);
                return;
            }
        }
        queue.push_back(data);
    }

    fn next(&mut self, user_ready: bool) -> Option<InputFrame> {
        self.replies.pop_front().map(InputFrame::Reply).or_else(|| {
            user_ready
                .then(|| self.frames.pop_front())
                .flatten()
                .map(InputFrame::User)
        })
    }

    /// Bytes stay charged while a blocking task owns them. Only a definite zero-byte refusal
    /// returns a user frame to the queue; a partial write must never be retried as a whole.
    fn complete(
        &mut self,
        frame: InputFrame,
        result: Result<(), crate::terminal::ptyio::PartialWrite>,
    ) -> Result<bool, crate::terminal::ptyio::PartialWrite> {
        match (frame, result) {
            (InputFrame::User(data), Err(error))
                if error.written == 0 && error.error.kind() == std::io::ErrorKind::WouldBlock =>
            {
                self.frames.push_front(data);
                Ok(false)
            }
            (InputFrame::User(data), Err(error)) => {
                self.typing_lost |= !is_terminal_protocol(&data);
                Err(error)
            }
            (frame, result) => {
                self.bytes -= frame.len();
                if let Err(error) = result {
                    tracing::debug!(%error, "writing a query reply failed");
                }
                Ok(true)
            }
        }
    }

    /// A write task that failed took its frame with it, so whatever it held counts as lost.
    fn write_task_failed(
        &mut self,
        error: tokio::task::JoinError,
    ) -> crate::terminal::ptyio::PartialWrite {
        self.typing_lost = true;
        crate::terminal::ptyio::PartialWrite {
            written: 0,
            error: std::io::Error::other(format!("the terminal input task failed: {error}")),
        }
    }

    /// Whether input someone may have typed was not written. Frames made up only of the
    /// terminal's own reports (focus, mouse movement, wheel and release, query replies) hold no
    /// draft, so losing them loses nothing.
    fn lost_typing(&self) -> bool {
        self.typing_lost || self.frames.iter().any(|frame| !is_terminal_protocol(frame))
    }
}

type InputWrite = (InputFrame, Result<(), crate::terminal::ptyio::PartialWrite>);

fn start_input_write(terminal: Terminal, frame: InputFrame) -> tokio::task::JoinHandle<InputWrite> {
    tokio::task::spawn_blocking(move || {
        let result = match &frame {
            InputFrame::User(data) => terminal.write_input(data),
            InputFrame::Reply(data) => terminal.write_reply(data),
        };
        (frame, result)
    })
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
    let terminal_param = match &terminal {
        Terminal::Session(_) => "session",
        Terminal::Shell(_) => "shell_terminal",
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
    // exited answers with its replay alone, read-only, as a session without a process does. A live
    // one is first told where its replay stands, for answering the program's queries in it.
    let Attached {
        replay,
        mut live,
        answering,
    } = pty.attach();
    if let Some(answering) = &answering {
        let mark = TermEvent::Replay {
            length: replay.len(),
            offset: answering.mark.offset,
            answered: answering.mark.answered,
        };
        let text = serde_json::to_string(&mark).expect("a replay mark serializes");
        if socket.send(Message::Text(text)).await.is_err() {
            return;
        }
    }
    if !replay.is_empty() && socket.send(Message::Binary(replay)).await.is_err() {
        return;
    }
    let Some(Answering { client, .. }) = answering else {
        let _ = socket.send(Message::Close(None)).await;
        return;
    };
    let mut deferred = DeferredInput::default();
    let mut retry = tokio::time::interval(std::time::Duration::from_millis(25));
    retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut writing: Option<tokio::task::JoinHandle<InputWrite>> = None;
    let mut user_ready = true;
    let mut failure = None;
    loop {
        if writing.is_none() {
            // While the session's messages are held (`OutboxHold`: a handoff, an agent's transfer
            // of a session it owns, a sharing change), what is typed waits here too: a turn it
            // started now would run against work that is being moved. Released, it goes in as
            // usual; never written, it is reported as lost when the socket ends.
            if user_ready
                && !deferred.frames.is_empty()
                && matches!(terminal, Terminal::Session(_))
                && state.outbox.is_held(&id)
            {
                user_ready = false;
            }
            if let Some(frame) = deferred.next(user_ready) {
                writing = Some(start_input_write(terminal.clone(), frame));
            }
        }
        tokio::select! {
            incoming = socket.recv(), if deferred.bytes < 64 * 1024 * 1024 => {
                match incoming {
                    Some(Ok(Message::Binary(data))) => {
                        deferred.push(data);
                    }
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<TermControl>(&text) {
                            Ok(TermControl::Resize { cols, rows }) => {
                                if let Err(err) = pty.resize(cols, rows) {
                                    tracing::debug!(terminal = %id, %err, "resize failed");
                                }
                            }
                            Ok(TermControl::Reply { data, offset }) => {
                                if !pty.admit_reply(client, offset) {
                                    continue;
                                }
                                deferred.push_reply(data.into_bytes());
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
            _ = retry.tick(), if !user_ready && !deferred.frames.is_empty() => {
                user_ready = true;
            }
            completed = async { writing.as_mut().expect("an active input write").await }, if writing.is_some() => {
                writing = None;
                let result = match completed {
                    Ok((frame, result)) => deferred.complete(frame, result),
                    Err(error) => Err(deferred.write_task_failed(error)),
                };
                match result {
                    Ok(ready) => {
                        if !ready { user_ready = false; }
                    }
                    Err(error) => {
                        failure = Some(error.to_string());
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
    // Closing the subscriber releases a reader blocked on delivery while its last write settles.
    // A blocking PTY write cannot be cancelled after bytes may have reached the child.
    drop(live);
    if let Some(writing) = writing {
        match writing.await {
            Ok((frame, result)) => {
                if let Err(error) = deferred.complete(frame, result) {
                    failure = Some(error.to_string());
                }
            }
            Err(error) => failure = Some(deferred.write_task_failed(error).to_string()),
        }
    }
    if deferred.lost_typing() {
        let mut params = vec![(terminal_param, id.as_str())];
        let suffix = match &failure {
            Some(detail) => {
                params.push(("detail", detail.as_str()));
                format!(": {detail}")
            }
            None => String::new(),
        };
        state.broadcast(Event::from_error(
            None,
            &CodedError::raised(
                error_code::TERMINAL_INPUT_LOST,
                format!(
                    "The terminal {id} disconnected before it had taken everything typed into it{suffix}; check its input line before continuing."
                ),
                &params,
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};

    use super::*;
    use crate::protocol::AgentId;
    use crate::server::router;
    use crate::server::test_support::{serve_with_output_dir, websocket_frames};
    use crate::test_support::{app_state, PATIENCE, TEST_KEY};

    #[test]
    fn deferred_user_input_retries_in_order_without_repeating_accepted_frames() {
        let mut pending = DeferredInput::default();
        pending.push(b"first".to_vec());
        let first = pending.next(true).unwrap();
        assert_eq!(pending.bytes, 5, "in-flight input remains charged");
        pending.push(b"second".to_vec());
        pending.complete(first, Ok(())).unwrap();
        let second = pending.next(true).unwrap();
        pending
            .complete(
                second,
                Err(crate::terminal::ptyio::PartialWrite {
                    written: 0,
                    error: std::io::Error::from(std::io::ErrorKind::WouldBlock),
                }),
            )
            .unwrap();
        pending.push(b"third".to_vec());
        assert!(
            pending.next(false).is_none(),
            "refused user input waits for the retry tick"
        );
        pending.push_reply(b"reply".to_vec());
        let reply = pending.next(false).unwrap();
        assert!(matches!(&reply, InputFrame::Reply(data) if data == b"reply"));
        pending.complete(reply, Ok(())).unwrap();
        let second = pending.next(true).unwrap();
        assert!(matches!(&second, InputFrame::User(data) if data == b"secondthird"));
        pending.complete(second, Ok(())).unwrap();
        assert!(pending.frames.is_empty());
        assert_eq!(pending.bytes, 0);
    }

    #[test]
    fn deferred_small_frames_have_bounded_overhead() {
        let mut pending = DeferredInput::default();
        for _ in 0..100_000 {
            pending.push(Vec::new());
            pending.push(b"x".to_vec());
        }
        assert_eq!(pending.bytes, 100_000);
        assert_eq!(pending.frames.len(), 2);
        assert_eq!(
            pending.frames.iter().map(Vec::len).sum::<usize>(),
            pending.bytes
        );
    }

    #[test]
    fn deferred_user_input_stops_on_partial_failure() {
        let mut pending = DeferredInput::default();
        pending.push(b"first".to_vec());
        let first = pending.next(true).unwrap();
        pending.push(b"second".to_vec());
        let error = pending
            .complete(
                first,
                Err(crate::terminal::ptyio::PartialWrite {
                    written: 2,
                    error: std::io::Error::from(std::io::ErrorKind::BrokenPipe),
                }),
            )
            .unwrap_err();
        assert_eq!(error.written, 2);
        assert_eq!(pending.frames.len(), 1);
        assert_eq!(
            pending.frames.front().unwrap(),
            b"second",
            "the partially accepted first frame is never put back"
        );
    }

    #[test]
    fn deferred_terminal_reports_are_not_lost_typing() {
        let refused = || {
            Err(crate::terminal::ptyio::PartialWrite {
                written: 0,
                error: std::io::Error::from(std::io::ErrorKind::BrokenPipe),
            })
        };
        let mut pending = DeferredInput::default();
        pending.push(b"\x1b[O".to_vec());
        let focus = pending.next(true).unwrap();
        pending.complete(focus, refused()).unwrap_err();
        pending.push(b"\x1b[<35;10;4M".to_vec());
        assert!(
            !pending.lost_typing(),
            "a refused focus report and a queued mouse movement hold no draft"
        );

        pending.push(b"x".to_vec());
        assert!(pending.lost_typing(), "a queued key is typing");

        let mut pending = DeferredInput::default();
        pending.push(b"x".to_vec());
        let key = pending.next(true).unwrap();
        pending.complete(key, refused()).unwrap_err();
        assert!(pending.lost_typing(), "a refused key is typing");
    }

    #[tokio::test]
    async fn a_terminal_write_keeps_draining_echo_while_the_input_is_in_flight() {
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-input-echo");
        let live = crate::test_support::fake_live_session(
            "echo",
            AgentId::CLAUDE,
            80,
            24,
            "stty raw -echo; printf READY; cat",
        );
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = state.clone();
        let server = tokio::spawn(async move { axum::serve(listener, router(served)).await });
        let (mut socket, _) = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{port}/ws/term/echo?key={TEST_KEY}"
        ))
        .await
        .unwrap();
        let mut ready = Vec::new();
        while !ready.windows(5).any(|text| text == b"READY") {
            if let Some(Ok(Frame::Binary(data))) =
                tokio::time::timeout(PATIENCE, socket.next()).await.unwrap()
            {
                ready.extend_from_slice(&data);
            }
        }
        // One frame exceeds both the PTY and subscriber queues, so writing it synchronously
        // prevents the echo from draining and times out even though the child is healthy.
        let payload = vec![b'x'; 4 * 1024 * 1024];
        socket.send(Frame::Binary(payload.clone())).await.unwrap();
        let echoed = tokio::time::timeout(PATIENCE, async {
            let mut echoed = Vec::with_capacity(payload.len());
            while echoed.len() < payload.len() {
                match socket.next().await {
                    Some(Ok(Frame::Binary(data))) => echoed.extend_from_slice(&data),
                    Some(Ok(Frame::Text(_))) => {}
                    other => panic!("input stopped before its echo completed: {other:?}"),
                }
            }
            echoed
        })
        .await
        .expect("the write and its echo make progress together");
        assert_eq!(echoed, payload);
        socket.close(None).await.unwrap();
        server.abort();
        let _ = server.await;
        drop((live, dir));
    }

    /// What is typed while the session's messages are held for a handoff waits, unwritten, and goes
    /// in once the hold ends.
    #[tokio::test]
    async fn typing_waits_while_the_sessions_messages_are_held_and_goes_in_after() {
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-input-held");
        let live = crate::test_support::fake_live_session(
            "held",
            AgentId::CLAUDE,
            80,
            24,
            "stty raw -echo; printf READY; cat",
        );
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = state.clone();
        let server = tokio::spawn(async move { axum::serve(listener, router(served)).await });
        let (mut socket, _) = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{port}/ws/term/held?key={TEST_KEY}"
        ))
        .await
        .unwrap();
        let mut output = Vec::new();
        while !output.windows(5).any(|text| text == b"READY") {
            if let Some(Ok(Frame::Binary(data))) =
                tokio::time::timeout(PATIENCE, socket.next()).await.unwrap()
            {
                output.extend_from_slice(&data);
            }
        }
        output.clear();

        let hold = state.hold_outbox("held");
        socket.send(Frame::Binary(b"typed".to_vec())).await.unwrap();
        let waited = tokio::time::timeout(std::time::Duration::from_millis(400), async {
            loop {
                match socket.next().await {
                    Some(Ok(Frame::Binary(data))) => output.extend_from_slice(&data),
                    None => break,
                    _ => {}
                }
            }
        })
        .await;
        assert!(
            waited.is_err() && output.is_empty(),
            "held typing is not written"
        );

        drop(hold);
        tokio::time::timeout(PATIENCE, async {
            while !output.windows(5).any(|text| text == b"typed") {
                if let Some(Ok(Frame::Binary(data))) = socket.next().await {
                    output.extend_from_slice(&data);
                }
            }
        })
        .await
        .expect("the typing goes in once the hold ends");
        socket.close(None).await.unwrap();
        server.abort();
        let _ = server.await;
        drop((live, dir));
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
            crate::protocol::AgentId::GROK,
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
}
