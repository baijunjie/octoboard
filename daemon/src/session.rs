//! One running agent process: its PTY, its output fan-out, and how it is stopped.

use std::io;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use portable_pty::{Child, MasterPty};
use tokio::sync::mpsc;

use crate::protocol::Agent;
use crate::ptyio;
use crate::ringbuf::RingBuffer;

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

struct Subscriber {
    id: u64,
    tx: mpsc::Sender<Bytes>,
}

/// The ring buffer and the subscriber list share one lock so that an attaching client can take the
/// replay snapshot and start receiving live output with no gap and no duplicate, even while the
/// reader thread is pushing bytes.
struct Fan {
    ring: RingBuffer,
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
    /// safe here. Milestone 00's prototype signalled a process group by a pid it no longer owned
    /// and killed the spawn helper of the editor the daemon ran under, leaving that process unable
    /// to start any child until it was restarted.
    child: Mutex<Box<dyn Child + Send + Sync>>,
    reaped: AtomicBool,
    fan: Mutex<Fan>,
    /// Per-session scratch directory (Grok's `GROK_HOME` symlink farm), removed when the session's
    /// process is gone.
    scratch_dir: Mutex<Option<std::path::PathBuf>>,
}

impl LiveSession {
    pub fn new(
        id: String,
        agent: Agent,
        pid: u32,
        fd: RawFd,
        master: Box<dyn MasterPty + Send>,
        child: Box<dyn Child + Send + Sync>,
        scratch_dir: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            id,
            agent,
            pid,
            fd,
            master: Mutex::new(master),
            child: Mutex::new(child),
            reaped: AtomicBool::new(false),
            fan: Mutex::new(Fan {
                ring: RingBuffer::new(RING_CAPACITY),
                subscribers: Vec::new(),
                next_id: 0,
            }),
            scratch_dir: Mutex::new(scratch_dir),
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

    /// Called from the PTY reader thread for every chunk read. Blocks while a slow client catches
    /// up, which is how backpressure reaches the agent: with nobody reading the PTY, its own writes
    /// block. Milestone 00 measured the alternative — a reader that always runs ahead fills any
    /// bounded channel within a fraction of a second under heavy output and the client is dropped,
    /// at every frame size tested.
    pub fn on_output(&self, data: &[u8]) {
        let chunk = Bytes::copy_from_slice(data);
        let targets: Vec<(u64, mpsc::Sender<Bytes>)> = {
            let mut fan = self.fan.lock().expect("fan mutex poisoned");
            fan.ring.push(data);
            fan.subscribers
                .iter()
                .map(|sub| (sub.id, sub.tx.clone()))
                .collect()
        };

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

    pub fn write_input(&self, data: &[u8]) -> io::Result<()> {
        ptyio::write_all(self.fd, data)
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
        tracing::info!(session = %session.id, "PTY reader thread exiting");
    });
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{send_with_backpressure, SUBSCRIBER_GRACE};
    use bytes::Bytes;
    use tokio::sync::mpsc;

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
