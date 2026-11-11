//! The terminal socket (`/ws/term/:session`): the first paint a client attaches with, the live
//! output after it, and the writer that puts what the client types into the process's PTY without
//! stalling the output.

use std::collections::VecDeque;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;

use crate::messaging::outbox::HoldKind;
use crate::protocol::{error_code, CodedError, Event, TermControl, TermEvent};
use crate::session::LiveSession;
use crate::state::AppState;
use crate::terminal::display::DisplayFacts;
use crate::terminal::fan::{Answering, Attached, Delivery, Outgoing};
use crate::terminal::input_class::is_terminal_protocol;
use crate::terminal::pty::PtyCore;
use crate::terminal::shell::ShellProcess;

/// What a client asks for in the attach request's query string.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
pub(super) struct AttachRequest {
    /// How many rows of the normal screen's scrollback the first paint carries at most.
    history: usize,
}

pub(super) async fn term_ws(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    Query(request): Query<AttachRequest>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_term(socket, state, session_id, request.history))
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

    fn write_input(&self, data: &[u8]) -> Result<(), crate::platform::term::PartialWrite> {
        match self {
            Terminal::Session(session) => session.write_terminal_input(data),
            Terminal::Shell(shell) => shell.write(data),
        }
    }

    /// A client's reply to a query in output it was sent, which a terminal still takes while it is
    /// handed over at a planned stop, as it takes the daemon's own answers
    /// ([`PtyCore::write_reply`]).
    fn write_reply(&self, data: &[u8]) -> Result<(), crate::platform::term::PartialWrite> {
        match self {
            Terminal::Session(session) => session.write_terminal_reply(data),
            Terminal::Shell(shell) => shell.write_reply(data),
        }
    }

    /// Runs `step` on the terminal's PTY core, off the runtime's worker where it may change the
    /// size of a held terminal, which waits on its holder ([`PtyCore::resize_waits`]).
    async fn moving_size(&self, step: impl FnOnce(&PtyCore) + Send + 'static) {
        if !self.pty().resize_waits() {
            step(self.pty());
            return;
        }
        let terminal = self.clone();
        if let Err(err) = tokio::task::spawn_blocking(move || step(terminal.pty())).await {
            tracing::debug!(%err, "a terminal's size task failed");
        }
    }
}

/// A live client's place among a terminal's attached clients, from its attach to the end of its
/// connection. Settles the activity once the client's attach loses its precedence, at the one
/// deadline the attach set, in a task of its own: the connection's loop waits on slow socket
/// sends, which must not hold the size and display back. Dropped, however the connection ended,
/// it lets the client go, so its size and display stop counting and the next client's apply.
struct Registration {
    terminal: Terminal,
    client: u64,
    settle: tokio::task::JoinHandle<()>,
}

impl Registration {
    fn new(terminal: Terminal, client: u64, precedence_lapses: std::time::Instant) -> Self {
        let settle = {
            let terminal = terminal.clone();
            tokio::spawn(async move {
                tokio::time::sleep_until(tokio::time::Instant::from_std(precedence_lapses)).await;
                terminal.moving_size(PtyCore::settle_activity).await;
            })
        };
        Self {
            terminal,
            client,
            settle,
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.settle.abort();
        // A panic that unwinds through here may have poisoned the fan's lock, and a second panic
        // would abort the daemon rather than end this connection's task.
        if std::thread::panicking() {
            return;
        }
        let client = self.client;
        if !self.terminal.pty().resize_waits() {
            self.terminal.pty().leave(client);
            return;
        }
        // A held terminal's leaving waits on its holder, which must not hold up the runtime's
        // worker this is dropped on: it is made on a blocking thread, and nothing waits for it.
        let terminal = self.terminal.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn_blocking(move || terminal.pty().leave(client))),
            Err(_) => terminal.pty().leave(client),
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
    /// task carrying one failed.
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
        result: Result<(), crate::platform::term::PartialWrite>,
    ) -> Result<bool, crate::platform::term::PartialWrite> {
        match (frame, result) {
            (InputFrame::User(data), Err(error))
                if error.written == 0 && error.error.kind() == std::io::ErrorKind::WouldBlock =>
            {
                self.frames.push_front(data);
                Ok(false)
            }
            (InputFrame::User(data), Err(error)) => {
                self.bytes -= data.len();
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

    /// A user frame refused while the terminal is handed over to the next daemon at a planned stop
    /// (see [`refused_for_handover`]): dropped and no longer charged. The person is told of the
    /// refusal at once, so it does not count as lost typing as well.
    fn drop_refused(&mut self, frame: InputFrame) {
        self.bytes -= frame.len();
    }

    /// A write task that failed took its frame with it, which `carried` describes: it is no longer
    /// charged, and typing it held counts as lost.
    fn write_task_failed(
        &mut self,
        carried: Carried,
        error: tokio::task::JoinError,
    ) -> crate::platform::term::PartialWrite {
        self.bytes -= carried.bytes;
        self.typing_lost |= carried.typing;
        crate::platform::term::PartialWrite {
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

    /// The bytes of the replies still queued, which the connection ending leaves unwritten.
    fn unsent_replies(&self) -> usize {
        self.replies.iter().map(Vec::len).sum()
    }
}

/// Settles with the terminal a write of the client's replies that has come to an end
/// ([`PtyCore::settle_reply`]), so that a planned stop knows whether the program still waits on
/// one of them.
fn settle_reply_write(
    pty: &PtyCore,
    frame: &InputFrame,
    result: &Result<(), crate::platform::term::PartialWrite>,
) {
    if let InputFrame::Reply(data) = frame {
        pty.settle_reply(data.len(), result.is_ok());
    }
}

/// Whether `result`, of writing `frame`, is a user frame refused whole because the terminal is
/// handed over to the next daemon at a planned stop. A refusal to try again later (`WouldBlock`)
/// is not one: it waits as ever. Nor is a write that got partway in, such as a large paste whose
/// first chunks went in before the mark: that is typing lost, reported as such.
fn refused_for_handover(
    state: &AppState,
    frame: &InputFrame,
    result: &Result<(), crate::platform::term::PartialWrite>,
) -> bool {
    let Err(error) = result else {
        return false;
    };
    let refused_whole = error.written == 0 && error.error.kind() != std::io::ErrorKind::WouldBlock;
    matches!(frame, InputFrame::User(_)) && refused_whole && state.is_handing_over()
}

/// Whether `frame` holds typing, not only the terminal's own reports (focus, mouse), which a
/// person who is told their input was refused would not count as theirs.
fn holds_typing(frame: &InputFrame) -> bool {
    matches!(frame, InputFrame::User(data) if !is_terminal_protocol(data))
}

/// Tells the person the terminal `id` takes no input while it is handed over to the next daemon:
/// what they typed is dropped. Told once per connection; the frames refused whole that it covers
/// are not reported again as lost typing when the connection ends.
fn tell_input_refused(state: &AppState, id: &str) {
    state.broadcast(Event::from_error(
        None,
        &CodedError::raised(
            error_code::DAEMON_SHUTTING_DOWN,
            format!(
                "The terminal {id} takes no input while the daemon hands it over to the next daemon."
            ),
            &[],
        ),
    ));
}

type InputWrite = (InputFrame, Result<(), crate::platform::term::PartialWrite>);

/// What an input write in flight carries, kept outside its task so that one that fails still
/// tells what it took with it.
#[derive(Clone, Copy)]
struct Carried {
    /// The bytes it holds, charged to the connection until the write ends.
    bytes: usize,
    /// The bytes of the client's replies it writes, if it writes replies, which the terminal
    /// counts as owed until they are settled ([`PtyCore::settle_reply`]).
    reply_bytes: usize,
    /// It writes a user frame that is not only the terminal's own reports.
    typing: bool,
}

/// An input write in flight.
struct Writing {
    task: tokio::task::JoinHandle<InputWrite>,
    carried: Carried,
}

fn start_input_write(terminal: Terminal, frame: InputFrame) -> Writing {
    let carried = match &frame {
        InputFrame::User(data) => Carried {
            bytes: data.len(),
            reply_bytes: 0,
            typing: !is_terminal_protocol(data),
        },
        InputFrame::Reply(data) => Carried {
            bytes: data.len(),
            reply_bytes: data.len(),
            typing: false,
        },
    };
    let task = tokio::task::spawn_blocking(move || {
        let result = match &frame {
            InputFrame::User(data) => terminal.write_input(data),
            InputFrame::Reply(data) => terminal.write_reply(data),
        };
        (frame, result)
    });
    Writing { task, carried }
}

/// The end of a write task that failed, with what it carried: its replies are settled as never
/// written.
fn write_task_failed(
    pty: &PtyCore,
    deferred: &mut DeferredInput,
    carried: Carried,
    error: tokio::task::JoinError,
) -> crate::platform::term::PartialWrite {
    if carried.reply_bytes > 0 {
        pty.settle_reply(carried.reply_bytes, false);
    }
    deferred.write_task_failed(carried, error)
}

async fn handle_term(mut socket: WebSocket, state: Arc<AppState>, id: String, history: usize) {
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

    // The first paint and the subscription happen under one lock inside `attach`, at the point
    // output is appended, so live output follows the paint without missing a byte or repeating
    // one even while the process is producing output right now. A shell that has exited answers
    // with the paint of its last screen alone, read-only, as a session without a process does. A
    // live one is first told where its paint leaves the output, for answering the program's
    // queries after it.
    let Attached {
        opening,
        live,
        answering,
    } = pty.attach(history);
    let (
        Some(Answering {
            client,
            offset,
            precedence_lapses,
        }),
        Some(wake),
    ) = (answering, live)
    else {
        if !opening.is_empty() && socket.send(Message::Binary(opening)).await.is_err() {
            return;
        }
        let _ = socket.send(Message::Close(None)).await;
        return;
    };
    // Let go however the connection ends from here on, an early return included.
    let registration = Registration::new(terminal.clone(), client, precedence_lapses);
    if send_paint(&mut socket, offset, opening).await.is_err() {
        return;
    }
    // Control frames are read only from here on, so a resize the client sent on connecting is
    // applied after its first paint: the program redraws for the client's size over a screen the
    // client already shows. Attaching made this the most recently active client, so the size it
    // asks for is applied, until its attach's precedence lapses without it being used.
    let mut deferred = DeferredInput::default();
    let mut retry = tokio::time::interval(std::time::Duration::from_millis(25));
    retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut writing: Option<Writing> = None;
    let mut user_ready = true;
    let mut failure = None;
    // Whether the client was told its typing is refused while the terminal is handed over.
    let mut told_handover = false;
    loop {
        if writing.is_none() {
            // While the session's messages are held (`OutboxHold`: a handoff, an agent's transfer
            // of a session it owns, a sharing change), what is typed waits here too: a turn it
            // started now would run against work that is being moved. Released, it goes in as
            // usual; never written, it is reported as lost when the socket ends. A restart's hold
            // keeps nothing of the person's: their typing goes to the process as it stands.
            if user_ready
                && !deferred.frames.is_empty()
                && matches!(terminal, Terminal::Session(_))
                && matches!(
                    state.outbox.held_for(&id),
                    Some(HoldKind::Handoff | HoldKind::Sharing)
                )
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
                        // A focus or pointer report is the terminal's own traffic, not the person
                        // turning to this client.
                        if !is_terminal_protocol(&data) {
                            terminal.moving_size(move |pty| pty.activate(client)).await;
                        }
                        deferred.push(data);
                    }
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<TermControl>(&text) {
                            Ok(TermControl::Resize { cols, rows }) => {
                                // A held terminal's resize waits on its holder, so it is not
                                // made on the runtime's worker.
                                let id = id.clone();
                                terminal
                                    .moving_size(move |pty| {
                                        if let Err(err) = pty.request_size(client, cols, rows) {
                                            tracing::debug!(terminal = %id, %err, "resize failed");
                                        }
                                    })
                                    .await;
                            }
                            Ok(TermControl::Reply { data, offset }) => {
                                if !pty.admit_reply(client, offset, data.as_bytes()) {
                                    continue;
                                }
                                deferred.push_reply(data.into_bytes());
                            }
                            Ok(TermControl::Display { foreground, background, cursor, cell_width, cell_height }) => {
                                match DisplayFacts::reported(&foreground, &background, &cursor, cell_width.zip(cell_height)) {
                                    Some(display) => pty.report_display(client, display),
                                    None => tracing::debug!(terminal = %id, "unreadable display colours"),
                                }
                            }
                            // Wakes this connection for the output the credit lets through.
                            Ok(TermControl::Ack { bytes }) => pty.acknowledge(client, bytes),
                            Ok(TermControl::Activate) => {
                                terminal.moving_size(move |pty| pty.activate(client)).await;
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
            completed = async { (&mut writing.as_mut().expect("an active input write").task).await }, if writing.is_some() => {
                let carried = writing.take().expect("an active input write").carried;
                let result = match completed {
                    // Refused while the terminal is handed over to the next daemon at a planned
                    // stop: what was typed is dropped and the person told (once, and only for
                    // typing, not terminal reports), but the socket stays open, so the replies
                    // this client still owes to the output it was sent before the terminal's mark
                    // go in.
                    Ok((frame, result)) if refused_for_handover(&state, &frame, &result) => {
                        if holds_typing(&frame) && !told_handover {
                            told_handover = true;
                            tell_input_refused(&state, &id);
                        }
                        deferred.drop_refused(frame);
                        Ok(true)
                    }
                    Ok((frame, result)) => {
                        settle_reply_write(pty, &frame, &result);
                        deferred.complete(frame, result)
                    }
                    Err(error) => Err(write_task_failed(pty, &mut deferred, carried, error)),
                };
                match result {
                    Ok(ready) => {
                        if !ready { user_ready = false; }
                    }
                    // Whatever else failed during a handover, the socket stays open for the
                    // replies the client still owes; what was lost is reported when it ends.
                    Err(error) if state.is_handing_over() => {
                        tracing::debug!(terminal = %id, %error, "writing a terminal's input failed during the handover");
                        failure = Some(error.to_string());
                    }
                    Err(error) => {
                        failure = Some(error.to_string());
                        break;
                    }
                }
            }
            () = wake.notified() => {
                // Ends once the output is over and its last bytes have been sent: the session
                // ended, or the shell it followed exited or was closed.
                if !forward_output(&mut socket, pty, client).await {
                    break;
                }
            }
        }
    }
    // Its size no longer decides the terminal's, nor its display the answers to the program's
    // queries.
    drop(registration);
    if let Some(Writing { task, carried }) = writing {
        match task.await {
            Ok((frame, result)) if refused_for_handover(&state, &frame, &result) => {
                if holds_typing(&frame) && !told_handover {
                    told_handover = true;
                    tell_input_refused(&state, &id);
                }
                deferred.drop_refused(frame);
            }
            Ok((frame, result)) => {
                settle_reply_write(pty, &frame, &result);
                if let Err(error) = deferred.complete(frame, result) {
                    failure = Some(error.to_string());
                }
            }
            Err(error) => {
                failure = Some(write_task_failed(pty, &mut deferred, carried, error).to_string());
            }
        }
    }
    // During a handover, typing still queued would only be refused: it is dropped as refused
    // typing is, and told as such rather than reported lost.
    if state.is_handing_over() && !deferred.frames.is_empty() {
        let queued = std::mem::take(&mut deferred.frames);
        let typing = queued.iter().any(|frame| !is_terminal_protocol(frame));
        if typing && !told_handover {
            tell_input_refused(&state, &id);
        }
    }
    // Admitted but never written: the program may wait on them, at a planned stop for good.
    let unsent = deferred.unsent_replies();
    if unsent > 0 {
        pty.settle_reply(unsent, false);
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
    #[cfg(test)]
    pty.connection_ended();
}

/// Sends `client` what is waiting for it, as its credit allows; `false` once its stream is over or
/// the socket has failed. A fresh paint goes behind a replay mark, as the first paint of an attach
/// does, so the client counts the output on from where the paint leaves it.
async fn forward_output(socket: &mut WebSocket, pty: &PtyCore, client: u64) -> bool {
    loop {
        let frames = match pty.take_output(client) {
            Delivery::Send(frames) => frames,
            Delivery::Wait => return true,
            Delivery::Over => return false,
        };
        for frame in frames {
            let sent = match frame {
                Outgoing::Output(bytes) => socket.send(Message::Binary(bytes.to_vec())).await,
                Outgoing::Paint { offset, paint } => {
                    send_paint(socket, offset, paint.to_vec()).await
                }
            };
            if sent.is_err() {
                return false;
            }
        }
    }
}

/// Sends a paint behind its replay mark: its length, `offset`, where it leaves the output, and
/// that all of it is output whose queries are not the client's to answer, since a paint asks
/// nothing.
async fn send_paint(
    socket: &mut WebSocket,
    offset: u64,
    paint: Vec<u8>,
) -> Result<(), axum::Error> {
    let mark = TermEvent::Replay {
        length: paint.len(),
        offset,
        answered: paint.len(),
    };
    let text = serde_json::to_string(&mark).expect("a replay mark serializes");
    socket.send(Message::Text(text)).await?;
    socket.send(Message::Binary(paint)).await
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
                Err(crate::platform::term::PartialWrite {
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
                Err(crate::platform::term::PartialWrite {
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
            Err(crate::platform::term::PartialWrite {
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

    /// A user frame that failed, or that a handover refused, is no longer charged, so a socket that
    /// goes on reading during a handover is never gated by it; a refused one is not lost typing,
    /// which the person was told of already.
    #[test]
    fn deferred_frames_that_failed_or_were_refused_are_no_longer_charged() {
        let mut pending = DeferredInput::default();
        pending.push(b"x".to_vec());
        let key = pending.next(true).unwrap();
        pending
            .complete(
                key,
                Err(crate::platform::term::PartialWrite {
                    written: 0,
                    error: std::io::Error::from(std::io::ErrorKind::BrokenPipe),
                }),
            )
            .unwrap_err();
        assert_eq!(pending.bytes, 0);

        let mut pending = DeferredInput::default();
        pending.push(b"typed".to_vec());
        let key = pending.next(true).unwrap();
        pending.drop_refused(key);
        assert_eq!(pending.bytes, 0);
        assert!(!pending.lost_typing());
    }

    /// During a handover only a user frame refused with nothing written is a refusal: one that got
    /// partway in is lost typing, one to try again later waits, and a reply is settled as ever.
    /// Only a frame that holds typing is told as refused.
    #[tokio::test]
    async fn only_a_user_frame_refused_whole_during_a_handover_is_a_refusal() {
        use std::io::ErrorKind;

        let (state, _dir) = app_state("server-refused-whole");
        let failed =
            |written: usize, kind: ErrorKind| -> Result<(), crate::platform::term::PartialWrite> {
                Err(crate::platform::term::PartialWrite {
                    written,
                    error: std::io::Error::from(kind),
                })
            };
        let key = InputFrame::User(b"x".to_vec());
        assert!(
            !refused_for_handover(&state, &key, &failed(0, ErrorKind::BrokenPipe)),
            "a refusal before the handover"
        );
        state.settle_for_planned_stop().await;
        assert!(refused_for_handover(
            &state,
            &key,
            &failed(0, ErrorKind::BrokenPipe)
        ));
        assert!(!refused_for_handover(
            &state,
            &key,
            &failed(3, ErrorKind::BrokenPipe)
        ));
        assert!(!refused_for_handover(
            &state,
            &key,
            &failed(0, ErrorKind::WouldBlock)
        ));
        assert!(!refused_for_handover(&state, &key, &Ok(())));
        let reply = InputFrame::Reply(b"\x1b[1;1R".to_vec());
        assert!(!refused_for_handover(
            &state,
            &reply,
            &failed(0, ErrorKind::BrokenPipe)
        ));
        assert!(holds_typing(&key));
        assert!(!holds_typing(&InputFrame::User(b"\x1b[O".to_vec())));
        assert!(!holds_typing(&reply));
    }

    /// A write task that failed is no longer charged, and counts as lost typing only when what it
    /// carried was typing.
    #[tokio::test]
    async fn a_failed_write_task_is_no_longer_charged_and_loses_typing_only_if_it_carried_some() {
        async fn failed() -> tokio::task::JoinError {
            let task = tokio::spawn(std::future::pending::<()>());
            task.abort();
            task.await.unwrap_err()
        }
        let mut pending = DeferredInput::default();
        pending.push_reply(b"\x1b[1;1R".to_vec());
        pending.next(true).unwrap();
        let carried = Carried {
            bytes: 6,
            reply_bytes: 6,
            typing: false,
        };
        pending.write_task_failed(carried, failed().await);
        assert_eq!(pending.bytes, 0);
        assert!(!pending.lost_typing());

        pending.push(b"x".to_vec());
        pending.next(true).unwrap();
        let carried = Carried {
            bytes: 1,
            reply_bytes: 0,
            typing: true,
        };
        pending.write_task_failed(carried, failed().await);
        assert_eq!(pending.bytes, 0);
        assert!(pending.lost_typing());
    }

    /// While a terminal is handed over at a planned stop, typing refused for it is dropped and the
    /// person told once, but the socket stays open, for the replies the client still owes; a reply
    /// that does not go in counts as an answer lost, and nothing is reported as lost typing.
    #[tokio::test]
    async fn typing_refused_during_a_handover_keeps_the_socket_open_and_is_told_once() {
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-input-handover");
        let live = raw_echo("handover");
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let mut events = state.subscribe();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = state.clone();
        let server = tokio::spawn(async move { axum::serve(listener, router(served)).await });
        let (mut socket, _) = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{port}/ws/term/handover?key={TEST_KEY}"
        ))
        .await
        .unwrap();
        read_until(&mut socket, b"READY").await;

        state.settle_for_planned_stop().await;
        // Every write into the terminal is refused from here, as a handed-over terminal's input
        // is.
        live.freeze_output();
        socket.send(Frame::Binary(b"one".to_vec())).await.unwrap();
        socket.send(Frame::Binary(b"two".to_vec())).await.unwrap();
        let reply = serde_json::json!({
            "type": "reply",
            "data": "\u{1b}[1;1R",
            "offset": live.output_total(),
        });
        socket.send(Frame::Text(reply.to_string())).await.unwrap();
        // Still open: what the program prints now still reaches the client.
        live.on_output(b"still-open");
        read_until(&mut socket, b"still-open").await;
        tokio::time::timeout(PATIENCE, async {
            while !live.answers_lost() || live.replies_owed() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the refused reply counts as an answer lost");

        socket.close(None).await.unwrap();
        assert!(
            crate::test_support::within(PATIENCE, {
                let live = live.clone();
                move || live.connections_ended() == 1
            })
            .await,
            "the terminal connection never ended"
        );
        let (mut refused, mut lost) = (0, 0);
        while let Ok(event) = events.try_recv() {
            if let Event::Error { code, .. } = event {
                refused += usize::from(code == error_code::DAEMON_SHUTTING_DOWN);
                lost += usize::from(code == error_code::TERMINAL_INPUT_LOST);
            }
        }
        assert_eq!((refused, lost), (1, 0));
        server.abort();
        let _ = server.await;
        drop((live, dir));
    }

    /// A stand-in session `id` that writes `READY` and then echoes what it reads, byte for byte:
    /// a native process on Windows (`test_support::fixture`), whose echo is the console host's
    /// pace alone.
    fn raw_echo(id: &str) -> crate::test_support::StandIn {
        #[cfg(unix)]
        return crate::test_support::fake_live_session(
            id,
            AgentId::CLAUDE,
            80,
            24,
            "stty raw -echo; printf READY; cat",
        );
        #[cfg(windows)]
        return crate::test_support::fake_live_session_running(
            id,
            AgentId::CLAUDE,
            80,
            24,
            &crate::test_support::fixture::Steps::new()
                .print("READY")
                .echo_raw()
                .program("echo"),
        );
    }

    #[tokio::test]
    async fn a_terminal_write_keeps_draining_echo_while_the_input_is_in_flight() {
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-input-echo");
        let live = raw_echo("echo");
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
        // One frame exceeds both the PTY's buffer and the credit window, so writing it
        // synchronously prevents the echo from draining and times out even though the child is
        // healthy. It stays below the backlog cap, so the echo is sent byte for byte.
        let payload = vec![b'x'; 2 * 1024 * 1024];
        socket.send(Frame::Binary(payload.clone())).await.unwrap();
        let echoed = tokio::time::timeout(PATIENCE, async {
            let mut echoed = Vec::with_capacity(payload.len());
            while echoed.len() < payload.len() {
                match socket.next().await {
                    Some(Ok(Frame::Binary(data))) => {
                        echoed.extend_from_slice(&data);
                        socket.send(ack(data.len())).await.unwrap();
                    }
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

    type Socket = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    /// Reads what reaches `socket` until it holds `needle`.
    async fn read_until(socket: &mut Socket, needle: &[u8]) -> Vec<u8> {
        use tokio_tungstenite::tungstenite::Message as Frame;
        tokio::time::timeout(PATIENCE, async move {
            let mut seen = Vec::new();
            while !seen.windows(needle.len()).any(|window| window == needle) {
                match socket.next().await {
                    Some(Ok(Frame::Binary(data))) => seen.extend_from_slice(&data),
                    Some(Ok(_)) => {}
                    other => panic!("the socket ended before {needle:?}: {other:?}"),
                }
            }
            seen
        })
        .await
        .unwrap_or_else(|_| panic!("{:?} never arrived", String::from_utf8_lossy(needle)))
    }

    /// The acknowledgement of `bytes` of output, as a client sends once its terminal has consumed
    /// them.
    fn ack(bytes: usize) -> tokio_tungstenite::tungstenite::Message {
        tokio_tungstenite::tungstenite::Message::Text(
            serde_json::json!({ "type": "ack", "bytes": bytes }).to_string(),
        )
    }

    /// Reads what reaches `socket` until it holds `needle`, acknowledging every binary frame as a
    /// client does once its terminal has consumed it. Returns the bytes and the text frames.
    async fn read_acking(socket: &mut Socket, needle: &[u8]) -> (Vec<u8>, Vec<String>) {
        use tokio_tungstenite::tungstenite::Message as Frame;
        tokio::time::timeout(PATIENCE, async move {
            let (mut seen, mut texts) = (Vec::new(), Vec::new());
            while !seen.windows(needle.len()).any(|window| window == needle) {
                match socket.next().await {
                    Some(Ok(Frame::Binary(data))) => {
                        seen.extend_from_slice(&data);
                        socket.send(ack(data.len())).await.unwrap();
                    }
                    Some(Ok(Frame::Text(text))) => texts.push(text.to_string()),
                    Some(Ok(_)) => {}
                    other => panic!("the socket ended before {needle:?}: {other:?}"),
                }
            }
            (seen, texts)
        })
        .await
        .unwrap_or_else(|_| panic!("{:?} never arrived", String::from_utf8_lossy(needle)))
    }

    /// The completion criterion of the stream's flow control, end to end: a client that stops
    /// reading while the program floods output stays connected, and once it reads again it is
    /// sent a fresh paint in place of the backlog. The other client, reading all along, is sent
    /// every byte and no paint.
    #[tokio::test]
    async fn a_client_that_stops_reading_through_a_flood_stays_connected_and_resumes_on_a_fresh_paint(
    ) {
        use crate::terminal::fan::BACKLOG_CAP;

        let (state, dir) = app_state("server-flood");
        let live = raw_echo("flood");
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        printed_since(&live, 0, b"READY").await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = state.clone();
        let server = tokio::spawn(async move { axum::serve(listener, router(served)).await });
        let url = format!("ws://127.0.0.1:{port}/ws/term/flood?key={TEST_KEY}");

        // Each has had its first paint, so both follow the output from here.
        let (mut stalled, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        read_acking(&mut stalled, b"READY").await;
        let (mut reading, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        read_acking(&mut reading, b"READY").await;

        // The program's flood, paced by the reading client alone: each chunk is let out once
        // that client has been sent the one before, so it never falls behind, while the stalled
        // client reads nothing at all.
        let filler = "f".repeat(64 * 1024);
        let (mut flooded, mut chunks) = (0, 0);
        while flooded < 2 * BACKLOG_CAP {
            let marker = format!("<{chunks}>");
            let chunk = format!("{filler}{marker}\r\n");
            live.on_output(chunk.as_bytes());
            let (seen, texts) = read_acking(&mut reading, marker.as_bytes()).await;
            assert!(
                texts.is_empty(),
                "the reading client was repainted: {texts:?}"
            );
            assert!(
                seen.ends_with(chunk.as_bytes()),
                "the reading client lost output"
            );
            flooded += chunk.len();
            chunks += 1;
        }

        stalled
            .send(tokio_tungstenite::tungstenite::Message::Binary(
                b"<alive>".to_vec(),
            ))
            .await
            .unwrap();
        let (seen, texts) = read_acking(&mut stalled, b"<alive>").await;
        let paint = texts
            .iter()
            .filter_map(|text| serde_json::from_str::<serde_json::Value>(text).ok())
            .find(|mark| mark["type"] == "replay")
            .unwrap_or_else(|| panic!("no fresh paint for the stalled client: {texts:?}"));
        assert!(paint["length"].as_u64().unwrap() > 0);
        assert!(
            seen.len() < flooded,
            "the backlog was discarded rather than sent: {} of {flooded} bytes",
            seen.len()
        );
        read_acking(&mut reading, b"<alive>").await;

        stalled.close(None).await.unwrap();
        reading.close(None).await.unwrap();
        server.abort();
        let _ = server.await;
        drop((live, dir));
    }

    /// Sends `frames`, then types `typed`, and reads until its echo comes back. A connection reads
    /// control frames and input in order, so by then the frames have been taken.
    async fn send_and_see(
        socket: &mut Socket,
        frames: Vec<tokio_tungstenite::tungstenite::Message>,
        typed: &[u8],
    ) {
        use tokio_tungstenite::tungstenite::Message as Frame;
        for frame in frames {
            socket.send(frame).await.unwrap();
        }
        socket.send(Frame::Binary(typed.to_vec())).await.unwrap();
        read_until(socket, typed).await;
    }

    /// Asks the daemon for the text area's size (`CSI 18t`, answered from its parsed screen, which
    /// is resized under the same lock as the PTY) until the answer names `rows` by `cols`: a
    /// client's leaving, and an attach's lapse, are taken in their own time.
    async fn size_becomes(live: &crate::test_support::StandIn, rows: u16, cols: u16) {
        let expected = format!("\x1b[8;{rows};{cols}t");
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            let mark = live.output_total();
            live.on_output(b"\x1b[18t");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let printed = live.output_since(mark);
            if printed
                .windows(expected.len())
                .any(|window| window == expected.as_bytes())
            {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the size never became {rows} by {cols}: {:?}",
                String::from_utf8_lossy(&printed)
            );
        }
    }

    /// What the program has printed since `mark`, once it holds `needle`.
    async fn printed_since(
        live: &crate::test_support::StandIn,
        mark: u64,
        needle: &[u8],
    ) -> Vec<u8> {
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            let printed = live.output_since(mark);
            if printed.windows(needle.len()).any(|window| window == needle) {
                return printed;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{:?} never printed in {:?}",
                String::from_utf8_lossy(needle),
                String::from_utf8_lossy(&printed)
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    /// The program's queries are answered once, by the daemon, with no client attached and with
    /// two, in the colours of the client that was active last, and none of them reaches a client.
    /// The stand-in echoes what it reads, so an answer written into its input comes back as output.
    #[tokio::test]
    async fn the_daemon_answers_the_programs_queries_in_the_most_recently_active_clients_colours() {
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-queries");
        let live = raw_echo("queries");
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        printed_since(&live, 0, b"READY").await;

        let mark = live.output_total();
        live.on_output(b"\x1b[6n");
        printed_since(&live, mark, b"\x1b[1;6R").await;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = state.clone();
        let server = tokio::spawn(async move { axum::serve(listener, router(served)).await });
        let url = format!("ws://127.0.0.1:{port}/ws/term/queries?key={TEST_KEY}");
        let display = |foreground: &str, background: &str| {
            Frame::Text(
                serde_json::json!({
                    "type": "display",
                    "foreground": foreground,
                    "background": background,
                    "cursor": foreground,
                    "cell_width": 9,
                    "cell_height": 20,
                })
                .to_string(),
            )
        };
        let asks_background = b"\x1b]11;?\x07";
        // The echo of an answer is the program setting that colour, which it would then read back
        // whatever the display: once the echo has reached a client, it has been parsed, and the
        // program resets the colour (OSC 111).
        let forget_echo = || live.on_output(b"\x1b]111\x07");

        let (mut first, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        send_and_see(&mut first, vec![display("#383a42", "#fafafa")], b"<one>").await;
        let mark = live.output_total();
        live.on_output(asks_background);
        let seen = read_until(&mut first, b"\x1b]11;rgb:fafa/fafa/fafa\x07").await;
        assert!(!seen
            .windows(asks_background.len())
            .any(|window| window == asks_background));
        printed_since(&live, mark, b"rgb:fafa").await;
        forget_echo();

        let (mut second, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        send_and_see(&mut second, vec![display("#ffffff", "#000000")], b"<two>").await;
        let mark = live.output_total();
        live.on_output(asks_background);
        let seen = read_until(&mut first, b"\x1b]11;rgb:0000/0000/0000\x07").await;
        assert!(!seen
            .windows(asks_background.len())
            .any(|window| window == asks_background));
        read_until(&mut second, b"\x1b]11;rgb:0000/0000/0000\x07").await;
        forget_echo();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let printed = live.output_since(mark);
        assert_eq!(
            printed
                .windows(8)
                .filter(|window| window == b"\x1b]11;rgb")
                .count(),
            1,
            "answered once: {:?}",
            String::from_utf8_lossy(&printed)
        );

        send_and_see(&mut first, Vec::new(), b"<three>").await;
        live.on_output(asks_background);
        read_until(&mut second, b"\x1b]11;rgb:fafa/fafa/fafa\x07").await;

        first.close(None).await.unwrap();
        second.close(None).await.unwrap();
        server.abort();
        let _ = server.await;
        drop((live, dir));
    }

    /// The terminal takes the size of the client that typed last, typing again or an `activate`
    /// frame takes back the size a client last asked for, and the size goes back to the other
    /// client when that one leaves.
    /// The daemon's answer to the text area query (`CSI 18t`) is the size of its parsed screen,
    /// which is resized under the same lock as the PTY.
    #[tokio::test]
    async fn the_terminal_takes_the_size_of_the_client_that_typed_last() {
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-sizes");
        let live = raw_echo("sizes");
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        printed_since(&live, 0, b"READY").await;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = state.clone();
        let server = tokio::spawn(async move { axum::serve(listener, router(served)).await });
        let url = format!("ws://127.0.0.1:{port}/ws/term/sizes?key={TEST_KEY}");
        let resize = |cols: u16, rows: u16| {
            Frame::Text(
                serde_json::json!({ "type": "resize", "cols": cols, "rows": rows }).to_string(),
            )
        };
        let (mut first, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        send_and_see(&mut first, vec![resize(100, 30)], b"<one>").await;
        size_becomes(&live, 30, 100).await;

        let (mut second, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        send_and_see(&mut second, vec![resize(80, 20)], b"<two>").await;
        size_becomes(&live, 20, 80).await;

        send_and_see(&mut first, vec![resize(120, 40)], b"<three>").await;
        size_becomes(&live, 40, 120).await;
        send_and_see(&mut second, Vec::new(), b"<four>").await;
        size_becomes(&live, 20, 80).await;
        // Turning back to the first client, with nothing typed and no output, takes its size back.
        first
            .send(Frame::Text(r#"{"type":"activate"}"#.into()))
            .await
            .unwrap();
        size_becomes(&live, 40, 120).await;
        send_and_see(&mut second, Vec::new(), b"<five>").await;
        size_becomes(&live, 20, 80).await;

        second.close(None).await.unwrap();
        size_becomes(&live, 40, 120).await;

        first.close(None).await.unwrap();
        server.abort();
        let _ = server.await;
        drop((live, dir));
    }

    /// A quiet attach gives the size back at its deadline from its registration's own task, with
    /// nothing driving a connection, and a registration dropped lets its client go, so the next
    /// client's size applies.
    #[tokio::test]
    async fn a_registration_settles_at_its_deadline_and_lets_its_client_go() {
        let live = raw_echo("registration");
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        printed_since(&live, 0, b"READY").await;
        let terminal = Terminal::Session(Arc::clone(&live));
        let pty = terminal.pty();
        let client = |attached: Attached| attached.answering.expect("a live client").client;

        let first = client(pty.attach(0));
        pty.request_size(first, 100, 30).unwrap();
        pty.activate(first);
        crate::terminal::activity::ATTACH_CLAIM_OVERRIDE
            .set(Some(std::time::Duration::from_secs(1)));
        let attached = pty.attach(0);
        crate::terminal::activity::ATTACH_CLAIM_OVERRIDE.set(None);
        let Answering {
            client: second,
            precedence_lapses,
            ..
        } = attached.answering.expect("a live client");
        pty.request_size(second, 80, 20).unwrap();
        size_becomes(&live, 20, 80).await;

        let quiet = Registration::new(terminal.clone(), second, precedence_lapses);
        size_becomes(&live, 30, 100).await;

        let far = std::time::Instant::now() + 100 * PATIENCE;
        drop(Registration::new(terminal.clone(), first, far));
        size_becomes(&live, 20, 80).await;

        drop(quiet);
        drop(live);
    }

    /// What is typed while the session's messages are held for a handoff waits, unwritten, and goes
    /// in once the hold ends.
    #[tokio::test]
    async fn typing_waits_while_the_sessions_messages_are_held_and_goes_in_after() {
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-input-held");
        let live = raw_echo("held");
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

    /// A restart's hold on the session's messages keeps nothing of what the person types: it goes
    /// to the process as it stands.
    #[tokio::test]
    async fn typing_is_not_held_by_a_restart_s_hold_on_the_sessions_messages() {
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-input-restart");
        let live = raw_echo("restarting");
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = state.clone();
        let server = tokio::spawn(async move { axum::serve(listener, router(served)).await });
        let (mut socket, _) = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{port}/ws/term/restarting?key={TEST_KEY}"
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

        let hold = state.hold_outbox_for_restart("restarting");
        socket.send(Frame::Binary(b"typed".to_vec())).await.unwrap();
        tokio::time::timeout(PATIENCE, async {
            while !output.windows(5).any(|text| text == b"typed") {
                if let Some(Ok(Frame::Binary(data))) = socket.next().await {
                    output.extend_from_slice(&data);
                }
            }
        })
        .await
        .expect("the typing goes in while the hold lasts");
        drop(hold);
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
