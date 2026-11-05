//! A process the daemon runs on a PTY, as an agent session and a shell terminal each have one,
//! once `platform::term` has started it: the reader thread that feeds its output to the fan-out
//! (`terminal::fan`), input, resizing, ending it, and its place in the crash cleanup. What the
//! process is — an agent or a shell — is the owner's business, not this module's.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::platform::proc::{crash, ChildHandle, Polite, Sent};
use crate::platform::term::{self, Foreground, Input, PartialWrite, Pty, Size, Spawned};
use crate::terminal::fan::{Attached, Fan};
use crate::terminal::snapshot::{MAX_COLS, MAX_ROWS};
use crate::terminal::title::TitleChange;

/// Size the PTY starts at. The UI sends a `resize` as soon as it attaches; this only has to be
/// something sane for the output a process produces before that.
pub const INITIAL_SIZE: Size = Size {
    rows: 32,
    cols: 120,
};

/// The terminal the UI actually renders with, for a process's `TERM`. A program that inherits
/// anything less — notably the snapshot shell's `TERM=dumb` — downgrades its own renderer, colour
/// and mouse handling.
pub const TERM: &str = "xterm-256color";

/// Size of the buffer the PTY reader thread reads into, which also bounds the size of every
/// daemon-to-client terminal frame. Not a tuning lever: sweeping it from 4 KiB to 256 KiB made no
/// measurable difference to throughput at two very different source rates.
pub const READ_BUF: usize = 8 * 1024;

/// How long [`PtyCore::stop`] waits for the process to take the polite signal before killing it,
/// past whatever the host does before it makes the request ([`Pty::polite_stop_lead`]).
const STOP_GRACE: Duration = Duration::from_secs(3);

/// How long the PTY reader is waited for to take what the process wrote just before it exited.
/// Bounded because the reader only finishes once nothing holds the PTY open, and a background
/// process the process started may hold it long after the process is gone.
pub const FINAL_OUTPUT_DRAIN: Duration = Duration::from_millis(500);

pub struct PtyCore {
    /// The owner's id, for the log.
    label: String,
    pub pid: u32,
    /// The terminal: reads, writes, resizing and the hang-up all go through it. Every write is
    /// made under its input lock ([`Self::lock_input`]), which [`Self::hang_up`] takes too. The
    /// lock is held by a freeze as well, so that a write and the capture boundary of a frozen
    /// screen never interleave, and an owner that keeps bookkeeping about its writes takes it for
    /// that bookkeeping too. It is the terminal's, so that a polite stop's quit keys are pressed
    /// under it.
    pty: Pty,
    /// The reader thread, kept to be joined when the PTY is hung up.
    reader: Mutex<Option<JoinHandle<()>>>,
    /// Held across reaping and every signal, which are only sent while the child is unreaped.
    /// The child is deliberately never reaped until the exit watcher observes the exit: an
    /// unreaped child keeps its pid and process group reserved, which is what makes signalling by
    /// number safe here. Signalling a process group by a pid that had already been recycled is not
    /// a theoretical hazard: it once killed the spawn helper of the editor the daemon ran under,
    /// leaving that process unable to start any child at all until it was restarted.
    child: Mutex<()>,
    /// The child as the platform signals, sweeps and reaps it; signalled only while unreaped.
    handle: ChildHandle,
    reaped: AtomicBool,
    /// `reaped` gates signals; this gates observing the exit. It is set once the exit hook and
    /// scratch removal have run, or unwound. Pollers read it without waiting.
    exit_complete: AtomicBool,
    /// Held by the reaping observer from before `reaped` is published until `exit_complete` is
    /// set, so a stop that finds the child reaped waits for the cleanup on it.
    exit_cleanup: Mutex<()>,
    /// The child's place in the cleanup that kills the daemon's processes when it goes down, held
    /// from [`Self::enter_crash_cleanup`] until the child has exited, and given up before it is
    /// reaped.
    crash_cleanup: Mutex<Option<crash::Entry>>,
    fan: Fan,
    /// Where the answers the fan makes to the program's queries go to be written by a thread of
    /// their own, for as long as the reader runs; see [`spawn_reader_thread`].
    answers: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    /// A reader thread is reading the PTY into `fan`; see [`spawn_reader_thread`].
    reader_running: AtomicBool,
    /// [`Self::keep_final_output`] has run. A lock rather than a flag, so that a second caller
    /// returns only once the first has finished.
    final_output_kept: Mutex<bool>,
    /// A scratch directory of the process's own, removed once the process is gone.
    scratch_dir: Mutex<Option<std::path::PathBuf>>,
    /// Runs after the process is reaped and before [`Self::clean_scratch_dir`] removes the
    /// directory, so it can still read what the process wrote there. It runs under
    /// `exit_cleanup`, so every stop waits for it, and it must not stop this process itself.
    before_scratch_removal: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    /// Whether what the process leaves running in its session is killed once it exits; see
    /// [`NewPty::kill_session_leftovers`].
    kill_session_leftovers: bool,
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
    /// Runs after the process is reaped and before its scratch directory is removed. A stop of
    /// the process waits for it, so it must not stop the process itself.
    pub before_scratch_removal: Option<Box<dyn FnOnce() + Send>>,
}

/// Marks the exit complete when dropped, so a hook that unwinds still lets observers see the exit.
struct ExitComplete<'a>(&'a AtomicBool);

impl Drop for ExitComplete<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl PtyCore {
    pub fn new(new: NewPty) -> Self {
        Self::answering(new, term::facts().host_waits_on_queries)
    }

    /// As [`Self::new`], with the daemon answering the program's device attributes and cursor
    /// position queries while no client is attached where `daemon_answers`
    /// (`TermFacts::host_waits_on_queries`).
    fn answering(new: NewPty, daemon_answers: bool) -> Self {
        let spawned = new.spawned;
        let size = spawned.pty.size().unwrap_or(Size { rows: 24, cols: 80 });
        Self {
            label: new.label,
            pid: spawned.pid.0,
            pty: spawned.pty,
            reader: Mutex::new(None),
            child: Mutex::new(()),
            handle: spawned.child,
            reaped: AtomicBool::new(false),
            exit_complete: AtomicBool::new(false),
            exit_cleanup: Mutex::new(()),
            crash_cleanup: Mutex::new(None),
            fan: Fan::new(size.rows, size.cols, daemon_answers),
            answers: Mutex::new(None),
            reader_running: AtomicBool::new(false),
            final_output_kept: Mutex::new(false),
            scratch_dir: Mutex::new(new.scratch_dir),
            before_scratch_removal: Mutex::new(new.before_scratch_removal),
            kill_session_leftovers: new.kill_session_leftovers,
        }
    }

    /// The process as the platform signals and sweeps it.
    pub fn handle(&self) -> &ChildHandle {
        &self.handle
    }

    /// Registers a terminal client; see [`Fan::attach`].
    pub fn attach(&self) -> Attached {
        self.fan.attach()
    }

    /// Whether this client's answer to the output up to `offset` is the one written; see
    /// [`Fan::admit_reply`].
    pub fn admit_reply(&self, client: u64, offset: u64, reply: &[u8]) -> bool {
        self.fan.admit_reply(client, offset, reply)
    }

    /// Ends every attached client's stream once the output it follows is over, and makes every
    /// later [`Self::attach`] a replay alone. Output that still arrives is kept for that replay.
    pub fn close_output(&self) {
        self.fan.close();
    }

    /// The tail of what this process has printed, at most `max_bytes` of it. Taken from the same
    /// ring buffer a terminal client replays, so it is exactly what the user would see — raw PTY
    /// bytes, escape sequences included, which the caller is responsible for making readable.
    pub fn recent_output(&self, max_bytes: usize) -> Vec<u8> {
        self.fan.recent_output(max_bytes)
    }

    /// Freezes a readable screen before termination can clear it. Input and the parser share
    /// the capture boundary; later cleanup output continues to reach live subscribers.
    pub fn freeze_output(&self) {
        let _input = self.lock_input();
        self.fan.freeze();
    }

    /// Freezes before a record without a process can become observable. The closure only updates
    /// storage; publishing happens after this returns. A failed update rolls back only this call's
    /// freeze, while the input lock keeps termination or another update from claiming that same
    /// screen.
    pub fn freeze_output_for(
        &self,
        update: impl FnOnce() -> anyhow::Result<bool>,
    ) -> anyhow::Result<bool> {
        let _input = self.lock_input();
        let created = self.fan.freeze();
        let result = update();
        if created && !matches!(result, Ok(true)) {
            self.fan.discard_frozen();
        }
        result
    }

    pub fn frozen_output(&self) -> Option<Vec<u8>> {
        self.fan.frozen_output()
    }

    pub fn is_frozen(&self) -> bool {
        self.fan.is_frozen()
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
        keep(&self.fan.output());
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
        self.fan.total()
    }

    /// Whether bracketed paste is on now; see
    /// [`crate::terminal::snapshot::TerminalSnapshot::bracketed_paste_on`].
    pub fn bracketed_paste_on(&self) -> bool {
        self.fan.bracketed_paste_on()
    }

    /// The text of the rows the parsed screen shows now; see
    /// [`crate::terminal::snapshot::TerminalSnapshot::visible_text`].
    pub fn screen_text(&self) -> String {
        self.fan.screen_text()
    }

    /// [`Self::screen_text`] with faint cells blanked; see
    /// [`crate::terminal::snapshot::TerminalSnapshot::visible_text_undimmed`].
    pub fn screen_text_undimmed(&self) -> String {
        self.fan.screen_text_undimmed()
    }

    /// [`Self::screen_text`] with only the cells drawn in inverse video; see
    /// [`crate::terminal::snapshot::TerminalSnapshot::visible_text_inverse`].
    pub fn screen_text_inverse(&self) -> String {
        self.fan.screen_text_inverse()
    }

    /// What this process printed after `mark`, as taken from [`Self::output_total`] — less of it if
    /// the ring buffer has already let some go.
    pub fn output_since(&self, mark: u64) -> Vec<u8> {
        self.fan.output_since(mark)
    }

    /// Takes one chunk read from the PTY into the replay, the parsed screen and every attached
    /// client; see [`Fan::push`], which blocks while a slow client catches up.
    pub fn push_output(&self, data: &[u8], before_delivery: impl FnOnce(u64, Vec<TitleChange>)) {
        let answers = self.fan.push(data, before_delivery);
        if answers.is_empty() {
            return;
        }
        // Never written from here: the reader must not wait on the input lock, which a long write
        // holds while the host waits for the output to be read.
        if let Some(writer) = self
            .answers
            .lock()
            .expect("answers mutex poisoned")
            .as_ref()
        {
            writer.send(answers).ok();
        }
    }

    /// Has [`Self::push_output`] hand over the title changes each chunk makes on the parsed screen;
    /// see [`crate::terminal::snapshot::TerminalSnapshot::report_titles`].
    pub fn report_titles(&self) {
        self.fan.report_titles();
    }

    /// Logs the program's device attributes and cursor position queries from now on, with who
    /// answered each; see [`Fan::log_queries`].
    pub fn log_queries(&self) {
        self.fan.log_queries(self.label.clone());
    }

    /// The lock every write into the input is made under. An owner takes it to keep its own
    /// bookkeeping about a write in step with the write, and then writes with
    /// [`Self::write_locked`] or [`Self::write_input_locked`].
    pub fn lock_input(&self) -> MutexGuard<'_, ()> {
        self.pty.input_lock().lock().expect("input mutex poisoned")
    }

    /// Writes raw input — a client's keystrokes or its reply to a query — under the lock `input`
    /// is a guard of, taken with [`Self::lock_input`]. The error says how many bytes went in,
    /// because a caller deciding whether to retry has to know: whatever was accepted is already in
    /// the child's input buffer.
    pub fn write_locked(
        &self,
        _input: &MutexGuard<'_, ()>,
        data: &[u8],
    ) -> Result<(), PartialWrite> {
        self.pty.write_raw(data)
    }

    /// Writes the daemon's own `input` (a paste, a key) as [`Self::write_locked`] writes raw
    /// input; the error counts the bytes the inputs were encoded as.
    pub fn write_input_locked(
        &self,
        _input: &MutexGuard<'_, ()>,
        input: &[Input],
    ) -> Result<(), PartialWrite> {
        self.pty.write_input(input)
    }

    /// Writes raw input, as [`Self::write_locked`] does, taking the lock itself.
    pub fn write(&self, data: &[u8]) -> Result<(), PartialWrite> {
        let input = self.lock_input();
        self.write_locked(&input, data)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        anyhow::ensure!(
            (2..=MAX_COLS).contains(&cols) && (1..=MAX_ROWS).contains(&rows),
            "terminal dimensions exceed the supported range"
        );
        self.fan
            .resize(rows, cols, || self.pty.resize(Size { rows, cols }))
    }

    /// Whose job is in the foreground of the PTY, the one the terminal's input and its signals go
    /// to: the process's own, or another it started. Only meaningful while the process runs.
    pub fn foreground_job(&self) -> Foreground {
        self.pty.foreground_job()
    }

    /// The device path of the PTY's slave side, what a process on the terminal has open as its
    /// stdin, stdout or stderr, or `None` when the system cannot say or the PTY is hung up.
    pub fn slave_path(&self) -> Option<std::path::PathBuf> {
        self.pty.slave_path()
    }

    /// Whether [`Self::hang_up`] has closed the PTY.
    #[cfg(test)]
    pub fn is_hung_up(&self) -> bool {
        self.pty.is_hung_up()
    }

    /// Hangs the terminal up, as closing a terminal window does: stops the reader thread, lets
    /// every attached client go, and closes the PTY master, after which the kernel hangs up the
    /// other side — a process still on it gets `SIGHUP` where it is in the terminal's foreground,
    /// and an error on every read and write from then on. Nothing reads the PTY afterwards, so
    /// whatever still holds the other side open no longer keeps this core alive. On Linux nothing
    /// revokes the terminal when its process exits, so this is the only thing that ends the
    /// reader while a leftover job holds the PTY open; on macOS that exit already ends it, so a
    /// test there cannot show the need. **Blocks** until the reader thread has finished, which may
    /// wait out an attached client that has stopped draining (see [`Fan::push`]).
    pub fn hang_up(&self) {
        self.close_output();
        let reader = self.reader.lock().expect("reader mutex poisoned").take();
        if let Some(reader) = reader {
            self.pty.stop_reader();
            if reader.thread().id() != std::thread::current().id() && reader.join().is_err() {
                tracing::warn!(terminal = %self.label, "the PTY reader thread panicked");
            }
        }
        let _input = self.lock_input();
        self.pty.hang_up();
    }

    /// Polls the child, returning `true` only after its exit hook and scratch removal finish.
    /// The observer that reaps it runs that cleanup; concurrent pollers return `false` while it
    /// is in progress, without waiting on the hook.
    pub fn poll_exit(&self) -> bool {
        if self.reaped.load(Ordering::Acquire) {
            return self.exit_complete.load(Ordering::Acquire);
        }
        let child = self.child.lock().expect("child mutex poisoned");
        self.poll_exit_locked(child)
    }

    /// The body of `poll_exit`, taking the already-held `child` lock. Reaping and the flag that
    /// records it happen under that lock, which is what lets `signal` hold it to keep the child
    /// from being reaped between its check and its `kill`.
    fn poll_exit_locked(&self, child: MutexGuard<'_, ()>) -> bool {
        if self.reaped.load(Ordering::Acquire) {
            return self.exit_complete.load(Ordering::Acquire);
        }
        // The crash cleanup signals by number, so the child leaves it while it is exited but not
        // yet reaped, its pid still reserved. When the host cannot tell, reaping decides, and the
        // child leaves only after it is reaped.
        match self.handle.exited_unreaped() {
            Some(false) => return false,
            Some(true) => {
                self.leave_crash_cleanup();
                // Its zombie keeps the session id from passing to another process, so what is in
                // the session is the child's: a helper that moved to a process group of its own
                // too, which a signal to the child's group would miss.
                if self.kill_session_leftovers {
                    self.handle
                        .kill_leftovers(&[])
                        .log(self.handle.pid(), &self.label);
                }
            }
            None => {}
        }
        let exited = match self.handle.reap_if_exited() {
            Ok(reaped) => reaped,
            // Nothing left to wait on, so treat the child as gone rather than keep signalling a
            // pid we can no longer account for.
            Err(err) => {
                tracing::warn!(terminal = %self.label, %err, "reaping failed; treating the process as exited");
                true
            }
        };
        if exited {
            let _cleanup = self.lock_exit_cleanup();
            self.reaped.store(true, Ordering::Release);
            // Dropped before `_cleanup`, so a stop waiting on the lock finds the exit complete.
            let _complete = ExitComplete(&self.exit_complete);
            self.leave_crash_cleanup();
            drop(child);
            self.clean_scratch_dir();
        }
        exited
    }

    /// A stop already blocks for the child; once reaped, it also waits for whichever observer
    /// owns cleanup, without keeping the child lock needed by signal's pid-safety check.
    fn wait_for_reaped_cleanup(&self) -> bool {
        if !self.reaped.load(Ordering::Acquire) {
            return false;
        }
        let _cleanup = self.lock_exit_cleanup();
        true
    }

    /// The lock guards no data, so one poisoned by an unwinding hook is still good to wait on.
    fn lock_exit_cleanup(&self) -> MutexGuard<'_, ()> {
        self.exit_cleanup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn clean_scratch_dir(&self) {
        let dir = self
            .scratch_dir
            .lock()
            .expect("scratch dir mutex poisoned")
            .take();
        if let Some(dir) = dir {
            // Before the directory goes: a caller may still need a file the process wrote there,
            // for instance to copy a setting out of it. The removal itself only deletes this
            // directory. Every entry in it is either a symlink into the user's own agent directory
            // or a file Octoboard wrote, so the removal never touches the user's configuration.
            if let Some(hook) = self
                .before_scratch_removal
                .lock()
                .expect("scratch hook mutex poisoned")
                .take()
            {
                hook();
            }
            if let Err(err) = std::fs::remove_dir_all(&dir) {
                tracing::warn!(terminal = %self.label, path = %dir.display(), %err, "removing the scratch directory failed");
            }
        }
    }

    /// Asks the process to exit with `polite`, escalating to a kill only if it does not. Both
    /// signals are gated on the child not having been reaped, which is what keeps the pid (and the
    /// process group it leads, as the leader of its terminal's session) from being a number that now
    /// belongs to something else. **Blocks** until the process is gone, or a few seconds past the
    /// kill. Once reaped, waits for the exit hook and scratch removal too.
    pub fn stop(&self, polite: Polite) {
        if self.poll_exit() || self.wait_for_reaped_cleanup() {
            return;
        }
        self.signal(Some(polite));

        let deadline = Instant::now() + self.pty.polite_stop_lead(polite) + STOP_GRACE;
        while Instant::now() < deadline {
            if self.poll_exit() || self.wait_for_reaped_cleanup() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        // The process did not take the hint. Kill the whole process group this time: the process
        // leads its terminal's session, so its pgid is its own pid and the children it did not
        // move to a group of their own are in that same group — killing the leader alone would
        // leave them running.
        self.signal(None);
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if self.poll_exit() || self.wait_for_reaped_cleanup() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if !self.wait_for_reaped_cleanup() {
            tracing::warn!(terminal = %self.label, pid = self.pid, "the process is still alive after SIGKILL");
        }
    }

    /// Enters the child in the cleanup that kills the daemon's processes when it goes down
    /// (`crate::platform::proc::crash`). Under the lock reaping takes, so a child reaped in the
    /// meantime is never entered: the cleanup signals by number.
    pub fn enter_crash_cleanup(&self) {
        let child = self.child.lock().expect("child mutex poisoned");
        if self.reaped.load(Ordering::Acquire) {
            return;
        }
        let entry = crash::register(&self.handle, self.kill_session_leftovers);
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
    ///
    /// With `polite`, asks the process alone to end that way; without it, kills the process and
    /// what it started without moving it elsewhere.
    fn signal(&self, polite: Option<Polite>) {
        let child = self.child.lock().expect("child mutex poisoned");
        if self.reaped.load(Ordering::Acquire) {
            return;
        }
        // The handle of a child this struct holds unreaped, so the number has not been recycled —
        // holding the lock reaping takes guarantees that.
        let sent = match polite {
            Some(polite) => self.handle.stop(polite),
            None => self.handle.kill(),
        };
        // A process that is gone just exited in the meantime, which the caller finds out by
        // polling.
        if let Sent::Failed(err) = sent {
            tracing::warn!(terminal = %self.label, pid = self.pid, %err, "signalling the process failed");
        }
        drop(child);
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
    let pty_reader = pty.pty.reader();
    if pty.fan.answers_queries() {
        let (sender, answers) = mpsc::channel::<Vec<u8>>();
        *pty.answers.lock().expect("answers mutex poisoned") = Some(sender);
        let writer_owner = owner.clone();
        // One writer, so the answers go in in order, and promptly: the host holds the program up
        // until a query is answered, for about half a second at most.
        std::thread::spawn(move || {
            for answer in answers {
                if let Err(err) = writer_owner.pty().write(&answer) {
                    tracing::debug!(terminal = %writer_owner.pty().label, %err, "answering the program's query failed");
                }
            }
        });
    }
    *reader = Some(std::thread::spawn(move || {
        let owner = thread_owner;
        let mut buf = vec![0u8; read_buf_bytes];
        loop {
            match pty_reader.read_chunk(&mut buf) {
                Ok(None | Some(0)) => break,
                Ok(Some(n)) => owner.on_output(&buf[..n]),
                Err(err) => {
                    tracing::debug!(terminal = %owner.pty().label, %err, "PTY read failed; closing the reader");
                    break;
                }
            }
        }
        owner.pty().reader_running.store(false, Ordering::Release);
        // Ends the answers' writer, once it has written what is queued.
        *owner.pty().answers.lock().expect("answers mutex poisoned") = None;
        tracing::info!(terminal = %owner.pty().label, "PTY reader thread exiting");
    }));
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crate::platform::proc::Polite;
    use crate::platform::term::{self, Size};
    use crate::test_support::{is_running, KillOnDrop};

    const SIZE: Size = Size { rows: 24, cols: 80 };

    #[test]
    fn a_wrapped_ring_replays_behind_the_modes_it_lost() {
        let live = crate::test_support::idle_stand_in("replay-modes");
        live.on_output(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h");
        let replay = live.attach().replay;
        assert!(replay.starts_with(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h"));
        assert_eq!(
            replay.len(),
            24,
            "a ring that has not wrapped is replayed as is"
        );

        let redraw = b"\r\x1b[2Kframe";
        for _ in 0..crate::terminal::fan::RING_CAPACITY / redraw.len() + 1 {
            live.on_output(redraw);
        }
        let replay = live.attach().replay;
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
                    live.pty.input_lock().try_lock().is_err(),
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
                assert!(live.pty.input_lock().try_lock().is_err());
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

    /// What a [`super::PtyCore`] needs to have its reader thread run.
    struct Owner(super::PtyCore);

    impl super::PtyOwner for Owner {
        fn pty(&self) -> &super::PtyCore {
            &self.0
        }

        fn on_output(&self, data: &[u8]) {
            self.0.push_output(data, |_, _| {});
        }
    }

    /// The answers to a query go into the program's input from a thread of their own: the reader
    /// that took the query in does not wait on the input lock a long paste may be holding.
    #[test]
    fn a_query_is_answered_into_the_programs_input_without_the_reader_waiting_on_the_input_lock() {
        let cmd = crate::test_support::programs::hold_stdin().term_command();
        let core = super::PtyCore::answering(
            super::NewPty {
                label: "answers".to_string(),
                spawned: term::spawn(cmd, SIZE, None).expect("process"),
                scratch_dir: None,
                kill_session_leftovers: false,
                before_scratch_removal: None,
            },
            true,
        );
        let owner = std::sync::Arc::new(Owner(core));
        super::spawn_reader_thread(owner.clone(), 8 * 1024);

        let paste = owner.0.lock_input();
        let started = Instant::now();
        super::PtyOwner::on_output(&*owner, b"\x1b[c");
        assert!(
            started.elapsed() < crate::test_support::PATIENCE,
            "the reader waited on the input lock"
        );
        drop(paste);
        // The answer has no line break of its own for a program reading lines to take it by.
        owner.0.write(b"\r\n").expect("the write");
        let heard = loop {
            let heard = String::from_utf8_lossy(&owner.0.recent_output(1024)).into_owned();
            if heard.contains("?6c") || started.elapsed() >= crate::test_support::PATIENCE {
                break heard;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(heard.contains("?6c"), "{heard:?}");

        owner.0.hang_up();
        owner.0.stop(Polite::Terminate);
    }

    /// The reader is stopped by its wake pipe, not by the PTY reaching its end: here the process
    /// is still running, holding the PTY open, when the terminal is hung up.
    #[test]
    fn hanging_up_stops_the_reader_of_a_process_still_running() {
        let live = crate::test_support::fake_live_session_running(
            "hang-up",
            crate::protocol::AgentId::CLAUDE,
            80,
            24,
            &crate::test_support::programs::ignore_hangup(30),
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

    /// Starts a process that leaves two helpers running and exits at once on a polite request to
    /// terminate. Returns it with the pids of the helpers, which `guard` kills if the test ends
    /// without having.
    fn start_leader(
        kill_session_leftovers: bool,
        guard: &mut KillOnDrop,
    ) -> (super::PtyCore, i32, i32) {
        let dir = crate::test_support::ScratchDir::new("pty-leftovers");
        let pids = dir.join("pids");
        let cmd = leader(&pids);
        let spawned = term::spawn(cmd, SIZE, None).expect("the leader");
        let core = super::PtyCore::new(super::NewPty {
            label: "leftovers".to_string(),
            spawned,
            scratch_dir: None,
            kill_session_leftovers,
            before_scratch_removal: None,
        });
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        let helpers = loop {
            if let Some(helpers) = helpers_of(&pids) {
                break helpers;
            }
            assert!(
                Instant::now() < deadline,
                "the leader never listed its helpers"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        guard.0.extend(&helpers);
        (core, helpers[0], helpers[1])
    }

    /// The leader of [`start_leader`], which writes its helpers' pids to `pids`. The first helper
    /// stays in the leader's process group and ignores the hang-up; the second is moved to a group
    /// of its own by job control, as a background task of an agent is. Both keep the session.
    #[cfg(unix)]
    fn leader(pids: &std::path::Path) -> term::Command {
        let script = format!(
            "trap 'exit 0' TERM; (trap '' HUP; exec sleep 300) & same=$!; set -m; sleep 300 & own=$!; \
             set +m; echo \"$same $own\" > {pids}.tmp; mv {pids}.tmp {pids}; while :; do sleep 0.1; done",
            pids = pids.display()
        );
        let mut cmd = term::Command::new("/bin/bash");
        cmd.args(["-c", &script]);
        cmd
    }

    /// The pids [`leader`] wrote to `pids`, once it has.
    #[cfg(unix)]
    fn helpers_of(pids: &std::path::Path) -> Option<Vec<i32>> {
        let listed = std::fs::read_to_string(pids).ok()?;
        Some(
            listed
                .split_whitespace()
                .map(|pid| pid.parse().unwrap())
                .collect(),
        )
    }

    /// The leader of [`start_leader`] on Windows, a fixture process (`test_support::fixture`). Its
    /// helpers are on its console, its session there, and hold on when the console closes, which
    /// the host ends only after a grace of about five seconds each: the close that stops the
    /// leader does not end them meanwhile, as a hang-up does not end the Unix helpers. Each writes
    /// its pid beside `pids` once it holds on.
    #[cfg(windows)]
    fn leader(pids: &std::path::Path) -> term::Command {
        use crate::test_support::fixture::Steps;
        let helper = |file| Steps::new().hold_on_close().write_pid(file).sleep(300);
        Steps::new()
            .spawn_on_console("same", helper(&pids.with_extension("same")))
            .spawn_on_console("own", helper(&pids.with_extension("own")))
            .sleep(300)
            .program("leader")
            .term_command()
    }

    /// The pids [`leader`] wrote beside `pids`, once both helpers have.
    #[cfg(windows)]
    fn helpers_of(pids: &std::path::Path) -> Option<Vec<i32>> {
        let read = |kind| crate::test_support::read_pid_file(&pids.with_extension(kind));
        Some(vec![read("same")?, read("own")?])
    }

    #[test]
    fn an_agent_leaves_nothing_running_in_its_session_after_a_graceful_exit() {
        let mut guard = KillOnDrop(Vec::new());
        let (core, same_group, own_group) = start_leader(true, &mut guard);
        assert!(is_running(same_group) && is_running(own_group));
        #[cfg(windows)]
        let watched = [same_group, own_group].map(crate::test_support::Watched::new);
        core.stop(Polite::Terminate);
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
        // Ended by the sweep, which kills with 1, not by the host once its grace ran out.
        #[cfg(windows)]
        for helper in watched {
            assert_eq!(helper.exit_code(), Some(1));
        }
        guard.0.clear();
    }

    #[test]
    fn a_shell_terminal_leaves_a_job_it_started_running_when_it_exits() {
        let mut guard = KillOnDrop(Vec::new());
        let (core, same_group, own_group) = start_leader(false, &mut guard);
        core.stop(Polite::Terminate);
        // The sweep, if there were one, runs before the shell is reaped, which `stop` waited for.
        assert!(is_running(same_group) && is_running(own_group));
    }

    /// The hook still sees the scratch directory, and the directory is gone afterwards.
    #[test]
    fn the_scratch_hook_runs_before_the_directory_is_removed() {
        // The session's scratch directory is one inside the test's, which `PtyCore` removes once
        // the process exits; the test's goes with the guard, the marker in it included.
        let root = crate::test_support::ScratchDir::new("scratch-hook");
        let dir = root.join("session");
        std::fs::create_dir(&dir).expect("scratch directory");
        std::fs::write(dir.join("config.toml"), "kept").expect("file");
        let marker = root.join("marker");
        let dir_for_hook = dir.clone();
        let marker_for_hook = marker.clone();
        let cmd = crate::test_support::programs::sleep(0).term_command();
        let spawned = term::spawn(cmd, SIZE, None).expect("process");
        let core = super::PtyCore::new(super::NewPty {
            label: "scratch-hook".to_string(),
            spawned,
            scratch_dir: Some(dir.clone()),
            kill_session_leftovers: false,
            before_scratch_removal: Some(Box::new(move || {
                let text = std::fs::read_to_string(dir_for_hook.join("config.toml")).expect("file");
                std::fs::write(&marker_for_hook, text).expect("marker");
            })),
        });
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !core.poll_exit() {
            assert!(Instant::now() < deadline, "the process never exited");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(std::fs::read_to_string(&marker).expect("marker"), "kept");
        assert!(!dir.exists());
    }

    #[test]
    fn an_exit_is_not_observable_until_the_scratch_hook_finishes() {
        use std::sync::atomic::Ordering;
        use std::sync::{mpsc, Arc};

        let dir = crate::test_support::ScratchDir::new("exit-cleanup-visible");
        let scratch = dir.join("session");
        std::fs::create_dir(&scratch).expect("session scratch");
        let marker = dir.join("written-back");
        let marker_for_hook = marker.clone();
        let (entered, hook_started) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let cmd = crate::test_support::programs::sleep(0).term_command();
        let core = Arc::new(super::PtyCore::new(super::NewPty {
            label: "exit-cleanup-visible".to_string(),
            spawned: term::spawn(cmd, SIZE, None).expect("process"),
            scratch_dir: Some(scratch.clone()),
            kill_session_leftovers: false,
            before_scratch_removal: Some(Box::new(move || {
                entered.send(()).expect("hook entered");
                released
                    .recv_timeout(crate::test_support::PATIENCE)
                    .expect("hook released");
                std::fs::write(marker_for_hook, "saved").expect("writeback");
            })),
        }));
        let reaping = core.clone();
        let observer = std::thread::spawn(move || {
            let deadline = Instant::now() + crate::test_support::PATIENCE;
            while !reaping.poll_exit() {
                assert!(Instant::now() < deadline, "the process never exited");
                std::thread::yield_now();
            }
        });
        hook_started
            .recv_timeout(crate::test_support::PATIENCE)
            .expect("the observer reached the hook");
        assert!(core.reaped.load(Ordering::Acquire));
        assert!(core.exit_cleanup.try_lock().is_err());
        assert!(scratch.exists());
        assert!(!marker.exists());
        // The channel holds cleanup at an exact boundary: this second observer must return
        // without waiting, but cannot announce completion before the writeback is visible.
        let complete_while_hook_held = core.poll_exit();
        // A stop finding the child reaped must likewise not return until the hook is done.
        let (stopped, stop_returned) = mpsc::channel();
        let stopping = core.clone();
        let stopper = std::thread::spawn(move || {
            stopping.stop(Polite::Terminate);
            stopped.send(()).expect("stop returned");
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        let stop_returned_while_hook_held = stop_returned.try_recv().is_ok();
        release.send(()).expect("release cleanup");
        assert!(
            !stop_returned_while_hook_held,
            "the stop returned before the exit hook finished"
        );
        stop_returned
            .recv_timeout(crate::test_support::PATIENCE)
            .expect("the stop returned once cleanup finished");
        assert_eq!(
            std::fs::read_to_string(&marker).expect("writeback"),
            "saved"
        );
        assert!(!scratch.exists());
        stopper.join().expect("stopper joined");
        observer.join().expect("exit observer joined");
        assert!(
            !complete_while_hook_held,
            "cleanup was announced prematurely"
        );
        assert!(core.poll_exit());
    }
}
