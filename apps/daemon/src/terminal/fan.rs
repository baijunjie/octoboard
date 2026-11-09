//! The output side of a process on a PTY: the replay ring, the parsed screen, and the clients
//! attached to follow the output, behind one lock.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::Bytes;
use tokio::sync::Notify;

use crate::terminal::activity::Activity;
use crate::terminal::display::DisplayFacts;
use crate::terminal::program_status::ReportedState;
use crate::terminal::queries::Query;
use crate::terminal::query_log::{self, QueryLog};
use crate::terminal::reply_route::ReplyRoute;
use crate::terminal::ringbuf::RingBuffer;
use crate::terminal::saved_screen::{ReplayRule, SavedScreen};
use crate::terminal::snapshot::{TakenIn, TerminalSnapshot};
use crate::terminal::title::TitleChange;

/// Terminal replay buffer per process.
pub const RING_CAPACITY: usize = 2 * 1024 * 1024;

/// How many bytes of output a client may have been sent and not yet acknowledged before nothing
/// more is sent to it. A client acknowledges what its terminal has consumed, so this bounds what
/// waits unparsed in the client, and on the way to it, whatever the link.
pub const CREDIT_WINDOW: usize = 512 * 1024;

/// How many bytes of output may wait unsent for one client before they are discarded and replaced
/// with a fresh paint of the screen: past this a client is so far behind that catching up byte by
/// byte would only show it what is no longer on screen. The reader never waits on a client, so
/// one slow client neither throttles the program nor holds up the other clients.
pub const BACKLOG_CAP: usize = 4 * 1024 * 1024;

/// What is sent to a client following live output, in order.
#[derive(Debug, Clone)]
pub enum Outgoing {
    /// Live output.
    Output(Bytes),
    /// A fresh paint of the screen, without history, in place of output the client had fallen too
    /// far behind on ([`BACKLOG_CAP`]): `offset` is where it leaves the output, as the replay mark
    /// of an attach says.
    Paint { offset: u64, paint: Bytes },
}

impl Outgoing {
    /// The bytes it sends.
    pub fn size(&self) -> usize {
        match self {
            Self::Output(bytes) | Self::Paint { paint: bytes, .. } => bytes.len(),
        }
    }
}

/// What [`Fan::take_output`] has for a client.
#[derive(Debug)]
pub enum Delivery {
    /// To send now, in order.
    Send(Vec<Outgoing>),
    /// Nothing to send until the client is woken: no output is waiting, or its credit is used up.
    Wait,
    /// The stream is over: the output ended and everything for the client has been taken, or the
    /// client has left.
    Over,
}

struct Subscriber {
    id: u64,
    /// The count of bytes of output handed to the clients when the client attached: a chunk
    /// ending at or before it is already in its first paint.
    from: u64,
    /// Output waiting to be sent to the client, oldest first, and its size in bytes.
    backlog: VecDeque<Outgoing>,
    backlog_bytes: usize,
    /// Bytes sent to the client and not yet acknowledged, its first paint included.
    unacknowledged: usize,
    /// Whether the client has acknowledged anything yet: until it does, the credit does not hold
    /// it back, so a client that predates the acknowledgements is not stalled by them.
    acknowledging: bool,
    /// Wakes the client's connection when there may be something for it to take.
    wake: Arc<Notify>,
}

/// Raw replay, parsed screen and subscribers share one lock: a client's first paint is taken and
/// the client subscribed at the same point output is appended, so attaching has no output gap or
/// duplicate, and freezing captures one exact boundary before exit cleanup can change the screen.
struct State {
    ring: RingBuffer,
    terminal: TerminalSnapshot,
    /// Every byte ever pushed into `ring`, which it forgets and this does not: what lets a caller
    /// mark a point in the output and later read exactly what came after it.
    total: u64,
    /// Every byte ever handed to the clients: the output without the queries the daemon answered
    /// and without a sequence held back until its end arrives. Points in the output the clients
    /// name (the replay mark, a reply's offset) count these.
    delivered: u64,
    subscribers: Vec<Subscriber>,
    next_id: u64,
    /// No client follows the output any more; see [`Fan::close`].
    closed: bool,
    /// Which clients' answers to the queries passed through to them are written. Beside the
    /// output, since an answer is admitted against how far the output has come.
    replies: ReplyRoute,
    /// Which attached client was active most recently.
    activity: Activity,
    /// The size, as rows and columns, each attached client last asked for. The most recently
    /// active one's is the terminal's size; see [`Fan::request_size`].
    sizes: HashMap<u64, (u16, u16)>,
    /// The size the PTY and the parsed screen have, as rows and columns.
    size: (u16, u16),
    /// The display each attached client last reported. The most recently active one's is what
    /// the program's colour and size queries are answered with.
    displays: HashMap<u64, DisplayFacts>,
    /// Where the device attributes and cursor position queries are logged; see
    /// [`Fan::log_queries`].
    queries: Option<QueryLog>,
    /// The moves of the program's root status record not yet taken; see
    /// [`Fan::take_program_status`].
    program_status: Vec<Option<ReportedState>>,
    /// The tail being replayed after a handover, while it lasts; see [`Fan::begin_replay`].
    replay: Option<Replay>,
}

/// A tail replayed after a handover, and how much of it has been pushed.
struct Replay {
    rule: ReplayRule,
    pushed: u64,
}

/// A terminal client registered by [`Fan::attach`].
pub struct Attached {
    /// What the client is sent before the live output: the first paint of a live terminal, the
    /// frozen screen, or the paint of a terminal whose output is over.
    pub opening: Vec<u8>,
    /// For a client following live output, what wakes its connection to [`Fan::take_output`].
    /// `None` when nothing follows.
    pub live: Option<Arc<Notify>>,
    /// For a client following live output: its id among the clients answering the program's
    /// queries, and where its first paint leaves the output. `None` when nothing follows.
    pub answering: Option<Answering>,
}

/// A live client's part in answering the program's queries.
pub struct Answering {
    pub client: u64,
    /// The count of bytes of output handed to the clients when the first paint was taken, which
    /// live output counts on from. The paint asks nothing, so the client answers only what
    /// follows.
    pub offset: u64,
    /// When the precedence attaching gave the client lapses, unless it is used before: the one
    /// deadline [`Fan::settle_activity`] is to be called at.
    pub precedence_lapses: Instant,
}

pub struct Fan(Mutex<State>);

impl Fan {
    /// A fan for a terminal of `rows` by `cols`, whose [`Self::push`] answers the program's
    /// queries; the device attributes, status and cursor reports pass through to the clients
    /// where `host_answers_queries` (`TermFacts::host_answers_queries`).
    pub fn new(rows: u16, cols: u16, host_answers_queries: bool) -> Self {
        let mut terminal = TerminalSnapshot::new(rows, cols);
        terminal.answer_queries(host_answers_queries);
        Self(Mutex::new(State {
            ring: RingBuffer::new(RING_CAPACITY),
            terminal,
            total: 0,
            delivered: 0,
            subscribers: Vec::new(),
            next_id: 0,
            closed: false,
            replies: ReplyRoute::default(),
            activity: Activity::default(),
            sizes: HashMap::new(),
            size: (rows, cols),
            displays: HashMap::new(),
            queries: None,
            program_status: Vec::new(),
            replay: None,
        }))
    }

    /// Keeps the program's status reports (OSC 7501) from now on and answers the protocol's
    /// support query; see [`TerminalSnapshot::keep_program_status`].
    pub fn keep_program_status(&self) {
        self.lock().terminal.keep_program_status();
    }

    /// Forgets the program's `working` and `blocked` status records once its process has ended;
    /// see [`TerminalSnapshot::end_program_status`].
    pub fn end_program_status(&self) {
        self.lock().terminal.end_program_status();
    }

    /// The states the program's root status record moved to since the last call, in order;
    /// `None` where it went. See [`Self::keep_program_status`].
    pub fn take_program_status(&self) -> Vec<Option<ReportedState>> {
        std::mem::take(&mut self.lock().program_status)
    }

    /// Logs the program's device attributes and cursor position queries from now on, under
    /// `label`, with who answered each and how long it took; see [`QueryLog`].
    pub fn log_queries(&self, label: String) {
        self.lock().queries = Some(QueryLog::new(label));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().expect("fan mutex poisoned")
    }

    /// Registers a terminal client. A live terminal's client is given its first paint, with up to
    /// `history` rows of scrollback (see [`TerminalSnapshot::paint`]), and the receiver picks up
    /// with the very next byte of output after it, taken with [`Self::take_output`] once `live`
    /// wakes. Once frozen, returns the archived screen alone, so exit cleanup cannot replace it;
    /// once the output is closed, the paint of the last screen alone.
    pub fn attach(&self, history: usize) -> Attached {
        let mut fan = self.lock();
        if let Some(output) = fan.terminal.frozen() {
            let opening = output.to_vec();
            return Attached {
                opening,
                live: None,
                answering: None,
            };
        }
        if fan.closed {
            // Painted from the parsed screen as a live attach is, so it shows the last screen
            // whatever the client's width, with the scrollback asked for, rather than replaying
            // raw output laid out for the width the terminal had.
            return Attached {
                opening: fan.terminal.paint(history),
                live: None,
                answering: None,
            };
        }
        let opening = fan.terminal.paint(history);
        let client = fan.next_id;
        fan.next_id += 1;
        let wake = Arc::new(Notify::new());
        let from = fan.delivered;
        fan.subscribers.push(Subscriber {
            id: client,
            from,
            backlog: VecDeque::new(),
            backlog_bytes: 0,
            unacknowledged: opening.len(),
            acknowledging: false,
            wake: wake.clone(),
        });
        // Attaching is activity, though it changes neither the display nor the size: the client
        // has reported neither yet. The size it asks for once its paint has been sent is applied
        // then, being the most recently active client's.
        let precedence_lapses = fan.activity.attach(client, Instant::now());
        Attached {
            opening,
            live: Some(wake),
            answering: Some(Answering {
                client,
                offset: from,
                precedence_lapses,
            }),
        }
    }

    /// What there is to send to `client` now: the output waiting for it, as much as its credit
    /// allows ([`CREDIT_WINDOW`]) once it has acknowledged anything, all of it before; a frame is
    /// sent whole once any credit is left, so a client may hold up to one frame more than the
    /// window. Once the output is closed, everything still waiting, whatever the credit, so a
    /// finishing terminal's last output reaches the client before its stream ends, and then
    /// [`Delivery::Over`].
    pub fn take_output(&self, client: u64) -> Delivery {
        let mut fan = self.lock();
        let closed = fan.closed;
        let Some(index) = fan.subscribers.iter().position(|sub| sub.id == client) else {
            return Delivery::Over;
        };
        if closed {
            let sub = fan.subscribers.remove(index);
            return if sub.backlog.is_empty() {
                Delivery::Over
            } else {
                Delivery::Send(sub.backlog.into())
            };
        }
        let sub = &mut fan.subscribers[index];
        let mut frames = Vec::new();
        while !sub.acknowledging || sub.unacknowledged < CREDIT_WINDOW {
            let Some(frame) = sub.backlog.pop_front() else {
                break;
            };
            sub.backlog_bytes -= frame.size();
            sub.unacknowledged += frame.size();
            frames.push(frame);
        }
        if frames.is_empty() {
            Delivery::Wait
        } else {
            Delivery::Send(frames)
        }
    }

    /// Notes that `client` has consumed `bytes` more of what it was sent, which gives it that
    /// much credit back.
    pub fn acknowledge(&self, client: u64, bytes: u64) {
        let mut fan = self.lock();
        if let Some(sub) = fan.subscribers.iter_mut().find(|sub| sub.id == client) {
            let bytes = usize::try_from(bytes).unwrap_or(usize::MAX);
            sub.unacknowledged = sub.unacknowledged.saturating_sub(bytes);
            sub.acknowledging = true;
            sub.wake.notify_one();
        }
    }

    /// Notes that `client` was used (it sent input, or the person turned to it), which makes it
    /// the most recently active client: its size and display are applied, as
    /// [`Self::settle_activity`] does. Returns what to write to the program, as
    /// [`Self::report_display`] does.
    #[must_use = "what is returned is the program's to read, and only the caller can write it"]
    pub fn activate(
        &self,
        client: u64,
        resize_pty: impl FnOnce(u16, u16) -> anyhow::Result<()>,
    ) -> Vec<u8> {
        let mut fan = self.lock();
        if fan.closed || !fan.subscribers.iter().any(|sub| sub.id == client) {
            return Vec::new();
        }
        // Applied even when the client was the most recently active one already: another
        // client's attach may have lapsed since, which nothing has applied yet. Nothing that is
        // already so is changed again.
        fan.activity.activate(client);
        fan.reconcile(Instant::now(), resize_pty)
    }

    /// Records the size, `rows` by `cols`, that `client` asks for, and applies it with
    /// `resize_pty` while it is the most recently active client that has asked for one; a size
    /// asked for by any other client waits until that client is the most recently active. The
    /// display is brought up to date with it, as [`Self::settle_activity`] does, both ranked at the
    /// same moment. Returns what to write to the program, as
    /// [`Self::report_display`] does. Refused once the screen is frozen.
    #[must_use = "what is returned is the program's to read, and only the caller can write it"]
    pub fn request_size(
        &self,
        client: u64,
        rows: u16,
        cols: u16,
        resize_pty: impl FnOnce(u16, u16) -> anyhow::Result<()>,
    ) -> anyhow::Result<Vec<u8>> {
        let mut fan = self.lock();
        anyhow::ensure!(
            fan.terminal.frozen().is_none(),
            "the terminal's screen has been frozen"
        );
        if fan.closed || !fan.subscribers.iter().any(|sub| sub.id == client) {
            return Ok(Vec::new());
        }
        fan.sizes.insert(client, (rows, cols));
        Ok(fan.reconcile(Instant::now(), resize_pty))
    }

    /// Applies what follows the most recently active client at `now` again, together under the
    /// lock, once an attach's precedence may have lapsed ([`Answering::precedence_lapses`]): the
    /// size with `resize_pty`, and the display. Returns what to write to the program, as
    /// [`Self::report_display`] does.
    #[must_use = "what is returned is the program's to read, and only the caller can write it"]
    pub fn settle_activity(
        &self,
        now: Instant,
        resize_pty: impl FnOnce(u16, u16) -> anyhow::Result<()>,
    ) -> Vec<u8> {
        let mut fan = self.lock();
        if fan.closed {
            return Vec::new();
        }
        fan.reconcile(now, resize_pty)
    }

    /// Records the display `client` reports, which the program's colour and size queries are
    /// answered with while it is the most recently active client that has reported one. Returns
    /// what to write to the program: that the colour scheme changed, where the colours the queries
    /// are answered with did and the program subscribed to such changes.
    #[must_use = "what is returned is the program's to read, and only the caller can write it"]
    pub fn report_display(&self, client: u64, display: DisplayFacts) -> Vec<u8> {
        let mut fan = self.lock();
        if fan.closed || !fan.subscribers.iter().any(|sub| sub.id == client) {
            return Vec::new();
        }
        fan.displays.insert(client, display);
        fan.apply_display(Instant::now())
    }

    /// Forgets `client`, whose connection has ended: it no longer follows the output, and
    /// neither its size nor its display counts any more. The size of the client that is the most
    /// recently active one now is applied with `resize_pty`, as [`Self::request_size`] does.
    /// Returns what to write to the program, as [`Self::report_display`] does.
    #[must_use = "what is returned is the program's to read, and only the caller can write it"]
    pub fn leave(
        &self,
        client: u64,
        resize_pty: impl FnOnce(u16, u16) -> anyhow::Result<()>,
    ) -> Vec<u8> {
        let mut fan = self.lock();
        fan.subscribers.retain(|sub| sub.id != client);
        fan.activity.forget(client);
        if fan.closed {
            return Vec::new();
        }
        fan.sizes.remove(&client);
        fan.displays.remove(&client);
        fan.reconcile(Instant::now(), resize_pty)
    }

    /// Whether this client's answer to the output up to `offset`, `reply`, is the one written; see
    /// [`ReplyRoute::admit`].
    pub fn admit_reply(&self, client: u64, offset: u64, reply: &[u8]) -> bool {
        let mut fan = self.lock();
        let total = fan.delivered;
        let now = Instant::now();
        let admitted = fan.replies.admit(client, offset, total, now);
        if let Some(queries) = fan.queries.as_mut() {
            queries.reply(client, offset, &query_log::answers_in(reply), admitted, now);
        }
        admitted
    }

    /// Whether a client owes the program the answer to a query the parsed terminal left to the
    /// clients; see [`ReplyRoute::owed`].
    pub fn replies_owed(&self) -> bool {
        self.lock().replies.owed(Instant::now())
    }

    /// Ends every attached client's stream once the output it follows is over: what is waiting for
    /// a client is still taken ([`Self::take_output`]), and then its stream is over. Makes every
    /// later [`Self::attach`] a paint alone. Output that still arrives reaches the parsed screen
    /// and the ring alone.
    pub fn close(&self) {
        let mut fan = self.lock();
        fan.closed = true;
        for sub in &fan.subscribers {
            sub.wake.notify_one();
        }
        fan.activity = Activity::default();
        fan.sizes.clear();
        fan.displays.clear();
    }

    /// The tail of the output, at most `max_bytes` of it, from the replay ring.
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

    /// The size, as rows and columns, the parsed screen was last given: the one the rule last
    /// applied ([`Self::request_size`]), or the size the terminal started at until it applied one.
    pub fn screen_size(&self) -> (u16, u16) {
        self.lock().size
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
    /// under the lock so that no chunk is parsed between the two, whichever client is the most
    /// recently active and without counting as any client's activity or changing the display: a
    /// test's resize, a repaint nudge that ends at the size it started from
    /// (`PtyCore::nudge_repaint`), or the size of a saved screen restored ([`Self::restore`]).
    /// Refused once the screen is frozen.
    pub fn resize(
        &self,
        rows: u16,
        cols: u16,
        resize_pty: impl FnOnce(u16, u16) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut fan = self.lock();
        anyhow::ensure!(
            fan.terminal.frozen().is_none(),
            "the terminal's screen has been frozen"
        );
        fan.resize_screen(rows, cols, resize_pty)
    }

    /// Takes output that is history into the replay and the parsed screen as plain output, as
    /// [`Self::push`] does, `chunk_size` bytes at a time: what it answers is dropped, and so are
    /// the moves of the program's status records and the title changes it makes. It reaches no
    /// client, since none is attached yet.
    pub fn seed(&self, output: &[u8], chunk_size: usize) {
        for chunk in output.chunks(chunk_size) {
            let _answers = self.push(chunk, |_, _| {});
        }
        self.take_program_status();
    }

    /// Takes a saved screen (see [`SavedScreen`]) in: the parsed screen is given its size, `rows`
    /// by `cols`, and then `bytes`, as [`Self::seed`] takes output in. The PTY is not resized:
    /// the caller has made sure it has that size.
    pub fn restore(&self, rows: u16, cols: u16, bytes: &[u8], chunk_size: usize) {
        if let Err(err) = self.resize(rows, cols, |_, _| Ok(())) {
            tracing::warn!(%err, "giving the parsed screen the saved screen's size failed");
        }
        self.seed(bytes, chunk_size);
    }

    /// Sets whether the parsed screen has shown something since titles were reported, for a
    /// restored one; see [`TerminalSnapshot::set_drawn`].
    pub fn set_drawn(&self, drawn: bool) {
        self.lock().terminal.set_drawn(drawn);
    }

    /// The parsed screen as the bytes that rebuild it; see [`TerminalSnapshot::save`].
    pub fn save_screen(&self) -> Option<SavedScreen> {
        self.lock().terminal.save()
    }

    /// Has [`Self::push`] take the next `rule.len` bytes in as a replayed tail: within it a query
    /// is answered, or passed to the clients to answer, only where the rule allows, and dropped
    /// otherwise; see [`ReplayRule`]. Everything else is taken in as live output. Called before the
    /// reader starts, once whatever is restored is in, so that the tail is the first output
    /// pushed.
    pub fn begin_replay(&self, rule: ReplayRule) {
        self.lock().replay = (rule.len > 0).then_some(Replay { rule, pushed: 0 });
    }

    /// Takes one chunk read from the PTY into the replay, the parsed screen and every attached
    /// client. `before_delivery` is told the new total and the title changes the chunk made on the
    /// parsed screen (none unless [`Self::report_titles`] asked for them) once the chunk is
    /// buffered and before any client is handed it, and runs outside the lock.
    ///
    /// Never waits on a client: the chunk joins each client's backlog, which its connection sends
    /// as the client's credit allows ([`Self::take_output`]). A client whose backlog grows past
    /// [`BACKLOG_CAP`] has it replaced with a fresh paint of the screen.
    ///
    /// Returns the answers to write into the program's input: those to the queries in this chunk
    /// the parsed terminal answers ([`TerminalSnapshot::take_in`]), whether any client is attached
    /// or not. The clients are handed the chunk without them, so no client answers them again; a
    /// query the parsed terminal cannot answer reaches the clients for one of them to answer.
    /// Within a replayed tail ([`Self::begin_replay`]), a query its rule does not allow does
    /// neither.
    #[must_use = "the answers are the program's to read, and only the caller can write them"]
    pub fn push(
        &self,
        data: &[u8],
        before_delivery: impl FnOnce(u64, Vec<TitleChange>),
    ) -> Vec<u8> {
        let (total, titles, delivered, end, answers) = {
            let mut fan = self.lock();
            fan.ring.push(data);
            fan.total += data.len() as u64;
            let now = Instant::now();
            let mut taken = fan.take_in(data, now);
            fan.program_status.append(&mut taken.program_status);
            let titles = fan.terminal.take_titles();
            if !taken.delivered.is_empty() {
                fan.delivered += taken.delivered.len() as u64;
                let end = fan.delivered;
                fan.replies.note_output(end, now);
                // A query the parsed terminal left to the clients is owed by one of them, while
                // any is attached to answer it.
                let left_to_clients = taken.asked.iter().any(|(_, by_daemon)| !by_daemon);
                if left_to_clients && !fan.subscribers.is_empty() {
                    fan.replies.owe(end);
                }
            }
            let (clients, end) = (fan.subscribers.len(), fan.delivered);
            if let Some(queries) = fan.queries.as_mut() {
                let (mut asked, mut answered) = (Vec::new(), Vec::new());
                for (query, by_daemon) in &taken.asked {
                    if let Some(query) = query_log::Query::of(query) {
                        asked.push(query);
                        if *by_daemon {
                            answered.push(query);
                        }
                    }
                }
                queries.note(end, &asked, clients, now);
                if !answered.is_empty() {
                    queries.reply(query_log::DAEMON, end, &answered, true, now);
                }
            }
            (
                fan.total,
                titles,
                Bytes::from(taken.delivered),
                end,
                taken.answers,
            )
        };

        before_delivery(total, titles);

        if !delivered.is_empty() {
            self.lock().deliver(&delivered, end);
        }
        answers
    }
}

impl State {
    /// Takes `data` into the parsed terminal, which answers the queries in it, under the rule of
    /// a replayed tail for the part of it that is still within one ([`Fan::begin_replay`]): the
    /// queries there are judged, at `now`, by how old the output was where the stretch between two
    /// known ages that the query's last byte is in starts.
    fn take_in(&mut self, data: &[u8], now: Instant) -> TakenIn {
        let Some(replay) = self.replay.as_mut() else {
            return self.terminal.take_in(data);
        };
        let start = replay.pushed;
        let left = replay.rule.len - start;
        let within = data.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        let end = start + within as u64;
        let mut cuts: Vec<u64> = replay
            .rule
            .ages
            .iter()
            .map(|(from, _)| *from)
            .filter(|from| *from > start && *from < end)
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        let segments: Vec<(u64, bool)> = std::iter::once(start)
            .chain(cuts)
            .map(|from| (from, replay.rule.fresh(from, now)))
            .collect();
        let skip_device_attributes = replay.rule.skip_device_attributes;
        replay.pushed = end;
        if end >= replay.rule.len {
            self.replay = None;
        }
        let mut taken = TakenIn::default();
        for (at, &(from, fresh)) in segments.iter().enumerate() {
            let to = segments.get(at + 1).map_or(end, |next| next.0);
            let part = &data[(from - start) as usize..(to - start) as usize];
            taken.append(self.terminal.take_in_allowing(part, |query| {
                fresh && !(skip_device_attributes && *query == Query::DeviceAttributes)
            }));
        }
        if within < data.len() {
            taken.append(self.terminal.take_in(&data[within..]));
        }
        taken
    }

    /// Adds `chunk`, the output that ends at `end`, to the backlog of every client that attached
    /// before that point (a client that attached since has it in its first paint already), and
    /// replaces a backlog grown past [`BACKLOG_CAP`] with a fresh paint, taken now: the chunk has
    /// been parsed, so the paint leaves the output where the backlog would have.
    fn deliver(&mut self, chunk: &Bytes, end: u64) {
        if self.closed {
            return;
        }
        let offset = self.delivered;
        let Self {
            subscribers,
            terminal,
            ..
        } = self;
        let mut paint: Option<Bytes> = None;
        for sub in subscribers.iter_mut().filter(|sub| sub.from < end) {
            sub.backlog_bytes += chunk.len();
            sub.backlog.push_back(Outgoing::Output(chunk.clone()));
            if sub.backlog_bytes > BACKLOG_CAP {
                let paint = paint
                    .get_or_insert_with(|| Bytes::from(terminal.paint(0)))
                    .clone();
                sub.backlog.clear();
                sub.backlog_bytes = paint.len();
                sub.backlog.push_back(Outgoing::Paint { offset, paint });
            }
            sub.wake.notify_one();
        }
    }

    /// Has the parsed terminal answer with the display of the most recently active client at
    /// `now` that has reported one, or the default display while none has. Returns what to tell
    /// the program; see [`TerminalSnapshot::set_display`].
    fn apply_display(&mut self, now: Instant) -> Vec<u8> {
        let displays = &self.displays;
        let display = self
            .activity
            .most_recent_of(|client| displays.contains_key(&client), now)
            .and_then(|client| displays.get(&client).copied())
            .unwrap_or_default();
        self.terminal.set_display(display)
    }

    /// Brings what follows the most recently active client at `now` up to date, both ranked at the
    /// same moment: the size with `resize_pty`, then the display, each among the clients that have
    /// reported one.
    /// A failed resize is only logged: what moved the size is a client's activity, not a request
    /// the client could be refused. Returns what to tell the program, as [`Self::apply_display`]
    /// does.
    fn reconcile(
        &mut self,
        now: Instant,
        resize_pty: impl FnOnce(u16, u16) -> anyhow::Result<()>,
    ) -> Vec<u8> {
        if self.terminal.frozen().is_none() {
            if let Err(err) = self.resize_to_most_recent(now, resize_pty) {
                tracing::debug!(
                    %err,
                    "resizing the terminal for the most recently active client failed"
                );
            }
        }
        self.apply_display(now)
    }

    /// Resizes the terminal with `resize_pty` to the size the most recently active client at
    /// `now` that has asked for one asked for, unless it has that size already. While no attached
    /// client has asked for a size, the terminal keeps the one it has.
    fn resize_to_most_recent(
        &mut self,
        now: Instant,
        resize_pty: impl FnOnce(u16, u16) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let sizes = &self.sizes;
        let Some((rows, cols)) = self
            .activity
            .most_recent_of(|client| sizes.contains_key(&client), now)
            .and_then(|client| sizes.get(&client).copied())
        else {
            return Ok(());
        };
        if (rows, cols) == self.size {
            return Ok(());
        }
        self.resize_screen(rows, cols, resize_pty)
    }

    /// Resizes the PTY with `resize_pty`, then the parsed screen, so that no chunk is parsed
    /// between the two: the caller holds the fan's lock.
    fn resize_screen(
        &mut self,
        rows: u16,
        cols: u16,
        resize_pty: impl FnOnce(u16, u16) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        resize_pty(rows, cols)?;
        self.terminal.resize(rows, cols);
        self.size = (rows, cols);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use super::{Attached, Delivery, Fan, Outgoing, BACKLOG_CAP, CREDIT_WINDOW, RING_CAPACITY};
    use crate::terminal::activity::ATTACH_CLAIM;
    use crate::terminal::display::DisplayFacts;
    use crate::terminal::saved_screen::ReplayRule;
    use crate::terminal::snapshot::assert_same_state;
    use crate::terminal::snapshot::TerminalSnapshot;
    use crate::terminal::title::TitleChange;

    fn push(fan: &Fan, data: &[u8]) -> Vec<u8> {
        fan.push(data, |_, _| {})
    }

    /// A client that reads everything it is sent as soon as it looks, acknowledging it at once.
    struct Reader {
        client: u64,
        received: Vec<u8>,
        /// How many fresh paints replaced output it had fallen behind on.
        paints: usize,
    }

    impl Reader {
        fn new(fan: &Fan, attached: &Attached) -> Self {
            let client = client_of(attached);
            fan.acknowledge(client, attached.opening.len() as u64);
            Self {
                client,
                received: Vec::new(),
                paints: 0,
            }
        }

        /// Takes what is waiting for the client; `false` once its stream is over.
        fn read(&mut self, fan: &Fan) -> bool {
            loop {
                match fan.take_output(self.client) {
                    Delivery::Send(frames) => {
                        for frame in frames {
                            let len = frame.size() as u64;
                            match frame {
                                Outgoing::Output(bytes) => self.received.extend_from_slice(&bytes),
                                Outgoing::Paint { paint, .. } => {
                                    self.paints += 1;
                                    self.received.extend_from_slice(&paint);
                                }
                            }
                            fan.acknowledge(self.client, len);
                        }
                    }
                    Delivery::Wait => return true,
                    Delivery::Over => return false,
                }
            }
        }
    }

    /// What has reached a client so far.
    fn received(fan: &Fan, attached: &Attached) -> Vec<u8> {
        let mut reader = Reader::new(fan, attached);
        reader.read(fan);
        reader.received
    }

    /// A PTY resize for a test that does not follow the size.
    fn keep_size(_rows: u16, _cols: u16) -> anyhow::Result<()> {
        Ok(())
    }

    /// The live client's id among the attached clients.
    fn client_of(attached: &super::Attached) -> u64 {
        attached.answering.as_ref().expect("a live client").client
    }

    fn light() -> DisplayFacts {
        DisplayFacts::reported("#383a42", "#fafafa", "#101010", Some((9, 20))).expect("a display")
    }

    #[test]
    fn the_daemon_answers_with_nobody_attached() {
        let fan = Fan::new(24, 80, false);
        assert_eq!(push(&fan, b"\x1b[c"), b"\x1b[?6c");
        assert_eq!(push(&fan, b"ab\x1b[6n"), b"\x1b[1;3R");
        assert_eq!(push(&fan, b"\x1b[5n"), b"\x1b[0n");
        assert_eq!(
            push(&fan, b"\x1b]11;?\x07"),
            b"\x1b]11;rgb:1e1e/1f1f/2222\x07"
        );
        assert_eq!(
            fan.lock().delivered,
            2,
            "only the output that is not a query counts"
        );
    }

    #[test]
    fn with_clients_attached_each_query_is_answered_once_and_reaches_none_of_them() {
        let fan = Fan::new(24, 80, false);
        let first = fan.attach(0);
        let second = fan.attach(0);
        assert_eq!(
            push(&fan, b"x\x1b[6ny\x1b[c\x1b]10;?\x07"),
            b"\x1b[1;2R\x1b[?6c\x1b]10;rgb:e4e4/e4e4/e6e6\x07"
        );
        assert_eq!(received(&fan, &first), b"xy");
        assert_eq!(received(&fan, &second), b"xy");
        assert_eq!(push(&fan, b"\x1b[c"), b"\x1b[?6c");
        assert!(
            received(&fan, &first).is_empty(),
            "a chunk that was all queries reaches nobody"
        );
    }

    #[test]
    fn a_query_the_daemon_cannot_answer_reaches_the_clients_whose_answers_are_routed() {
        let fan = Fan::new(24, 80, false);
        assert!(!push(&fan, b"ab\x1b[6n").is_empty());
        let attached = fan.attach(0);
        let answering = attached.answering.as_ref().expect("a live client");
        assert_eq!(answering.offset, 2, "the answered query is not counted");
        let palette = b"\x1b]4;1;?\x07";
        assert!(push(&fan, palette).is_empty());
        assert_eq!(received(&fan, &attached), palette);
        let end = 2 + palette.len() as u64;
        assert!(fan.admit_reply(answering.client, end, b"\x1b]4;1;rgb:0000/0000/0000\x07"));
        assert!(
            !fan.admit_reply(answering.client, end + 1, b"x"),
            "a point past the delivered output"
        );
    }

    #[test]
    fn where_the_host_answers_the_reports_they_reach_the_clients() {
        let fan = Fan::new(24, 80, true);
        let attached = fan.attach(0);
        assert!(push(&fan, b"\x1b[c\x1b[6n").is_empty());
        assert_eq!(received(&fan, &attached), b"\x1b[c\x1b[6n");
        assert!(
            !push(&fan, b"\x1b]11;?\x07").is_empty(),
            "a colour is still answered"
        );
    }

    #[test]
    fn the_most_recently_active_client_with_a_display_decides_the_colours() {
        let fan = Fan::new(24, 80, false);
        let background = |fan: &Fan| push(fan, b"\x1b]11;?\x07");
        let (first_attached, second_attached) = (fan.attach(0), fan.attach(0));
        let first = first_attached
            .answering
            .as_ref()
            .expect("a live client")
            .client;
        let second = second_attached
            .answering
            .as_ref()
            .expect("a live client")
            .client;
        assert_eq!(
            background(&fan),
            b"\x1b]11;rgb:1e1e/1f1f/2222\x07",
            "the default"
        );

        assert!(fan.report_display(first, light()).is_empty());
        assert_eq!(
            background(&fan),
            b"\x1b]11;rgb:fafa/fafa/fafa\x07",
            "the second client is more recent but has reported nothing"
        );
        let dark = DisplayFacts::reported("#ffffff", "#000000", "#ffffff", None).unwrap();
        assert!(fan.report_display(second, dark).is_empty());
        assert_eq!(background(&fan), b"\x1b]11;rgb:0000/0000/0000\x07");

        assert!(fan.activate(first, keep_size).is_empty());
        assert_eq!(background(&fan), b"\x1b]11;rgb:fafa/fafa/fafa\x07");
        assert!(fan.leave(first, keep_size).is_empty());
        assert_eq!(background(&fan), b"\x1b]11;rgb:0000/0000/0000\x07");
        assert!(fan.leave(second, keep_size).is_empty());
        assert_eq!(background(&fan), b"\x1b]11;rgb:1e1e/1f1f/2222\x07");
        assert!(
            fan.report_display(second, dark).is_empty() && background(&fan).ends_with(b"2222\x07"),
            "a client that left reports nothing"
        );
    }

    #[test]
    fn a_program_subscribed_to_colour_scheme_updates_is_told_when_another_client_takes_over() {
        let fan = Fan::new(24, 80, false);
        let (first_attached, second_attached) = (fan.attach(0), fan.attach(0));
        let first = first_attached
            .answering
            .as_ref()
            .expect("a live client")
            .client;
        let second = second_attached
            .answering
            .as_ref()
            .expect("a live client")
            .client;
        assert!(push(&fan, b"\x1b[?2031h").is_empty());
        assert!(
            fan.report_display(first, DisplayFacts::default())
                .is_empty(),
            "the same colours"
        );
        assert_eq!(fan.report_display(second, light()), b"\x1b[?997;2n");
        assert_eq!(fan.activate(first, keep_size), b"\x1b[?997;1n");
        assert!(
            fan.activate(first, keep_size).is_empty(),
            "still the most recent"
        );
        assert_eq!(fan.leave(first, keep_size), b"\x1b[?997;2n");
    }

    #[test]
    fn the_most_recently_active_client_that_asked_for_a_size_decides_it() {
        let fan = Fan::new(24, 80, false);
        let resized = std::cell::RefCell::new(Vec::new());
        let resize = |rows: u16, cols: u16| {
            resized.borrow_mut().push((rows, cols));
            Ok(())
        };
        let taken = || std::mem::take(&mut *resized.borrow_mut());
        let (first_attached, second_attached) = (fan.attach(0), fan.attach(0));
        let (first, second) = (client_of(&first_attached), client_of(&second_attached));

        fan.request_size(first, 30, 100, resize).unwrap();
        assert_eq!(
            taken(),
            [(30, 100)],
            "the second client is more recent but has asked for no size"
        );
        fan.request_size(second, 24, 80, resize).unwrap();
        assert_eq!(taken(), [(24, 80)]);
        fan.request_size(first, 40, 120, resize).unwrap();
        assert!(taken().is_empty(), "kept until the first client is active");

        assert!(fan.activate(first, resize).is_empty());
        assert_eq!(taken(), [(40, 120)], "becoming active applies its size");
        assert!(fan.activate(first, resize).is_empty());
        assert!(taken().is_empty(), "the terminal has that size already");
        assert!(fan.activate(second, resize).is_empty());
        assert_eq!(taken(), [(24, 80)]);

        assert!(fan.leave(second, resize).is_empty());
        assert_eq!(
            taken(),
            [(40, 120)],
            "the client left is followed by the one before"
        );
        fan.request_size(second, 10, 20, resize).unwrap();
        assert!(
            !fan.lock().sizes.contains_key(&second),
            "a client that left leaves no size behind"
        );
        assert!(fan.leave(first, resize).is_empty());
        assert!(
            taken().is_empty(),
            "with nobody attached the terminal keeps its size"
        );
    }

    #[test]
    fn a_client_that_attaches_and_stays_quiet_stops_deciding_once_its_claim_lapses() {
        let fan = Fan::new(24, 80, false);
        let resized = std::cell::RefCell::new(Vec::new());
        let resize = |rows: u16, cols: u16| {
            resized.borrow_mut().push((rows, cols));
            Ok(())
        };
        let background = |fan: &Fan| push(fan, b"\x1b]11;?\x07");
        let first_attached = fan.attach(0);
        let first = client_of(&first_attached);
        fan.request_size(first, 40, 120, resize).unwrap();
        assert!(fan.report_display(first, light()).is_empty());
        assert!(fan.activate(first, resize).is_empty());

        let second_attached = fan.attach(0);
        let second = client_of(&second_attached);
        fan.request_size(second, 20, 50, resize).unwrap();
        let dark = DisplayFacts::reported("#ffffff", "#000000", "#ffffff", None).unwrap();
        assert!(fan.report_display(second, dark).is_empty());
        assert_eq!(background(&fan), b"\x1b]11;rgb:0000/0000/0000\x07");
        assert!(fan.settle_activity(Instant::now(), resize).is_empty());
        assert_eq!(*resized.borrow(), [(40, 120), (20, 50)], "not lapsed yet");

        assert!(fan
            .settle_activity(Instant::now() + ATTACH_CLAIM, resize)
            .is_empty());
        assert_eq!(
            *resized.borrow(),
            [(40, 120), (20, 50), (40, 120)],
            "the size goes back to the client that typed"
        );
        assert_eq!(
            background(&fan),
            b"\x1b]11;rgb:fafa/fafa/fafa\x07",
            "and so do the colours"
        );
    }

    #[test]
    fn turning_back_to_a_client_takes_its_size_and_colours_together_and_tells_the_program_once() {
        let fan = Fan::new(24, 80, false);
        let resized = std::cell::RefCell::new(Vec::new());
        let resize = |rows: u16, cols: u16| {
            resized.borrow_mut().push((rows, cols));
            Ok(())
        };
        assert!(push(&fan, b"\x1b[?2031h").is_empty());
        let first_attached = fan.attach(0);
        let first = client_of(&first_attached);
        assert!(fan.request_size(first, 40, 120, resize).unwrap().is_empty());
        assert_eq!(fan.report_display(first, light()), b"\x1b[?997;2n");
        let second_attached = fan.attach(0);
        let second = client_of(&second_attached);
        assert!(fan.request_size(second, 20, 50, resize).unwrap().is_empty());
        assert_eq!(
            fan.report_display(second, DisplayFacts::default()),
            b"\x1b[?997;1n"
        );
        assert!(fan.activate(second, resize).is_empty());
        assert_eq!(
            std::mem::take(&mut *resized.borrow_mut()),
            [(40, 120), (20, 50)]
        );

        // No input, and no output either: the person turned back to the first client.
        assert_eq!(fan.activate(first, resize), b"\x1b[?997;2n");
        assert_eq!(*resized.borrow(), [(40, 120)], "the size with the colours");
        assert!(fan.activate(first, resize).is_empty(), "told once");
        assert_eq!(*resized.borrow(), [(40, 120)], "resized once");
    }

    #[test]
    fn being_used_never_lapses_and_without_use_the_latest_attach_stays_the_fallback() {
        let fan = Fan::new(24, 80, false);
        let resized = std::cell::RefCell::new(Vec::new());
        let resize = |rows: u16, cols: u16| {
            resized.borrow_mut().push((rows, cols));
            Ok(())
        };
        let (first_attached, second_attached) = (fan.attach(0), fan.attach(0));
        let (first, second) = (client_of(&first_attached), client_of(&second_attached));
        fan.request_size(first, 40, 120, resize).unwrap();
        fan.request_size(second, 20, 50, resize).unwrap();
        assert_eq!(*resized.borrow(), [(40, 120), (20, 50)]);
        let much_later = Instant::now() + 10 * ATTACH_CLAIM;
        assert!(fan.settle_activity(much_later, resize).is_empty());
        assert_eq!(
            *resized.borrow(),
            [(40, 120), (20, 50)],
            "neither was used, so the latest attach keeps the size"
        );

        assert!(fan.activate(first, resize).is_empty());
        let third_attached = fan.attach(0);
        let third = client_of(&third_attached);
        fan.request_size(third, 10, 30, resize).unwrap();
        assert!(fan.settle_activity(much_later, resize).is_empty());
        assert_eq!(
            *resized.borrow(),
            [(40, 120), (20, 50), (40, 120), (10, 30), (40, 120)],
            "the activation outlasts the attach after it"
        );
    }

    #[test]
    fn the_precedence_of_an_attach_lapses_at_one_deadline_taken_when_it_attached() {
        let fan = Fan::new(24, 80, false);
        let before = Instant::now();
        let attached = fan.attach(0);
        let after = Instant::now();
        let lapses = attached
            .answering
            .as_ref()
            .expect("a live client")
            .precedence_lapses;
        assert!(before + ATTACH_CLAIM <= lapses && lapses <= after + ATTACH_CLAIM);
    }

    #[test]
    fn a_frozen_screen_refuses_sizes_and_keeps_its_own() {
        let fan = Fan::new(24, 80, false);
        let (first_attached, second_attached) = (fan.attach(0), fan.attach(0));
        let (first, second) = (client_of(&first_attached), client_of(&second_attached));
        fan.request_size(first, 30, 100, keep_size).unwrap();
        fan.request_size(second, 20, 50, keep_size).unwrap();
        assert!(fan.freeze());

        let resize =
            |_: u16, _: u16| -> anyhow::Result<()> { panic!("a frozen screen is resized") };
        let refused = fan
            .request_size(second, 40, 120, resize)
            .expect_err("a size asked for once the screen is frozen");
        assert_eq!(refused.to_string(), "the terminal's screen has been frozen");
        assert!(
            fan.activate(first, resize).is_empty(),
            "the first client's size is not applied"
        );
    }

    #[test]
    fn logging_the_queries_records_the_daemons_answers() {
        let fan = Fan::new(24, 80, false);
        fan.log_queries("t".into());
        assert_eq!(push(&fan, b"\x1b[c\x1b[6n"), b"\x1b[?6c\x1b[1;1R");
        assert!(fan
            .lock()
            .queries
            .as_ref()
            .expect("logging")
            .waiting()
            .is_empty());

        let fan = Fan::new(24, 80, true);
        fan.log_queries("t".into());
        let mut attached = fan.attach(0);
        let answering = attached.answering.take().expect("a live client");
        assert!(push(&fan, b"\x1b[6n").is_empty());
        assert_eq!(fan.lock().queries.as_ref().unwrap().waiting().len(), 1);
        assert!(fan.admit_reply(answering.client, 4, b"\x1b[1;1R"));
        assert!(fan.lock().queries.as_ref().unwrap().waiting().is_empty());
    }

    /// A query left to the clients is owed while one is attached to answer it, until one does;
    /// with none attached, nobody owes it.
    #[test]
    fn a_query_left_to_the_clients_is_owed_until_one_answers() {
        let fan = Fan::new(24, 80, true);
        assert!(push(&fan, b"\x1b[6n").is_empty());
        assert!(!fan.replies_owed(), "owed with no client attached");
        let mut attached = fan.attach(0);
        let answering = attached.answering.take().expect("a live client");
        assert!(push(&fan, b"\x1b[6n").is_empty());
        assert!(fan.replies_owed());
        assert!(fan.admit_reply(answering.client, 8, b"\x1b[1;1R"));
        assert!(!fan.replies_owed());
    }

    /// A styled line on the normal screen, with an application cursor mode change now and then.
    fn line(i: usize) -> Vec<u8> {
        let mode = if i.is_multiple_of(7) {
            "\x1b[?1h"
        } else {
            "\x1b[?1l"
        };
        let pad = "x".repeat(i % 90);
        format!("{mode}\x1b[3{}mline {i:07} {pad}\x1b[0m\r\n", i % 8).into_bytes()
    }

    /// Entering the alternate screen with mouse reporting, bracketed paste and a hidden cursor.
    const ENTER_ALTERNATE: &[u8] = b"\x1b[?1049h\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[?25l";
    const LEAVE_ALTERNATE: &[u8] = b"\x1b[?1049l\x1b[?1002l\x1b[?1006l\x1b[?2004l\x1b[?25h";

    /// A full-screen redraw on the alternate screen.
    fn frame(i: usize) -> Vec<u8> {
        let mut chunk = String::from("\x1b[H\x1b[2J");
        for row in 1..=24 {
            chunk.push_str(&format!(
                "\x1b[{row};3H\x1b[7;9{}mframe {i} row {row}\x1b[0m",
                row % 8
            ));
        }
        chunk.push_str("\x1b[12;10H");
        chunk.into_bytes()
    }

    /// Reads what the client is sent on a thread of its own until its stream is over.
    fn drain(fan: Arc<Fan>, attached: &Attached) -> std::thread::JoinHandle<Reader> {
        let mut reader = Reader::new(&fan, attached);
        std::thread::spawn(move || {
            while reader.read(&fan) {
                std::thread::sleep(std::time::Duration::from_micros(100));
            }
            reader
        })
    }

    fn client_terminal(opening: &[u8], live: &[u8]) -> TerminalSnapshot {
        let mut terminal = TerminalSnapshot::new(24, 80);
        terminal.process(opening);
        terminal.process(live);
        terminal
    }

    /// A client attaching while output goes on, after well over the ring's size of it with
    /// alternate-screen stretches and mode changes, ends up in the state of a client attached
    /// throughout, scrollback included. What is written after the attach stays well within the
    /// scrollback, so the rows of the first paint and the point the live output takes over are
    /// among the rows compared, and a byte lost or repeated there would show.
    #[test]
    fn a_client_attaching_after_heavy_output_shows_what_a_client_attached_throughout_shows() {
        for on_alternate in [false, true] {
            let fan = Arc::new(Fan::new(24, 80, false));
            let throughout = fan.attach(10_000);
            // Read after every chunk, so it never falls behind however busy the machine is, and
            // so is sent every byte rather than a fresh paint.
            let mut throughout_reader = Reader::new(&fan, &throughout);

            let mut i = 0;
            while fan.total() < 2 * RING_CAPACITY as u64 {
                let chunk = if i % 500 == 499 {
                    [ENTER_ALTERNATE, &frame(i), LEAVE_ALTERNATE].concat()
                } else {
                    line(i)
                };
                let _ = push(&fan, &chunk);
                throughout_reader.read(&fan);
                i += 1;
            }
            if on_alternate {
                let _ = push(&fan, &[ENTER_ALTERNATE, &frame(i)].concat());
                throughout_reader.read(&fan);
            }
            // The program's own output in the mode it is in: lines on the normal screen, redraws
            // on the alternate one, which a client attaching then is painted on.
            let output = move |n: usize| if on_alternate { frame(n) } else { line(n) };

            // The pusher writes until the late client has attached, and some more after.
            let attached = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (started, attach_now) = std::sync::mpsc::channel();
            let pusher = {
                let (fan, attached) = (fan.clone(), attached.clone());
                let mut reader = throughout_reader;
                std::thread::spawn(move || {
                    let mut push = |data: &[u8]| {
                        let _ = fan.push(data, |_, _| {});
                        reader.read(&fan);
                    };
                    started.send(()).expect("the test waits");
                    let mut n = i;
                    let deadline = Instant::now() + std::time::Duration::from_secs(30);
                    while !attached.load(std::sync::atomic::Ordering::SeqCst)
                        && Instant::now() < deadline
                    {
                        push(&output(n));
                        n += 1;
                        // Leaves the lock to the attach.
                        std::thread::yield_now();
                    }
                    let seen = attached.load(std::sync::atomic::Ordering::SeqCst);
                    for n in n..n + 200 {
                        push(&output(n));
                    }
                    if !on_alternate {
                        push(&[ENTER_ALTERNATE, &frame(n)].concat());
                        for n in n..n + 50 {
                            push(&frame(n));
                        }
                        push(LEAVE_ALTERNATE);
                        for n in n..n + 100 {
                            push(&line(n));
                        }
                    }
                    (seen, reader)
                })
            };
            attach_now.recv().expect("the pusher starts");
            let late = fan.attach(10_000);
            attached.store(true, std::sync::atomic::Ordering::SeqCst);
            let late_reader = drain(fan.clone(), &late);
            let (seen, mut throughout_reader) = pusher.join().expect("the pusher");
            assert!(seen, "the late client attached while the output went on");
            fan.close();
            while throughout_reader.read(&fan) {}
            let late_reader = late_reader.join().unwrap();
            assert_eq!(
                (throughout_reader.paints, late_reader.paints),
                (0, 0),
                "neither client fell behind"
            );

            let throughout = client_terminal(&throughout.opening, &throughout_reader.received);
            let late = client_terminal(&late.opening, &late_reader.received);
            assert_same_state(&late, &throughout);
            assert_same_state(&late, &fan.lock().terminal);
        }
    }

    /// The size of the output a client has been sent, from what [`Fan::take_output`] returned.
    fn sent(delivery: &Delivery) -> usize {
        match delivery {
            Delivery::Send(frames) => frames.iter().map(Outgoing::size).sum(),
            Delivery::Wait | Delivery::Over => 0,
        }
    }

    #[test]
    fn a_client_is_sent_no_more_than_its_credit_until_it_acknowledges() {
        let fan = Fan::new(24, 80, false);
        let attached = fan.attach(0);
        let client = client_of(&attached);
        let opening = attached.opening.len();
        let chunk = vec![b'x'; 64 * 1024];
        let _ = push(&fan, &chunk);
        let _ = push(&fan, &chunk);
        assert_eq!(
            sent(&fan.take_output(client)),
            2 * chunk.len(),
            "a client that has acknowledged nothing yet is not held back"
        );
        fan.acknowledge(client, 2 * chunk.len() as u64);
        for _ in 0..(2 * CREDIT_WINDOW / chunk.len()) {
            let _ = push(&fan, &chunk);
        }

        let first = sent(&fan.take_output(client));
        assert!(
            opening + first >= CREDIT_WINDOW && opening + first - chunk.len() < CREDIT_WINDOW,
            "sent until the window is used up, the paint included: {opening} + {first}"
        );
        assert!(matches!(fan.take_output(client), Delivery::Wait));

        fan.acknowledge(client, chunk.len() as u64);
        assert_eq!(
            sent(&fan.take_output(client)),
            chunk.len(),
            "an acknowledgement lets as much through again"
        );
        fan.acknowledge(client, u64::MAX);
        let rest = sent(&fan.take_output(client));
        assert_eq!(first + chunk.len() + rest, 2 * CREDIT_WINDOW);
        assert!(matches!(fan.take_output(client), Delivery::Wait));
    }

    /// The completion criterion of the stream's flow control: a client that stops reading while
    /// the program floods output stays attached, is sent a fresh paint of the screen in place of
    /// the backlog when it reads again, and holds the other client up in nothing.
    #[test]
    fn a_client_that_falls_past_the_cap_gets_a_fresh_paint_and_the_others_every_byte() {
        let fan = Fan::new(24, 80, false);
        let stalled = fan.attach(100);
        let stalled_client = client_of(&stalled);
        let reading = fan.attach(100);
        let mut reader = Reader::new(&fan, &reading);
        let mut pushed = Vec::new();
        let mut i = 0;
        while pushed.len() < 2 * BACKLOG_CAP {
            let chunk = line(i);
            let _ = push(&fan, &chunk);
            pushed.extend_from_slice(&chunk);
            reader.read(&fan);
            let waiting = fan
                .lock()
                .subscribers
                .iter()
                .find(|sub| sub.id == stalled_client)
                .map(|sub| sub.backlog_bytes)
                .expect("the stalled client stays attached");
            assert!(waiting <= BACKLOG_CAP, "{waiting} bytes wait for it");
            i += 1;
        }
        assert_eq!(reader.paints, 0);
        assert_eq!(
            reader.received, pushed,
            "the reading client is sent every byte"
        );
        let reading = client_terminal(&reading.opening, &reader.received);
        assert_same_state(&reading, &fan.lock().terminal);

        // Reads again, acknowledging what it is sent as it goes.
        let mut resumed = Vec::new();
        while let Delivery::Send(frames) = fan.take_output(stalled_client) {
            fan.acknowledge(
                stalled_client,
                frames.iter().map(Outgoing::size).sum::<usize>() as u64,
            );
            resumed.extend(frames);
        }
        let Some(Outgoing::Paint { offset, paint }) = resumed.first() else {
            panic!("the resumed client is not sent a paint first: {resumed:?}");
        };
        assert_eq!(
            *offset,
            fan.lock().delivered - resumed[1..].iter().map(|f| f.size() as u64).sum::<u64>(),
            "the paint leaves the output where the output after it takes over"
        );
        let mut shown = TerminalSnapshot::new(24, 80);
        shown.process(paint);
        for frame in &resumed[1..] {
            let Outgoing::Output(bytes) = frame else {
                panic!("a second paint queued behind the first");
            };
            shown.process(bytes);
        }
        assert_eq!(shown.visible_text(), fan.lock().terminal.visible_text());
        assert!(
            paint.len() + resumed[1..].iter().map(Outgoing::size).sum::<usize>() <= BACKLOG_CAP,
            "the backlog was discarded rather than sent"
        );
    }

    #[test]
    fn a_closed_fan_hands_over_what_waits_whatever_the_credit_and_then_ends_the_stream() {
        let fan = Fan::new(24, 80, false);
        let attached = fan.attach(0);
        let client = client_of(&attached);
        let chunk = vec![b'y'; 64 * 1024];
        let pushes = 2 * CREDIT_WINDOW / chunk.len();
        fan.acknowledge(client, 0);
        for _ in 0..pushes {
            let _ = push(&fan, &chunk);
        }
        let first = sent(&fan.take_output(client));
        fan.close();
        let _ = push(&fan, b"after the close");
        let rest = sent(&fan.take_output(client));
        assert_eq!(
            first + rest,
            pushes * chunk.len(),
            "the last output reaches the client, and nothing after the close does"
        );
        assert!(matches!(fan.take_output(client), Delivery::Over));
    }

    /// A terminal as an agent session's is on a host that renders: it keeps the program's status
    /// reports and reports title changes.
    fn session_terminal(rows: u16, cols: u16) -> Fan {
        let fan = Fan::new(rows, cols, false);
        fan.keep_program_status();
        fan.report_titles();
        fan
    }

    /// Pushes `data`, returning the answers and the title changes it made.
    fn push_titled(fan: &Fan, data: &[u8]) -> (Vec<u8>, Vec<TitleChange>) {
        let mut titles = Vec::new();
        let answers = fan.push(data, |_, changes| titles = changes);
        (answers, titles)
    }

    /// A screen saved wherever the output was cut, sequences, UTF-8 characters and queries cut
    /// short included, and restored into a fresh terminal, takes the rest of the output as the
    /// original does: the same state, scrollback, status records and colour scheme updates
    /// included, the same answers, the same status moves and the same title changes.
    #[test]
    fn a_restored_screen_goes_on_from_wherever_the_output_was_cut_as_the_original_does() {
        let output = [
            &b"\x1b]7501;state=working:app=x\x07line one\r\nline two\r\n"[..],
            "wide \u{754c} caf\u{e9}\r\n".as_bytes(),
            b"\x1b[1;31mred\x1b[0m \x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\\r\n",
            b"\x1b[?2031h\x1bPq#0;2;0;0;0#0~~\x1b\\\x1b_Gf=24;AAAA\x1b\\",
            b"\x1b]7501;state=blocked:id=a:kind=question\x1b\\\x1b[6n",
            b"more\r\nlines\r\nscroll\r\ninto\r\nthe\r\nscrollback\r\n",
            b"\x1b]0;a title\x07\x1b[2;5r\x1b[3;1H\x1b[?1h\x1b[5 q",
            b"\x1b[3\n\r1m after a control \x1b]11;?\x07\x1b7\x1b[1;1H\x1b8",
            b"\x1b[r\x1b[H\x1b[2J\x1b[3J\x1b]2;after the clear\x07",
            b"drawn again \x1b]7501;state=idle\x07\x1b[?2031$p\x1b[c",
        ]
        .concat();
        for at in 0..=output.len() {
            let (head, tail) = output.split_at(at);
            let original = session_terminal(6, 20);
            let _ = push(&original, head);
            let _ = original.take_program_status();
            let saved = original.save_screen().expect("nothing too long to keep");
            assert_eq!((saved.rows, saved.cols, saved.inexact), (6, 20, false));

            let restored = session_terminal(4, 10);
            restored.set_drawn(saved.drawn);
            restored.restore(saved.rows, saved.cols, &saved.bytes, 8 * 1024);
            assert!(restored.take_program_status().is_empty());
            let context = format!("cut before {:?}", String::from_utf8_lossy(tail));
            assert_eq!(
                push_titled(&restored, tail),
                push_titled(&original, tail),
                "{context}"
            );
            assert_eq!(
                restored.take_program_status(),
                original.take_program_status(),
                "{context}"
            );
            let (restored, original) = (restored.lock(), original.lock());
            assert_same_state(&restored.terminal, &original.terminal);
            assert_eq!(
                restored.terminal.program_status_reports(),
                original.terminal.program_status_reports(),
                "{context}"
            );
        }
    }

    #[test]
    fn a_screen_in_the_middle_of_a_sequence_too_long_to_keep_is_not_saved() {
        let fan = Fan::new(24, 80, false);
        let _ = push(&fan, b"\x1bPq");
        let _ = push(&fan, &vec![b'#'; 100 * 1024]);
        assert!(fan.save_screen().is_none());
        let _ = push(&fan, b"\x1b\\");
        assert!(fan.save_screen().is_some());
    }

    /// Within a replayed tail a query is answered only where the output that asked it is still
    /// fresh, never the device attributes where the rule skips them, and one not answered reaches
    /// no client either; once the tail's length has been pushed, the output is live again, from
    /// within the chunk that crosses its end.
    #[test]
    fn a_replayed_tail_answers_only_the_queries_still_waited_for_up_to_its_length() {
        let fan = Fan::new(24, 80, false);
        let attached = fan.attach(0);
        let stale = b"old\x1b[6n";
        let (attributes, position) = (b"\x1b[c", b"\x1b[6n");
        fan.begin_replay(ReplayRule {
            len: (stale.len() + attributes.len() + position.len() + 2) as u64,
            ages: vec![
                (0, std::time::Duration::from_secs(5)),
                (stale.len() as u64, std::time::Duration::ZERO),
            ],
            measured_at: Instant::now(),
            skip_device_attributes: true,
        });
        assert!(push(&fan, stale).is_empty(), "stale");
        assert!(push(&fan, attributes).is_empty(), "skipped");
        assert_eq!(push(&fan, position), b"\x1b[1;4R", "fresh");
        assert_eq!(push(&fan, b"ab\x1b[c"), b"\x1b[?6c", "past the tail");
        assert!(fan.lock().replay.is_none());
        assert_eq!(received(&fan, &attached), b"oldab");
    }

    /// A query is judged by the stretch of the tail its last byte is in, not by the chunk it
    /// arrives in: a stale burst followed in the same chunk by a fresh query leaves the fresh one
    /// answered.
    #[test]
    fn a_replayed_chunk_judges_each_query_by_its_own_place_in_the_tail() {
        let fan = Fan::new(24, 80, false);
        let attached = fan.attach(0);
        let chunk = b"old\x1b[6nx\x1b[6n";
        fan.begin_replay(ReplayRule {
            len: chunk.len() as u64,
            ages: vec![
                (0, std::time::Duration::from_secs(5)),
                (7, std::time::Duration::ZERO),
            ],
            measured_at: Instant::now(),
            skip_device_attributes: false,
        });
        assert_eq!(push(&fan, chunk), b"\x1b[1;5R", "only the fresh one");
        assert!(fan.lock().replay.is_none());
        assert_eq!(received(&fan, &attached), b"oldx");
    }

    #[test]
    fn a_terminal_whose_output_is_over_is_painted_with_its_last_screen() {
        let fan = Fan::new(24, 80, false);
        let _ = push(&fan, b"\x1b[?1000h\x1b[?1006hfirst\r\nlast words");
        fan.close();
        let attached = fan.attach(10);
        assert!(attached.live.is_none() && attached.answering.is_none());
        assert!(attached.opening.starts_with(b"\x1bc"), "a paint");
        let shown = client_terminal(&attached.opening, b"");
        assert_same_state(&shown, &fan.lock().terminal);
    }
}
