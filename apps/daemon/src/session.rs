//! One running agent process: its PTY (`crate::pty`), and what is the agent's own on top of it —
//! the agent, its title, its folder-trust watch and the count of writes into its input.

use std::io;
mod draft;
mod swap;
use draft::Draft;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
pub(crate) use swap::RecoveryPhase;

use crate::agent_dialogs::{self, Dialog, Sighting};
use crate::protocol::Agent;
use crate::pty::{NewPty, PtyCore, PtyOwner, Spawned};
use crate::ptyio;
use crate::title_activity::TitleActivity;
use crate::trust::{CarriedTrust, TrustState};

pub use crate::pty::spawn_reader_thread;

/// Graceful teardown: agents are asked to exit with `SIGTERM` and only killed if they do not.
/// This is not politeness — Grok's `Stop` hook fires with `reason: "shutdown"` on a `SIGTERM`
/// teardown and not at all on a kill, and repeatedly killing Claude Code mid-startup trips its own
/// renderer-fallback counter, which is machine-level state it keeps about itself.
const AGENT_STOP_SIGNAL: libc::c_int = libc::SIGTERM;

/// How long an agent's error holds its session's writes ([`LiveSession::agent_error`]). Every flush
/// looks at the screen afresh, so this covers what a look can miss: the PTY reader getting the
/// modal's bytes into the parsed screen (`on_output` can block behind a slow client), and a version
/// that draws the modal slightly after its hooks.
pub const AGENT_ERROR_SETTLE: Duration = Duration::from_secs(2);

#[derive(Default)]
struct TitleObserver {
    parser: crate::terminal_title::Parser,
    latest: Option<Option<String>>,
    /// What the agent's titles have said about its turns; see `crate::title_activity`.
    activity: TitleActivity,
    state: Weak<crate::state::AppState>,
    runtime: Option<tokio::runtime::Handle>,
}

impl TitleObserver {
    fn publish(&self, session: &LiveSession) {
        if let (Some(state), Some(title)) = (self.state.upgrade(), &self.latest) {
            let cleared = state.record_terminal_title(session, title.as_deref());
            let action_required = session.needs_user_action();
            // Output arrives on the PTY reader thread, outside the Tokio runtime. A write
            // must not block that reader while the agent is still draining its output.
            if let (true, Some(runtime)) = (cleared || action_required, &self.runtime) {
                let id = session.id.clone();
                runtime.spawn_blocking(move || {
                    if action_required {
                        crate::reporting::tell_owner_it_waits(&state, &id);
                    }
                    if cleared {
                        if let Ok(record) = state.session_record(&id) {
                            state.flush_outbox(&id, state.agent_status(&record));
                        }
                    }
                });
            }
        }
    }
}

/// A running agent session. Everything about its process and PTY is its [`PtyCore`]'s, reached
/// through it directly (it dereferences to it); what is here is the agent's own.
pub struct LiveSession {
    pub id: String,
    pub agent: Agent,
    /// The launch this process is, as its processes are tagged with (`crate::process_ledger`).
    pub launch_id: String,
    pty: PtyCore,
    pub(crate) codex_transport: Option<Arc<crate::codex_transport::CodexTransport>>,
    pub(crate) checks_composer: bool,
    title: Mutex<TitleObserver>,
    action_required: AtomicBool,
    /// Permission hooks can describe automatic reviews without a dialog. Visible action prompts
    /// are observed separately, since this setting does not rule out manual approvals.
    pub resolves_approvals_itself: bool,
    /// Watches the session's output for its agent's folder-trust confirmation.
    pub trust: TrustState,
    /// How many times anything but the trust path has written into this session's input. An
    /// attached terminal's own protocol replies (focus reports, answers to the agent's terminal
    /// queries) are not counted, because they cannot move a cursor. Changed only under the PTY's
    /// input lock, so that the trust path's check of the count and its key are one step that no
    /// other writer can fall between. See [`Self::write_input`].
    input_writes: AtomicU64,
    draft: Mutex<Draft>,
    swap: Mutex<swap::InputSwap>,
    native_stash: AtomicBool,
    recovery_scheduled: AtomicBool,
    /// How many prompts the agent's hooks have reported it sending (`UserPromptSubmit`), counted
    /// once the event's status has been applied. What the outbox waits on to know that a message
    /// it wrote was sent rather than left in the input box.
    prompts_sent: AtomicU64,
    /// The acknowledgement count and grace-period start of an unconfirmed message. A prompt
    /// acknowledgement or one bounded submit-key retry releases its hold for this process.
    unsent_at: Mutex<Option<(u64, Instant)>>,
    unsent_retry_scheduled: AtomicBool,
    uncertain_at: Mutex<Option<(u64, Instant)>>,
    /// When the agent stopped on an error. It holds the session's writes for [`AGENT_ERROR_SETTLE`]
    /// after that, or until the next prompt is sent; gone with the process.
    agent_error_at: Mutex<Option<Instant>>,
    /// What the screen last showed of the dialogs only a person can answer; see
    /// [`crate::agent_dialogs`]. Read and updated under this lock together, so a look that read
    /// an older screen never overwrites a newer one.
    dialogs: Mutex<Sighting>,
    /// How many checked writes are in flight, each of which has the screen looked at on every chunk
    /// of output so a dialog drawn and closed inside the write is still seen; see
    /// [`Self::watch_screen`].
    screen_watchers: AtomicU32,
    /// Set by [`Self::end`] when it found the process running and stopped it: the end of this
    /// process was somebody's doing, not the agent's own.
    ended_on_purpose: AtomicBool,
    /// Set while Octoboard's own account switch, or the restoring of a session after a failed one
    /// (`LaunchClaim::watches_launch`), watches this process come up under its claim: if it dies
    /// meanwhile that operation reports the outcome, and the owner is not also told the process
    /// ended on its own.
    watched_under_claim: AtomicBool,
    /// Set by the exit watcher (`AppState::watch_exit`) once it is done with this process's end:
    /// the end is recorded and the owner, if it was to be told, has been sent the notice (written
    /// to it or queued for it) or could not be.
    exit_settled: AtomicBool,
}

impl std::ops::Deref for LiveSession {
    type Target = PtyCore;

    fn deref(&self) -> &PtyCore {
        &self.pty
    }
}

impl PtyOwner for LiveSession {
    fn pty(&self) -> &PtyCore {
        &self.pty
    }

    fn on_output(&self, data: &[u8]) {
        LiveSession::on_output(self, data);
    }
}

/// Everything one running session is built from. A struct rather than a parameter list: these are
/// all values the launch produced, and half of them are indistinguishable by type.
pub struct NewSession {
    pub id: String,
    pub agent: Agent,
    /// See `LiveSession::launch_id`.
    pub launch_id: String,
    pub spawned: Spawned,
    pub codex_transport: Option<Arc<crate::codex_transport::CodexTransport>>,
    /// Real agents have a composer; stand-in PTYs can explicitly opt out of its screen check.
    pub checks_composer: bool,
    /// Per-session scratch directory (Grok's `GROK_HOME` symlink farm), removed when the session's
    /// process is gone.
    pub scratch_dir: Option<std::path::PathBuf>,
    pub resolves_approvals_itself: bool,
    /// Grok's: where the trust it records when its confirmation is pressed is carried from and to;
    /// see `adapter::LaunchPlan::carried_trust`.
    pub carried_trust: Option<CarriedTrust>,
    /// Runs after the process is reaped and before its scratch directory is removed.
    pub before_scratch_removal: Option<Box<dyn FnOnce() + Send>>,
}

impl LiveSession {
    pub fn new(session: NewSession) -> Self {
        Self {
            pty: PtyCore::new(NewPty {
                label: session.id.clone(),
                spawned: session.spawned,
                scratch_dir: session.scratch_dir,
                kill_session_leftovers: true,
                before_scratch_removal: session.before_scratch_removal,
            }),
            codex_transport: session.codex_transport,
            checks_composer: session.checks_composer,
            id: session.id,
            agent: session.agent,
            launch_id: session.launch_id,
            title: Mutex::new(TitleObserver::default()),
            action_required: AtomicBool::new(false),
            resolves_approvals_itself: session.resolves_approvals_itself,
            trust: TrustState::new(session.agent, session.carried_trust),
            input_writes: AtomicU64::new(0),
            draft: Mutex::new(Draft::default()),
            swap: Mutex::new(swap::InputSwap::default()),
            native_stash: AtomicBool::new(false),
            recovery_scheduled: AtomicBool::new(false),
            prompts_sent: AtomicU64::new(0),
            unsent_at: Mutex::new(None),
            unsent_retry_scheduled: AtomicBool::new(false),
            uncertain_at: Mutex::new(None),
            agent_error_at: Mutex::new(None),
            dialogs: Mutex::new(Sighting::default()),
            screen_watchers: AtomicU32::new(0),
            ended_on_purpose: AtomicBool::new(false),
            watched_under_claim: AtomicBool::new(false),
            exit_settled: AtomicBool::new(false),
        }
    }

    /// Output may already have arrived before the launch is registered. Bind and publish the
    /// retained observation under the same lock as feeding, so an older title cannot win later.
    pub(crate) fn observe_titles(&self, state: &Arc<crate::state::AppState>) {
        let mut observer = self.title.lock().expect("title observer lock poisoned");
        observer.state = Arc::downgrade(state);
        observer.runtime = tokio::runtime::Handle::try_current().ok();
        observer.publish(self);
    }

    /// Consult hook and lifecycle holds without retaining the observer lock across state reads.
    fn allows_automatic_input(&self) -> bool {
        let state = self
            .title
            .lock()
            .expect("title observer lock poisoned")
            .state
            .clone();
        match state.upgrade() {
            Some(state) => state.permits_automatic_input(&self.id),
            // Isolated sessions have no observer; a formerly bound session fails closed.
            None => state.ptr_eq(&Weak::new()),
        }
    }

    /// What the agent's terminal title has said about its turns so far.
    pub fn title_activity(&self) -> TitleActivity {
        self.title
            .lock()
            .expect("title observer lock poisoned")
            .activity
    }

    /// The dialog the screen showed at the latest look, whoever made it: the watch, a write about
    /// to be made, a description of the session, or the output of a write in flight.
    pub fn agent_dialog(&self) -> Option<Dialog> {
        self.dialogs.lock().expect("dialogs lock poisoned").current
    }

    /// Looks at the screen now for a dialog and notes what it shows. Takes the sighting lock and
    /// then the output's, never the other way round, so the reader thread can call it between
    /// chunks.
    pub fn look_for_dialog(&self) -> Sighting {
        let mut shown = self.dialogs.lock().expect("dialogs lock poisoned");
        *shown = shown.after(agent_dialogs::recognise(
            self.agent,
            &self.pty.screen_text(),
        ));
        *shown
    }

    /// Starts (`true`) or ends (`false`) having the screen looked at on every chunk of output, for
    /// the length of a write whose outcome depends on what a dialog did to it. A dialog a write
    /// closes within milliseconds is gone before any look made after the write, and the watch's
    /// own polling is too slow to catch it.
    pub fn watch_screen(&self, on: bool) {
        if on {
            self.screen_watchers.fetch_add(1, Ordering::AcqRel);
        } else {
            self.screen_watchers.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// Whether the process is held up by something only the user can answer that no hook reports:
    /// the folder-trust confirmation, Codex's title saying it waits on the user, or a dialog on
    /// the screen as the latest look left it.
    pub(crate) fn waits_for_user(&self) -> bool {
        self.trust.needs_user() || self.needs_user_action() || self.agent_dialog().is_some()
    }

    /// [`Self::waits_for_user`], except that a usage-limit dialog on the screen (see
    /// `Dialog::is_usage_limit`) is not a wait that keeps the session from being switched.
    pub(crate) fn waits_for_user_beyond_usage_limit(&self) -> bool {
        self.trust.needs_user()
            || self.needs_user_action()
            || self
                .agent_dialog()
                .is_some_and(|dialog| !dialog.is_usage_limit())
    }

    pub(crate) fn needs_user_action(&self) -> bool {
        self.action_required.load(Ordering::Acquire)
    }

    pub(crate) fn observe_action_title(&self, title: Option<&str>) -> bool {
        let waiting = self.agent == Agent::Codex
            && title.is_some_and(crate::terminal_title::codex_action_required);
        self.action_required.swap(waiting, Ordering::AcqRel) != waiting
    }

    /// Called from the PTY reader thread for every chunk read. Blocks while a slow client catches
    /// up; see [`PtyCore::push_output`].
    pub fn on_output(&self, data: &[u8]) {
        let titled = {
            let mut observer = self.title.lock().expect("title observer lock poisoned");
            let completed = observer.parser.feed(data);
            if let Some(title) = &completed {
                let glyph = observer.parser.leading_glyph();
                // A title of nothing but a spinner reaches here empty, and still shows a turn.
                let empty = title.is_none() && glyph.is_none();
                let shown = crate::title_activity::shown(self.agent, glyph, empty);
                observer.activity.note(shown, Instant::now());
            }
            if let Some(title) = &completed {
                // A title that only spun since the last one changes nothing, so it does not reach
                // the store's lock.
                let changed = observer.latest.as_ref() != Some(title);
                observer.latest = Some(title.clone());
                if changed {
                    observer.publish(self);
                }
            }
            completed.is_some()
        };
        self.pty.push_output(data, |total| {
            // The count without its lock: a writer holds that lock while the agent may be waiting
            // for this very thread to drain its output.
            self.trust
                .feed(data, self.input_writes.load(Ordering::Acquire), total);
            // Run outside the output's lock, as a look takes that after the sighting's.
            if self.screen_watchers.load(Ordering::Acquire) > 0 {
                self.look_for_dialog();
            }
            if self.trust.follows_paste_mode() {
                self.trust.note_paste_mode(self.pty.bracketed_paste_on());
            }
        });
        if titled {
            self.trust.note_title();
        }
        self.schedule_input_recovery();
    }

    /// Writes raw input. The error says how many bytes went in, because a caller deciding whether
    /// to retry has to know: whatever was accepted is already in the child's input buffer.
    ///
    /// Counted: every writer but the trust path comes through here or through
    /// [`Self::write_keys_if`], which counts its keys too, and that is how the trust path learns
    /// that someone else has typed into the session since it last looked. A write made up
    /// only of what an attached terminal sends by itself — focus reports, replies to the agent's
    /// terminal queries, mouse movement, wheel turns and button releases — is not counted: Claude
    /// Code turns focus reporting on as it draws its trust screen, and Grok Build turns on
    /// reporting every mouse movement as it draws its own, so an attached terminal writes that
    /// traffic all the time and none of it answers a screen. A mouse button press is counted: it
    /// is the person acting. Where mouse reporting is off, as at Codex's trust screen, the wheel
    /// arrives as arrow keys, which are counted too.
    pub fn write_input(&self, data: &[u8]) -> Result<(), ptyio::PartialWrite> {
        self.write_raw_input(data, false)
    }

    pub(crate) fn write_terminal_input(&self, data: &[u8]) -> Result<(), ptyio::PartialWrite> {
        if self.buffer_terminal_input(data)? {
            return Ok(());
        }
        self.write_raw_input(data, true)
    }

    pub(crate) fn write_terminal_reply(&self, data: &[u8]) -> Result<(), ptyio::PartialWrite> {
        let input = self.pty.lock_input();
        if self.pty.is_frozen() {
            return Err(swap::blocked("the session output has been archived"));
        }
        self.pty.write_locked(&input, data)
    }

    fn write_raw_input(&self, data: &[u8], user: bool) -> Result<(), ptyio::PartialWrite> {
        let input = self.pty.lock_input();
        let track_draft = if user {
            match self.route_terminal_input(data)? {
                swap::InputRoute::Buffered => return Ok(()),
                swap::InputRoute::Dialog => false,
                swap::InputRoute::Direct => true,
            }
        } else {
            false
        };
        if !user && self.input_swap_active() && !is_terminal_protocol(data) {
            return Err(swap::blocked("an input replacement is in progress"));
        }
        if self.pty.is_frozen() {
            return Err(ptyio::PartialWrite {
                written: 0,
                error: io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the session output has been archived",
                ),
            });
        }
        if !is_terminal_protocol(data) {
            self.input_writes.fetch_add(1, Ordering::AcqRel);
            if track_draft {
                self.draft.lock().expect("draft lock poisoned").input(data);
            }
        }
        self.pty.write_locked(&input, data)
    }

    /// Checks the composer and the unacknowledged user input under the same lock as the write.
    /// A rendered empty box alone cannot acknowledge keys still waiting in the child's input.
    /// An expired protection budget bypasses editor checks but never dialogs or process lifetime.
    fn write_empty_message_input(
        &self,
        data: &[u8],
        prompt: &str,
        allow_mixed: bool,
    ) -> Result<(), ptyio::PartialWrite> {
        let input = self.pty.lock_input();
        if self.pty.is_frozen() {
            return Err(ptyio::PartialWrite {
                written: 0,
                error: io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the session output has been archived",
                ),
            });
        }
        if self.input_swap_active() {
            return Err(swap::blocked("an input replacement holds the composer"));
        }
        if !self.allows_automatic_input()
            || self.trust.holds_writes()
            || self.look_for_dialog().current.is_some()
        {
            return Err(swap::blocked(
                "the session is waiting for user action or showing a dialog or trust confirmation",
            ));
        }
        let mut draft = self.draft.lock().expect("draft lock poisoned");
        if self.checks_composer
            && !allow_mixed
            && (self.stash_shown()
                || draft.holds()
                || crate::interrupt::input_box::input_box(self.agent, &self.screen_text_undimmed())
                    .as_deref()
                    != Some(""))
        {
            return Err(ptyio::PartialWrite {
                written: 0,
                error: io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "the session's composer is not known to be empty",
                ),
            });
        }
        draft.submission(None, prompt.to_string());
        self.input_writes.fetch_add(1, Ordering::AcqRel);
        self.pty.write_locked(&input, data)
    }

    pub(crate) fn note_launch_prompt(&self, prompt: String) {
        self.draft
            .lock()
            .expect("draft lock poisoned")
            .submission(None, prompt);
    }

    /// Writes `data` only if nobody else has written into the input since the count read as
    /// `expected`, and says whether it did. For the trust path alone: its own keys are not counted,
    /// and the check and the write happen under the lock every other writer takes.
    pub(crate) fn write_input_if_untouched(
        &self,
        expected: u64,
        data: &[u8],
    ) -> Result<bool, ptyio::PartialWrite> {
        let input = self.pty.lock_input();
        if self.input_swap_active() || self.input_writes.load(Ordering::Acquire) != expected {
            return Ok(false);
        }
        self.pty.write_locked(&input, data).map(|()| true)
    }

    /// Writes `keys` only if nobody else has written into the input since the count read as
    /// `expected` and `gate` still holds, and says whether it did; `gate` runs under the lock every
    /// other writer takes, so nothing written in between can make it stale. Unlike the trust
    /// path's keys these are counted, so a later step of the same caller is told apart from
    /// someone else's write only by expecting the count past its own.
    pub(crate) fn write_keys_if(
        &self,
        expected: u64,
        gate: impl FnOnce() -> bool,
        keys: &[u8],
    ) -> Result<bool, ptyio::PartialWrite> {
        let input = self.pty.lock_input();
        if self.pty.is_frozen() {
            return Err(ptyio::PartialWrite {
                written: 0,
                error: io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the session output has been archived",
                ),
            });
        }
        if self.input_swap_active()
            || self.input_writes.load(Ordering::Acquire) != expected
            || !gate()
        {
            return Ok(false);
        }
        self.input_writes.fetch_add(1, Ordering::AcqRel);
        self.pty.write_locked(&input, keys).map(|()| true)
    }

    /// How many writes anyone else has made into this session's input so far.
    pub fn input_writes(&self) -> u64 {
        // Under the lock, so that the count is never read in the middle of a write.
        let _input = self.pty.lock_input();
        self.input_writes.load(Ordering::Acquire)
    }

    /// Counts a prompt the agent's hooks reported it sending, which also ends the hold an agent
    /// error put on the session's writes.
    pub fn note_prompt_sent(&self, prompt: Option<&str>) {
        self.draft
            .lock()
            .expect("draft lock poisoned")
            .prompt_sent(prompt, self.agent);
        self.prompts_sent.fetch_add(1, Ordering::AcqRel);
        *self
            .agent_error_at
            .lock()
            .expect("agent error lock poisoned") = None;
    }

    /// Records that the agent stopped on an error.
    pub fn mark_agent_error(&self) {
        *self
            .agent_error_at
            .lock()
            .expect("agent error lock poisoned") = Some(Instant::now());
    }

    /// Whether the agent stopped on an error less than [`AGENT_ERROR_SETTLE`] ago and no prompt
    /// has been sent since. Past that, whatever the error left on screen is the dialog watch's to
    /// recognise ([`crate::agent_dialogs`]): Grok Build draws its usage-limit modal before the
    /// error's hooks arrive, and after an error that draws none (a refused request, retries run
    /// out, a modal dismissed) its input is free, so holding on until a prompt would only keep
    /// messages from a session that can take them.
    pub fn agent_error(&self) -> bool {
        self.agent_error_at
            .lock()
            .expect("agent error lock poisoned")
            .is_some_and(|at| at.elapsed() < AGENT_ERROR_SETTLE)
    }

    /// How many prompts the agent's hooks have reported it sending so far.
    pub fn prompts_sent(&self) -> u64 {
        self.prompts_sent.load(Ordering::Acquire)
    }

    /// Records that a message written when the count stood at `before` was not sent.
    pub fn mark_unsent(&self, before: u64) {
        *self.unsent_at.lock().expect("unsent mark lock poisoned") = Some((before, Instant::now()));
    }

    /// Whether a message marked unsent is still taken to be waiting in the input box: until the
    /// agent reports a prompt sent after it or the bounded submit-key retry runs.
    pub fn unsent_waiting(&self) -> bool {
        let mut unsent_at = self.unsent_at.lock().expect("unsent mark lock poisoned");
        match *unsent_at {
            Some((before, _)) if self.prompts_sent() <= before => true,
            Some(_) => {
                *unsent_at = None;
                false
            }
            None => false,
        }
    }

    fn retry_unsent_submit(
        &self,
        permits_submit: impl FnOnce() -> bool,
    ) -> Result<bool, ptyio::PartialWrite> {
        let input = self.pty.lock_input();
        if self.is_frozen()
            || self.poll_exit()
            || self.trust.holds_writes()
            || self.look_for_dialog().current.is_some()
            || self.input_swap_active()
            || self.needs_user_action()
            || !permits_submit()
        {
            return Ok(false);
        }
        let mut unsent = self.unsent_at.lock().expect("unsent mark lock poisoned");
        let Some((before, started)) = *unsent else {
            return Ok(false);
        };
        if self.prompts_sent() > before {
            *unsent = None;
            return Ok(false);
        }
        if started.elapsed() < crate::outbox::DRAFT_WAIT {
            return Ok(false);
        }
        // The payload was already written. Retry only its submit key, once, rather than appending
        // it a second time. New user text in the composer may be included under delivery priority.
        *unsent = None;
        drop(unsent);
        self.native_stash.store(false, Ordering::Release);
        self.input_writes.fetch_add(1, Ordering::AcqRel);
        self.pty.write_locked(&input, b"\r")?;
        Ok(true)
    }

    pub(crate) fn schedule_unsent_submit_retry(&self) {
        let due = self
            .unsent_at
            .lock()
            .expect("unsent mark lock poisoned")
            .is_some_and(|(_, started)| started.elapsed() >= crate::outbox::DRAFT_WAIT);
        if !due || self.unsent_retry_scheduled.swap(true, Ordering::AcqRel) {
            return;
        }
        let observer = self.title.lock().expect("title observer lock poisoned");
        let state = observer.state.upgrade();
        let runtime = observer.runtime.clone();
        drop(observer);
        let (Some(state), Some(runtime)) = (state, runtime) else {
            self.unsent_retry_scheduled.store(false, Ordering::Release);
            return;
        };
        let id = self.id.clone();
        let launch = self.launch_id.clone();
        runtime.spawn_blocking(move || {
            let Some(live) = state.live_session(&id).filter(|live| live.launch_id == launch) else { return; };
            if let Err(error) = live.retry_unsent_submit(|| state.permits_automatic_input(&id)) {
                state.broadcast(crate::protocol::Event::from_error(None, &anyhow::anyhow!(
                    "Session {id} could not retry submitting its pending message: {error}. Check its input before continuing."
                )));
            }
            live.unsent_retry_scheduled.store(false, Ordering::Release);
            state.retry_pending_outbox(&id);
        });
    }

    /// Briefly holds later deliveries after native acceptance became unknown, without touching the editor.
    pub fn mark_submission_uncertain(&self, before: u64) {
        *self
            .uncertain_at
            .lock()
            .expect("uncertain submission lock poisoned") = Some((before, Instant::now()));
    }

    pub fn submission_uncertain(&self) -> bool {
        let mut at = self
            .uncertain_at
            .lock()
            .expect("uncertain submission lock poisoned");
        match *at {
            Some((before, started))
                if self.prompts_sent() <= before
                    && started.elapsed() < crate::outbox::DRAFT_WAIT =>
            {
                true
            }
            Some(_) => {
                *at = None;
                false
            }
            None => false,
        }
    }

    /// Asks the agent to exit, escalating to a kill only if it does not, with its screen frozen
    /// first so that what it draws on the way out does not replace it. **Blocks**; see
    /// [`PtyCore::stop`].
    pub fn terminate(&self) {
        self.note_input_recovery_ended();
        if self.pty.poll_exit() {
            return;
        }
        self.pty.freeze_output();
        self.pty.stop(AGENT_STOP_SIGNAL);
    }

    /// [`Self::terminate`] for an operation that ends the process on purpose (an archive, an
    /// account switch, a change of sharing), which records that it did: whether it found the
    /// process still running and stopped it. A process that had exited by itself before is not the
    /// operation's end, whatever the operation did next. **Blocks**.
    pub fn end(&self) -> bool {
        if self.pty.poll_exit() {
            return false;
        }
        self.ended_on_purpose.store(true, Ordering::Release);
        self.terminate();
        true
    }

    /// Marks the process as watched by the operation that launched it under its claim, until that
    /// operation calls [`Self::release_watch`].
    pub fn watch_under_claim(&self) {
        self.watched_under_claim.store(true, Ordering::Release);
    }

    /// The operation that watched the process has seen it come up: its end from here on is the
    /// agent's own again.
    pub fn release_watch(&self) {
        self.watched_under_claim.store(false, Ordering::Release);
    }

    /// Whether an operation holding the session's claim is watching this process come up.
    pub fn is_watched_under_claim(&self) -> bool {
        self.watched_under_claim.load(Ordering::Acquire)
    }

    /// Whether [`Self::end`] stopped this process.
    pub fn was_ended_on_purpose(&self) -> bool {
        self.ended_on_purpose.load(Ordering::Acquire)
    }

    /// Notes that the exit watcher is done with this process's end.
    pub fn settle_exit(&self) {
        self.exit_settled.store(true, Ordering::Release);
    }

    /// Whether the exit watcher is done with this process's end, so that what it would send is
    /// sent: what a test waits for before asserting an owner was not told.
    #[cfg(test)]
    pub fn is_exit_settled(&self) -> bool {
        self.exit_settled.load(Ordering::Acquire)
    }
}

/// Whether a chunk written to a session's input is nothing but a terminal's own protocol traffic:
/// focus in/out (`CSI I`, `CSI O`), device attributes (`CSI … c`), cursor position (`CSI … R`),
/// mode reports (`CSI … $ y`), window reports (`CSI … t`), status reports (`CSI … n`), mouse
/// movement, wheel and button-release reports (`CSI < b;x;y M` with the motion bit, 32, or the
/// wheel bit, 64, set in `b`, and `CSI < … m`; the older `CSI M` with three bytes alike), and
/// operating system or device control string replies. Conservative: an empty chunk, any other
/// byte, any other final byte — the arrow keys included — a mouse button press, and a sequence cut
/// short all say no.
///
/// A pointer passing or scrolling over the terminal is not the person typing. Where mouse
/// reporting is off the terminal turns the wheel into arrow keys, which say no.
pub(crate) fn is_terminal_protocol(chunk: &[u8]) -> bool {
    if chunk.is_empty() {
        return false;
    }
    let mut at = 0;
    while at < chunk.len() {
        if chunk[at] != 0x1b {
            return false;
        }
        match chunk.get(at + 1) {
            // The older mouse report: `M` and three bytes of button and position.
            Some(b'[') if chunk.get(at + 2) == Some(&b'M') => {
                if chunk.len() < at + 6 || chunk[at + 3..at + 6].iter().any(|byte| *byte < 0x20) {
                    return false;
                }
                // A release is button 3; movement has the motion bit, the wheel its own bit.
                let button = chunk[at + 3] - 0x20;
                if button & (32 | 64) == 0 && button & 3 != 3 {
                    return false;
                }
                at += 6;
            }
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
                    b'm' => chunk.get(start) == Some(&b'<'),
                    b'M' => {
                        chunk.get(start) == Some(&b'<')
                            && std::str::from_utf8(&chunk[start + 1..at])
                                .ok()
                                .and_then(|params| params.split(';').next()?.parse::<u32>().ok())
                                .is_some_and(|button| button & (32 | 64) != 0)
                    }
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

#[cfg(test)]
mod tests {
    use super::is_terminal_protocol;

    #[test]
    fn a_submit_retry_waits_for_a_dialog_without_resetting_its_deadline() {
        let live = crate::test_support::fake_live_session(
            "unsent-dialog",
            crate::protocol::Agent::Claude,
            120,
            40,
            "sleep 30",
        );
        live.mark_unsent(0);
        *live.unsent_at.lock().unwrap() =
            Some((0, std::time::Instant::now() - crate::outbox::DRAFT_WAIT));
        live.on_output(
            b"\x1b[2J\x1b[1;1HResume this conversation?\r\nEnter to confirm - Esc to dismiss",
        );
        let before = live.input_writes();
        assert!(!live.retry_unsent_submit(|| true).unwrap());
        assert!(live.unsent_waiting());
        assert_eq!(live.input_writes(), before);
        live.on_output(b"\x1b[2J\x1b[1;1H");
        assert!(live.retry_unsent_submit(|| true).unwrap());
        assert!(!live.unsent_waiting());
    }

    #[test]
    fn an_unconfirmed_submit_gets_one_bounded_enter_retry_without_repeating_text() {
        let live = crate::test_support::fake_live_session(
            "unsent-bounded",
            crate::protocol::Agent::Claude,
            120,
            40,
            "sleep 30",
        );
        live.mark_unsent(0);
        assert!(!live.retry_unsent_submit(|| true).unwrap());
        *live.unsent_at.lock().unwrap() =
            Some((0, std::time::Instant::now() - crate::outbox::DRAFT_WAIT));
        let before = live.input_writes();
        assert!(!live.retry_unsent_submit(|| false).unwrap());
        assert!(live.unsent_waiting());
        assert_eq!(live.input_writes(), before);
        assert!(live.retry_unsent_submit(|| true).unwrap());
        assert_eq!(live.input_writes(), before + 1);
        assert!(!live.unsent_waiting());
        assert!(!live.retry_unsent_submit(|| true).unwrap());
        assert_eq!(live.input_writes(), before + 1);

        live.mark_unsent(0);
        *live.unsent_at.lock().unwrap() =
            Some((0, std::time::Instant::now() - crate::outbox::DRAFT_WAIT));
        live.note_prompt_sent(None);
        assert!(!live.retry_unsent_submit(|| true).unwrap());
        assert_eq!(live.input_writes(), before + 1);
    }

    #[test]
    fn uncertain_native_acceptance_cannot_hold_later_messages_forever() {
        let live = crate::test_support::fake_live_session(
            "uncertain-bounded",
            crate::protocol::Agent::Codex,
            120,
            40,
            "sleep 30",
        );
        live.mark_submission_uncertain(0);
        assert!(live.submission_uncertain());
        assert!(!live.unsent_waiting());
        *live.uncertain_at.lock().unwrap() =
            Some((0, std::time::Instant::now() - crate::outbox::DRAFT_WAIT));
        assert!(!live.submission_uncertain());
        assert_eq!(*live.uncertain_at.lock().unwrap(), None);
    }

    #[test]
    fn a_stale_empty_screen_cannot_send_keys_that_have_not_been_rendered() {
        use crate::protocol::Agent;
        let live = crate::test_support::fake_live_session(
            "draft-gate",
            Agent::Claude,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.on_output(include_bytes!("../testdata/claude_input_placeholder.bin"));
        assert_eq!(
            crate::interrupt::input_box::input_box(Agent::Claude, &live.screen_text_undimmed())
                .as_deref(),
            Some("")
        );
        live.write_terminal_input(b"unrendered draft").unwrap();
        let before = live.input_writes();
        let error = live
            .write_message_input(b"must not be written", "program", false)
            .unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(live.input_writes(), before);
        live.note_prompt_sent(None);
        assert!(
            live.write_message_input(b"still held", "program", false)
                .is_err(),
            "a hook without a user submission candidate does not clear edits"
        );
        live.write_terminal_input(b"\rnext").unwrap();
        live.note_prompt_sent(Some("unrendered draft"));
        assert!(
            live.write_message_input(b"still held", "program", false)
                .is_err(),
            "a hook for Enter cannot clear later input"
        );
        live.write_terminal_input(b"\r").unwrap();
        live.note_prompt_sent(Some("next"));
        live.write_message_input(b"now empty and acknowledged", "program", false)
            .unwrap();
    }

    #[test]
    fn a_known_draft_or_unknown_composer_prevents_a_message_for_all_three_agents() {
        use crate::protocol::Agent;
        for (agent, draft) in [
            (
                Agent::Claude,
                &include_bytes!("../testdata/claude_input_restored.bin")[..],
            ),
            (
                Agent::Grok,
                &include_bytes!("../testdata/grok_composer_draft.bin")[..],
            ),
            (
                Agent::Codex,
                &include_bytes!("../testdata/codex_blank_first_draft.bin")[..],
            ),
        ] {
            let live =
                crate::test_support::fake_live_session("composer-gate", agent, 140, 45, "sleep 30")
                    .with_composer();
            assert!(live
                .write_message_input(b"unknown", "unknown", false)
                .is_err());
            live.on_output(draft);
            let before = live.input_writes();
            assert!(live.write_message_input(b"draft", "draft", false).is_err());
            assert_eq!(live.input_writes(), before);
        }
    }

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
            b"\x1b[<35;12;7M",
            b"\x1b[<0;12;7m",
            b"\x1b[M#,'",
            b"\x1b[MC,'",
            b"\x1b[<64;12;7M",
            b"\x1b[Ma,'",
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
            b"\x1b[35;12;7M",
            b"\x1b[M#",
            b"\x1b[<35;12;7",
            b"\x1b[<0;12;7M",
            b"\x1b[<0;12;7M\x1b[<0;12;7m",
            b"\x1b[M ,'",
        ] {
            assert!(!is_terminal_protocol(input), "{input:?}");
        }
    }
}
