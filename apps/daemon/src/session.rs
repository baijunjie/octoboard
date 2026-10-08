//! One running agent process: its PTY, its output fan-out, and how it is stopped.

use std::io;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use portable_pty::{Child, MasterPty};
use tokio::sync::mpsc;

use crate::protocol::Agent;
use crate::ptyio;
use crate::ringbuf::RingBuffer;
use crate::trust::TrustState;

/// Terminal replay buffer per session.
pub const RING_CAPACITY: usize = 2 * 1024 * 1024;

/// Frames a single attached client may fall behind by before the PTY reader starts waiting on it.
const SUBSCRIBER_QUEUE: usize = 64;

/// How long the reader waits for one client to drain before dropping it. Until this elapses the
/// reader does not read from the PTY at all, which is the point: the agent's own writes block, so
/// output is throttled at the source instead of being buffered without bound in the daemon. A
/// client that has not taken a single frame in this long is gone or wedged, and dropping it (its
/// socket closes, and it reconnects to a fresh replay) is better than throttling the agent for it.
const SUBSCRIBER_GRACE: Duration = Duration::from_secs(3);

/// Graceful teardown: agents are asked to exit with `SIGTERM` and only killed if they do not.
/// This is not politeness — Grok's `Stop` hook fires with `reason: "shutdown"` on a `SIGTERM`
/// teardown and not at all on a kill, and repeatedly killing Claude Code mid-startup trips its own
/// renderer-fallback counter, which is machine-level state it keeps about itself.
const TERM_GRACE: Duration = Duration::from_secs(3);

/// How long [`LiveSession::keep_final_output`] waits for the PTY reader to take what the process
/// wrote just before it exited. Bounded because the reader only finishes once nothing holds the PTY
/// open, and a background process the agent started may hold it long after the agent is gone.
const FINAL_OUTPUT_DRAIN: Duration = Duration::from_millis(500);

struct Subscriber {
    id: u64,
    tx: mpsc::Sender<Bytes>,
}

/// The ring buffer and the subscriber list share one lock so that an attaching client can take the
/// replay snapshot and start receiving live output with no gap and no duplicate, even while the
/// reader thread is pushing bytes.
struct Fan {
    ring: RingBuffer,
    /// Every byte ever pushed into `ring`, which it forgets and this does not: what lets a caller
    /// mark a point in the output and later read exactly what came after it.
    total: u64,
    subscribers: Vec<Subscriber>,
    next_id: u64,
}

pub struct LiveSession {
    pub id: String,
    pub agent: Agent,
    pub pid: u32,
    /// The PTY master descriptor, in non-blocking mode. Reads and writes both go through it;
    /// `master` is kept only so the window can be resized.
    fd: RawFd,
    master: Mutex<Box<dyn MasterPty + Send>>,
    /// Held, and deliberately never reaped until the exit watcher observes the exit: an unreaped
    /// child keeps its pid and process group reserved, which is what makes signalling by number
    /// safe here. Signalling a process group by a pid that had already been recycled is not a
    /// theoretical hazard: it once killed the spawn helper of the editor the daemon ran under,
    /// leaving that process unable to start any child at all until it was restarted.
    child: Mutex<Box<dyn Child + Send + Sync>>,
    reaped: AtomicBool,
    fan: Mutex<Fan>,
    /// A reader thread is reading the PTY into `fan`; see [`spawn_reader_thread`].
    reader_running: AtomicBool,
    /// [`Self::keep_final_output`] has run. A lock rather than a flag, so that a second caller
    /// returns only once the first has finished.
    final_output_kept: Mutex<bool>,
    /// Per-session scratch directory (Grok's `GROK_HOME` symlink farm), removed when the session's
    /// process is gone.
    scratch_dir: Mutex<Option<std::path::PathBuf>>,
    /// The agent resolves approval requests itself for this session, so its permission hook fires
    /// with no dialog ever shown. A raised hand here would ask the user to answer something they
    /// never see.
    pub resolves_approvals_itself: bool,
    /// Watches a Claude Code session's output for its workspace-trust screen; inert for the other
    /// agents.
    pub trust: TrustState,
    /// How many times anything but the trust path has written into this session's input. An
    /// attached terminal's own protocol replies (focus reports, answers to the agent's terminal
    /// queries) are not counted, because they cannot move a cursor. See [`Self::write_input`].
    input_writes: AtomicU64,
    /// Held across every write into the input and its count, so that the trust path's check of the
    /// count and its key are one step that no other writer can fall between.
    input_lock: Mutex<()>,
}

/// Everything one running session is built from. A struct rather than a parameter list: these are
/// all values the launch produced, and half of them are indistinguishable by type.
pub struct NewSession {
    pub id: String,
    pub agent: Agent,
    pub pid: u32,
    pub fd: RawFd,
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn Child + Send + Sync>,
    pub scratch_dir: Option<std::path::PathBuf>,
    pub resolves_approvals_itself: bool,
}

impl LiveSession {
    pub fn new(session: NewSession) -> Self {
        Self {
            id: session.id,
            agent: session.agent,
            pid: session.pid,
            fd: session.fd,
            master: Mutex::new(session.master),
            child: Mutex::new(session.child),
            reaped: AtomicBool::new(false),
            fan: Mutex::new(Fan {
                ring: RingBuffer::new(RING_CAPACITY),
                total: 0,
                subscribers: Vec::new(),
                next_id: 0,
            }),
            reader_running: AtomicBool::new(false),
            final_output_kept: Mutex::new(false),
            scratch_dir: Mutex::new(session.scratch_dir),
            resolves_approvals_itself: session.resolves_approvals_itself,
            trust: TrustState::new(session.agent),
            input_writes: AtomicU64::new(0),
            input_lock: Mutex::new(()),
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Registers a terminal client. The returned snapshot is everything buffered up to this
    /// instant, and the receiver picks up with the very next byte after it.
    pub fn attach(&self) -> (Vec<u8>, mpsc::Receiver<Bytes>) {
        let (tx, rx) = mpsc::channel(SUBSCRIBER_QUEUE);
        let mut fan = self.fan.lock().expect("fan mutex poisoned");
        let id = fan.next_id;
        fan.next_id += 1;
        let snapshot = fan.ring.snapshot();
        fan.subscribers.push(Subscriber { id, tx });
        (snapshot, rx)
    }

    /// The tail of what this session has printed, at most `max_bytes` of it. Taken from the same
    /// ring buffer a terminal client replays, so it is exactly what the user would see — raw PTY
    /// bytes, escape sequences included, which the caller is responsible for making readable.
    pub fn recent_output(&self, max_bytes: usize) -> Vec<u8> {
        self.fan
            .lock()
            .expect("fan mutex poisoned")
            .ring
            .tail(max_bytes)
    }

    /// Hands everything still buffered to `keep`, once the process has exited, and only the first
    /// time: the exit watcher and a shutdown can both see the same exit. A later caller waits for
    /// the first to finish, so a shutdown that returns has its output kept whichever of the two got
    /// there first. Waits up to `FINAL_OUTPUT_DRAIN` for the reader to finish before taking the
    /// buffer, since what an agent prints on its way out can still be in the PTY when its exit is
    /// observed. **Blocks** for that long.
    pub fn keep_final_output(&self, keep: impl FnOnce(&[u8])) {
        let mut kept = self
            .final_output_kept
            .lock()
            .expect("final output mutex poisoned");
        if *kept {
            return;
        }
        let deadline = Instant::now() + FINAL_OUTPUT_DRAIN;
        while self.reader_running.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = self.fan.lock().expect("fan mutex poisoned").ring.snapshot();
        keep(&output);
        *kept = true;
    }

    /// How many bytes this session has printed in all. A mark for [`Self::output_since`].
    pub fn output_total(&self) -> u64 {
        self.fan.lock().expect("fan mutex poisoned").total
    }

    /// What this session printed after `mark`, as taken from [`Self::output_total`] — less of it if
    /// the ring buffer has already let some go.
    pub fn output_since(&self, mark: u64) -> Vec<u8> {
        let fan = self.fan.lock().expect("fan mutex poisoned");
        let wanted = fan.total.saturating_sub(mark);
        fan.ring.tail(usize::try_from(wanted).unwrap_or(usize::MAX))
    }

    /// Called from the PTY reader thread for every chunk read. Blocks while a slow client catches
    /// up, which is how backpressure reaches the agent: with nobody reading the PTY, its own writes
    /// block. The alternative was measured: a reader that always runs ahead fills any bounded
    /// channel within a fraction of a second under heavy output and the client is dropped, at every
    /// frame size tested.
    pub fn on_output(&self, data: &[u8]) {
        let chunk = Bytes::copy_from_slice(data);
        let targets: Vec<(u64, mpsc::Sender<Bytes>)> = {
            let mut fan = self.fan.lock().expect("fan mutex poisoned");
            fan.ring.push(data);
            fan.total += data.len() as u64;
            fan.subscribers
                .iter()
                .map(|sub| (sub.id, sub.tx.clone()))
                .collect()
        };

        self.trust.feed(data);

        let mut dropped = Vec::new();
        for (id, tx) in targets {
            if !send_with_backpressure(&tx, chunk.clone()) {
                dropped.push(id);
            }
        }
        if !dropped.is_empty() {
            let mut fan = self.fan.lock().expect("fan mutex poisoned");
            fan.subscribers.retain(|sub| !dropped.contains(&sub.id));
        }
    }

    /// Writes raw input. The error says how many bytes went in, because a caller deciding whether
    /// to retry has to know: whatever was accepted is already in the child's input buffer.
    ///
    /// Counted: every writer but the trust path comes through here, which is how the trust path
    /// learns that someone else has typed into the session since it last looked. A write made up
    /// only of what an attached terminal sends by itself — focus reports, replies to the agent's
    /// terminal queries — is not counted: Claude Code turns focus reporting on as it draws the trust
    /// screen, so an attached terminal writes that traffic all the time and none of it can move
    /// the cursor.
    pub fn write_input(&self, data: &[u8]) -> Result<(), ptyio::PartialWrite> {
        let _input = self.input_lock.lock().expect("input mutex poisoned");
        if !is_terminal_protocol(data) {
            self.input_writes.fetch_add(1, Ordering::AcqRel);
        }
        ptyio::write_all(self.fd, data)
    }

    /// Writes `data` only if nobody else has written into the input since the count read as
    /// `expected`, and says whether it did. For the trust path alone: its own keys are not counted,
    /// and the check and the write happen under the lock every other writer takes.
    pub(crate) fn write_input_if_untouched(
        &self,
        expected: u64,
        data: &[u8],
    ) -> Result<bool, ptyio::PartialWrite> {
        let _input = self.input_lock.lock().expect("input mutex poisoned");
        if self.input_writes.load(Ordering::Acquire) != expected {
            return Ok(false);
        }
        ptyio::write_all(self.fd, data).map(|()| true)
    }

    /// How many writes anyone else has made into this session's input so far.
    pub fn input_writes(&self) -> u64 {
        // Under the lock, so that the count is never read in the middle of a write.
        let _input = self.input_lock.lock().expect("input mutex poisoned");
        self.input_writes.load(Ordering::Acquire)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let master = self.master.lock().expect("master mutex poisoned");
        master.resize(portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    /// Polls the child without blocking. Returns `true` once it has exited; from that point on the
    /// pid must not be signalled again.
    pub fn poll_exit(&self) -> bool {
        if self.reaped.load(Ordering::Acquire) {
            return true;
        }
        let child = self.child.lock().expect("child mutex poisoned");
        self.poll_exit_locked(child)
    }

    /// The body of `poll_exit`, taking the already-held `child` lock. Reaping and the flag that
    /// records it happen under that lock, which is what lets `signal` hold it to keep the child
    /// from being reaped between its check and its `kill`.
    fn poll_exit_locked(
        &self,
        mut child: std::sync::MutexGuard<'_, Box<dyn Child + Send + Sync>>,
    ) -> bool {
        if self.reaped.load(Ordering::Acquire) {
            return true;
        }
        let exited = match child.try_wait() {
            Ok(Some(_)) => true,
            Ok(None) => false,
            // Nothing left to wait on, so treat the child as gone rather than keep signalling a
            // pid we can no longer account for.
            Err(err) => {
                tracing::warn!(session = %self.id, %err, "try_wait failed; treating the session as exited");
                true
            }
        };
        if exited {
            self.reaped.store(true, Ordering::Release);
            drop(child);
            self.clean_scratch_dir();
        }
        exited
    }

    fn clean_scratch_dir(&self) {
        let dir = self
            .scratch_dir
            .lock()
            .expect("scratch dir mutex poisoned")
            .take();
        if let Some(dir) = dir {
            // Every entry in it is either a symlink into the user's own agent directory or a file
            // Octoboard wrote, so removing the directory never touches the user's configuration.
            if let Err(err) = std::fs::remove_dir_all(&dir) {
                tracing::warn!(session = %self.id, path = %dir.display(), %err, "removing the session scratch directory failed");
            }
        }
    }

    /// Asks the agent to exit, escalating to a kill only if it does not. Both signals are gated on
    /// the child not having been reaped, which is what keeps the pid (and the process group that
    /// the `setsid()` in `portable-pty` gave it) from being a number that now belongs to something
    /// else.
    pub fn terminate(&self) {
        if self.poll_exit() {
            return;
        }
        self.signal(libc::SIGTERM, false);

        let deadline = Instant::now() + TERM_GRACE;
        while Instant::now() < deadline {
            if self.poll_exit() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        // The agent did not take the hint. Kill the whole process group this time: `portable-pty`
        // calls `setsid()`, so the agent's pgid is its own pid and its tool subprocesses are in
        // that same group — killing the leader alone would leave them running.
        self.signal(libc::SIGKILL, true);
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if self.poll_exit() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        tracing::warn!(session = %self.id, pid = self.pid, "session process is still alive after SIGKILL");
    }

    /// Kills the session outright, with no graceful period and without taking a single lock — the
    /// path a panic hook uses, where locking anything could deadlock against the thread that is
    /// already going down. It reads only the pid and the reaped flag, so the worst a lost race
    /// costs is a signal to a pid that has just been reaped.
    pub fn kill_hard(&self) {
        self.send_signal(libc::SIGKILL, true);
    }

    /// Signals the process while holding the lock that reaping happens under, so the child cannot
    /// be reaped between the check and the `kill` — which is the whole basis for signalling by
    /// number here: an unreaped child keeps its pid and its process group reserved.
    fn signal(&self, signal: libc::c_int, whole_group: bool) {
        let child = self.child.lock().expect("child mutex poisoned");
        if self.reaped.load(Ordering::Acquire) {
            return;
        }
        self.send_signal(signal, whole_group);
        drop(child);
    }

    fn send_signal(&self, signal: libc::c_int, whole_group: bool) {
        if self.reaped.load(Ordering::Acquire) {
            return;
        }
        let target = if whole_group {
            -(self.pid as i32)
        } else {
            self.pid as i32
        };
        // SAFETY: the pid of a child this struct holds unreaped, so the number has not been
        // recycled — `signal` guarantees that by holding the lock reaping takes; `kill_hard`
        // accepts the narrow race deliberately.
        if unsafe { libc::kill(target, signal) } != 0 {
            let err = io::Error::last_os_error();
            // ESRCH just means it exited in the meantime, which the caller finds out by polling.
            if err.raw_os_error() != Some(libc::ESRCH) {
                tracing::warn!(session = %self.id, pid = self.pid, %err, "signalling the session process failed");
            }
        }
    }
}

/// Whether a chunk written to a session's input is nothing but a terminal's own protocol traffic:
/// focus in/out (`CSI I`, `CSI O`), device attributes (`CSI … c`), cursor position (`CSI … R`),
/// mode reports (`CSI … $ y`), window reports (`CSI … t`), status reports (`CSI … n`), and operating
/// system or device control string replies. Conservative: an empty chunk, any other byte, any other
/// final byte — the arrow keys included — and a sequence cut short all say no.
fn is_terminal_protocol(chunk: &[u8]) -> bool {
    if chunk.is_empty() {
        return false;
    }
    let mut at = 0;
    while at < chunk.len() {
        if chunk[at] != 0x1b {
            return false;
        }
        match chunk.get(at + 1) {
            Some(b'[') => {
                at += 2;
                let start = at;
                while at < chunk.len() && (0x20..=0x3f).contains(&chunk[at]) {
                    at += 1;
                }
                let Some(&final_byte) = chunk.get(at) else {
                    return false;
                };
                let known = match final_byte {
                    b'I' | b'O' | b'c' | b'R' | b't' | b'n' => true,
                    b'y' => chunk[start..at].contains(&b'$'),
                    _ => false,
                };
                if !known {
                    return false;
                }
                at += 1;
            }
            Some(&opener @ (b']' | b'P')) => {
                at += 2;
                // Up to a string terminator (an escape and a backslash), or for an operating
                // system command a bell as well.
                // Nothing but printable bytes in between: a control byte is typed input, not a reply.
                loop {
                    match chunk.get(at) {
                        None => return false,
                        Some(0x07) if opener == b']' => {
                            at += 1;
                            break;
                        }
                        Some(0x1b) if chunk.get(at + 1) == Some(&b'\\') => {
                            at += 2;
                            break;
                        }
                        Some(byte) if *byte < 0x20 => return false,
                        Some(_) => at += 1,
                    }
                }
            }
            _ => return false,
        }
    }
    true
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

/// Spawns the OS thread that reads this session's PTY. A blocking reader on a dedicated thread
/// rather than a tokio task, because the throttling in `on_output` is the mechanism that applies
/// backpressure and must not occupy a runtime worker.
pub fn spawn_reader_thread(session: Arc<LiveSession>, read_buf_bytes: usize) {
    session.reader_running.store(true, Ordering::Release);
    std::thread::spawn(move || {
        let mut buf = vec![0u8; read_buf_bytes];
        loop {
            match ptyio::read_chunk(session.fd(), &mut buf) {
                Ok(0) => break,
                Ok(n) => session.on_output(&buf[..n]),
                Err(err) => {
                    tracing::debug!(session = %session.id, %err, "PTY read failed; closing the reader");
                    break;
                }
            }
        }
        session.reader_running.store(false, Ordering::Release);
        tracing::info!(session = %session.id, "PTY reader thread exiting");
    });
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{is_terminal_protocol, send_with_backpressure, SUBSCRIBER_GRACE};
    use bytes::Bytes;
    use tokio::sync::mpsc;

    #[test]
    fn only_a_terminals_own_protocol_traffic_is_told_apart_from_input() {
        for protocol in [
            &b"\x1b[I"[..],
            b"\x1b[O",
            b"\x1b[I\x1b[O",
            b"\x1b[?1;2c",
            b"\x1b[>0;276;0c",
            b"\x1b[24;80R",
            b"\x1b[?2004;2$y",
            b"\x1b[8;32;120t",
            b"\x1b[0n",
            b"\x1b]11;rgb:0000/0000/0000\x07",
            b"\x1b]11;rgb:0000/0000/0000\x1b\\",
            b"\x1bP>|xterm.js\x1b\\",
            b"\x1b[I\x1b[?1;2c\x1b]10;rgb:ffff/ffff/ffff\x07",
        ] {
            assert!(is_terminal_protocol(protocol), "{protocol:?}");
        }
        for input in [
            &b""[..],
            b"\r",
            b"x",
            b"\x1b[B",
            b"\x1b[A",
            b"\x1b[C",
            b"\x1b[3~",
            b"\x1b",
            b"\x1b[",
            b"\x1b[I\x1b[B",
            b"\x1b[Ix",
            b"\x1b[I\r",
            b"\x1b[?2004;2y",
            b"\x1b[1",
            b"\x1b]11;rgb:0000",
            b"\x1b]11;rgb\r:0000\x07",
            b"\x1b]11;rgb\x1b[B:0000\x07",
            b"\x1bP>|xterm\x07.js\x1b\\",
            b"\x1bP>|xterm.js",
            b"\x1bOB",
        ] {
            assert!(!is_terminal_protocol(input), "{input:?}");
        }
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
