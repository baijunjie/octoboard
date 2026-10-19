//! A process the daemon runs on a PTY, as an agent session and a shell terminal each have one:
//! starting it, the output fan-out to attached terminal clients over a replay ring and a parsed
//! screen, the reader thread, input, resizing, ending it, and its place in the crash cleanup. What
//! the process is — an agent or a shell — is the owner's business, not this module's.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tokio::sync::mpsc;

use crate::crash_cleanup;
use crate::ptyio;
use crate::ringbuf::RingBuffer;
use crate::terminal_snapshot::{TerminalSnapshot, MAX_COLS, MAX_ROWS};

/// Terminal replay buffer per process.
pub const RING_CAPACITY: usize = 2 * 1024 * 1024;

/// Size the PTY starts at. The UI sends a `resize` as soon as it attaches; this only has to be
/// something sane for the output a process produces before that.
pub const INITIAL_COLS: u16 = 120;
pub const INITIAL_ROWS: u16 = 32;

/// The terminal the UI actually renders with, for a process's `TERM`. A program that inherits
/// anything less — notably the snapshot shell's `TERM=dumb` — downgrades its own renderer, colour
/// and mouse handling.
pub const TERM: &str = "xterm-256color";

/// Size of the buffer the PTY reader thread reads into, which also bounds the size of every
/// daemon-to-client terminal frame. Not a tuning lever: sweeping it from 4 KiB to 256 KiB made no
/// measurable difference to throughput at two very different source rates.
pub const READ_BUF: usize = 8 * 1024;

/// Frames a single attached client may fall behind by before the PTY reader starts waiting on it.
const SUBSCRIBER_QUEUE: usize = 64;

/// How long the reader waits for one client to drain before dropping it. Until this elapses the
/// reader does not read from the PTY at all, which is the point: the process's own writes block,
/// so output is throttled at the source instead of being buffered without bound in the daemon. A
/// client that has not taken a single frame in this long is gone or wedged, and dropping it (its
/// socket closes, and it reconnects to a fresh replay) is better than throttling the process for
/// it.
const SUBSCRIBER_GRACE: Duration = Duration::from_secs(3);

/// How long [`PtyCore::stop`] waits for the process to take the polite signal before killing it.
const STOP_GRACE: Duration = Duration::from_secs(3);

/// How long the PTY reader is waited for to take what the process wrote just before it exited.
/// Bounded because the reader only finishes once nothing holds the PTY open, and a background
/// process the process started may hold it long after the process is gone.
pub const FINAL_OUTPUT_DRAIN: Duration = Duration::from_millis(500);

struct Subscriber {
    id: u64,
    tx: mpsc::Sender<Bytes>,
}

/// Raw replay, parsed screen and subscribers share one lock: attaching has no output gap or
/// duplicate, and freezing captures one exact boundary before exit cleanup can change the screen.
struct Fan {
    ring: RingBuffer,
    terminal: TerminalSnapshot,
    /// Every byte ever pushed into `ring`, which it forgets and this does not: what lets a caller
    /// mark a point in the output and later read exactly what came after it.
    total: u64,
    subscribers: Vec<Subscriber>,
    next_id: u64,
    /// No client follows the output any more; see [`PtyCore::close_output`].
    closed: bool,
}

/// A process just started on a PTY, by [`spawn`].
pub struct Spawned {
    pub pid: u32,
    /// The master descriptor, already in non-blocking mode.
    pub fd: RawFd,
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn Child + Send + Sync>,
    wake: WakePipe,
}

/// What the reader thread waits on beside the PTY, so that it can be told to stop without the
/// PTY being closed under it: a descriptor another thread is polling must never be closed, since
/// its number can be reused at once for something else.
struct WakePipe {
    read: OwnedFd,
    write: OwnedFd,
}

impl WakePipe {
    fn new() -> io::Result<Self> {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: a two-element array is exactly what `pipe` writes into.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `pipe` succeeded, so both are open descriptors owned by nothing else.
        let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        for fd in [read.as_raw_fd(), write.as_raw_fd()] {
            // Not inherited by what the daemon starts. `portable-pty` closes stray descriptors in
            // its children anyway; a child started any other way would hold the pipe open.
            // SAFETY: `fd` is open for as long as this call.
            if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        ptyio::set_nonblocking(write.as_raw_fd())?;
        Ok(Self { read, write })
    }

    /// Makes the read end readable for good. Never blocks: once the pipe holds a byte, a full
    /// pipe is as good as another byte.
    fn wake(&self) {
        // SAFETY: one byte from a valid buffer into a descriptor this struct owns.
        unsafe { libc::write(self.write.as_raw_fd(), [1u8].as_ptr().cast(), 1) };
    }
}

/// Opens a PTY of `rows` by `cols` and starts `command` on it. `portable-pty` makes the process
/// the leader of a session of its own (`setsid()`) with the PTY as its controlling terminal, so it
/// is outside the daemon's process group and its pgid is its own pid.
pub fn spawn(command: CommandBuilder, rows: u16, cols: u16) -> Result<Spawned> {
    let pty = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("opening a PTY")?;
    let child = pty.slave.spawn_command(command)?;
    let pid = child
        .process_id()
        .ok_or_else(|| anyhow!("the spawned process has no pid"))?;
    let fd = pty
        .master
        .as_raw_fd()
        .ok_or_else(|| anyhow!("the PTY master has no file descriptor"))?;
    ptyio::set_nonblocking(fd).context("putting the PTY master into non-blocking mode")?;
    let wake = WakePipe::new().context("opening the reader's wake pipe")?;
    Ok(Spawned {
        pid,
        fd,
        master: pty.master,
        child,
        wake,
    })
}

pub struct PtyCore {
    /// The owner's id, for the log.
    label: String,
    pub pid: u32,
    /// The PTY master descriptor, in non-blocking mode, open for as long as `master` holds it.
    /// Reads and writes both go through it; `master` is kept so the window can be resized, and so
    /// [`Self::hang_up`] can close it.
    fd: RawFd,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    /// [`Self::hang_up`] has closed `master`: `fd` may since name something else. Set under the
    /// input lock, which every write checks it under.
    hung_up: AtomicBool,
    wake: WakePipe,
    /// The reader thread, kept to be joined when the PTY is hung up.
    reader: Mutex<Option<JoinHandle<()>>>,
    /// Held, and deliberately never reaped until the exit watcher observes the exit: an unreaped
    /// child keeps its pid and process group reserved, which is what makes signalling by number
    /// safe here. Signalling a process group by a pid that had already been recycled is not a
    /// theoretical hazard: it once killed the spawn helper of the editor the daemon ran under,
    /// leaving that process unable to start any child at all until it was restarted.
    child: Mutex<Box<dyn Child + Send + Sync>>,
    reaped: AtomicBool,
    /// The child's place in the cleanup that kills the daemon's processes when it goes down, held
    /// from [`Self::enter_crash_cleanup`] until the child has exited, and given up before it is
    /// reaped.
    crash_cleanup: Mutex<Option<crash_cleanup::Entry>>,
    fan: Mutex<Fan>,
    /// A reader thread is reading the PTY into `fan`; see [`spawn_reader_thread`].
    reader_running: AtomicBool,
    /// [`Self::keep_final_output`] has run. A lock rather than a flag, so that a second caller
    /// returns only once the first has finished.
    final_output_kept: Mutex<bool>,
    /// A scratch directory of the process's own, removed once the process is gone.
    scratch_dir: Mutex<Option<std::path::PathBuf>>,
    /// Whether what the process leaves running in its session is killed once it exits; see
    /// [`NewPty::kill_session_leftovers`].
    kill_session_leftovers: bool,
    /// Held across every write into the input, and by a freeze, so that a write and the capture
    /// boundary of a frozen screen never interleave. An owner that keeps bookkeeping about its
    /// writes takes it for that bookkeeping too; see [`Self::lock_input`].
    input_lock: Mutex<()>,
}

/// Everything a [`PtyCore`] is built from: a [`Spawned`] process, named for the log, with the
/// scratch directory that goes when it does.
pub struct NewPty {
    pub label: String,
    pub spawned: Spawned,
    pub scratch_dir: Option<std::path::PathBuf>,
    /// Kill every process still in the process's session once it has exited, including helpers
    /// that moved to a process group of their own. Left off, whatever the process started and
    /// detached (`nohup`, `disown`) keeps running after it.
    pub kill_session_leftovers: bool,
}

impl PtyCore {
    pub fn new(new: NewPty) -> Self {
        let spawned = new.spawned;
        let size = spawned.master.get_size().unwrap_or(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        });
        Self {
            label: new.label,
            pid: spawned.pid,
            fd: spawned.fd,
            master: Mutex::new(Some(spawned.master)),
            hung_up: AtomicBool::new(false),
            wake: spawned.wake,
            reader: Mutex::new(None),
            child: Mutex::new(spawned.child),
            reaped: AtomicBool::new(false),
            crash_cleanup: Mutex::new(None),
            fan: Mutex::new(Fan {
                ring: RingBuffer::new(RING_CAPACITY),
                terminal: TerminalSnapshot::new(size.rows, size.cols),
                total: 0,
                subscribers: Vec::new(),
                next_id: 0,
                closed: false,
            }),
            reader_running: AtomicBool::new(false),
            final_output_kept: Mutex::new(false),
            scratch_dir: Mutex::new(new.scratch_dir),
            kill_session_leftovers: new.kill_session_leftovers,
            input_lock: Mutex::new(()),
        }
    }

    /// Registers a terminal client. The returned snapshot is everything buffered up to this
    /// instant, preceded by the terminal's current modes once the buffer no longer holds the
    /// start, and the receiver picks up with the very next byte after it. Once frozen, returns the
    /// archived screen with a closed receiver, so exit cleanup cannot replace it; once the output
    /// is closed, the replay with a closed receiver.
    pub fn attach(&self) -> (Vec<u8>, mpsc::Receiver<Bytes>) {
        let (tx, rx) = mpsc::channel(SUBSCRIBER_QUEUE);
        let mut fan = self.fan.lock().expect("fan mutex poisoned");
        if let Some(output) = fan.terminal.frozen() {
            return (output.to_vec(), rx);
        }
        let mut snapshot = fan.ring.snapshot();
        if fan.total > snapshot.len() as u64 {
            // The ring has lost its head, and with it the mode sequences a program sends once at
            // startup. The parsed terminal still knows them, and they describe the end of the
            // window, which is where the replay leaves the client, so any toggles inside the
            // window land on the same state.
            let mut replay = fan.terminal.mode_preamble();
            replay.append(&mut snapshot);
            snapshot = replay;
        }
        if fan.closed {
            return (snapshot, rx);
        }
        let id = fan.next_id;
        fan.next_id += 1;
        fan.subscribers.push(Subscriber { id, tx });
        (snapshot, rx)
    }

    /// Ends every attached client's stream once the output it follows is over, and makes every
    /// later [`Self::attach`] a replay alone. Output that still arrives is kept for that replay.
    pub fn close_output(&self) {
        let mut fan = self.fan.lock().expect("fan mutex poisoned");
        fan.closed = true;
        fan.subscribers.clear();
    }

    /// The tail of what this process has printed, at most `max_bytes` of it. Taken from the same
    /// ring buffer a terminal client replays, so it is exactly what the user would see — raw PTY
    /// bytes, escape sequences included, which the caller is responsible for making readable.
    pub fn recent_output(&self, max_bytes: usize) -> Vec<u8> {
        self.fan
            .lock()
            .expect("fan mutex poisoned")
            .ring
            .tail(max_bytes)
    }

    /// Freezes a readable screen before termination can clear it. Input and the parser share
    /// the capture boundary; later cleanup output continues to reach live subscribers.
    pub fn freeze_output(&self) {
        let _input = self.lock_input();
        self.fan
            .lock()
            .expect("fan mutex poisoned")
            .terminal
            .freeze();
    }

    /// Freezes before a dormant record can become observable. The closure only updates storage;
    /// publishing happens after this returns. A failed update rolls back only this call's freeze,
    /// while the input lock keeps termination or another update from claiming that same screen.
    pub fn freeze_output_for(
        &self,
        update: impl FnOnce() -> anyhow::Result<bool>,
    ) -> anyhow::Result<bool> {
        let _input = self.lock_input();
        let created = self
            .fan
            .lock()
            .expect("fan mutex poisoned")
            .terminal
            .freeze();
        let result = update();
        if created && !matches!(result, Ok(true)) {
            self.fan
                .lock()
                .expect("fan mutex poisoned")
                .terminal
                .discard_frozen();
        }
        result
    }

    pub fn frozen_output(&self) -> Option<Vec<u8>> {
        self.fan
            .lock()
            .expect("fan mutex poisoned")
            .terminal
            .frozen()
            .map(<[u8]>::to_vec)
    }

    pub fn is_frozen(&self) -> bool {
        self.fan
            .lock()
            .expect("fan mutex poisoned")
            .terminal
            .frozen()
            .is_some()
    }

    /// Hands the frozen screen, or the final parsed screen, to `keep` after exit, only once:
    /// the exit watcher and a shutdown can both see the same exit. A later caller waits for
    /// the first to finish, so a shutdown that returns has its output kept whichever of the two got
    /// there first. Waits up to `FINAL_OUTPUT_DRAIN` for the reader to finish before taking the
    /// buffer, since what a process prints on its way out can still be in the PTY when its exit is
    /// observed. **Blocks** for that long.
    pub fn keep_final_output(&self, keep: impl FnOnce(&[u8])) {
        let mut kept = self
            .final_output_kept
            .lock()
            .expect("final output mutex poisoned");
        if *kept {
            return;
        }
        self.wait_for_reader(FINAL_OUTPUT_DRAIN);
        let output = self
            .fan
            .lock()
            .expect("fan mutex poisoned")
            .terminal
            .output();
        keep(&output);
        *kept = true;
    }

    /// Waits up to `timeout` for the reader thread to have read everything the PTY will give it.
    /// **Blocks** for that long.
    pub fn wait_for_reader(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while self.reader_running.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// How many bytes this process has printed in all. A mark for [`Self::output_since`].
    pub fn output_total(&self) -> u64 {
        self.fan.lock().expect("fan mutex poisoned").total
    }

    /// What this process printed after `mark`, as taken from [`Self::output_total`] — less of it if
    /// the ring buffer has already let some go.
    pub fn output_since(&self, mark: u64) -> Vec<u8> {
        let fan = self.fan.lock().expect("fan mutex poisoned");
        let wanted = fan.total.saturating_sub(mark);
        fan.ring.tail(usize::try_from(wanted).unwrap_or(usize::MAX))
    }

    /// Takes one chunk read from the PTY into the replay, the parsed screen and every attached
    /// client. `before_delivery` is told the new total once the chunk is buffered and before any
    /// client is handed it, and runs outside every lock here.
    ///
    /// Blocks while a slow client catches up, which is how backpressure reaches the process: with
    /// nobody reading the PTY, its own writes block. The alternative was measured: a reader that
    /// always runs ahead fills any bounded channel within a fraction of a second under heavy output
    /// and the client is dropped, at every frame size tested.
    pub fn push_output(&self, data: &[u8], before_delivery: impl FnOnce(u64)) {
        let chunk = Bytes::copy_from_slice(data);
        let (targets, total): (Vec<(u64, mpsc::Sender<Bytes>)>, u64) = {
            let mut fan = self.fan.lock().expect("fan mutex poisoned");
            fan.ring.push(data);
            fan.terminal.process(data);
            fan.total += data.len() as u64;
            let targets = fan
                .subscribers
                .iter()
                .map(|sub| (sub.id, sub.tx.clone()))
                .collect();
            (targets, fan.total)
        };

        before_delivery(total);

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

    /// The lock every write into the input is made under. An owner takes it to keep its own
    /// bookkeeping about a write in step with the write, and then writes with
    /// [`Self::write_locked`].
    pub fn lock_input(&self) -> MutexGuard<'_, ()> {
        self.input_lock.lock().expect("input mutex poisoned")
    }

    /// Writes raw input under the lock `input` is a guard of, taken with [`Self::lock_input`]. The
    /// error says how many bytes went in, because a caller deciding whether to retry has to know:
    /// whatever was accepted is already in the child's input buffer.
    pub fn write_locked(
        &self,
        _input: &MutexGuard<'_, ()>,
        data: &[u8],
    ) -> Result<(), ptyio::PartialWrite> {
        if self.hung_up.load(Ordering::Acquire) {
            return Err(ptyio::PartialWrite {
                written: 0,
                error: io::Error::new(io::ErrorKind::BrokenPipe, "the terminal has been hung up"),
            });
        }
        ptyio::write_all(self.fd, data)
    }

    /// Writes raw input, as [`Self::write_locked`] does, taking the lock itself.
    pub fn write(&self, data: &[u8]) -> Result<(), ptyio::PartialWrite> {
        let input = self.lock_input();
        self.write_locked(&input, data)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        anyhow::ensure!(
            (2..=MAX_COLS).contains(&cols) && (1..=MAX_ROWS).contains(&rows),
            "terminal dimensions exceed the supported range"
        );
        let mut fan = self.fan.lock().expect("fan mutex poisoned");
        anyhow::ensure!(
            fan.terminal.frozen().is_none(),
            "the terminal's screen has been frozen"
        );
        let master = self.master.lock().expect("master mutex poisoned");
        let Some(master) = master.as_ref() else {
            anyhow::bail!("the terminal has been hung up");
        };
        master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        fan.terminal.resize(rows, cols);
        Ok(())
    }

    /// The process group in the foreground of the PTY, the one the terminal's input and its
    /// signals go to, or `None` when the system cannot say. Only meaningful while the process
    /// runs.
    pub fn foreground_process_group(&self) -> Option<libc::pid_t> {
        let master = self.master.lock().expect("master mutex poisoned");
        master.as_ref()?;
        // SAFETY: `tcgetpgrp` only reads the terminal's state, through a descriptor that holding
        // `master` keeps open.
        let group = unsafe { libc::tcgetpgrp(self.fd) };
        (group > 0).then_some(group)
    }

    /// Whether [`Self::hang_up`] has closed the PTY.
    #[cfg(test)]
    pub fn is_hung_up(&self) -> bool {
        self.hung_up.load(Ordering::Acquire)
    }

    /// Hangs the terminal up, as closing a terminal window does: stops the reader thread, lets
    /// every attached client go, and closes the PTY master, after which the kernel hangs up the
    /// other side — a process still on it gets `SIGHUP` where it is in the terminal's foreground,
    /// and an error on every read and write from then on. Nothing reads the PTY afterwards, so
    /// whatever still holds the other side open no longer keeps this core alive. On Linux nothing
    /// revokes the terminal when its process exits, so this is the only thing that ends the
    /// reader while a leftover job holds the PTY open; on macOS that exit already ends it, so a
    /// test there cannot show the need. **Blocks** until the reader thread has finished, which may
    /// wait out an attached client that has stopped draining (see `SUBSCRIBER_GRACE`).
    pub fn hang_up(&self) {
        self.close_output();
        let reader = self.reader.lock().expect("reader mutex poisoned").take();
        if let Some(reader) = reader {
            self.wake.wake();
            if reader.thread().id() != std::thread::current().id() && reader.join().is_err() {
                tracing::warn!(terminal = %self.label, "the PTY reader thread panicked");
            }
        }
        let _input = self.lock_input();
        let mut master = self.master.lock().expect("master mutex poisoned");
        self.hung_up.store(true, Ordering::Release);
        master.take();
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
    fn poll_exit_locked(&self, mut child: MutexGuard<'_, Box<dyn Child + Send + Sync>>) -> bool {
        if self.reaped.load(Ordering::Acquire) {
            return true;
        }
        // The crash cleanup signals by number, so the child leaves it while it is exited but not
        // yet reaped, its pid still reserved. When `waitid` cannot tell, `try_wait` decides, and
        // the child leaves only after it is reaped.
        match self.exited_unreaped() {
            Some(false) => return false,
            Some(true) => {
                self.leave_crash_cleanup();
                if self.kill_session_leftovers {
                    self.kill_session_members();
                }
            }
            None => {}
        }
        let exited = match child.try_wait() {
            Ok(Some(_)) => true,
            Ok(None) => false,
            // Nothing left to wait on, so treat the child as gone rather than keep signalling a
            // pid we can no longer account for.
            Err(err) => {
                tracing::warn!(terminal = %self.label, %err, "try_wait failed; treating the process as exited");
                true
            }
        };
        if exited {
            self.reaped.store(true, Ordering::Release);
            self.leave_crash_cleanup();
            drop(child);
            self.clean_scratch_dir();
        }
        exited
    }

    /// Kills every process left in the child's session, which the child itself leads. Called only
    /// while the child is exited but not reaped: its zombie keeps its pid, and with it the session
    /// id, from being handed to anything else, so a process found in that session is the child's.
    /// This reaches what a signal to the child's process group does not: a helper that moved to a
    /// process group of its own still has the session. One that called `setsid()` has left the
    /// session and is out of reach.
    fn kill_session_members(&self) {
        // A process killed in one pass may have forked just before it died, and the child is in
        // the session too, so passes repeat until one signals no process it had not already
        // signalled. A killed process can stay in the list as a zombie, which is why those are
        // remembered.
        const MAX_PASSES: usize = 3;
        let leader = self.pid as libc::pid_t;
        let mut signalled = std::collections::HashSet::new();
        for _ in 0..MAX_PASSES {
            let Some(candidates) = session_candidates() else {
                tracing::warn!(terminal = %self.label, pid = self.pid, killed = signalled.len(), "listing processes failed; the sweep of the exited process's session may be incomplete");
                return;
            };
            let before = signalled.len();
            for pid in candidates {
                if pid == leader || signalled.contains(&pid) {
                    continue;
                }
                // SAFETY: `getsid` only reads; a pid that is gone in the meantime reports an error.
                if unsafe { libc::getsid(pid) } != leader {
                    continue;
                }
                // SAFETY: the pid was in the session of the unreaped leader a moment ago. It may
                // have exited since, which is the `ESRCH` that is ignored.
                if unsafe { libc::kill(pid, libc::SIGKILL) } == 0 {
                    signalled.insert(pid);
                }
            }
            if signalled.len() == before {
                break;
            }
        }
        let killed = signalled.len();
        if killed > 0 {
            tracing::info!(terminal = %self.label, pid = self.pid, killed, "killed processes the exited process left in its session");
        } else {
            tracing::debug!(terminal = %self.label, pid = self.pid, "no process left in the exited process's session");
        }
    }

    /// Whether the child has exited, found out without reaping it (`WNOWAIT`), or `None` when
    /// `waitid` cannot say.
    fn exited_unreaped(&self) -> Option<bool> {
        // SAFETY: a zeroed `siginfo_t` is a valid buffer for `waitid` to fill in, and it is only
        // read once `waitid` has succeeded.
        unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            let flags = libc::WEXITED | libc::WNOHANG | libc::WNOWAIT;
            if libc::waitid(libc::P_PID, self.pid as libc::id_t, &mut info, flags) != 0 {
                return None;
            }
            // With `WNOHANG`, a child that has not exited leaves `si_pid` as it was: 0.
            Some(info.si_pid() != 0)
        }
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
                tracing::warn!(terminal = %self.label, path = %dir.display(), %err, "removing the scratch directory failed");
            }
        }
    }

    /// Asks the process to exit with `polite`, escalating to a kill only if it does not. Both
    /// signals are gated on the child not having been reaped, which is what keeps the pid (and the
    /// process group that the `setsid()` in `portable-pty` gave it) from being a number that now
    /// belongs to something else. **Blocks** until the process is gone, or a few seconds past the
    /// kill.
    pub fn stop(&self, polite: libc::c_int) {
        if self.poll_exit() {
            return;
        }
        self.signal(polite, false);

        let deadline = Instant::now() + STOP_GRACE;
        while Instant::now() < deadline {
            if self.poll_exit() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        // The process did not take the hint. Kill the whole process group this time:
        // `portable-pty` calls `setsid()`, so the process's pgid is its own pid and the children
        // it did not move to a group of their own are in that same group — killing the leader
        // alone would leave them running.
        self.signal(libc::SIGKILL, true);
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if self.poll_exit() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        tracing::warn!(terminal = %self.label, pid = self.pid, "the process is still alive after SIGKILL");
    }

    /// Enters the child in the cleanup that kills the daemon's processes when it goes down
    /// (`crate::crash_cleanup`). Under the lock reaping takes, so a child reaped in the meantime
    /// is never entered: the cleanup signals by number.
    pub fn enter_crash_cleanup(&self) {
        let child = self.child.lock().expect("child mutex poisoned");
        if self.reaped.load(Ordering::Acquire) {
            return;
        }
        let entry = crash_cleanup::register(self.pid, self.kill_session_leftovers);
        if entry.is_none() {
            tracing::warn!(terminal = %self.label, "the crash cleanup is full; this process would outlive a daemon crash");
        }
        *self
            .crash_cleanup
            .lock()
            .expect("crash cleanup mutex poisoned") = entry;
        drop(child);
    }

    /// Takes the child out of the crash cleanup; a no-op once it is out.
    fn leave_crash_cleanup(&self) {
        self.crash_cleanup
            .lock()
            .expect("crash cleanup mutex poisoned")
            .take();
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
        // recycled — `signal` guarantees that by holding the lock reaping takes.
        if unsafe { libc::kill(target, signal) } != 0 {
            let err = io::Error::last_os_error();
            // ESRCH just means it exited in the meantime, which the caller finds out by polling.
            if err.raw_os_error() != Some(libc::ESRCH) {
                tracing::warn!(terminal = %self.label, pid = self.pid, %err, "signalling the process failed");
            }
        }
    }
}

/// Every process id on the system, a superset of the session's members for the caller to filter
/// with `getsid`, or `None` when the system cannot list them.
#[cfg(target_os = "linux")]
fn session_candidates() -> Option<Vec<libc::pid_t>> {
    let entries = std::fs::read_dir("/proc").ok()?;
    Some(
        entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
            .collect(),
    )
}

/// As the Linux one, from `libproc`.
#[cfg(target_os = "macos")]
fn session_candidates() -> Option<Vec<libc::pid_t>> {
    // SAFETY: a null buffer of size 0 only asks how many processes there are.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return None;
    }
    // Room beyond that count for processes started since.
    let mut pids = vec![0 as libc::pid_t; count as usize + 256];
    let size = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
    // SAFETY: `pids` is writable for `size` bytes.
    let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
    if filled <= 0 {
        return None;
    }
    pids.truncate(filled as usize);
    // Pid 0 is the kernel, which `getsid(0)` would answer for the caller instead.
    pids.retain(|&pid| pid > 0);
    Some(pids)
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

/// What owns a [`PtyCore`] and wants to see each chunk the reader takes from it.
pub trait PtyOwner: Send + Sync + 'static {
    fn pty(&self) -> &PtyCore;

    /// Called from the PTY reader thread for every chunk read; it is expected to pass the chunk on
    /// through [`PtyCore::push_output`], which may block while a client catches up.
    fn on_output(&self, data: &[u8]);
}

/// Spawns the OS thread that reads the owner's PTY. A blocking reader on a dedicated thread
/// rather than a tokio task, because the throttling in `push_output` is the mechanism that applies
/// backpressure and must not occupy a runtime worker.
pub fn spawn_reader_thread<T: PtyOwner>(owner: Arc<T>, read_buf_bytes: usize) {
    let pty = owner.pty();
    pty.reader_running.store(true, Ordering::Release);
    // Held while the thread is stored, so a hang-up cannot look for it in between.
    let mut reader = pty.reader.lock().expect("reader mutex poisoned");
    let thread_owner = owner.clone();
    *reader = Some(std::thread::spawn(move || {
        let owner = thread_owner;
        let (fd, wake) = (owner.pty().fd, owner.pty().wake.read.as_raw_fd());
        let mut buf = vec![0u8; read_buf_bytes];
        loop {
            match ptyio::read_chunk(fd, wake, &mut buf) {
                Ok(None | Some(0)) => break,
                Ok(Some(n)) => owner.on_output(&buf[..n]),
                Err(err) => {
                    tracing::debug!(terminal = %owner.pty().label, %err, "PTY read failed; closing the reader");
                    break;
                }
            }
        }
        owner.pty().reader_running.store(false, Ordering::Release);
        tracing::info!(terminal = %owner.pty().label, "PTY reader thread exiting");
    }));
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{send_with_backpressure, SUBSCRIBER_GRACE};
    use crate::test_support::{is_running, KillOnDrop};
    use bytes::Bytes;
    use tokio::sync::mpsc;

    #[test]
    fn a_wrapped_ring_replays_behind_the_modes_it_lost() {
        let live = crate::test_support::idle_stand_in("replay-modes");
        live.on_output(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h");
        let (replay, _rx) = live.attach();
        assert!(replay.starts_with(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h"));
        assert_eq!(
            replay.len(),
            24,
            "a ring that has not wrapped is replayed as is"
        );

        let redraw = b"\r\x1b[2Kframe";
        for _ in 0..super::RING_CAPACITY / redraw.len() + 1 {
            live.on_output(redraw);
        }
        let (replay, _rx) = live.attach();
        let head = String::from_utf8_lossy(&replay[..80]);
        for expected in ["?1049h", "?1000h", "?1006h"] {
            assert!(head.contains(expected), "{expected} missing from {head:?}");
        }
    }

    #[test]
    fn a_failed_record_update_rolls_back_only_its_own_freeze() {
        let live = crate::test_support::idle_stand_in("freeze-rollback");
        live.on_output(b"before update");
        for failed in [Ok(false), Err(anyhow::anyhow!("storage failed"))] {
            let failed_with_error = failed.is_err();
            let result = live.freeze_output_for(|| {
                assert!(live.frozen_output().is_some(), "freeze precedes the write");
                assert!(
                    live.input_lock.try_lock().is_err(),
                    "writes and termination wait"
                );
                live.on_output(b"\r\x1b[2Kafter update");
                failed
            });
            assert_eq!(result.is_err(), failed_with_error);
            assert!(live.frozen_output().is_none());
        }
        assert!(live.freeze_output_for(|| Ok(true)).unwrap());
        let frozen = live.frozen_output().unwrap();
        live.on_output(b"\x1b[2J\x1b[Hcleanup");
        assert!(live
            .freeze_output_for(|| Err(anyhow::anyhow!("later failure")))
            .is_err());
        assert!(!live.freeze_output_for(|| Ok(false)).unwrap());
        assert_eq!(live.frozen_output().unwrap(), frozen);
    }

    #[test]
    fn another_freeze_waits_until_an_unsuccessful_update_has_rolled_back() {
        let live = crate::test_support::idle_stand_in("freeze-serialized");
        live.on_output(b"original");
        let (entered, started) = std::sync::mpsc::channel();
        let (done, completed) = std::sync::mpsc::channel();
        let concurrent = live.clone();
        let freezer = std::thread::spawn(move || {
            started.recv().expect("the update started");
            // The main thread checks the mutex is held before releasing this worker.
            concurrent.freeze_output();
            done.send(()).expect("the concurrent freeze finished");
        });
        assert!(!live
            .freeze_output_for(|| {
                assert!(live.input_lock.try_lock().is_err());
                entered.send(()).unwrap();
                assert!(completed.try_recv().is_err());
                live.on_output(b"\r\x1b[2Klatest");
                Ok(false)
            })
            .unwrap());
        freezer.join().expect("the concurrent freeze joined");
        completed.recv().unwrap();
        let frozen = String::from_utf8(live.frozen_output().unwrap()).unwrap();
        assert!(frozen.contains("latest"), "{frozen}");
        assert!(!frozen.contains("original"));
    }

    /// The reader is stopped by its wake pipe, not by the PTY reaching its end: here the process
    /// is still running, holding the PTY open, when the terminal is hung up.
    #[test]
    fn hanging_up_stops_the_reader_of_a_process_still_running() {
        let live = crate::test_support::fake_live_session(
            "hang-up",
            crate::protocol::Agent::Claude,
            80,
            24,
            "trap '' HUP; echo ready; exec sleep 30",
        );
        super::spawn_reader_thread((*live).clone(), 8 * 1024);
        let ready = crate::test_support::output_containing(&live, "ready");
        assert!(ready.contains("ready"), "{ready}");
        let (done, hung_up) = std::sync::mpsc::channel();
        let pty = (*live).clone();
        std::thread::spawn(move || {
            pty.hang_up();
            done.send(()).ok();
        });
        hung_up
            .recv_timeout(crate::test_support::PATIENCE)
            .expect("the hang-up waited on a reader that never stopped");
        assert!(!live
            .reader_running
            .load(std::sync::atomic::Ordering::Acquire));
        assert!(live.is_hung_up());
        assert!(live.write_input(b"x").is_err());
        assert!(
            !live.poll_exit(),
            "the process ignores the hang-up and runs on"
        );
    }

    /// Starts a process that leaves two helpers running, one in the leader's process group and one
    /// in a group of its own, and exits at once on `SIGTERM`. Returns it with the pids of the
    /// helpers, which `guard` kills if the test ends without having.
    fn start_leader(
        kill_session_leftovers: bool,
        guard: &mut KillOnDrop,
    ) -> (super::PtyCore, libc::pid_t, libc::pid_t) {
        let dir = std::env::temp_dir().join(format!(
            "octoboardd-leftovers-{}-{kill_session_leftovers}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let pids = dir.join("pids");
        std::fs::remove_file(&pids).ok();
        // The first helper stays in the leader's process group and ignores the hang-up; the
        // second is moved to a group of its own by job control, as a background task of an agent
        // is. Both keep the session.
        let script = format!(
            "trap 'exit 0' TERM; (trap '' HUP; exec sleep 300) & same=$!; set -m; sleep 300 & own=$!; \
             set +m; echo \"$same $own\" > {pids}.tmp; mv {pids}.tmp {pids}; while :; do sleep 0.1; done",
            pids = pids.display()
        );
        let mut cmd = portable_pty::CommandBuilder::new("/bin/bash");
        cmd.args(["-c", &script]);
        let spawned = super::spawn(cmd, 24, 80).expect("the leader");
        let core = super::PtyCore::new(super::NewPty {
            label: "leftovers".to_string(),
            spawned,
            scratch_dir: None,
            kill_session_leftovers,
        });
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        let listed = loop {
            if let Ok(listed) = std::fs::read_to_string(&pids) {
                break listed;
            }
            assert!(
                Instant::now() < deadline,
                "the leader never listed its helpers"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        std::fs::remove_dir_all(&dir).ok();
        let helpers: Vec<libc::pid_t> = listed
            .split_whitespace()
            .map(|pid| pid.parse().unwrap())
            .collect();
        guard.0.extend(&helpers);
        (core, helpers[0], helpers[1])
    }

    #[test]
    fn an_agent_leaves_nothing_running_in_its_session_after_a_graceful_exit() {
        let mut guard = KillOnDrop(Vec::new());
        let (core, same_group, own_group) = start_leader(true, &mut guard);
        assert!(is_running(same_group) && is_running(own_group));
        core.stop(libc::SIGTERM);
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while is_running(same_group) || is_running(own_group) {
            assert!(
                Instant::now() < deadline,
                "a helper outlived the agent: same group running {}, own group running {}",
                is_running(same_group),
                is_running(own_group)
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        guard.0.clear();
    }

    #[test]
    fn a_shell_terminal_leaves_a_job_it_started_running_when_it_exits() {
        let mut guard = KillOnDrop(Vec::new());
        let (core, same_group, own_group) = start_leader(false, &mut guard);
        core.stop(libc::SIGTERM);
        // The sweep, if there were one, runs before the shell is reaped, which `stop` waited for.
        assert!(is_running(same_group) && is_running(own_group));
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
