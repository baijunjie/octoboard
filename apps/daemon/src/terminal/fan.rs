//! The output side of a process on a PTY: the replay ring, the parsed screen, and the clients
//! attached to follow the output, behind one lock.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::sync::mpsc;

use crate::terminal::query_log::{self, QueryLog};
use crate::terminal::reply_route::{ReplayMark, ReplyRoute};
use crate::terminal::ringbuf::RingBuffer;
use crate::terminal::snapshot::TerminalSnapshot;
use crate::terminal::title::TitleChange;

/// Terminal replay buffer per process.
pub const RING_CAPACITY: usize = 2 * 1024 * 1024;

/// Frames a single attached client may fall behind by before the PTY reader starts waiting on it.
const SUBSCRIBER_QUEUE: usize = 64;

/// How long the reader waits for one client to drain before dropping it. Until this elapses the
/// reader does not read from the PTY at all, which is the point: the process's own writes block,
/// so output is throttled at the source instead of being buffered without bound in the daemon. A
/// client that has not taken a single frame in this long is gone or wedged, and dropping it (its
/// socket closes, and it reconnects to a fresh replay) is better than throttling the process for
/// it.
const SUBSCRIBER_GRACE: Duration = Duration::from_secs(3);

struct Subscriber {
    id: u64,
    tx: mpsc::Sender<Bytes>,
}

/// Raw replay, parsed screen and subscribers share one lock: attaching has no output gap or
/// duplicate, and freezing captures one exact boundary before exit cleanup can change the screen.
struct State {
    ring: RingBuffer,
    terminal: TerminalSnapshot,
    /// Every byte ever pushed into `ring`, which it forgets and this does not: what lets a caller
    /// mark a point in the output and later read exactly what came after it.
    total: u64,
    subscribers: Vec<Subscriber>,
    next_id: u64,
    /// No client follows the output any more; see [`Fan::close`].
    closed: bool,
    /// Which answers to the program's queries are written. Beside the output, since a replay's
    /// [`ReplayMark`] is taken with it.
    replies: ReplyRoute,
    /// The daemon answers the program's device attributes and cursor position queries where no
    /// client is attached to; see [`Fan::new`].
    answering: bool,
    /// Where those queries are logged; see [`Fan::log_queries`].
    queries: Option<QueryLog>,
}

/// A terminal client registered by [`Fan::attach`].
pub struct Attached {
    pub replay: Vec<u8>,
    pub live: mpsc::Receiver<Bytes>,
    /// For a client following live output: its id among the clients answering the program's
    /// queries, and where its replay stands for answering them. `None` for a replay alone.
    pub answering: Option<Answering>,
}

/// A live client's part in answering the program's queries.
pub struct Answering {
    pub client: u64,
    /// Taken under the same lock as the replay, so no answer lands between the two.
    pub mark: ReplayMark,
}

pub struct Fan(Mutex<State>);

impl Fan {
    /// A fan for a terminal of `rows` by `cols`. With `daemon_answers`, [`Self::push`] hands out
    /// the answers to the program's device attributes and cursor position queries that no client
    /// is attached to give.
    pub fn new(rows: u16, cols: u16, daemon_answers: bool) -> Self {
        let mut terminal = TerminalSnapshot::new(rows, cols);
        if daemon_answers {
            terminal.answer_queries();
        }
        Self(Mutex::new(State {
            ring: RingBuffer::new(RING_CAPACITY),
            terminal,
            total: 0,
            subscribers: Vec::new(),
            next_id: 0,
            closed: false,
            replies: ReplyRoute::default(),
            answering: daemon_answers,
            queries: None,
        }))
    }

    /// Logs the program's device attributes and cursor position queries from now on, under
    /// `label`, with who answered each and how long it took; see [`QueryLog`].
    pub fn log_queries(&self, label: String) {
        let mut fan = self.lock();
        fan.terminal.answer_queries();
        fan.queries = Some(QueryLog::new(label));
    }

    /// Whether [`Self::push`] hands out answers: the fan was made to ([`Self::new`]).
    pub fn answers_queries(&self) -> bool {
        self.lock().answering
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().expect("fan mutex poisoned")
    }

    /// Registers a terminal client. The returned replay is everything buffered up to this
    /// instant, preceded by the terminal's current modes once the buffer no longer holds the
    /// start, and the receiver picks up with the very next byte after it. Once frozen, returns the
    /// archived screen with a closed receiver, so exit cleanup cannot replace it; once the output
    /// is closed, the replay with a closed receiver.
    pub fn attach(&self) -> Attached {
        let (tx, live) = mpsc::channel(SUBSCRIBER_QUEUE);
        let mut fan = self.lock();
        if let Some(output) = fan.terminal.frozen() {
            let replay = output.to_vec();
            return Attached {
                replay,
                live,
                answering: None,
            };
        }
        let mut replay = fan.ring.snapshot();
        let replayed = replay.len();
        let mut preamble = 0;
        if fan.total > replay.len() as u64 {
            // The ring has lost its head, and with it the mode sequences a program sends once at
            // startup. The parsed terminal still knows them, and they describe the end of the
            // window, which is where the replay leaves the client, so any toggles inside the
            // window land on the same state.
            let mut modes = fan.terminal.mode_preamble();
            preamble = modes.len();
            modes.append(&mut replay);
            replay = modes;
        }
        if fan.closed {
            return Attached {
                replay,
                live,
                answering: None,
            };
        }
        let client = fan.next_id;
        fan.next_id += 1;
        fan.subscribers.push(Subscriber { id: client, tx });
        let total = fan.total;
        let mark = fan
            .replies
            .replay_mark(total, replayed, preamble, Instant::now());
        Attached {
            replay,
            live,
            answering: Some(Answering { client, mark }),
        }
    }

    /// Whether this client's answer to the output up to `offset`, `reply`, is the one written; see
    /// [`ReplyRoute::admit`].
    pub fn admit_reply(&self, client: u64, offset: u64, reply: &[u8]) -> bool {
        let mut fan = self.lock();
        let total = fan.total;
        let now = Instant::now();
        let admitted = fan.replies.admit(client, offset, total, now);
        if let Some(queries) = fan.queries.as_mut() {
            queries.reply(client, offset, &query_log::answers_in(reply), admitted, now);
        }
        admitted
    }

    /// Ends every attached client's stream once the output it follows is over, and makes every
    /// later [`Self::attach`] a replay alone. Output that still arrives is kept for that replay.
    pub fn close(&self) {
        let mut fan = self.lock();
        fan.closed = true;
        fan.subscribers.clear();
    }

    /// The tail of the output, at most `max_bytes` of it, from the same ring buffer a terminal
    /// client replays.
    pub fn recent_output(&self, max_bytes: usize) -> Vec<u8> {
        self.lock().ring.tail(max_bytes)
    }

    /// Freezes the screen; `true` when this call is the one that froze it.
    pub fn freeze(&self) -> bool {
        self.lock().terminal.freeze()
    }

    pub fn discard_frozen(&self) {
        self.lock().terminal.discard_frozen();
    }

    pub fn frozen_output(&self) -> Option<Vec<u8>> {
        self.lock().terminal.frozen().map(<[u8]>::to_vec)
    }

    pub fn is_frozen(&self) -> bool {
        self.lock().terminal.frozen().is_some()
    }

    /// The final parsed screen, or the frozen one.
    pub fn output(&self) -> Vec<u8> {
        self.lock().terminal.output()
    }

    /// How many bytes have been pushed in all. A mark for [`Self::output_since`].
    pub fn total(&self) -> u64 {
        self.lock().total
    }

    pub fn bracketed_paste_on(&self) -> bool {
        self.lock().terminal.bracketed_paste_on()
    }

    pub fn screen_text(&self) -> String {
        self.lock().terminal.visible_text()
    }

    pub fn screen_text_undimmed(&self) -> String {
        self.lock().terminal.visible_text_undimmed()
    }

    pub fn screen_text_inverse(&self) -> String {
        self.lock().terminal.visible_text_inverse()
    }

    /// Has [`Self::push`] hand over the title changes each chunk makes; see
    /// [`TerminalSnapshot::report_titles`].
    pub fn report_titles(&self) {
        self.lock().terminal.report_titles();
    }

    /// What was pushed after `mark`, as taken from [`Self::total`] — less of it if the ring
    /// buffer has already let some go.
    pub fn output_since(&self, mark: u64) -> Vec<u8> {
        let fan = self.lock();
        let wanted = fan.total.saturating_sub(mark);
        fan.ring.tail(usize::try_from(wanted).unwrap_or(usize::MAX))
    }

    /// Resizes the parsed screen to `rows` by `cols` once `resize_pty` has resized the PTY, both
    /// under the lock so that no chunk is parsed between the two. Refused once the screen is
    /// frozen.
    pub fn resize(
        &self,
        rows: u16,
        cols: u16,
        resize_pty: impl FnOnce() -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut fan = self.lock();
        anyhow::ensure!(
            fan.terminal.frozen().is_none(),
            "the terminal's screen has been frozen"
        );
        resize_pty()?;
        fan.terminal.resize(rows, cols);
        Ok(())
    }

    /// Takes one chunk read from the PTY into the replay, the parsed screen and every attached
    /// client. `before_delivery` is told the new total and the title changes the chunk made on the
    /// parsed screen (none unless [`Self::report_titles`] asked for them) once the chunk is
    /// buffered and before any client is handed it, and runs outside the lock.
    ///
    /// Blocks while a slow client catches up, which is how backpressure reaches the process: with
    /// nobody reading the PTY, its own writes block. The alternative was measured: a reader that
    /// always runs ahead fills any bounded channel within a fraction of a second under heavy output
    /// and the client is dropped, at every frame size tested.
    ///
    /// Returns the answers to write into the program's input: those of the device attributes and
    /// cursor position queries in this chunk, where the fan was made to answer them
    /// ([`Self::new`]) and no client is attached to. They are recorded as the daemon's own answer
    /// to the output up to the end of this chunk (see [`ReplyRoute::DAEMON`]), so a client that
    /// attaches later does not answer them again. A query that arrives while a client is attached
    /// is the clients' to answer, and the fan does not answer it even if every client leaves
    /// before it is.
    #[must_use = "the answers are the program's to read, and only the caller can write them"]
    pub fn push(
        &self,
        data: &[u8],
        before_delivery: impl FnOnce(u64, Vec<TitleChange>),
    ) -> Vec<u8> {
        let chunk = Bytes::copy_from_slice(data);
        let (targets, total, titles, answers): (Vec<(u64, mpsc::Sender<Bytes>)>, u64, _, _) = {
            let mut fan = self.lock();
            fan.ring.push(data);
            fan.terminal.process(data);
            let titles = fan.terminal.take_titles();
            let mut answers = fan.terminal.take_answers();
            fan.total += data.len() as u64;
            let total = fan.total;
            let now = Instant::now();
            fan.replies.note_output(total, now);
            let (clients, answering) = (fan.subscribers.len(), fan.answering);
            let asked = match fan.queries.as_mut() {
                Some(queries) => {
                    let asked = query_log::answers_in(&answers);
                    queries.note(total, &asked, clients, answering, now);
                    asked
                }
                None => Vec::new(),
            };
            // The parse also gives answers where the fan only logs the queries.
            if !fan.answering || !fan.subscribers.is_empty() {
                answers.clear();
            } else if !answers.is_empty() {
                fan.replies.admit(ReplyRoute::DAEMON, total, total, now);
                if let Some(queries) = fan.queries.as_mut() {
                    queries.reply(ReplyRoute::DAEMON, total, &asked, true, now);
                }
            }
            let targets = fan
                .subscribers
                .iter()
                .map(|sub| (sub.id, sub.tx.clone()))
                .collect();
            (targets, fan.total, titles, answers)
        };

        before_delivery(total, titles);

        let mut dropped = Vec::new();
        for (id, tx) in targets {
            if !send_with_backpressure(&tx, chunk.clone()) {
                dropped.push(id);
            }
        }
        if !dropped.is_empty() {
            self.lock()
                .subscribers
                .retain(|sub| !dropped.contains(&sub.id));
        }
        answers
    }
}

/// Hands one chunk to one client, waiting out a full queue up to `SUBSCRIBER_GRACE`. Returns
/// `false` when the client is gone or has stopped draining, meaning it should be dropped.
fn send_with_backpressure(tx: &mpsc::Sender<Bytes>, chunk: Bytes) -> bool {
    let deadline = Instant::now() + SUBSCRIBER_GRACE;
    let mut pending = chunk;
    loop {
        match tx.try_send(pending) {
            Ok(()) => return true,
            Err(mpsc::error::TrySendError::Closed(_)) => return false,
            Err(mpsc::error::TrySendError::Full(chunk)) => {
                if Instant::now() >= deadline {
                    return false;
                }
                pending = chunk;
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{send_with_backpressure, Fan, SUBSCRIBER_GRACE};
    use crate::terminal::reply_route::ReplayMark;
    use bytes::Bytes;
    use tokio::sync::mpsc;

    fn push(fan: &Fan, data: &[u8]) -> Vec<u8> {
        fan.push(data, |_, _| {})
    }

    #[test]
    fn the_daemon_answers_the_device_attributes_and_cursor_position_with_nobody_attached() {
        let fan = Fan::new(24, 80, true);
        assert_eq!(push(&fan, b"\x1b[c"), b"\x1b[?6c");
        assert_eq!(push(&fan, b"\x1b[0c"), b"\x1b[?6c");
        assert_eq!(push(&fan, b"ab\x1b[6n"), b"\x1b[1;3R");
        assert_eq!(
            push(&fan, b"\x1b[>c\x1b[5n\x1b]11;?\x07"),
            b"",
            "no other query is answered"
        );
    }

    #[test]
    fn nothing_is_answered_while_a_client_is_attached_or_where_the_daemon_does_not_answer() {
        let fan = Fan::new(24, 80, true);
        let attached = fan.attach();
        assert_eq!(push(&fan, b"\x1b[c\x1b[6n"), b"");
        drop(attached);

        let fan = Fan::new(24, 80, false);
        assert_eq!(push(&fan, b"\x1b[c\x1b[6n"), b"");
    }

    #[test]
    fn logging_the_queries_answers_none_the_daemon_does_not_answer() {
        let fan = Fan::new(24, 80, false);
        fan.log_queries("t".into());
        assert_eq!(push(&fan, b"\x1b[c\x1b[6n"), b"");
        let answering = fan.attach().answering.expect("a live client");
        assert_eq!(push(&fan, b"\x1b[6n"), b"");
        assert!(fan.admit_reply(answering.client, 11, b"\x1b[1;1R"));

        let fan = Fan::new(24, 80, true);
        fan.log_queries("t".into());
        assert_eq!(push(&fan, b"\x1b[c\x1b[6n"), b"\x1b[?6c\x1b[1;1R");
    }

    #[test]
    fn a_client_attaching_after_the_daemons_answer_is_told_it_is_settled_and_cannot_repeat_it() {
        let fan = Fan::new(24, 80, true);
        assert!(!push(&fan, b"\x1b[c").is_empty());
        let answering = fan.attach().answering.expect("a live client");
        assert_eq!(
            answering.mark,
            ReplayMark {
                offset: 3,
                answered: 3
            }
        );
        assert!(!fan.admit_reply(answering.client, 3, b"\x1b[?1;2c"));

        assert!(
            push(&fan, b"\x1b[6n").is_empty(),
            "the client attached is the one to answer from now on"
        );
        assert!(fan.admit_reply(answering.client, 7, b"\x1b[1;1R"));
    }

    #[test]
    fn a_client_attaching_while_a_query_from_an_attached_clients_time_is_unanswered_can_answer_it()
    {
        let fan = Fan::new(24, 80, true);
        let first = fan.attach();
        assert!(push(&fan, b"\x1b[c").is_empty());
        // The client leaves without having answered; its stream is dropped at the next send.
        drop(first);
        assert!(push(&fan, b"x").is_empty());

        let answering = fan.attach().answering.expect("a live client");
        assert_eq!(
            answering.mark,
            ReplayMark {
                offset: 4,
                answered: 0
            }
        );
        assert!(fan.admit_reply(answering.client, 4, b"\x1b[?1;2c"));
    }

    #[test]
    fn a_client_that_keeps_draining_is_never_dropped() {
        let (tx, mut rx) = mpsc::channel(1);
        assert!(send_with_backpressure(&tx, Bytes::from_static(b"first")));
        let drainer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            rx.blocking_recv();
            rx
        });
        // The queue is full until the drainer takes the first chunk, so this call has to wait
        // rather than give up — waiting is what pushes backpressure onto the agent.
        assert!(send_with_backpressure(&tx, Bytes::from_static(b"second")));
        drainer.join().expect("the drainer thread");
    }

    #[test]
    fn a_client_that_stops_draining_is_dropped_after_the_grace_period() {
        // Held open but never read: the case of a client that is gone or wedged.
        let (tx, _rx) = mpsc::channel(1);
        assert!(send_with_backpressure(&tx, Bytes::from_static(b"first")));

        let started = Instant::now();
        assert!(!send_with_backpressure(&tx, Bytes::from_static(b"second")));
        assert!(
            started.elapsed() >= SUBSCRIBER_GRACE,
            "it must wait out the whole grace period before giving up on a client"
        );
    }
}
