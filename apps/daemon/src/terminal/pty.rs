//! A process the daemon runs on a PTY, as an agent session and a shell terminal each have one,
//! once `platform::term` has started it: the reader thread that feeds its output to the fan-out
//! (`terminal::fan`), input, resizing, ending it, and its place in the crash cleanup. What the
//! process is — an agent or a shell — is the owner's business, not this module's.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::platform::proc::{crash, Polite, ProcessId, Sent};
use crate::platform::term::{self, Foreground, Input, PartialWrite, Size};
use crate::terminal::backend::{Backend, Started};
use crate::terminal::display::DisplayFacts;
use crate::terminal::fan::{Attached, Delivery, Fan};
use crate::terminal::program_status::ReportedState;
use crate::terminal::saved_screen::{ReplayRule, SavedScreen};
use crate::terminal::snapshot::is_supported_size;
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

/// Refuses a terminal size out of the range the daemon supports.
fn ensure_supported(rows: u16, cols: u16) -> anyhow::Result<()> {
    anyhow::ensure!(
        is_supported_size(rows, cols),
        "terminal dimensions exceed the supported range"
    );
    Ok(())
}

pub struct PtyCore {
    /// The owner's id, for the log.
    label: String,
    pub pid: u32,
    /// The terminal and the child on it, here or in a holder: reads, writes, resizing, the
    /// hang-up and signals all go through it. Every write is made under its input lock
    /// ([`Self::lock_input`]), which [`Self::hang_up`] takes too. The lock is held by a freeze as
    /// well, so that a write and the capture boundary of a frozen screen never interleave, and an
    /// owner that keeps bookkeeping about its writes takes it for that bookkeeping too. It is the
    /// terminal's, so that a polite stop's quit keys are pressed under it.
    backend: Backend,
    /// The reader thread, kept to be joined when the PTY is hung up.
    reader: Mutex<Option<JoinHandle<()>>>,
    /// Held across reaping and every signal, which are only sent while the child is unreaped.
    /// The child is deliberately never reaped until the exit watcher observes the exit: an
    /// unreaped child keeps its pid and process group reserved, which is what makes signalling by
    /// number safe here. Signalling a process group by a pid that had already been recycled is not
    /// a theoretical hazard: it once killed the spawn helper of the editor the daemon ran under,
    /// leaving that process unable to start any child at all until it was restarted.
    child: Mutex<()>,
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
    /// See [`Self::mark_screen_inexact`].
    screen_inexact: AtomicBool,
    /// Held across every change of the terminal's size and across both resizes of
    /// [`Self::nudge_repaint`]. A held terminal's PTY is resized once the fan's lock is released
    /// (see [`Self::move_size`]), so without this two overlapping changes could leave the PTY at
    /// one size and the parsed screen at the other.
    resize_lock: Mutex<()>,
    /// Set by [`Self::hold_client_changes`]: no client changes the terminal's size, or has the
    /// program told of its display or focus, any more. Changed under `resize_lock`.
    client_changes_held: AtomicBool,
    /// Where the answers the fan makes to the program's queries, and its colour scheme updates, go
    /// to be written by a thread of their own, for as long as the reader runs; see
    /// [`spawn_reader_thread`].
    answers: Mutex<Option<mpsc::Sender<ToProgram>>>,
    /// A reader thread is reading the PTY into `fan`; see [`spawn_reader_thread`].
    reader_running: AtomicBool,
    /// The answers' writer has something left to write: from [`spawn_reader_thread`] until the
    /// reader has ended and everything queued for the writer has gone in or failed, each write of
    /// a held terminal answered by its holder. See [`Self::answering`].
    answering: AtomicBool,
    /// A write of the answers to the program's queries failed, or its outcome was lost with the
    /// link; see [`Self::answers_lost`].
    answers_lost: AtomicBool,
    /// The bytes of the clients' replies admitted ([`Self::admit_reply`]) and not yet written or
    /// given up on ([`Self::settle_reply`]); see [`Self::replies_owed`].
    replies_pending: AtomicUsize,
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

/// What the daemon tells the program, queued for the answers' writer ([`spawn_reader_thread`]),
/// which writes it in this order.
enum ToProgram {
    /// The answers to the queries in output the reader took in. Once a held terminal is handed
    /// over at a planned stop, everything the reader still takes in came before the terminal's
    /// mark, and nobody else answers the queries in it, so these are written until the daemon
    /// parts ([`crate::holder::client::HeldTerminal::write_answers`]).
    Answers(Vec<u8>),
    /// What a client's doing has the daemon tell the program (a focus report, a colour scheme
    /// update after a display change): input like any other, refused once the terminal is handed
    /// over.
    Update(Vec<u8>),
}

/// Everything a [`PtyCore`] is built from: a process just started on a terminal, named for the
/// log, with the scratch directory that goes when it does.
pub struct NewPty {
    pub label: String,
    pub spawned: Started,
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
        Self::on_host(new, term::facts().host_answers_queries)
    }

    /// As [`Self::new`], with the device attributes, status and cursor reports left to the clients
    /// where `host_answers_queries` (`TermFacts::host_answers_queries`); see [`Fan::new`].
    fn on_host(new: NewPty, host_answers_queries: bool) -> Self {
        let backend = Backend::from(new.spawned);
        let size = backend.size().unwrap_or(Size { rows: 24, cols: 80 });
        Self {
            label: new.label,
            pid: backend.pid().0,
            backend,
            reader: Mutex::new(None),
            child: Mutex::new(()),
            reaped: AtomicBool::new(false),
            exit_complete: AtomicBool::new(false),
            exit_cleanup: Mutex::new(()),
            crash_cleanup: Mutex::new(None),
            fan: Fan::new(size.rows, size.cols, host_answers_queries),
            screen_inexact: AtomicBool::new(false),
            resize_lock: Mutex::new(()),
            client_changes_held: AtomicBool::new(false),
            answers: Mutex::new(None),
            reader_running: AtomicBool::new(false),
            answering: AtomicBool::new(false),
            answers_lost: AtomicBool::new(false),
            replies_pending: AtomicUsize::new(0),
            final_output_kept: Mutex::new(false),
            scratch_dir: Mutex::new(new.scratch_dir),
            before_scratch_removal: Mutex::new(new.before_scratch_removal),
            kill_session_leftovers: new.kill_session_leftovers,
        }
    }

    /// The process's pid, as its host knows it.
    pub fn process_id(&self) -> ProcessId {
        ProcessId(self.pid)
    }

    /// The local child, for the tests that put it in the crash cleanup by hand.
    #[cfg(test)]
    pub fn handle(&self) -> &crate::platform::proc::ChildHandle {
        self.backend.local_handle()
    }

    /// Registers a terminal client, whose first paint carries up to `history` rows of scrollback;
    /// see [`Fan::attach`].
    pub fn attach(&self, history: usize) -> Attached {
        self.fan.attach(history)
    }

    /// What there is to send to this client now, as its credit allows; see [`Fan::take_output`].
    pub fn take_output(&self, client: u64) -> Delivery {
        self.fan.take_output(client)
    }

    /// Notes that this client has consumed `bytes` more of what it was sent; see
    /// [`Fan::acknowledge`].
    pub fn acknowledge(&self, client: u64, bytes: u64) {
        self.fan.acknowledge(client, bytes);
    }

    /// Whether this client's answer to the output up to `offset` is the one written; see
    /// [`Fan::admit_reply`]. One admitted is owed to the program ([`Self::replies_owed`]) until
    /// the caller settles it ([`Self::settle_reply`]).
    pub fn admit_reply(&self, client: u64, offset: u64, reply: &[u8]) -> bool {
        // Counted before the fan records the answer, which ends what it owes, so that no moment
        // between the two reads as nothing owed; taken back if the reply is not admitted.
        self.replies_pending
            .fetch_add(reply.len(), Ordering::AcqRel);
        let admitted = self.fan.admit_reply(client, offset, reply);
        if !admitted {
            self.take_back_replies(reply.len());
        }
        admitted
    }

    /// Settles `bytes` of the clients' replies [`Self::admit_reply`] admitted: written into the
    /// program's input, or, not `written`, never to be, which counts as an answer lost
    /// ([`Self::answers_lost`]).
    pub fn settle_reply(&self, bytes: usize, written: bool) {
        if !written {
            self.note_lost_answers();
        }
        self.take_back_replies(bytes);
    }

    /// Takes `bytes` off the replies owed, saturating, in case a caller settles more than was
    /// admitted.
    fn take_back_replies(&self, bytes: usize) {
        let _ = self
            .replies_pending
            .try_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                Some(pending.saturating_sub(bytes))
            });
    }

    /// Whether a client still owes the program a reply: one of them was passed a query the
    /// daemon left to the clients and has not answered it yet, while the program may still wait
    /// for the answer ([`Fan::replies_owed`]), or a reply admitted has not been written yet. At a
    /// planned stop, the clients are sent only output from before the terminal's mark, and nobody
    /// else answers the queries in it they owe.
    pub fn replies_owed(&self) -> bool {
        self.replies_pending.load(Ordering::Acquire) > 0 || self.fan.replies_owed()
    }

    /// Waits up to `timeout` for [`Self::replies_owed`] to end. **Blocks** for that long.
    pub fn wait_for_replies(&self, timeout: Duration) {
        wait_while(timeout, || self.replies_owed());
    }

    /// Notes that this client was used (it sent input, or the person turned to it), applying its
    /// size and display; see [`Fan::activate`].
    pub fn activate(&self, client: u64) {
        let update = self.move_client_size(|resize| self.fan.activate(client, resize));
        self.tell_client_update(update);
    }

    /// Records the size this client asks for, `cols` by `rows`, and applies it while this is the
    /// most recently active client; see [`Fan::request_size`]. Refused when the size is out of the
    /// supported range or the screen has been frozen.
    pub fn request_size(&self, client: u64, cols: u16, rows: u16) -> anyhow::Result<()> {
        ensure_supported(rows, cols)?;
        let update =
            self.move_client_size(|resize| self.fan.request_size(client, rows, cols, resize))?;
        self.tell_client_update(update);
        Ok(())
    }

    /// Applies what follows the most recently active client again, once an attach's precedence
    /// may have lapsed; see [`Fan::settle_activity`].
    pub fn settle_activity(&self) {
        let update =
            self.move_client_size(|resize| self.fan.settle_activity(Instant::now(), resize));
        self.tell_client_update(update);
    }

    /// Records the display this client reports; see [`Fan::report_display`].
    pub fn report_display(&self, client: u64, display: DisplayFacts) {
        let update = self.fan.report_display(client, display);
        self.tell_client_update(update);
    }

    /// Forgets this client, whose connection has ended, handing the size to the client that is the
    /// most recently active one now; see [`Fan::leave`].
    pub fn leave(&self, client: u64) {
        let update = self.move_client_size(|resize| self.fan.leave(client, resize));
        self.tell_client_update(update);
    }

    /// From now on no client changes the terminal's size or has the program told of its display or
    /// its focus: the clients are still followed, but what they ask for is left to the daemon that
    /// takes the terminal over, which they ask again. A change under way when this is called is
    /// finished first. A planned stop calls it before the terminal's handover mark, so that the
    /// parsed screen and the program's terminal keep the size the screen is saved at until the
    /// daemon parts, and nothing but the answers to the output before the mark goes into the
    /// program's input meanwhile.
    pub fn hold_client_changes(&self) {
        let _resizing = self.resize_lock.lock().expect("resize mutex poisoned");
        self.client_changes_held.store(true, Ordering::Release);
    }

    /// Hands what a client's doing has the daemon tell the program to the answers' writer, unless
    /// the clients' changes are held ([`Self::hold_client_changes`]).
    fn tell_client_update(&self, update: Vec<u8>) {
        if !self.client_changes_held.load(Ordering::Acquire) {
            self.tell(ToProgram::Update(update));
        }
    }

    /// Ends every attached client's stream once the output waiting for it has been sent, and makes
    /// every later [`Self::attach`] a paint of the last screen alone; see [`Fan::close`].
    pub fn close_output(&self) {
        self.fan.close();
    }

    /// The tail of what this process has printed, at most `max_bytes` of it, from the replay ring:
    /// raw PTY bytes, escape sequences included, which the caller is responsible for making
    /// readable.
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
        wait_while(timeout, || self.reader_running.load(Ordering::Acquire));
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
    /// client; see [`Fan::push`], which never waits on a client.
    pub fn push_output(&self, data: &[u8], before_delivery: impl FnOnce(u64, Vec<TitleChange>)) {
        let answers = self.fan.push(data, before_delivery);
        self.tell(ToProgram::Answers(answers));
    }

    /// Hands what the daemon tells the program (its answers to queries, colour scheme updates) to
    /// the thread that writes it.
    fn tell(&self, message: ToProgram) {
        let (ToProgram::Answers(bytes) | ToProgram::Update(bytes)) = &message;
        if bytes.is_empty() {
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
            writer.send(message).ok();
        }
    }

    /// Takes the raw output a holder kept of a program the daemon did not start (`tail`) into the
    /// replay and the parsed screen, as plain output, before the reader thread starts: what a
    /// client attaching first is shown. It reaches no client and nothing the owner watches for.
    /// It is history, part of it already acted on by the daemon that read it first or by the
    /// holder, which answers some queries itself while no daemon is linked: the queries in it are
    /// not answered, and the moves of the program's status records it makes are dropped (the
    /// records themselves are rebuilt), so only output read from now on moves the status.
    pub fn seed_output(&self, tail: &[u8]) {
        self.fan.seed(tail, READ_BUF);
    }

    /// The parsed screen saved for the daemon that takes the terminal over at a planned restart,
    /// as the bytes that rebuild it (see [`SavedScreen`]), marked inexact where this screen is
    /// ([`Self::screen_inexact`]) or the bytes leave some of its state out. Taken once the reader
    /// has ended, so that nothing more comes after it. `None` while the output is in the middle of
    /// an escape sequence too long to keep, which leaves the screen nothing exact to save.
    pub fn save_screen(&self) -> Option<SavedScreen> {
        let mut saved = self.fan.save_screen()?;
        saved.inexact |= self.screen_inexact();
        Some(saved)
    }

    /// Takes in the screen a daemon saved at a planned restart ([`Self::save_screen`]) of the
    /// program this terminal was adopted for, in place of the holder's tail
    /// ([`Self::seed_output`]), before the reader thread starts: the parsed screen is given the
    /// saved size, which the caller has checked the PTY has, and then the saved bytes, as plain
    /// output, so that it is the screen the predecessor had and the output that follows parses as
    /// it would have there. Nothing in them is answered, and the moves of the program's status
    /// records and the title changes they make are dropped, so that only output read from now on
    /// moves either. The screen is marked inexact where the saved one was, or where its size is
    /// out of the supported range, which leaves the parsed screen at its size. That branch is
    /// unreachable today and kept so that an unchecked caller cannot restore a size the parsed
    /// screen does not support.
    pub fn restore_screen(&self, saved: &SavedScreen) {
        let supported = saved.restore_into(&self.fan, READ_BUF);
        if !supported {
            tracing::warn!(terminal = %self.label, rows = saved.rows, cols = saved.cols, "the saved screen's size is out of the supported range");
        }
        if saved.inexact || !supported {
            self.mark_screen_inexact();
        }
    }

    /// Has the reader take the first `rule.len` bytes it pushes in as the tail the terminal's
    /// holder kept since a handover, which the predecessor never read: within it only the queries
    /// still waited for are answered ([`ReplayRule`]); everything else is taken in as live output.
    /// Called once the saved screen is restored ([`Self::restore_screen`]) and before the reader
    /// thread starts.
    pub fn begin_replay(&self, rule: ReplayRule) {
        self.fan.begin_replay(rule);
    }

    /// Whether a reader thread is reading the PTY: not before [`spawn_reader_thread`] has started
    /// one, nor once it has read everything the PTY will give it.
    pub fn reader_running(&self) -> bool {
        self.reader_running.load(Ordering::Acquire)
    }

    /// Whether the answers to the queries the reader took in may still be on their way into the
    /// program's input: from [`spawn_reader_thread`] until the reader has ended and the answers'
    /// writer has written everything queued for it, a held terminal's each acknowledged by its
    /// holder. A reader that has ended ([`Self::reader_running`]) has left nothing more to answer,
    /// but its last answers may still be queued; at a planned stop, this is the fence that says
    /// the answers to every query before a terminal's mark went in.
    pub fn answering(&self) -> bool {
        self.answering.load(Ordering::Acquire)
    }

    /// Whether a write of the answers to the program's queries failed, or the link to its holder
    /// was lost before the holder said how it went, since [`Self::forget_lost_answers`]: the
    /// program may never have had one of them.
    pub fn answers_lost(&self) -> bool {
        self.answers_lost.load(Ordering::Acquire)
    }

    /// Starts [`Self::answers_lost`] over, as a planned stop does before it hands the terminal
    /// over.
    pub fn forget_lost_answers(&self) {
        self.answers_lost.store(false, Ordering::Release);
    }

    fn note_lost_answers(&self) {
        self.answers_lost.store(true, Ordering::Release);
    }

    /// Waits up to `timeout` for [`Self::answering`] to end. **Blocks** for that long.
    pub fn wait_for_answers(&self, timeout: Duration) {
        wait_while(timeout, || self.answering.load(Ordering::Acquire));
    }

    /// Notes that the parsed screen was rebuilt from the raw tail a holder kept (see
    /// [`Self::seed_output`]), which holds only the last of what the program drew, so what a
    /// client is first painted may differ from what the program has on its own screen. The mark
    /// stays for the life of the terminal.
    pub fn mark_screen_inexact(&self) {
        self.screen_inexact.store(true, Ordering::Release);
    }

    /// Has `hook` run, on a thread of its own, each time the terminal's holder was out of reach
    /// and is reached again with the program running: what it printed meanwhile never reached the
    /// parsed screen or the replay, so an owner marks the screen inexact and has the program
    /// repaint it ([`Self::nudge_repaint`]). Never runs for a terminal the daemon owns.
    pub fn on_resumed(&self, hook: impl Fn() + Send + Sync + 'static) {
        self.backend.on_resumed(hook);
    }

    /// Has `hook` run, on a thread of its own, each time the terminal's holder can no longer be
    /// linked to while it runs, and again when it can, or is known to be gone; see
    /// [`Self::holder_unlinked`]. Never runs for a terminal the daemon owns.
    pub fn on_link_changed(&self, hook: impl Fn() + Send + Sync + 'static) {
        self.backend.on_link_changed(hook);
    }

    /// Whether the terminal's holder runs but cannot be linked to for now: the program goes on,
    /// unreachable, and every call on the terminal fails until the holder is linked to again.
    pub fn holder_unlinked(&self) -> bool {
        self.backend.unlinked()
    }

    /// Which holder runs the terminal, and its id there; `None` for one the daemon owns.
    pub fn held_at(&self) -> Option<crate::holder::client::HeldAt> {
        self.backend.held_at()
    }

    /// Whether [`Self::mark_screen_inexact`] was called.
    pub fn screen_inexact(&self) -> bool {
        self.screen_inexact.load(Ordering::Acquire)
    }

    /// Has the program draw its screen again: the terminal is resized one row smaller and back to
    /// the size the clients' rule last gave it (one row larger and back where it has a single
    /// row), which a full-screen program answers by repainting at that size. The parsed screen
    /// follows both resizes, as for any other. No client asked for either, so neither counts as a
    /// client's activity, moves the size the clients decide ([`Fan::request_size`]) or changes the
    /// display; both are made under `resize_lock`, which every change the rule makes takes too, so
    /// none comes between them to be undone or repeated. Ending at the rule's size also brings a
    /// held PTY back to it after a resize its holder could not take while out of reach.
    /// **Blocks** on the answers of the terminal's host.
    pub fn nudge_repaint(&self) -> anyhow::Result<()> {
        let _resizing = self.resize_lock.lock().expect("resize mutex poisoned");
        let (rows, cols) = self.fan.screen_size();
        let nudged = if rows > 1 { rows - 1 } else { rows + 1 };
        self.move_size_locked(|resize| self.fan.resize(nudged, cols, resize))?;
        self.move_size_locked(|resize| self.fan.resize(rows, cols, resize))
    }

    /// Keeps the program's status reports (OSC 7501) from now on; see
    /// [`Fan::keep_program_status`].
    pub fn keep_program_status(&self) {
        self.fan.keep_program_status();
    }

    /// Forgets the program's `working` and `blocked` status records once its process has ended;
    /// see [`Fan::end_program_status`].
    pub fn end_program_status(&self) {
        self.fan.end_program_status();
    }

    /// The states the program's root status record moved to since the last call; see
    /// [`Fan::take_program_status`].
    pub fn take_program_status(&self) -> Vec<Option<ReportedState>> {
        self.fan.take_program_status()
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
        self.backend
            .input_lock()
            .lock()
            .expect("input mutex poisoned")
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
        self.backend.write_raw(data)
    }

    /// Writes the daemon's own `input` (a paste, a key) as [`Self::write_locked`] writes raw
    /// input; the error counts the bytes the inputs were encoded as.
    pub fn write_input_locked(
        &self,
        _input: &MutexGuard<'_, ()>,
        input: &[Input],
    ) -> Result<(), PartialWrite> {
        self.backend.write_input(input)
    }

    /// Writes raw input, as [`Self::write_locked`] does, taking the lock itself.
    pub fn write(&self, data: &[u8]) -> Result<(), PartialWrite> {
        let input = self.lock_input();
        self.write_locked(&input, data)
    }

    /// Writes a client's reply to a query in output it was sent, one [`Fan::admit_reply`] admitted,
    /// under the lock `input` is a guard of, as [`Self::write_answers_locked`] writes the daemon's
    /// own answers: a client is sent only output from before the terminal's mark, and the queries
    /// in it that the daemon leaves to a client nobody else answers.
    pub fn write_reply_locked(
        &self,
        input: &MutexGuard<'_, ()>,
        data: &[u8],
    ) -> Result<(), PartialWrite> {
        self.write_answers_locked(input, data)
    }

    /// [`Self::write_reply_locked`], taking the lock itself.
    pub fn write_reply(&self, data: &[u8]) -> Result<(), PartialWrite> {
        let input = self.lock_input();
        self.write_reply_locked(&input, data)
    }

    /// Writes answers to the queries in the output, under the lock `input` is a guard of, as
    /// [`Self::write_locked`] does, except that a held terminal still takes them once it is handed
    /// over, until the daemon parts (see [`ToProgram`]). An answer that does not go in counts as
    /// lost ([`Self::answers_lost`]), which keeps a planned stop from saving the screen as exact.
    fn write_answers_locked(
        &self,
        _input: &MutexGuard<'_, ()>,
        data: &[u8],
    ) -> Result<(), PartialWrite> {
        let written = self.backend.write_answers(data);
        if written.is_err() {
            self.note_lost_answers();
        }
        written
    }

    /// Resizes the terminal to `cols` by `rows` whichever client is the most recently active, as
    /// a test drives a process with no client attached. Clients go through
    /// [`Self::request_size`].
    #[cfg(test)]
    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        ensure_supported(rows, cols)?;
        self.move_size(|resize| self.fan.resize(rows, cols, resize))
    }

    /// Whether a change of the terminal's size waits on its host's answer, which a held terminal's
    /// does: a caller on the runtime makes one off its worker then ([`Self::activate`],
    /// [`Self::request_size`], [`Self::settle_activity`], [`Self::leave`]).
    pub fn resize_waits(&self) -> bool {
        self.backend.resize_waits_on_reply()
    }

    /// [`Self::move_size`] for a change a client's doing asks for: once the clients' changes are
    /// held ([`Self::hold_client_changes`]), `step` runs with a resize that is refused, so the fan
    /// still follows the client and neither the parsed screen nor the PTY changes size.
    fn move_client_size<R>(
        &self,
        step: impl FnOnce(&mut dyn FnMut(u16, u16) -> anyhow::Result<()>) -> R,
    ) -> R {
        let _resizing = self.resize_lock.lock().expect("resize mutex poisoned");
        if self.client_changes_held.load(Ordering::Acquire) {
            return step(&mut |_: u16, _: u16| -> anyhow::Result<()> {
                anyhow::bail!("the terminal's size is held for its handover")
            });
        }
        self.move_size_locked(step)
    }

    /// Runs `step`, a call on the fan that may change the terminal's size, with what resizes the
    /// PTY, under `resize_lock`.
    #[cfg(test)]
    fn move_size<R>(
        &self,
        step: impl FnOnce(&mut dyn FnMut(u16, u16) -> anyhow::Result<()>) -> R,
    ) -> R {
        let _resizing = self.resize_lock.lock().expect("resize mutex poisoned");
        self.move_size_locked(step)
    }

    /// [`Self::move_size`] for a caller that holds `resize_lock`. A local PTY is resized under the
    /// fan's lock, so that no chunk is parsed between the PTY's resize and the screen's. A held
    /// terminal's resize waits on its holder, whose answer may sit behind this terminal's own
    /// output, which the reader needs the fan's lock to take in: its parsed screen is resized
    /// under the lock and its PTY once the lock is released. Output parsed between the two was
    /// drawn at the old size; the program's repaint, which follows the PTY's resize, is parsed at
    /// the new one. A held resize that fails is logged and leaves the screen at the new size; one
    /// that failed for a holder out of reach is made good by the repaint nudge once the holder is
    /// reached again, which ends the PTY at the screen's size ([`Self::nudge_repaint`]).
    fn move_size_locked<R>(
        &self,
        step: impl FnOnce(&mut dyn FnMut(u16, u16) -> anyhow::Result<()>) -> R,
    ) -> R {
        if !self.backend.resize_waits_on_reply() {
            return step(&mut |rows: u16, cols: u16| -> anyhow::Result<()> {
                self.backend.resize(Size { rows, cols })
            });
        }
        let mut target = None;
        let moved = step(&mut |rows: u16, cols: u16| -> anyhow::Result<()> {
            target = Some(Size { rows, cols });
            Ok(())
        });
        if let Some(size) = target {
            if let Err(err) = self.backend.resize(size) {
                tracing::debug!(terminal = %self.label, %err, "resizing the held terminal failed");
            }
        }
        moved
    }

    /// Whose job is in the foreground of the PTY, the one the terminal's input and its signals go
    /// to: the process's own, or another it started. Only meaningful while the process runs.
    pub fn foreground_job(&self) -> Foreground {
        self.backend.foreground_job()
    }

    /// The device path of the PTY's slave side, what a process on the terminal has open as its
    /// stdin, stdout or stderr, or `None` when the system cannot say or the PTY is hung up.
    pub fn slave_path(&self) -> Option<std::path::PathBuf> {
        self.backend.slave_path()
    }

    /// Whether [`Self::hang_up`] has closed the PTY.
    #[cfg(test)]
    pub fn is_hung_up(&self) -> bool {
        self.backend.is_hung_up()
    }

    /// Hangs the terminal up, as closing a terminal window does: stops the reader thread, lets
    /// every attached client go, and closes the PTY master, after which the kernel hangs up the
    /// other side — a process still on it gets `SIGHUP` where it is in the terminal's foreground,
    /// and an error on every read and write from then on. Nothing reads the PTY afterwards, so
    /// whatever still holds the other side open no longer keeps this core alive. On Linux nothing
    /// revokes the terminal when its process exits, so this is the only thing that ends the
    /// reader while a leftover job holds the PTY open; on macOS that exit already ends it, so a
    /// test there cannot show the need. **Blocks** until the reader thread has finished.
    pub fn hang_up(&self) {
        self.close_output();
        let reader = self.reader.lock().expect("reader mutex poisoned").take();
        if let Some(reader) = reader {
            self.backend.stop_reader();
            if reader.thread().id() != std::thread::current().id() && reader.join().is_err() {
                tracing::warn!(terminal = %self.label, "the PTY reader thread panicked");
            }
        }
        let _input = self.lock_input();
        self.backend.hang_up();
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
        match self.backend.exited_unreaped() {
            Some(false) => return false,
            Some(true) => {
                self.leave_crash_cleanup();
                // Its zombie keeps the session id from passing to another process, so what is in
                // the session is the child's: a helper that moved to a process group of its own
                // too, which a signal to the child's group would miss.
                if self.kill_session_leftovers {
                    self.backend
                        .kill_leftovers(&[])
                        .log(self.backend.pid(), &self.label);
                }
            }
            None => {}
        }
        let exited = match self.backend.reap_if_exited() {
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

        let deadline = Instant::now() + self.backend.polite_stop_lead(polite) + STOP_GRACE;
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
        let Some(entry) = self
            .backend
            .enter_crash_cleanup(self.kill_session_leftovers)
        else {
            return;
        };
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
            Some(polite) => self.backend.stop(polite),
            None => self.backend.kill(),
        };
        // A process that is gone just exited in the meantime, which the caller finds out by
        // polling.
        if let Sent::Failed(err) = sent {
            tracing::warn!(terminal = %self.label, pid = self.pid, %err, "signalling the process failed");
        }
        drop(child);
    }
}

/// Waits, up to `timeout`, while `busy` says so. **Blocks** for that long.
fn wait_while(timeout: Duration, busy: impl Fn() -> bool) {
    let deadline = Instant::now() + timeout;
    while busy() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// What owns a [`PtyCore`] and wants to see each chunk the reader takes from it.
pub trait PtyOwner: Send + Sync + 'static {
    fn pty(&self) -> &PtyCore;

    /// Called from the PTY reader thread for every chunk read; it is expected to pass the chunk on
    /// through [`PtyCore::push_output`].
    fn on_output(&self, data: &[u8]);
}

/// Spawns the OS thread that reads the owner's PTY. A blocking reader on a dedicated thread
/// rather than a tokio task, because its reads block and parsing what they return must not occupy
/// a runtime worker.
pub fn spawn_reader_thread<T: PtyOwner>(owner: Arc<T>, read_buf_bytes: usize) {
    let pty = owner.pty();
    pty.reader_running.store(true, Ordering::Release);
    // Held while the thread is stored, so a hang-up cannot look for it in between.
    let mut reader = pty.reader.lock().expect("reader mutex poisoned");
    let thread_owner = owner.clone();
    let pty_reader = pty.backend.reader();
    let (sender, answers) = mpsc::channel::<ToProgram>();
    *pty.answers.lock().expect("answers mutex poisoned") = Some(sender);
    pty.answering.store(true, Ordering::Release);
    let writer_owner = owner.clone();
    // One writer, so the answers go in in order, and promptly: a program waits for its answer, and
    // a host may hold it up until a query is answered, for about half a second at most. A held
    // terminal's answers go to its holder as any input does, each write answered by the holder
    // once it is in the program's input.
    std::thread::spawn(move || {
        for message in answers {
            let pty = writer_owner.pty();
            let written = match &message {
                ToProgram::Answers(bytes) => {
                    let input = pty.lock_input();
                    pty.write_answers_locked(&input, bytes)
                }
                ToProgram::Update(bytes) => pty.write(bytes),
            };
            if let Err(err) = written {
                tracing::debug!(terminal = %pty.label, %err, "telling the program the daemon's answer or update failed");
            }
        }
        // The channel closes once the reader has ended and dropped its sender, so everything it
        // queued has been written by now.
        writer_owner.pty().answering.store(false, Ordering::Release);
    });
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
    fn an_exited_terminal_is_painted_with_its_last_screen_and_modes() {
        let live = crate::test_support::idle_stand_in("exited-paint");
        live.on_output(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h");
        for _ in 0..10 {
            live.on_output(b"\r\x1b[2Kframe");
        }
        live.close_output();
        let paint = live.attach(0).opening;
        assert!(
            paint.starts_with(b"\x1bc"),
            "a paint, not the ring's replay"
        );
        let paint = String::from_utf8_lossy(&paint);
        for expected in ["?1049h", "?1000h", "?1006h", "frame"] {
            assert!(
                paint.contains(expected),
                "{expected} missing from {paint:?}"
            );
        }
    }

    /// Once the clients' changes are held for a handover, no client changes the terminal's size,
    /// by asking for one, by being used or by leaving, while the clients are still followed.
    #[test]
    fn once_client_changes_are_held_no_client_changes_the_size() {
        let live = crate::test_support::idle_stand_in("held-size");
        let first = live.attach(0).answering.expect("a live client").client;
        live.request_size(first, 100, 30).expect("the size");
        assert_eq!(live.fan.screen_size(), (30, 100));
        live.hold_client_changes();
        let second = live.attach(0).answering.expect("a live client").client;
        live.request_size(second, 120, 40)
            .expect("the size is asked for");
        live.activate(second);
        live.settle_activity();
        assert_eq!(live.fan.screen_size(), (30, 100));
        live.leave(second);
        live.leave(first);
        assert_eq!(live.fan.screen_size(), (30, 100));
    }

    /// A client's reply admitted is owed to the program until it is settled, and one settled
    /// without being written counts as an answer lost.
    #[test]
    fn a_reply_admitted_is_owed_until_settled_and_one_never_written_is_lost() {
        let live = crate::test_support::idle_stand_in("owed-reply");
        let client = live.attach(0).answering.expect("a live client").client;
        live.push_output(b"x", |_, _| {});
        assert!(!live.replies_owed());
        assert!(live.admit_reply(client, 1, b"\x1b[1;1R"));
        assert!(live.replies_owed());
        live.settle_reply(6, true);
        assert!(!live.replies_owed());
        assert!(!live.answers_lost());
        assert!(live.admit_reply(client, 1, b"\x1b[2;1R"));
        live.settle_reply(6, false);
        assert!(!live.replies_owed());
        assert!(live.answers_lost());
        live.leave(client);
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
                    live.backend.input_lock().try_lock().is_err(),
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
                assert!(live.backend.input_lock().try_lock().is_err());
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
        let core = super::PtyCore::on_host(
            super::NewPty {
                label: "answers".to_string(),
                spawned: term::spawn(cmd, SIZE, None).expect("process").into(),
                scratch_dir: None,
                kill_session_leftovers: false,
                before_scratch_removal: None,
            },
            false,
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

    /// A holder's tail is history: a query in it was answered already, by the daemon that read it
    /// first or by the holder while no daemon was linked, so taking it in answers none of it,
    /// while a query read live after it is answered.
    #[test]
    fn the_queries_in_a_holders_tail_are_never_answered_again() {
        let cmd = crate::test_support::programs::hold_stdin().term_command();
        let core = super::PtyCore::on_host(
            super::NewPty {
                label: "tail-answers".to_string(),
                spawned: term::spawn(cmd, SIZE, None).expect("process").into(),
                scratch_dir: None,
                kill_session_leftovers: false,
                before_scratch_removal: None,
            },
            false,
        );
        core.seed_output(b"tail \x1b[c more \x1b[0c");
        let owner = std::sync::Arc::new(Owner(core));
        super::spawn_reader_thread(owner.clone(), 8 * 1024);
        let heard = |what: &str| {
            let started = Instant::now();
            loop {
                let heard = String::from_utf8_lossy(&owner.0.recent_output(4096)).into_owned();
                if heard.contains(what) || started.elapsed() >= crate::test_support::PATIENCE {
                    return heard;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };

        owner.0.write(b"marker\r\n").expect("the write");
        assert!(heard("marker").contains("marker"));
        std::thread::sleep(std::time::Duration::from_millis(200));
        let before = heard("marker");
        assert!(!before.contains("?6c"), "{before:?}");

        super::PtyOwner::on_output(&*owner, b"\x1b[c");
        owner.0.write(b"\r\n").expect("the write");
        let after = heard("?6c");
        assert!(after.contains("?6c"), "{after:?}");

        owner.0.hang_up();
        owner.0.stop(Polite::Terminate);
    }

    /// A screen saved at a planned restart is the one the terminal adopted next restores: saved
    /// again there, before any output, it is the same, an unended sequence and the inexact mark
    /// included, and taking it in moves no status.
    #[test]
    fn a_restored_screen_saves_again_as_the_screen_it_was_saved_from() {
        let core = |label: &str| {
            let cmd = crate::test_support::programs::sleep(30).term_command();
            let core = super::PtyCore::on_host(
                super::NewPty {
                    label: label.to_string(),
                    spawned: term::spawn(cmd, SIZE, None).expect("process").into(),
                    scratch_dir: None,
                    kill_session_leftovers: false,
                    before_scratch_removal: None,
                },
                false,
            );
            core.keep_program_status();
            core
        };
        let original = core("saved");
        original.push_output(
            b"\x1b]7501;state=working\x07some output\r\n\x1b[1mbold\x1b[?2031h\x1b]0;tit",
            |_, _| {},
        );
        original.mark_screen_inexact();
        let saved = original.save_screen().expect("a screen to save");
        assert!(saved.inexact);
        assert_eq!((saved.rows, saved.cols), (SIZE.rows, SIZE.cols));
        assert!(
            saved.bytes.ends_with(b"\x1b]0;tit"),
            "the unended title last"
        );

        let restored = core("restored");
        assert!(!restored.reader_running());
        restored.restore_screen(&saved);
        assert!(restored.screen_inexact());
        assert!(restored.take_program_status().is_empty());
        assert_eq!(restored.screen_text(), original.screen_text());
        assert_eq!(restored.save_screen().as_ref(), Some(&saved));
        original.stop(Polite::Terminate);
        restored.stop(Polite::Terminate);
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
            spawned: spawned.into(),
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
            spawned: spawned.into(),
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
            spawned: term::spawn(cmd, SIZE, None).expect("process").into(),
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

    /// A [`super::PtyCore`] on a program run in a holder of this process, with its reader running.
    struct HeldCore<O = Owner> {
        owner: std::sync::Arc<O>,
        holders: crate::holder::client::Holders,
        /// Where the holders register, which a next daemon in the test links to them from.
        #[cfg_attr(not(unix), allow(dead_code))]
        dir: crate::test_support::ScratchDir,
    }

    fn held_core(label: &str, program: crate::test_support::programs::Program) -> HeldCore {
        held_core_of(label, program.term_command())
    }

    fn held_core_of(label: &str, command: term::Command) -> HeldCore {
        held_core_owned(label, command, Owner)
    }

    /// [`held_core_of`], with the owner `own` builds around the core.
    fn held_core_owned<O: super::PtyOwner>(
        label: &str,
        command: term::Command,
        own: impl FnOnce(super::PtyCore) -> O,
    ) -> HeldCore<O> {
        let dir = crate::test_support::ScratchDir::new("held-core");
        let holders = crate::holder::client::in_process(dir.join("holders")).expect("holders");
        let terminal = holders
            .spawn(crate::holder::protocol::SpawnSpec {
                id: label.to_string(),
                command,
                size: SIZE,
                containment: None,
                kill_session_leftovers: false,
                meta: serde_json::Value::Null,
            })
            .expect("the program starts in the holder");
        let core = super::PtyCore::on_host(
            super::NewPty {
                label: label.to_string(),
                spawned: super::Started::Held(terminal),
                scratch_dir: None,
                kill_session_leftovers: false,
                before_scratch_removal: None,
            },
            false,
        );
        let owner = std::sync::Arc::new(own(core));
        super::spawn_reader_thread(owner.clone(), 8 * 1024);
        HeldCore {
            owner,
            holders,
            dir,
        }
    }

    fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !ready() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn a_held_programs_exit_is_observed_only_after_what_it_printed() {
        let held = held_core(
            "held-echo",
            crate::test_support::programs::echo("held says hi"),
        );
        let core = &held.owner.0;
        wait_until("the program never exited", || core.poll_exit());
        let mut kept = String::new();
        core.keep_final_output(|output| kept = String::from_utf8_lossy(output).into_owned());
        assert!(kept.contains("held says hi"), "{kept:?}");
    }

    #[test]
    fn a_held_program_takes_input_a_resize_and_a_polite_stop() {
        let held = held_core("held-cat", crate::test_support::programs::hold_stdin());
        let core = &held.owner.0;
        core.write(b"hello\n").expect("the write");
        wait_until("the input never came back", || {
            String::from_utf8_lossy(&core.recent_output(1024)).contains("hello")
        });
        core.resize(100, 30).expect("the resize");
        #[cfg(unix)]
        {
            assert!(core.slave_path().is_some());
            assert_eq!(core.foreground_job(), term::Foreground::Own);
        }
        core.stop(Polite::Terminate);
        assert!(core.poll_exit());
    }

    /// The repaint nudge ends at the size the clients' rule gave the terminal, the held PTY's and
    /// the parsed screen's alike, and leaves the rule as it was: the client stays the one whose
    /// size the terminal takes.
    #[test]
    fn a_repaint_nudge_ends_a_held_terminal_at_the_size_the_rule_gave_it() {
        let held = held_core("held-nudge", crate::test_support::programs::hold_stdin());
        let core = &held.owner.0;
        let client = core.attach(0).answering.expect("a live client").client;
        core.request_size(client, 100, 30).expect("the size");
        core.nudge_repaint().expect("the nudge");
        let ruled = Size {
            rows: 30,
            cols: 100,
        };
        assert_eq!(core.fan.screen_size(), (ruled.rows, ruled.cols));
        assert_eq!(core.backend.size().expect("the held size"), ruled);
        core.activate(client);
        assert_eq!(core.backend.size().expect("the held size"), ruled);
        core.leave(client);
        core.stop(Polite::Terminate);
    }

    #[test]
    fn a_write_into_a_hung_up_held_terminal_fails_with_nothing_written() {
        let held = held_core("held-hung-up", crate::test_support::programs::sleep(30));
        let core = &held.owner.0;
        core.hang_up();
        let failed = core.write(b"x").expect_err("the terminal is hung up");
        assert_eq!(failed.written, 0);
        core.stop(Polite::HangUp);
    }

    #[test]
    fn a_held_terminal_whose_link_is_gone_reads_as_exited_and_takes_no_writes() {
        let held = held_core("held-gone", crate::test_support::programs::sleep(60));
        let core = &held.owner.0;
        let pid = core.pid as i32;
        let _guard = KillOnDrop(vec![pid]);
        assert!(is_running(pid));
        held.holders.shutdown();
        wait_until("the terminal never read as exited", || core.poll_exit());
        assert_eq!(core.write(b"x").expect_err("the holder is gone").written, 0);
        // The holder keeps what it runs for the daemon that links next.
        assert!(is_running(pid));
    }

    /// A program that reads nothing takes only what the terminal buffers, and the write that does
    /// not fit says how far it got, as a terminal the daemon owns does.
    #[cfg(unix)]
    #[test]
    fn a_partial_write_into_a_held_terminal_is_reported_as_one() {
        let mut deaf = term::Command::new("/bin/sh");
        deaf.args(["-c", "stty raw -echo; echo ready; exec sleep 60"]);
        let held = held_core_of("held-partial", deaf);
        let core = &held.owner.0;
        wait_until("the program never got ready", || {
            String::from_utf8_lossy(&core.recent_output(1024)).contains("ready")
        });
        let bytes = vec![b'x'; 4 * 1024 * 1024];
        let failed = core
            .write(&bytes)
            .expect_err("the whole write cannot go in");
        assert!(failed.written < bytes.len());
        core.stop(Polite::Terminate);
    }

    /// An owner whose reader stops once, before taking in the chunk that completes `gate_on`,
    /// says so on the first channel, and goes on once the second one says to.
    #[cfg(unix)]
    struct Gated {
        core: super::PtyCore,
        gate_on: &'static [u8],
        seen: std::sync::Mutex<Vec<u8>>,
        gate:
            std::sync::Mutex<Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>>,
    }

    #[cfg(unix)]
    impl super::PtyOwner for Gated {
        fn pty(&self) -> &super::PtyCore {
            &self.core
        }

        fn on_output(&self, data: &[u8]) {
            let reached = {
                let mut seen = self.seen.lock().unwrap();
                seen.extend_from_slice(data);
                seen.windows(self.gate_on.len())
                    .any(|window| window == self.gate_on)
            };
            let gate = if reached {
                self.gate.lock().unwrap().take()
            } else {
                None
            };
            if let Some((reached, release)) = gate {
                reached.send(()).ok();
                release.recv().ok();
            }
            self.core.push_output(data, |_, _| {});
        }
    }

    /// A query the reader takes in only after the handover's mark has arrived came before the
    /// mark, so the holder and the next daemon leave it to this one: its answer still goes into
    /// the program's input, once, before the answers' fence lifts, while input sent after the
    /// mark is refused.
    #[cfg(unix)]
    #[test]
    fn a_query_read_before_a_handover_mark_is_answered_once_though_the_mark_came_first() {
        use std::ffi::OsStr;

        let files = crate::test_support::ScratchDir::new("held-mark-answers");
        let heard = files.join("heard");
        let mut command = term::Command::new("/bin/sh");
        command.args([
            OsStr::new("-c"),
            OsStr::new("stty raw -echo; printf '\\033[6n'; exec cat > \"$1\""),
            OsStr::new("sh"),
            heard.as_os_str(),
        ]);
        let (reached, at_query) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let HeldCore {
            owner,
            holders,
            dir,
        } = held_core_owned("held-mark-answers", command, |core| Gated {
            core,
            gate_on: b"\x1b[6n",
            seen: std::sync::Mutex::new(Vec::new()),
            gate: std::sync::Mutex::new(Some((reached, released))),
        });
        let _guard = KillOnDrop(vec![owner.core.pid as i32]);
        at_query
            .recv_timeout(crate::test_support::PATIENCE)
            .expect("the query never reached the reader");

        // The mark is routed behind the query's output while the reader holds that output.
        let at = owner.core.held_at().expect("a held terminal");
        assert_eq!(
            holders.hand_over("tok", std::slice::from_ref(&at.holder_id)),
            vec![at]
        );
        release.send(()).unwrap();
        owner.core.wait_for_reader(crate::test_support::PATIENCE);
        assert!(
            !owner.core.reader_running(),
            "the reader did not end at the mark"
        );
        owner.core.wait_for_answers(crate::test_support::PATIENCE);
        assert!(!owner.core.answering(), "the answers were not all written");
        assert!(!owner.core.answers_lost(), "an answer was refused");
        assert!(
            !owner.core.replies_owed(),
            "a reply was owed with no client"
        );
        let refused = owner
            .core
            .write(b"x")
            .expect_err("input went in after the mark");
        assert_eq!(refused.written, 0);

        wait_until("the program never heard the cursor position", || {
            std::fs::read(&heard).unwrap_or_default().contains(&b'R')
        });
        std::thread::sleep(std::time::Duration::from_millis(300));
        let input = std::fs::read(&heard).expect("what the program heard");
        assert_eq!(
            input.iter().filter(|&&byte| byte == b'R').count(),
            1,
            "{input:?}"
        );
        assert!(
            input.starts_with(b"\x1b[") && input.ends_with(b"R"),
            "{input:?}"
        );

        holders.part_planned();
        drop(owner);
        let next = crate::holder::client::in_process(dir.join("holders")).expect("holders");
        let mut found = next
            .adopt_existing(crate::holder::client::unresolved())
            .found;
        found[0].terminals.remove(0).held.end();
    }
}
