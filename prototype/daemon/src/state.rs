//! Shared daemon state: the session table, and the per-session PTY plumbing.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Mutex, RwLock};

use portable_pty::{Child, MasterPty};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::protocol::{Agent, ControlToClient, Role, SessionState, StatusSource};
use crate::ringbuf::RingBuffer;

/// Size of the per-session terminal replay buffer (investigation item T3).
pub const RING_CAPACITY: usize = 2 * 1024 * 1024;

/// Capacity of the per-session live-output broadcast channel. T2 measured that the PTY reader
/// thread always outruns this channel — regardless of this constant's size, an unthrottled flood
/// disconnects a client in about 0.2 s at every frame size tested, because an OS thread doing
/// `memcpy` vastly outpaces what a client can drain. The real fix is the PTY reader taking
/// backpressure from this channel instead of running ahead of it, which milestone 01 owes (see
/// the technical-validation plan, T2); this constant only changes how soon a flood disconnects a
/// client, not whether one does.
const TERM_BROADCAST_CAPACITY: usize = 4096;

pub struct Session {
    pub id: Uuid,
    pub agent: Agent,
    pub role: Role,
    pub pid: u32,
    pub agent_session_id: Mutex<Option<String>>,

    child: Mutex<Box<dyn Child + Send + Sync>>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,

    /// Guards the ring buffer *and* doubles as the subscribe point for `term_tx`: a client must
    /// take this lock, subscribe, copy the snapshot, then release — see `attach_term`. That is
    /// what makes the replay boundary exact (no gap, no duplicate) even while the reader thread
    /// is actively pushing bytes.
    ring: Mutex<RingBuffer>,
    term_tx: broadcast::Sender<bytes::Bytes>,

    pub status: Mutex<SessionState>,

    /// Set once the exit watcher has seen the process go. After that the pid is free for the OS to
    /// reuse, and `kill` here signals a whole process group, so a stale pid can take out something
    /// that has nothing to do with Octoboard. This is not theoretical: during milestone 00 an
    /// unguarded version of this killed the spawn helper of the editor the daemon was launched
    /// from, leaving that process unable to start *any* child — its terminal included — until it
    /// was restarted.
    exited: std::sync::atomic::AtomicBool,
    pub message_queue: Mutex<Vec<String>>,
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        agent: Agent,
        role: Role,
        pid: u32,
        child: Box<dyn Child + Send + Sync>,
        writer: Box<dyn Write + Send>,
        master: Box<dyn MasterPty + Send>,
    ) -> Self {
        let (term_tx, _) = broadcast::channel(TERM_BROADCAST_CAPACITY);
        Self {
            id,
            agent,
            role,
            pid,
            agent_session_id: Mutex::new(None),
            child: Mutex::new(child),
            writer: Mutex::new(writer),
            master: Mutex::new(master),
            ring: Mutex::new(RingBuffer::new(RING_CAPACITY)),
            term_tx,
            status: Mutex::new(SessionState::Working),
            exited: std::sync::atomic::AtomicBool::new(false),
            message_queue: Mutex::new(Vec::new()),
        }
    }

    /// Called from the PTY reader thread for every chunk read from the child.
    pub fn on_pty_output(&self, data: &[u8]) {
        let mut ring = self.ring.lock().expect("ring mutex poisoned");
        ring.push(data);
        // Ignore the "no subscribers" error: nobody has a terminal socket open right now, and
        // the bytes are already safe in the ring buffer for the next attach.
        let _ = self.term_tx.send(bytes::Bytes::copy_from_slice(data));
    }

    /// Attaches a new terminal client: returns the replay snapshot and a receiver that is
    /// guaranteed to pick up with the very next byte after the snapshot — see the `ring` field
    /// doc for why holding this one lock across both steps is what makes that exact.
    pub fn attach_term(&self) -> (Vec<u8>, broadcast::Receiver<bytes::Bytes>) {
        let ring = self.ring.lock().expect("ring mutex poisoned");
        let rx = self.term_tx.subscribe();
        (ring.snapshot(), rx)
    }

    /// Prototype-only: a blocking `write_all` on whatever thread calls it. The settled rule (M1,
    /// and the "Writing into a running session" bullet of `docs/mvp.md` section 6) is a
    /// non-blocking partial-write-and-retry loop in slices, because a macOS PTY master accepts
    /// only about 1022 bytes before `EAGAIN` while the child is not draining. The blocking form
    /// delivers correctly, which is all the validation needed; it just parks the caller.
    pub fn write_input(&self, data: &[u8]) -> std::io::Result<()> {
        let mut writer = self.writer.lock().expect("writer mutex poisoned");
        writer.write_all(data)?;
        writer.flush()
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

    /// Records that the process has been observed to exit. Called by the exit watcher, next to
    /// unregistering the pid, so the two can never disagree.
    pub fn mark_exited(&self) {
        self.exited
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub fn has_exited(&self) -> bool {
        self.exited.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Kills the whole process group, not just the session leader: `portable-pty` calls `setsid()`
    /// on each agent, so its pgid equals its pid, and every tool subprocess plus the injected
    /// `obd-proto mcp` child live in that same group. Killing only the pid (what
    /// `portable_pty::Child::kill` does) would leave all of those running.
    ///
    /// Refuses once the process is known to have exited: the pid is reusable by then, and a whole
    /// group is a wide thing to signal at a stale number. Sessions are never removed from the
    /// registry, so the UI keeps offering Kill for a session that finished long ago.
    pub fn kill(&self) -> std::io::Result<()> {
        if self.has_exited() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "session already exited",
            ));
        }
        std::process::Command::new("kill")
            .args(["-9", &format!("-{}", self.pid)])
            .stderr(std::process::Stdio::null())
            .status()?;
        Ok(())
    }

    pub fn try_wait(&self) -> std::io::Result<Option<portable_pty::ExitStatus>> {
        let mut child = self.child.lock().expect("child mutex poisoned");
        child.try_wait()
    }
}

/// Global registry of agent-process PIDs, consulted by the watchdog / panic hook so that every
/// exit path (clean shutdown, parent-gone, panic) can kill every child it started. Plain PIDs
/// rather than `Session` handles because the panic hook must not touch locks that might already
/// be held by the panicking thread.
pub static CHILD_PIDS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

pub fn register_child_pid(pid: u32) {
    CHILD_PIDS
        .lock()
        .expect("CHILD_PIDS mutex poisoned")
        .push(pid);
}

pub fn unregister_child_pid(pid: u32) {
    let mut pids = CHILD_PIDS.lock().expect("CHILD_PIDS mutex poisoned");
    pids.retain(|&p| p != pid);
}

/// Kills every registered agent process. Safe to call from a panic hook: only touches a
/// `std::sync::Mutex` (poison-tolerant below) and shells out to `kill`, no async runtime
/// involvement.
pub fn kill_all_children() {
    let pids = match CHILD_PIDS.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    for pid in pids {
        // Negative pid targets the whole process group: `portable-pty` calls `setsid()` on each
        // agent, so a plain `kill -9 <pid>` would only take down the session leader and leave its
        // tool subprocesses and the injected `obd-proto mcp` child running.
        //
        // stderr is discarded because this races with agents exiting on their own: a group that
        // emptied a moment earlier makes `kill` complain, which is expected rather than a failure
        // worth surfacing. The result is ignored for the same reason.
        let _ = std::process::Command::new("kill")
            .args(["-9", &format!("-{pid}")])
            .stderr(std::process::Stdio::null())
            .status();
    }
}

pub struct AppState {
    pub sessions: RwLock<HashMap<Uuid, std::sync::Arc<Session>>>,
    pub control_tx: broadcast::Sender<ControlToClient>,
    pub port: u16,
    pub self_exe: String,
    /// When true, launch agents via the `$SHELL -l -i -c '<command>'` fallback instead of
    /// exec'ing the resolved binary directly with the snapshotted environment.
    pub login_shell_launch: bool,
    /// Size of the buffer `term.rs`'s PTY reader thread reads into. Configurable (CLI flag in
    /// `main.rs`, defaulting to 8 KiB) only so T2's bench can sweep the daemon-to-client framing
    /// size directly — it has no other reason to vary.
    pub pty_read_buf_bytes: usize,
}

impl AppState {
    pub fn new(
        port: u16,
        self_exe: String,
        login_shell_launch: bool,
        pty_read_buf_bytes: usize,
    ) -> Self {
        let (control_tx, _) = broadcast::channel(1024);
        Self {
            sessions: RwLock::new(HashMap::new()),
            control_tx,
            port,
            self_exe,
            login_shell_launch,
            pty_read_buf_bytes,
        }
    }

    pub fn broadcast(&self, event: ControlToClient) {
        let _ = self.control_tx.send(event);
    }

    pub fn get_session(&self, id: &Uuid) -> Option<std::sync::Arc<Session>> {
        self.sessions
            .read()
            .expect("sessions lock poisoned")
            .get(id)
            .cloned()
    }

    /// Updates a session's last-known status and, if it just became idle, flushes anything
    /// queued by `send_message` while it was busy (PROTOCOL.md `message_queued`).
    pub fn set_status(
        &self,
        session: &std::sync::Arc<Session>,
        state: SessionState,
        source: StatusSource,
        raw: serde_json::Value,
    ) {
        *session.status.lock().expect("status mutex poisoned") = state;
        self.broadcast(ControlToClient::Status {
            session: session.id,
            state,
            source,
            raw,
        });
        if state == SessionState::Idle {
            self.flush_queue(session);
        }
    }

    pub fn flush_queue(&self, session: &std::sync::Arc<Session>) {
        let queued: Vec<String> = {
            let mut q = session.message_queue.lock().expect("queue mutex poisoned");
            std::mem::take(&mut *q)
        };
        for text in queued {
            if let Err(err) = crate::term::send_message(session, &text) {
                tracing::warn!(session = %session.id, %err, "failed to flush queued message");
            }
        }
    }
}
