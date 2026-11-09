//! One terminal a holder runs: its program on the PTY, the threads that read what it prints,
//! write what it is sent and watch it exit, and the tail kept for a daemon that connects later.
//! While no daemon is linked it answers the queries [`super::offline`] says it may.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::extract::ws::Message;
use serde_json::Value;
use tokio::sync::mpsc as outbound;

use super::offline::{OfflineQueries, PRIMARY_DEVICE_ATTRIBUTES};
use crate::holder::protocol::{
    encode_bulk, BulkKind, FromHolder, HandoverTail, SignalKind, SpawnSpec, TailAge, TerminalInfo,
    WireError,
};
use crate::platform::proc::{crash, ChildHandle, ProcessId, ProcessIdentity, Sent};
use crate::platform::proc_table;
use crate::platform::term::{self, Input, Pty, Spawned};

/// How much of a terminal's output is kept for a daemon that connects after it was printed.
pub(super) const TAIL_BYTES: usize = 1024 * 1024;

/// How far apart in time the output since a handover's mark tells the age of: a chunk read this
/// long after the last place whose age is kept starts a place of its own.
const AGE_STEP: Duration = Duration::from_millis(25);

/// How much a tail holds while its front is pinned for a link's handshake; past it the pin is
/// given up, and the handshake is refused if the output it was told of has gone.
const PINNED_BYTES: usize = 2 * TAIL_BYTES;

/// The most places a tail keeps the age of; the oldest go first.
const AGE_PLACES: usize = 4096;

/// Size of the buffer a terminal's reader thread reads into.
const READ_BUF: usize = 8 * 1024;

/// How long the exit of a program waits for its reader to have read what it printed last.
/// Bounded because the reader only finishes once nothing holds the PTY open, and a process the
/// program left behind may hold it long after the program is gone.
const FINAL_OUTPUT_DRAIN: Duration = Duration::from_millis(500);

/// How often the exit watcher looks at its program.
const EXIT_POLL: Duration = Duration::from_millis(20);

/// How long a program that was still running when its terminal was released is waited for, to be
/// reaped once it exits. Closing its PTY hangs it up, which ends most programs at once.
const REAP_AFTER_RELEASE: Duration = Duration::from_secs(60);

/// Where a link's frames go: the link's writer takes them in order.
pub(super) type Sink = outbound::Sender<Message>;

/// A reply as the frame it is sent in.
pub(super) fn frame(reply: &FromHolder) -> Message {
    Message::Text(serde_json::to_string(reply).expect("a reply is JSON"))
}

/// The bounded tail of a terminal's output. Once a handover has marked it, it also tells where
/// the mark falls in it and how old the output since the mark is.
#[derive(Default)]
struct Tail {
    bytes: VecDeque<u8>,
    /// The token of the handover that marked the tail last.
    token: Option<String>,
    /// How many bytes the terminal has printed since it started.
    total: u64,
    /// Where the mark falls, counted in `total`.
    marked_at: u64,
    /// Places in the output since the mark, counted in `total`, each with when the chunk that
    /// starts there was read: a place per [`AGE_STEP`] at most, the latest [`AGE_PLACES`] of them,
    /// oldest first. Kept only once a handover has marked the tail.
    ages: VecDeque<(u64, Instant)>,
    /// Whether the front is held while a link's handshake is under way: the tail grows past
    /// [`TAIL_BYTES`] rather than drop output the welcome said it holds, up to [`PINNED_BYTES`].
    pinned: bool,
}

impl Tail {
    /// Keeps `bytes`, read at `read_at`, dropping the oldest output beyond [`TAIL_BYTES`], or
    /// beyond [`PINNED_BYTES`] while the front is pinned.
    fn push(&mut self, bytes: &[u8], read_at: Instant) {
        if bytes.is_empty() {
            return;
        }
        if self.token.is_some() {
            let new_place = self
                .ages
                .back()
                .is_none_or(|&(_, at)| read_at.saturating_duration_since(at) > AGE_STEP);
            if new_place {
                self.ages.push_back((self.total, read_at));
                if self.ages.len() > AGE_PLACES {
                    self.ages.pop_front();
                }
            }
        }
        self.total += bytes.len() as u64;
        self.bytes.extend(bytes);
        if self.pinned && self.bytes.len() > PINNED_BYTES {
            // Too far on to be held: the handshake's check finds the mark gone and refuses.
            self.pinned = false;
        }
        if !self.pinned {
            self.trim();
        }
    }

    /// Drops the oldest output beyond [`TAIL_BYTES`].
    fn trim(&mut self) {
        let excess = self.bytes.len().saturating_sub(TAIL_BYTES);
        if excess > 0 {
            self.bytes.drain(..excess);
        }
    }

    /// Holds the front of the tail, if a handover marked it and the mark is still in it, so that
    /// the offset a welcome tells stays true until the link attaches.
    fn pin(&mut self) {
        self.pinned = self.since().is_some();
    }

    /// Lets the front go again, dropping what was kept past [`TAIL_BYTES`] meanwhile.
    fn unpin(&mut self) {
        self.pinned = false;
        self.trim();
    }

    fn bytes(&self) -> Vec<u8> {
        self.bytes.iter().copied().collect()
    }

    /// Where the tail's first byte is in `total`.
    fn start(&self) -> u64 {
        self.total.saturating_sub(self.bytes.len() as u64)
    }

    /// Marks the tail for the handover `token` at the end of what it holds: the output pushed
    /// from now on is the output since the mark, of no known age yet.
    fn mark(&mut self, token: &str) {
        self.token = Some(token.to_string());
        self.marked_at = self.total;
        self.ages.clear();
    }

    /// The offset into the tail at which the output since the mark begins, when a handover marked
    /// it and the mark is still in the tail.
    fn since(&self) -> Option<u64> {
        self.token.as_ref()?;
        self.marked_at.checked_sub(self.start())
    }

    /// Where a handover marked the tail, as of `now`, if one did.
    fn handover(&self, now: Instant) -> Option<HandoverTail> {
        let token = self.token.clone()?;
        let since = self.since();
        let ages = self
            .ages
            .iter()
            .map(|&(place, at)| TailAge {
                offset: place.saturating_sub(self.marked_at),
                age_ms: now.saturating_duration_since(at).as_millis() as u64,
            })
            .collect();
        Some(HandoverTail {
            token,
            since: since.unwrap_or(0),
            truncated: since.is_none(),
            ages,
        })
    }
}

/// What a link's welcome said of a terminal's tail, for [`Terminal::attach`] to hold the tail to.
#[derive(Clone, Copy, Default)]
pub(super) struct Told<'a> {
    /// The token of the handover that marked the tail, if one did.
    pub(super) token: Option<&'a str>,
    /// Where the output since the mark begins, when the welcome said the tail still holds all of
    /// it.
    pub(super) since: Option<u64>,
}

/// What the reader thread, the exit watcher and a link's handshake share, behind one lock so that
/// a tail taken for a new link and the output sent to it neither overlap nor leave a gap. Nothing
/// waits while it is held: a thread that has to wait for room in the link does so first, and
/// only then takes the lock ([`Terminal::publish`]).
struct Stream {
    tail: Tail,
    /// The daemon link output goes to, while there is one.
    sink: Option<Sink>,
    /// The output followed for the queries the holder answers itself.
    queries: OfflineQueries,
}

impl Stream {
    /// Whether the output goes to `sink`.
    fn goes_to(&self, sink: &Sink) -> bool {
        self.sink.as_ref().is_some_and(|now| now.same_channel(sink))
    }

    /// Reads `chunk`, the next of the output, for the queries the holder answers itself, and
    /// returns how many of them it is to answer: those that end in it while no daemon is linked,
    /// where the holder `answers` at all. Read under the stream's lock, which a link attaches and
    /// detaches under, so that each query is the holder's or the linked daemon's, never both, and
    /// one read while a daemon is linked is left to it even if the link drops before it answers.
    fn queries_to_answer(&mut self, chunk: &[u8], answers: bool) -> usize {
        let asked = self.queries.feed(chunk);
        if answers && self.sink.is_none() {
            asked
        } else {
            0
        }
    }
}

enum Write {
    Raw(Vec<u8>),
    Input(Vec<Input>),
}

struct WriteJob {
    req: u64,
    write: Write,
    /// Where the result goes; `None` for the holder's own answer to a query, which nobody waits
    /// on.
    reply: Option<Sink>,
}

pub(super) struct Terminal {
    pub(super) id: String,
    pid: ProcessId,
    identity: Option<ProcessIdentity>,
    meta: Value,
    kill_session_leftovers: bool,
    pty: Pty,
    /// The holder answers the queries it may itself while no daemon is linked: not where the host
    /// answers them before the output is read (`TermFacts::host_answers_queries`), as the daemon
    /// leaves them there too.
    answers_offline: bool,
    /// Held across signals, sweeps and reaping, and `true` once reaped: a signal goes by pid, so
    /// it is only sent while the child is unreaped.
    child: Mutex<Reaping>,
    stream: Mutex<Stream>,
    /// What the threads that send to the link wait for room on.
    runtime: tokio::runtime::Handle,
    reader_done: AtomicBool,
    exited: AtomicBool,
    released: AtomicBool,
    writes: Mutex<Option<mpsc::Sender<WriteJob>>>,
}

struct Reaping {
    handle: ChildHandle,
    reaped: bool,
    /// The program's place in this holder's own crash cleanup, given up before it is reaped.
    crash: Option<crash::Entry>,
}

impl Terminal {
    /// Starts the program `spec` describes, with the threads that serve it, sending its output to
    /// `sink` from the first byte. The threads wait on `runtime` for room in the link.
    pub(super) fn start(
        spec: SpawnSpec,
        sink: Option<Sink>,
        runtime: tokio::runtime::Handle,
    ) -> anyhow::Result<Arc<Self>> {
        use anyhow::Context as _;

        let containment = spec
            .containment
            .as_ref()
            // The daemon made the launch's containment (on Linux an agent launch's cgroup group),
            // which the program joins here as it would started from the daemon.
            .map(|launch| {
                crate::platform::proc::Containment::join_launch(&launch.instance, &launch.launch)
            })
            .transpose()
            .context("opening the launch's containment")?;
        let Spawned { child, pid, pty } =
            term::spawn(spec.command, spec.size, containment.as_ref())?;
        let crash = crash::register(&child, spec.kill_session_leftovers);
        if crash.is_none() {
            tracing::warn!(terminal = %spec.id, "the crash cleanup is full; this program would outlive a holder crash");
        }
        let (writes, jobs) = mpsc::channel::<WriteJob>();
        let terminal = Arc::new(Self {
            id: spec.id,
            pid,
            identity: proc_table::start_of(pid.0 as i32)
                .map(|start_micros| ProcessIdentity { pid, start_micros }),
            meta: spec.meta,
            kill_session_leftovers: spec.kill_session_leftovers,
            pty,
            answers_offline: answers_offline(),
            child: Mutex::new(Reaping {
                handle: child,
                reaped: false,
                crash,
            }),
            stream: Mutex::new(Stream {
                tail: Tail::default(),
                sink,
                queries: OfflineQueries::default(),
            }),
            runtime,
            reader_done: AtomicBool::new(false),
            exited: AtomicBool::new(false),
            released: AtomicBool::new(false),
            writes: Mutex::new(Some(writes)),
        });
        terminal.start_reader();
        terminal.start_writer(jobs);
        terminal.start_exit_watch();
        Ok(terminal)
    }

    pub(super) fn pid(&self) -> ProcessId {
        self.pid
    }

    pub(super) fn identity_key(&self) -> Option<String> {
        self.identity.map(|identity| identity.key())
    }

    /// What a welcome lists of the terminal. A tail a handover marked is pinned, until
    /// [`Self::attach`] or [`Self::unpin`]: the output the welcome says it holds is still there
    /// when the link attaches, however much the terminal prints meanwhile.
    pub(super) fn info(&self) -> TerminalInfo {
        let handover = {
            let mut stream = self.stream();
            stream.tail.pin();
            stream.tail.handover(Instant::now())
        };
        TerminalInfo {
            id: self.id.clone(),
            pid: self.pid.0,
            identity: self.identity_key(),
            size: self.pty.size().ok(),
            meta: self.meta.clone(),
            exited: self.exited.load(Ordering::Acquire),
            handover,
        }
    }

    fn stream(&self) -> MutexGuard<'_, Stream> {
        self.stream.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn child(&self) -> MutexGuard<'_, Reaping> {
        self.child.lock().unwrap_or_else(|err| err.into_inner())
    }

    /// Hands a new link the tail so far, as the frame `tail` puts it in, and the output from then
    /// on, and with them the exit if the program has exited already. The frames are queued under
    /// the lock that output and the exit are sent under, so they go in order. `false` when the
    /// link cannot take its own handshake, which is no link worth keeping.
    ///
    /// `told` is what the link's welcome said of the tail. A tail that is no longer so (it was
    /// marked again, or the mark fell off its front or moved within it, since the welcome was
    /// made) would be taken for what it is not, so it is not handed over and `false` is returned:
    /// the daemon links again, and that welcome tells the tail as it is. A tail that only grew is
    /// handed over, as the welcome pinned its front. `None` checks nothing. A `told` also ends the
    /// pin, whatever the outcome.
    pub(super) fn attach(&self, sink: &Sink, told: Option<Told<'_>>) -> bool {
        let mut stream = self.stream();
        if let Some(told) = told {
            let tail = &stream.tail;
            let still = tail.token.as_deref() == told.token
                && told.since.is_none_or(|since| tail.since() == Some(since));
            if !still {
                stream.tail.unpin();
                return false;
            }
        }
        let tail = encode_bulk(BulkKind::Tail, &self.id, &stream.tail.bytes());
        if told.is_some() {
            stream.tail.unpin();
        }
        if sink.try_send(Message::Binary(tail)).is_err() {
            return false;
        }
        if self.exited.load(Ordering::Acquire) && sink.try_send(self.exited_frame()).is_err() {
            return false;
        }
        stream.sink = Some(sink.clone());
        true
    }

    /// Has the output go to `sink`, the link that is current, unless it goes there already: a
    /// terminal that was started while another link was current, which the new link's handshake
    /// did not find.
    pub(super) fn follow(&self, sink: &Sink) -> bool {
        let current = self.stream().goes_to(sink);
        current || self.attach(sink, None)
    }

    fn exited_frame(&self) -> Message {
        frame(&FromHolder::Exited {
            id: self.id.clone(),
            output_open: !self.reader_done.load(Ordering::Acquire),
        })
    }

    /// Stops sending output to a link; the output is kept in the tail.
    pub(super) fn detach(&self) {
        let mut stream = self.stream();
        stream.sink = None;
        stream.tail.unpin();
    }

    /// Lets the tail's front go after a welcome that listed it, for a link that ends before it
    /// attaches.
    pub(super) fn unpin(&self) {
        self.stream().tail.unpin();
    }

    /// Hands the terminal over for the handover `token`, if its output goes to `sink`: queues the
    /// [`FromHolder::HandoverMark`] behind the output sent there, stops sending output there and
    /// marks the tail, in one step under the lock output is sent under, so that the mark divides
    /// the output exactly: what came before it went to the link, what comes after is the tail's
    /// output since the mark. From then on no daemon is linked for the terminal's output, so the
    /// holder answers the queries in it it may. Room for the mark is waited for before the lock is
    /// taken, as [`Self::publish`] does. `false` when the output does not go to `sink`, or the
    /// link is closed: nothing is changed. **Blocks** while the link is full.
    pub(super) fn hand_over(&self, sink: &Sink, token: &str) -> bool {
        if !self.stream().goes_to(sink) {
            return false;
        }
        let Ok(permit) = self.runtime.block_on(sink.clone().reserve_owned()) else {
            return false;
        };
        let mut stream = self.stream();
        // The link was taken over, or the terminal released, while this waited.
        if !stream.goes_to(sink) {
            return false;
        }
        permit.send(frame(&FromHolder::HandoverMark {
            id: self.id.clone(),
        }));
        stream.sink = None;
        stream.tail.mark(token);
        true
    }

    /// Records something in the stream's state and sends `message` to the linked daemon, in one
    /// step under the lock, so that what is recorded and what the daemon is sent never come apart
    /// for a link that attaches meanwhile. Room for the message in the link is waited for before
    /// the lock is taken, and the step starts over if the link changed while waiting.
    fn publish(&self, message: Message, record: impl FnOnce(&mut Stream)) {
        loop {
            let sink = self.stream().sink.clone();
            let Some(sink) = sink else {
                let mut stream = self.stream();
                if stream.sink.is_none() {
                    record(&mut stream);
                    return;
                }
                continue;
            };
            let reserved = self.runtime.block_on(sink.clone().reserve_owned());
            let mut stream = self.stream();
            let current = stream.goes_to(&sink);
            match reserved {
                Ok(permit) if current => {
                    record(&mut stream);
                    permit.send(message);
                    return;
                }
                // The link changed while this waited: go round with the one that is current.
                Ok(_) => {}
                // The link is closed.
                Err(_) => {
                    if current {
                        stream.sink = None;
                    }
                }
            }
        }
    }

    fn start_reader(self: &Arc<Self>) {
        let terminal = self.clone();
        let reader = self.pty.reader();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; READ_BUF];
            loop {
                match reader.read_chunk(&mut buf) {
                    Ok(None | Some(0)) => break,
                    Ok(Some(n)) => terminal.take_output(&buf[..n]),
                    Err(err) => {
                        tracing::debug!(terminal = %terminal.id, %err, "PTY read failed; closing the reader");
                        break;
                    }
                }
            }
            terminal.reader_done.store(true, Ordering::Release);
        });
    }

    /// Keeps `chunk` in the tail and sends it to the daemon, if one is connected. Waits while the
    /// link is full, as a program waits on a PTY nobody reads. The queries that end in it while
    /// no daemon is linked are answered, behind the writes queued before.
    fn take_output(&self, chunk: &[u8]) {
        let read_at = Instant::now();
        let message = Message::Binary(encode_bulk(BulkKind::Output, &self.id, chunk));
        let mut asked = 0;
        self.publish(message, |stream| {
            stream.tail.push(chunk, read_at);
            asked = stream.queries_to_answer(chunk, self.answers_offline);
        });
        if asked > 0 {
            self.answer_offline(PRIMARY_DEVICE_ATTRIBUTES.repeat(asked));
        }
    }

    fn start_writer(self: &Arc<Self>, jobs: mpsc::Receiver<WriteJob>) {
        let terminal = self.clone();
        std::thread::spawn(move || {
            for job in jobs {
                let result = match &job.write {
                    Write::Raw(bytes) => terminal.pty.write_raw(bytes),
                    Write::Input(inputs) => terminal.pty.write_input(inputs),
                };
                let reply = match result {
                    Ok(()) => FromHolder::Written {
                        req: job.req,
                        // What a write that went in whole counts is not read: the daemon wants
                        // the count of a write that failed.
                        written: match &job.write {
                            Write::Raw(bytes) => bytes.len(),
                            Write::Input(_) => 0,
                        },
                        error: None,
                    },
                    Err(failed) => FromHolder::Written {
                        req: job.req,
                        written: failed.written,
                        error: Some(WireError::from(&failed)),
                    },
                };
                match &job.reply {
                    Some(sink) => {
                        sink.blocking_send(frame(&reply)).ok();
                    }
                    None => {
                        if let FromHolder::Written {
                            error: Some(error), ..
                        } = reply
                        {
                            tracing::debug!(terminal = %terminal.id, error = %error.message, "answering a query failed");
                        }
                    }
                }
            }
        });
    }

    /// Queues a write of `bytes` as they are behind the terminal's earlier ones, to be answered
    /// on `reply`.
    pub(super) fn write_raw(&self, req: u64, bytes: Vec<u8>, reply: Sink) {
        self.queue(req, Write::Raw(bytes), reply);
    }

    /// As [`Self::write_raw`], for inputs the terminal's host encodes.
    pub(super) fn write_input(&self, req: u64, inputs: Vec<Input>, reply: Sink) {
        self.queue(req, Write::Input(inputs), reply);
    }

    fn queue(&self, req: u64, write: Write, reply: Sink) {
        let queued = self
            .writes
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .as_ref()
            .map(|writes| {
                writes.send(WriteJob {
                    req,
                    write,
                    reply: Some(reply.clone()),
                })
            });
        if !matches!(queued, Some(Ok(()))) {
            let refused = frame(&FromHolder::Written {
                req,
                written: 0,
                error: Some(WireError {
                    kind: "BrokenPipe".to_string(),
                    message: "the terminal has been released".to_string(),
                }),
            });
            reply.try_send(refused).ok();
        }
    }

    /// Queues the holder's own `answer` to a query behind the terminal's earlier writes. A
    /// terminal that has been released takes no more, and the answer is dropped.
    fn answer_offline(&self, answer: Vec<u8>) {
        if let Some(writes) = self
            .writes
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .as_ref()
        {
            writes
                .send(WriteJob {
                    req: 0,
                    write: Write::Raw(answer),
                    reply: None,
                })
                .ok();
        }
    }

    fn start_exit_watch(self: &Arc<Self>) {
        let terminal = self.clone();
        std::thread::spawn(move || {
            loop {
                if terminal.released.load(Ordering::Acquire) {
                    return;
                }
                let exited = {
                    let child = terminal.child();
                    child.reaped || child.handle.exited_unreaped() == Some(true)
                };
                if exited {
                    break;
                }
                std::thread::sleep(EXIT_POLL);
            }
            let deadline = Instant::now() + FINAL_OUTPUT_DRAIN;
            while !terminal.reader_done.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            // Recorded as it is sent, so that a link that attaches meanwhile either is told by
            // the frame or finds the terminal exited when it attaches.
            let message = terminal.exited_frame();
            terminal.publish(message, |_| terminal.exited.store(true, Ordering::Release));
        });
    }

    /// Signals the program, unless it has been reaped.
    pub(super) fn signal(&self, signal: SignalKind) {
        if signal == SignalKind::Unknown {
            return;
        }
        let child = self.child();
        if child.reaped {
            return;
        }
        let sent = match signal.as_polite() {
            Some(polite) => child.handle.stop(polite),
            None => child.handle.kill(),
        };
        if let Sent::Failed(err) = sent {
            tracing::warn!(terminal = %self.id, pid = self.pid.0, %err, "signalling the process failed");
        }
    }

    pub(super) fn hang_up(&self) {
        self.pty.hang_up();
    }

    pub(super) fn pty(&self) -> &Pty {
        &self.pty
    }

    pub(super) fn exited_unreaped(&self) -> Option<bool> {
        let child = self.child();
        if child.reaped {
            return None;
        }
        child.handle.exited_unreaped()
    }

    pub(super) fn group_has_members(&self) -> Option<bool> {
        let child = self.child();
        if child.reaped {
            return None;
        }
        child.handle.group_has_members()
    }

    /// Kills what the program started, wherever it went, other than the processes `keep` names.
    pub(super) fn kill_leftovers(&self, keep: &[ProcessIdentity]) -> crate::platform::proc::Swept {
        let child = self.child();
        if child.reaped {
            return crate::platform::proc::Swept {
                killed: 0,
                complete: true,
            };
        }
        child.handle.kill_leftovers(keep)
    }

    /// Reaps the program if it has exited. It leaves the crash cleanup first, which signals by
    /// number, so only while the program is unreaped.
    pub(super) fn reap_if_exited(&self) -> std::io::Result<bool> {
        let mut child = self.child();
        if child.reaped {
            return Ok(true);
        }
        let exited = child.handle.reap_if_exited()?;
        if exited {
            child.reaped = true;
            child.crash = None;
        }
        Ok(exited)
    }

    /// Ends the program and what it left, for a holder with no daemon to answer to. **Blocks**
    /// for up to a second or two while the program is reaped.
    pub(super) fn end(self: &Arc<Self>) {
        {
            let child = self.child();
            if !child.reaped {
                if self.exited.load(Ordering::Acquire)
                    || child.handle.exited_unreaped() == Some(true)
                {
                    // Already gone; what it left behind may still be running.
                } else if let Sent::Failed(err) = child.handle.kill() {
                    tracing::warn!(terminal = %self.id, %err, "killing the process failed");
                }
                if self.kill_session_leftovers {
                    child.handle.kill_leftovers(&[]).log(self.pid, &self.id);
                }
            }
        }
        self.hang_up();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.reap_if_exited().unwrap_or(true) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.release();
    }

    /// Forgets the terminal: closes its PTY and ends its threads. The program is reaped, now if it
    /// has exited, or once it does from a thread of its own, since the exit watcher stops here
    /// and nothing else would: one killed just before its release is not gone yet, and would
    /// stay a zombie.
    pub(super) fn release(self: &Arc<Self>) {
        self.released.store(true, Ordering::Release);
        self.hang_up();
        self.writes
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .take();
        self.detach();
        if self.reap_if_exited().unwrap_or(true) {
            return;
        }
        let terminal = self.clone();
        std::thread::spawn(move || {
            let deadline = Instant::now() + REAP_AFTER_RELEASE;
            while Instant::now() < deadline {
                std::thread::sleep(EXIT_POLL);
                if terminal.reap_if_exited().unwrap_or(true) {
                    return;
                }
            }
        });
    }
}

/// Whether a holder answers the queries it may itself while no daemon is linked, on this host.
pub(super) fn answers_offline() -> bool {
    !term::facts().host_answers_queries
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{OfflineQueries, Stream, Tail, AGE_PLACES, TAIL_BYTES};
    use crate::holder::protocol::TailAge;

    fn stream(linked: bool) -> Stream {
        Stream {
            tail: Tail::default(),
            sink: linked.then(|| tokio::sync::mpsc::channel(1).0),
            queries: OfflineQueries::default(),
        }
    }

    #[test]
    fn a_query_read_while_no_daemon_is_linked_is_the_holders_once() {
        let mut stream = stream(false);
        assert_eq!(stream.queries_to_answer(b"a\x1b[c", true), 1);
        assert_eq!(stream.queries_to_answer(b"more", true), 0);
        // Where the host answers it, nobody here does.
        assert_eq!(stream.queries_to_answer(b"\x1b[c", false), 0);
    }

    #[test]
    fn a_query_read_while_a_daemon_is_linked_is_left_to_it() {
        let mut stream = stream(true);
        assert_eq!(stream.queries_to_answer(b"\x1b[c", true), 0);
        // Begun while linked and ended once the link is gone, it is the holder's: the daemon
        // never received its end.
        assert_eq!(stream.queries_to_answer(b"\x1b[", true), 0);
        stream.sink = None;
        assert_eq!(stream.queries_to_answer(b"0c", true), 1);
        // Begun while unlinked and ended once a daemon is linked, it is the daemon's, which
        // reads the start in the tail it is handed and the end live.
        assert_eq!(stream.queries_to_answer(b"\x1b[", true), 0);
        stream.sink = Some(tokio::sync::mpsc::channel(1).0);
        assert_eq!(stream.queries_to_answer(b"c", true), 0);
    }

    #[test]
    fn a_tail_no_handover_marked_tells_nothing_of_where_it_is_marked() {
        let mut tail = Tail::default();
        let now = Instant::now();
        tail.push(b"before", now);
        assert!(tail.handover(now).is_none());
        assert!(tail.ages.is_empty());
    }

    #[test]
    fn a_handover_keeps_the_history_before_the_mark_and_tells_where_the_output_since_begins() {
        let mut tail = Tail::default();
        let start = Instant::now();
        tail.push(b"before", start);
        tail.mark("tok");
        // Chunks read close together share a place; one read later starts its own.
        tail.push(b"one", start);
        tail.push(b"two", start + Duration::from_millis(10));
        tail.push(b"three", start + Duration::from_millis(100));
        let told = tail
            .handover(start + Duration::from_millis(1000))
            .expect("marked");
        assert_eq!(tail.bytes(), b"beforeonetwothree");
        assert_eq!(told.token, "tok");
        assert_eq!(told.since, 6);
        assert!(!told.truncated);
        assert_eq!(
            told.ages,
            vec![
                TailAge {
                    offset: 0,
                    age_ms: 1000,
                },
                TailAge {
                    offset: 6,
                    age_ms: 900,
                },
            ]
        );
    }

    #[test]
    fn a_mark_that_fell_off_the_front_of_the_tail_says_so() {
        let mut tail = Tail::default();
        let start = Instant::now();
        tail.push(&[b'z'; 10], start);
        tail.mark("tok");
        tail.push(&[b'a'; 100], start);
        tail.push(&[b'b'; 100], start + Duration::from_millis(50));
        tail.push(
            &vec![b'c'; TAIL_BYTES - 200],
            start + Duration::from_millis(100),
        );
        // Exactly full: the first byte kept is the first after the mark.
        assert_eq!(tail.bytes().len(), TAIL_BYTES);
        let told = tail.handover(start).expect("marked");
        assert_eq!((told.since, told.truncated), (0, false));
        tail.push(&[b'd'; 110], start + Duration::from_millis(150));
        let told = tail
            .handover(start + Duration::from_millis(200))
            .expect("marked");
        // The first 110 bytes after the mark are gone.
        assert!(told.truncated);
        assert_eq!(tail.since(), None);
        // Ages stay counted from the mark.
        assert_eq!(
            told.ages.iter().map(|age| age.offset).collect::<Vec<_>>(),
            vec![0, 100, 200, TAIL_BYTES as u64]
        );
        // A later mark makes it whole again.
        tail.mark("next");
        let told = tail.handover(start).expect("marked");
        assert_eq!((told.since, told.truncated), (TAIL_BYTES as u64, false));
    }

    #[test]
    fn a_pinned_tail_keeps_its_front_while_the_terminal_prints_on() {
        let mut tail = Tail::default();
        let start = Instant::now();
        tail.push(b"before", start);
        tail.mark("tok");
        tail.push(&vec![b'a'; TAIL_BYTES - 6], start);
        assert_eq!(tail.since(), Some(6));
        let told = tail.handover(start).expect("marked");
        tail.pin();
        // Printing continuously during the handshake: the told offset stays true.
        for _ in 0..8 {
            tail.push(&[b'b'; 4096], start);
            assert_eq!(tail.since(), Some(told.since));
        }
        assert!(tail.bytes().len() > TAIL_BYTES);
        // Letting go drops what no longer fits, and the mark with it.
        tail.unpin();
        assert_eq!(tail.bytes().len(), TAIL_BYTES);
        assert_eq!(tail.since(), None);
    }

    #[test]
    fn a_pin_is_given_up_once_the_tail_has_grown_twice_its_size() {
        let mut tail = Tail::default();
        let start = Instant::now();
        tail.mark("tok");
        tail.push(&vec![b'a'; TAIL_BYTES], start);
        tail.pin();
        tail.push(&vec![b'b'; TAIL_BYTES], start);
        assert_eq!(tail.since(), Some(0));
        tail.push(b"c", start);
        assert_eq!(tail.bytes().len(), TAIL_BYTES);
        assert_eq!(tail.since(), None);
    }

    #[test]
    fn a_tail_that_was_not_marked_or_lost_its_mark_is_not_pinned() {
        let mut tail = Tail::default();
        let start = Instant::now();
        tail.push(b"x", start);
        tail.pin();
        assert!(!tail.pinned);
        tail.mark("tok");
        tail.push(&vec![b'a'; TAIL_BYTES + 1], start);
        assert_eq!(tail.since(), None);
        tail.pin();
        assert!(!tail.pinned);
    }

    #[test]
    fn a_tail_keeps_the_age_of_its_latest_places_only() {
        let mut tail = Tail::default();
        let start = Instant::now();
        tail.mark("tok");
        for step in 0..(AGE_PLACES as u64 + 10) {
            tail.push(b"x", start + Duration::from_millis(step * 30));
        }
        assert_eq!(tail.ages.len(), AGE_PLACES);
        assert_eq!(tail.ages.front().map(|&(place, _)| place), Some(10));
    }
}
