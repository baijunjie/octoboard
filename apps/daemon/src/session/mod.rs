//! One running agent process: its PTY (`crate::terminal::pty`), and what is the agent's own on top
//! of it — the agent, its title, its folder-trust watch and the count of writes into its input.
//!
//! The submodules are the parts of that: `term/` launches the agent in a PTY and writes messages
//! into it, `interrupt/` interrupts its turn, `title_activity` reads what its terminal title, or
//! the status the agent reports, says about its turns and `title_interrupt` a cancel that left no
//! other trace off that reading, `dialogs` finds the dialogs only a person can answer, `trust/`
//! presses the folder-trust confirmation, `input_box` reads what the agent's input box holds off
//! its screen, `draft` mirrors the input to match the prompts that were submitted, `output_pace`
//! tells whether its parsed screen shows a whole frame, `swap` handles the native composer stashes
//! that keep editor state inside the CLI while a message uses the input, and `paste_apart`
//! submits a message whose Enter has to wait until the agent has taken its paste, and
//! `usage_limit` holds where the process stands with the hold on Octoboard's writes
//! after a turn stopped on its account's usage limit.

pub(crate) mod dialogs;
mod draft;
pub(crate) mod input_box;
pub(crate) mod interrupt;
mod output_pace;
mod paste_apart;
mod program_status;
pub(crate) mod screen;
mod swap;
pub(crate) mod term;
pub(crate) mod title_activity;
pub(crate) mod title_interrupt;
pub(crate) mod trust;
pub(crate) mod usage_limit;
pub(crate) mod wait;

#[cfg(test)]
mod rendered_tests;

use draft::Draft;
pub(crate) use paste_apart::NotSubmitted;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
pub(crate) use swap::{HeldFragment, RecoveryPhase};

use crate::platform::proc::Polite;
use crate::platform::term::{Input, PartialWrite};
use crate::protocol::AgentId;
use crate::session::trust::{TrustCarrier, TrustState};
use crate::terminal::input_class::is_terminal_protocol;
use crate::terminal::pty::{NewPty, PtyCore, PtyOwner};
use crate::terminal::title::names_an_executable;
use dialogs::{Dialog, Sighting};
use output_pace::OutputPace;
use title_activity::TitleActivity;

pub use crate::terminal::pty::spawn_reader_thread;

/// How long an agent's error holds its session's writes ([`LiveSession::agent_error`]). Every flush
/// looks at the screen afresh, so this covers what a look can miss: the PTY reader getting the
/// modal's bytes into the parsed screen (`on_output` can block behind a slow client), and a version
/// that draws the modal slightly after its hooks.
pub const AGENT_ERROR_SETTLE: Duration = Duration::from_secs(2);

#[derive(Default)]
struct TitleObserver {
    parser: crate::terminal::title::Parser,
    latest: Option<Option<String>>,
    /// What the agent's titles, or the status it reports, have said about its turns; see
    /// `title_activity`.
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
                        crate::messaging::owner_notices::tell_owner_it_waits(&state, &id);
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
    pub agent: AgentId,
    /// The launch this process is, as its processes are tagged with (`crate::processes::ledger`).
    pub launch_id: String,
    pty: PtyCore,
    pub(crate) transport: Option<Arc<dyn term::MessageTransport>>,
    pub(crate) checks_composer: bool,
    title: Mutex<TitleObserver>,
    /// Whether the session's terminal is read as a host that renders what the program draws has it
    /// read ([`Self::host_renders`]).
    host_renders: AtomicBool,
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
    /// other writer can fall between. See [`Self::write_raw_input`].
    input_writes: AtomicU64,
    draft: Mutex<Draft>,
    swap: Mutex<swap::InputSwap>,
    native_stash: AtomicBool,
    recovery_scheduled: AtomicBool,
    /// How many prompts the agent's hooks have reported it sending (`UserPromptSubmit`), counted
    /// once the event's status has been applied. What the outbox waits on to know that a message
    /// it wrote was sent rather than left in the input box.
    prompts_sent: AtomicU64,
    /// The acknowledgement count and grace-period start of an unconfirmed message, and whether its
    /// submit key is still to be retried. A prompt acknowledgement or one bounded submit-key retry
    /// releases its hold for this process; a message left in its input box, its submit key pressed
    /// or held back by the write ([`NotSubmitted`]), is released by an acknowledgement alone.
    unsent_at: Mutex<Option<(u64, Instant, bool)>>,
    unsent_retry_scheduled: AtomicBool,
    /// Set while a [`Self::schedule_draft_release`] task is under way.
    draft_release_scheduled: AtomicBool,
    /// The output total and the count of typed bytes at the latest reading of a whole frame that
    /// left the hold in place: until either moves, another reading would read the same.
    draft_release_tried: Mutex<Option<(u64, u64)>>,
    /// When the agent last printed something, and whether a synchronized update is open: whether
    /// the parsed screen shows a whole frame ([`OutputPace::whole_frame`]).
    output_pace: Mutex<OutputPace>,
    /// The acknowledgement count of the unconfirmed message the user was told was not sent, until a
    /// prompt reported after it withdraws that ([`Self::note_prompt_sent`]). Taken before
    /// `unsent_at` where both are.
    unsent_notice: Mutex<Option<u64>>,
    /// The acknowledgement count and the time at which a message the session's own server may
    /// have taken was not confirmed taken (`term::UncertainSubmission`). It holds later messages
    /// for [`crate::messaging::outbox::DRAFT_WAIT`], or until a prompt is reported after it.
    uncertain_at: Mutex<Option<(u64, Instant)>>,
    /// When the agent stopped on an error. It holds the session's writes for [`AGENT_ERROR_SETTLE`]
    /// after that, or until the next prompt is sent; gone with the process.
    agent_error_at: Mutex<Option<Instant>>,
    /// The hold on Octoboard's writes after a turn stopped on the account's usage limit; gone with
    /// the process.
    pub(crate) usage_limit: usage_limit::UsageLimitHold,
    /// What the screen last showed of the dialogs only a person can answer; see
    /// [`crate::session::dialogs`]. Read and updated under this lock together, so a look that read
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
    /// Set by the exit watcher (`crate::coordinator::exit_watch::watch_exit`) once it is done with
    /// this process's end: the end is recorded and the owner, if it was to be told, has been sent
    /// the notice (written to it or queued for it) or could not be.
    exit_settled: AtomicBool,
    /// The agent's hooks and signals give no status, so the program's own reports (OSC 7501) do;
    /// see [`program_status`].
    status_from_reports: bool,
    /// The moves of the program's root status record on their way to the session's status.
    status_reports: Arc<program_status::StatusReports>,
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
    pub agent: AgentId,
    /// See `LiveSession::launch_id`.
    pub launch_id: String,
    pub spawned: crate::terminal::backend::Started,
    pub transport: Option<Arc<dyn term::MessageTransport>>,
    /// Real agents have a composer; stand-in PTYs can explicitly opt out of its screen check.
    pub checks_composer: bool,
    /// Per-session scratch directory (Grok's `GROK_HOME` symlink farm), removed when the session's
    /// process is gone.
    pub scratch_dir: Option<std::path::PathBuf>,
    pub resolves_approvals_itself: bool,
    /// Grok's: where the trust it records when its confirmation is pressed is carried from and to;
    /// see `adapter::LaunchPlan::carried_trust`.
    pub carried_trust: Option<Arc<dyn TrustCarrier>>,
    /// Runs after the process is reaped and before its scratch directory is removed.
    pub before_scratch_removal: Option<Box<dyn FnOnce() + Send>>,
    /// See `NewPty::display`.
    pub display: crate::terminal::display::DisplaySeed,
}

impl LiveSession {
    pub fn new(session: NewSession) -> Self {
        let pty = PtyCore::new(NewPty {
            label: session.id.clone(),
            spawned: session.spawned,
            scratch_dir: session.scratch_dir,
            kill_session_leftovers: true,
            before_scratch_removal: session.before_scratch_removal,
            display: session.display,
        });
        // Every session's terminal supports the protocol: it keeps the reports and answers their
        // support query. Only an agent with no status of its own takes its status from them.
        pty.keep_program_status();
        let status_from_reports = !crate::adapter::provider(session.agent).has_status_integration();
        let host_renders = crate::platform::term::facts().host_renders;
        if host_renders {
            pty.report_titles();
        }
        // Where a paste reaches the agent as keys, a query its host holds it up on delays its
        // taking the paste in, which is when a message's submit key waits (`paste_apart`).
        if crate::platform::term::facts().paste_as_keys {
            pty.log_queries();
        }
        Self {
            pty,
            transport: session.transport,
            checks_composer: session.checks_composer,
            id: session.id,
            agent: session.agent,
            launch_id: session.launch_id,
            title: Mutex::new(TitleObserver::default()),
            host_renders: AtomicBool::new(host_renders),
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
            draft_release_scheduled: AtomicBool::new(false),
            draft_release_tried: Mutex::new(None),
            output_pace: Mutex::new(OutputPace::default()),
            unsent_notice: Mutex::new(None),
            uncertain_at: Mutex::new(None),
            agent_error_at: Mutex::new(None),
            usage_limit: Default::default(),
            dialogs: Mutex::new(Sighting::default()),
            screen_watchers: AtomicU32::new(0),
            ended_on_purpose: AtomicBool::new(false),
            watched_under_claim: AtomicBool::new(false),
            exit_settled: AtomicBool::new(false),
            status_from_reports,
            status_reports: Arc::default(),
        }
    }

    /// Whether the session's terminal is read as a host that renders what the program draws has it
    /// read (`TermFacts::host_renders`): the output is the host's repaint, so the titles are the
    /// parsed screen's title changes rather than what the output's own title sequences say, and an
    /// input box that shows the provider's placeholder text is empty whatever the attributes it is
    /// drawn with. The trust screen and the agent's readiness follow the same fact
    /// ([`TrustState::reads_screen`]).
    pub(crate) fn host_renders(&self) -> bool {
        self.host_renders.load(Ordering::Relaxed)
    }

    /// Reads the session's terminal from now on as a host that renders what the program draws has
    /// it read ([`Self::host_renders`], [`TrustState::read_the_screen`]), for a test of that
    /// reading on a host that does not. Called before any output, as a host's own reading starts.
    #[cfg(test)]
    pub(crate) fn read_as_rendered(&self) {
        self.host_renders.store(true, Ordering::Relaxed);
        self.pty.report_titles();
        self.trust.read_the_screen();
    }

    /// Output may already have arrived before the launch is registered. Bind and publish the
    /// retained observation under the same lock as feeding, so an older title cannot win later.
    pub(crate) fn observe_titles(&self, state: &Arc<crate::state::AppState>) {
        let mut observer = self.title.lock().expect("title observer lock poisoned");
        observer.state = Arc::downgrade(state);
        observer.runtime = tokio::runtime::Handle::try_current().ok();
        observer.publish(self);
        drop(observer);
        // Reports the program made before then are applied now.
        self.apply_program_status();
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

    /// Moves the latest key and the latest output back as far as the empty-box release and the
    /// writes past the draft's grace wait on them: as if the agent had been still since well
    /// after it drew what the key did.
    #[cfg(test)]
    pub(crate) fn settle_input(&self) {
        self.draft
            .lock()
            .expect("draft lock poisoned")
            .settle_typing();
        self.output_pace
            .lock()
            .expect("output pace lock poisoned")
            .quieten();
    }

    /// Whether the agent's hooks last reported a turn running, as the session's status reads; not
    /// for a session bound to no state.
    fn hooks_say_working(&self) -> bool {
        let state = self
            .title
            .lock()
            .expect("title observer lock poisoned")
            .state
            .clone();
        state.upgrade().is_some_and(|state| {
            state.session_record(&self.id).is_ok_and(|record| {
                state.agent_status(&record) == crate::protocol::SessionStatus::Working
            })
        })
    }

    /// What the agent's terminal title, or the status it reports, has said about its turns so far.
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
        *shown = shown.after(dialogs::recognise(self.agent, &self.pty.screen_text()));
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
        self.waits_for_user_unless(|_| false)
    }

    /// [`Self::waits_for_user`], except that a usage-limit dialog on the screen (see
    /// `Dialog::is_usage_limit`) or a view of the background agents is not a wait that keeps the
    /// session from being switched.
    pub(crate) fn waits_for_user_beyond_usage_limit(&self) -> bool {
        self.waits_for_user_unless(|dialog| dialog.is_usage_limit() || dialog.is_agents_view())
    }

    /// [`Self::waits_for_user`] as the status shown judges it: a view of the background agents is
    /// no wait for the user there, since the session is shown dormant for it instead.
    pub(crate) fn waits_for_user_outside_agents_view(&self) -> bool {
        self.waits_for_user_unless(Dialog::is_agents_view)
    }

    /// [`Self::waits_for_user`] with the dialogs `not_a_wait` picks out left out of it.
    fn waits_for_user_unless(&self, not_a_wait: impl Fn(&Dialog) -> bool) -> bool {
        self.trust.needs_user()
            || self.needs_user_action()
            || self
                .agent_dialog()
                .is_some_and(|dialog| !not_a_wait(&dialog))
    }

    /// Whether the screen shows one of Claude Code's views of its background agents (see
    /// `Dialog::is_agents_view`).
    pub(crate) fn shows_agents_view(&self) -> bool {
        self.agent_dialog()
            .is_some_and(|dialog| dialog.is_agents_view())
    }

    pub(crate) fn needs_user_action(&self) -> bool {
        self.action_required.load(Ordering::Acquire)
    }

    pub(crate) fn observe_action_title(&self, title: Option<&str>) -> bool {
        let screen = crate::adapter::provider(self.agent).screen();
        let waiting = title.is_some_and(|title| screen.title_needs_user(title));
        self.action_required.swap(waiting, Ordering::AcqRel) != waiting
    }

    /// Called from the PTY reader thread for every chunk read; see [`PtyCore::push_output`].
    pub fn on_output(&self, data: &[u8]) {
        // Where the host renders, the title the chunk completes is known once the parsed screen
        // has taken it in, which is still before any client is handed the chunk.
        let rendered = self.host_renders();
        let mut titled = !rendered && self.observe_title(|parser| parser.feed(data));
        // Before the screen takes the chunk in, so that a reading which finds the chunk on the
        // screen also finds it noted (`Self::composer_for_draft`).
        self.output_pace
            .lock()
            .expect("output pace lock poisoned")
            .note(data, Instant::now());
        self.pty.push_output(data, |total, mut titles| {
            if rendered {
                // The host's own title naming the agent's executable is not the agent's.
                titles.retain(|title| !title.as_deref().is_some_and(names_an_executable));
                titled = self.observe_title(|parser| parser.take(&titles));
            }
            // The count without its lock: a writer holds that lock while the agent may be waiting
            // for this very thread to drain its output.
            self.trust.on_output(
                data,
                || self.pty.screen_text(),
                self.input_writes.load(Ordering::Acquire),
                total,
            );
            // Run outside the output's lock, as a look takes that after the sighting's.
            if self.screen_watchers.load(Ordering::Acquire) > 0 {
                self.look_for_dialog();
            }
            if self.trust.follows_paste_mode() {
                self.trust.note_paste_mode(self.pty.bracketed_paste_on());
            }
            self.look_for_input_box();
        });
        if titled {
            self.trust.note_title();
        }
        self.note_program_status();
        self.schedule_input_recovery();
    }

    /// The rows of the agent's input box as `screen` (the visible rows, faint cells blanked) shows
    /// them ([`input_box::input_box_rows`]); where the host renders what the program draws
    /// ([`Self::host_renders`]), a box that shows only the provider's placeholder is an empty one
    /// whatever it is drawn with ([`input_box::placeholder_as_empty`]).
    pub(crate) fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        let rows = input_box::input_box_rows(self.agent, screen)?;
        Some(if self.host_renders() {
            input_box::placeholder_as_empty(self.agent, rows)
        } else {
            rows
        })
    }

    /// The text in the agent's input box as `screen` shows it, read as [`Self::input_box_rows`]
    /// reads its rows: empty for an empty box, `None` where no input box is recognised.
    pub(crate) fn input_box(&self, screen: &str) -> Option<String> {
        self.input_box_rows(screen).map(input_box::text_of)
    }

    /// Looks at the parsed screen for the agent's input box while the trust watch asks for it
    /// ([`TrustState::watches_input_box`]): where the host renders what the program draws, the box
    /// drawn after the agent's `SessionStart` hook says that it reads its input.
    pub(crate) fn look_for_input_box(&self) {
        if self.trust.watches_input_box() {
            let shown = input_box::input_box_rows(self.agent, &self.pty.screen_text()).is_some();
            self.trust.note_input_box(shown);
        }
    }

    /// Takes the title `read` completes, if any, as the agent's latest: what it says about its
    /// turns, and, when it changed, the session's terminal title. Says whether one was completed.
    fn observe_title(
        &self,
        read: impl FnOnce(&mut crate::terminal::title::Parser) -> Option<Option<String>>,
    ) -> bool {
        let mut observer = self.title.lock().expect("title observer lock poisoned");
        let completed = read(&mut observer.parser);
        if let Some(title) = &completed {
            let glyph = observer.parser.leading_glyph();
            // A title of nothing but a spinner reaches here empty, and still shows a turn.
            let empty = title.is_none() && glyph.is_none();
            let shown = title_activity::shown(self.agent, glyph, empty);
            observer.activity.note(shown, Instant::now());
        }
        if let Some(title) = &completed {
            // A title that only spun since the last one changes nothing, so it does not reach the
            // store's lock.
            let changed = observer.latest.as_ref() != Some(title);
            observer.latest = Some(title.clone());
            if changed {
                observer.publish(self);
            }
        }
        completed.is_some()
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
    #[cfg(test)]
    pub fn write_input(&self, data: &[u8]) -> Result<(), PartialWrite> {
        self.write_raw_input(data, false)
    }

    pub(crate) fn write_terminal_input(&self, data: &[u8]) -> Result<(), PartialWrite> {
        if self.buffer_terminal_input(data)? {
            return Ok(());
        }
        self.write_raw_input(data, true)
    }

    pub(crate) fn write_terminal_reply(&self, data: &[u8]) -> Result<(), PartialWrite> {
        let input = self.pty.lock_input();
        if self.pty.is_frozen() {
            return Err(swap::archived_refusal());
        }
        self.pty.write_reply_locked(&input, data)
    }

    fn write_raw_input(&self, data: &[u8], user: bool) -> Result<(), PartialWrite> {
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
            return Err(swap::archived_refusal());
        }
        if !is_terminal_protocol(data) {
            self.input_writes.fetch_add(1, Ordering::AcqRel);
            if track_draft {
                // An Esc on its own is the one key that reading as typed at the prompt changes
                // anything for, and it needs the hooks' status too: right after a submit the
                // title can still show the prompt.
                let mid_turn = self.title_activity().last_said_running()
                    || (data == b"\x1b" && self.hooks_say_working());
                let mut draft = self.draft.lock().expect("draft lock poisoned");
                draft.typed(data, self.output_total(), Instant::now());
                if mid_turn {
                    draft.typed_mid_turn();
                }
            }
        }
        self.pty.write_locked(&input, data)
    }

    /// Checks the composer and the unacknowledged user input under the same lock as the write.
    /// A rendered empty box acknowledges that input only off a screen that can no longer predate
    /// it ([`Self::composer_for_draft`]); keys still waiting in the child's input are not.
    /// An expired protection budget bypasses editor checks but never dialogs or process lifetime.
    fn write_empty_message_input(
        &self,
        message: &[Input],
        prompt: &str,
        allow_mixed: bool,
    ) -> Result<(), PartialWrite> {
        let input = self.pty.lock_input();
        if self.pty.is_frozen() {
            return Err(swap::archived_refusal());
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
            && (self.stash_shown() || {
                let (composer, _) = self.composer_for_draft(&mut draft);
                draft.holds() || composer.as_deref() != Some("")
            })
        {
            return Err(PartialWrite {
                written: 0,
                error: io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "the session's composer is not known to be empty",
                ),
            });
        }
        // Past the draft's grace a message may join what the box holds, but only text that shows.
        // It waits, the watch's retry coming back to it, while the box cannot be read after a key
        // that may have brought back text the mirror cannot know, such as a prompt from history;
        // while the latest key may have put text in the box that is not drawn yet; and while a box
        // read empty comes from a frame read in part. Once the text shows, the stash takes it where
        // the agent has one (`Self::write_message_input`).
        if self.checks_composer && allow_mixed {
            let now = Instant::now();
            let output = self.output_total();
            let whole = self.shows_whole_frame(now);
            let composer = self.input_box(&self.screen_text_undimmed());
            let whole = whole && self.output_total() == output;
            let refusal = if composer.is_none() && draft.may_hold_unknown_text() {
                Some("the session's input box cannot be read after keys that may have put text in it")
            } else if draft.text_may_be_undrawn(now) {
                Some("keys just typed into the session may not be drawn in its input box yet")
            } else if composer.as_deref() == Some("") && !whole {
                Some("the session's input box was read empty off a frame not yet drawn whole")
            } else {
                None
            };
            if let Some(refusal) = refusal {
                return Err(swap::blocked(refusal));
            }
        }
        let (now, rest) = paste_apart::split(message, self.submits_paste_apart());
        draft.submission(None, prompt.to_string());
        self.input_writes.fetch_add(1, Ordering::AcqRel);
        let written = self.pty.write_input_locked(&input, now);
        if written.is_ok() {
            draft.message_written(Instant::now());
            self.note_message_written();
        }
        drop(draft);
        drop(input);
        written?;
        // Only a box found empty above has to show the message alone.
        self.submit_rest(now, rest, self.checks_composer && !allow_mixed)
    }

    /// Whether the parsed screen can be taken to show a whole frame at `now`
    /// ([`OutputPace::whole_frame`]).
    fn shows_whole_frame(&self, now: Instant) -> bool {
        self.output_pace
            .lock()
            .expect("output pace lock poisoned")
            .whole_frame(now)
    }

    /// The text in the agent's input box as the screen shows it now, read with the input lock
    /// held and `draft` locked, and the output total the screen was read at where it showed a
    /// whole frame. A box read empty off a whole frame that can no longer predate the keys typed
    /// last ends the hold they keep on Octoboard's messages ([`Draft::shown_empty`]): the user
    /// typed something that left it empty — an Esc, text cleared again, a prompt sent that no hook
    /// matched — and with the lock held no key can come in between. A frame read in part can show
    /// the box cleared and not yet drawn again, so it ends nothing.
    fn composer_for_draft(&self, draft: &mut Draft) -> (Option<String>, Option<u64>) {
        // The total first, the pace next: a chunk counted in the total has been noted in the pace
        // already, and one taken in after the total was read changes the total read again below.
        let output = self.output_total();
        let now = Instant::now();
        let whole = self.shows_whole_frame(now);
        let composer = self.input_box(&self.screen_text_undimmed());
        let whole = (whole && self.output_total() == output).then_some(output);
        if composer.as_deref() == Some("") && whole.is_some() {
            draft.shown_empty(output, now);
        }
        (composer, whole)
    }

    /// Ends the hold the keys typed into the session keep on Octoboard's messages where the
    /// agent's input box shows that nothing typed is left in it ([`Self::composer_for_draft`]),
    /// and says whether it did. **Blocks** on the input lock, which a write holds while the agent
    /// is slow to read.
    pub(crate) fn release_draft_hold(&self) -> bool {
        if !self.checks_composer {
            return false;
        }
        let _input = self.pty.lock_input();
        let mut draft = self.draft.lock().expect("draft lock poisoned");
        if !draft.holds() || self.input_swap_active() || self.stash_shown() {
            return false;
        }
        let (_, whole) = self.composer_for_draft(&mut draft);
        let released = !draft.holds();
        if let (false, Some(output)) = (released, whole) {
            *self
                .draft_release_tried
                .lock()
                .expect("draft release lock poisoned") = Some((output, draft.edits()));
        }
        released
    }

    /// Runs [`Self::release_draft_hold`] off the runtime once the keys typed last can have been
    /// drawn ([`Draft::settled`]) and the screen shows a whole frame, and writes the session's
    /// queue out when it ends the hold: a message held for what the user typed goes in once the
    /// box is drawn empty, as no hook comes for keys that sent nothing. Not again while neither the
    /// output nor the keys have moved since a whole frame left the hold in place. For the
    /// session's watch, while something is queued.
    pub(crate) fn schedule_draft_release(&self) {
        let output = self.output_total();
        let now = Instant::now();
        let whole = self.shows_whole_frame(now);
        let tried = *self
            .draft_release_tried
            .lock()
            .expect("draft release lock poisoned");
        // Without waiting: a write holds the draft's lock while the agent is slow to read.
        let due = whole
            && self.draft.try_lock().is_ok_and(|draft| {
                draft.holds()
                    && draft.settled(output, now)
                    && tried != Some((output, draft.edits()))
            });
        if !due || self.draft_release_scheduled.swap(true, Ordering::AcqRel) {
            return;
        }
        let observer = self.title.lock().expect("title observer lock poisoned");
        let state = observer.state.upgrade();
        let runtime = observer.runtime.clone();
        drop(observer);
        let (Some(state), Some(runtime)) = (state, runtime) else {
            self.draft_release_scheduled.store(false, Ordering::Release);
            return;
        };
        let (id, launch) = (self.id.clone(), self.launch_id.clone());
        runtime.spawn_blocking(move || {
            let Some(live) = state
                .live_session(&id)
                .filter(|live| live.launch_id == launch)
            else {
                return;
            };
            let released = live.release_draft_hold();
            live.draft_release_scheduled.store(false, Ordering::Release);
            if released {
                state.retry_pending_outbox(&id);
            }
        });
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
        keys: &[Input],
    ) -> Result<bool, PartialWrite> {
        let input = self.pty.lock_input();
        if self.input_swap_active() || self.input_writes.load(Ordering::Acquire) != expected {
            return Ok(false);
        }
        self.pty.write_input_locked(&input, keys).map(|()| true)
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
        keys: &[Input],
    ) -> Result<bool, PartialWrite> {
        let input = self.pty.lock_input();
        if self.pty.is_frozen() {
            return Err(swap::archived_refusal());
        }
        if self.input_swap_active()
            || self.input_writes.load(Ordering::Acquire) != expected
            || !gate()
        {
            return Ok(false);
        }
        self.input_writes.fetch_add(1, Ordering::AcqRel);
        self.pty.write_input_locked(&input, keys).map(|()| true)
    }

    /// How many writes anyone else has made into this session's input so far.
    pub fn input_writes(&self) -> u64 {
        // Under the lock, so that the count is never read in the middle of a write.
        let _input = self.pty.lock_input();
        self.input_writes.load(Ordering::Acquire)
    }

    /// Counts a prompt the agent's hooks reported it sending, which also ends the hold an agent
    /// error put on the session's writes, and, for a prompt submitted after a partly written
    /// message, the hold a takeover keeps on them. Returns whether the prompt confirms a message
    /// the user was told was not sent ([`Self::notice_unsent`]), whose notice is then withdrawn.
    pub fn note_prompt_sent(&self, prompt: Option<&str>) -> bool {
        let after_fragment = {
            let mut draft = self.draft.lock().expect("draft lock poisoned");
            draft.prompt_sent(prompt, self.agent);
            draft.sent_after_fragment(prompt, self.agent)
        };
        if after_fragment {
            self.end_input_takeover();
        }
        self.prompts_sent.fetch_add(1, Ordering::AcqRel);
        *self
            .agent_error_at
            .lock()
            .expect("agent error lock poisoned") = None;
        self.unsent_notice
            .lock()
            .expect("unsent notice lock poisoned")
            .take_if(|before| self.prompts_sent() > *before)
            .is_some()
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
    /// recognise ([`crate::session::dialogs`]): Grok Build draws its usage-limit modal before the
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
        *self.unsent_at.lock().expect("unsent mark lock poisoned") =
            Some((before, Instant::now(), true));
    }

    /// Records that a message written when the count stood at `before` was left in the input box,
    /// its submit key pressed or held back by the write ([`NotSubmitted`]): it is unsent as
    /// [`Self::mark_unsent`] records, but no retry presses the key once more, which could answer
    /// whatever has come up in the box's place.
    pub fn mark_not_submitted(&self, before: u64) {
        *self.unsent_at.lock().expect("unsent mark lock poisoned") =
            Some((before, Instant::now(), false));
    }

    /// Has `tell` tell the user that the message marked unsent was not sent, given whether its
    /// submit key is still to be retried, and records that it did, so that a prompt reported
    /// after the message withdraws that ([`Self::note_prompt_sent`]). Nothing when a prompt has
    /// been reported since. `tell` runs under the lock the withdrawal is decided under, so the
    /// notice goes out before its withdrawal can.
    pub fn notice_unsent(&self, tell: impl FnOnce(bool)) {
        let mut noticed = self
            .unsent_notice
            .lock()
            .expect("unsent notice lock poisoned");
        let unsent = *self.unsent_at.lock().expect("unsent mark lock poisoned");
        let Some((before, _, retry)) = unsent else {
            return;
        };
        if self.prompts_sent() > before {
            return;
        }
        *noticed = Some(before);
        tell(retry);
    }

    /// Whether a message marked unsent is still taken to be waiting in the input box: until the
    /// agent reports a prompt sent after it or the bounded submit-key retry runs.
    pub fn unsent_waiting(&self) -> bool {
        let mut unsent_at = self.unsent_at.lock().expect("unsent mark lock poisoned");
        match *unsent_at {
            Some((before, _, _)) if self.prompts_sent() <= before => true,
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
    ) -> Result<bool, PartialWrite> {
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
        let Some((before, started, retry)) = *unsent else {
            return Ok(false);
        };
        if self.prompts_sent() > before {
            *unsent = None;
            return Ok(false);
        }
        if !retry || started.elapsed() < crate::messaging::outbox::DRAFT_WAIT {
            return Ok(false);
        }
        // The payload was already written. Retry only its submit key, once, rather than appending
        // it a second time. New user text in the composer may be included under delivery priority.
        *unsent = None;
        drop(unsent);
        self.native_stash.store(false, Ordering::Release);
        self.input_writes.fetch_add(1, Ordering::AcqRel);
        self.pty.write_input_locked(&input, &[Input::Enter])?;
        Ok(true)
    }

    pub(crate) fn schedule_unsent_submit_retry(&self) {
        let due = self
            .unsent_at
            .lock()
            .expect("unsent mark lock poisoned")
            .is_some_and(|(_, started, retry)| {
                retry && started.elapsed() >= crate::messaging::outbox::DRAFT_WAIT
            });
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
                let detail = error.to_string();
                tracing::error!(session = %id, %detail, "submit-key retry failed");
                state.broadcast(
                    crate::protocol::Notice::new(
                        crate::protocol::notice_code::SUBMIT_RETRY_FAILED,
                        format!("Retrying the submit key of a pending message failed: {detail}. Check the session's input box before continuing."),
                        &[("agent", live.agent.label()), ("detail", &detail)],
                    )
                    .about(&id),
                );
            }
            live.unsent_retry_scheduled.store(false, Ordering::Release);
            state.retry_pending_outbox(&id);
        });
    }

    /// Holds later messages briefly after one, written when the acknowledgement count stood at
    /// `before`, may have been taken by the session's own server without that being confirmed;
    /// nothing is written into its input box. The queue is written out again once the hold has run
    /// its course, which nothing else watches for promptly.
    pub fn mark_submission_uncertain(&self, before: u64) {
        *self
            .uncertain_at
            .lock()
            .expect("uncertain submission lock poisoned") = Some((before, Instant::now()));
        let observer = self.title.lock().expect("title observer lock poisoned");
        let (state, runtime) = (observer.state.clone(), observer.runtime.clone());
        drop(observer);
        let Some(runtime) = runtime else {
            return;
        };
        let (id, launch) = (self.id.clone(), self.launch_id.clone());
        runtime.spawn(async move {
            tokio::time::sleep(crate::messaging::outbox::DRAFT_WAIT).await;
            if let Some(state) = state.upgrade() {
                if state
                    .live_session(&id)
                    .is_some_and(|live| live.launch_id == launch)
                {
                    state.retry_pending_outbox(&id);
                }
            }
        });
    }

    /// Whether a message marked by [`Self::mark_submission_uncertain`] still holds later ones: for
    /// [`crate::messaging::outbox::DRAFT_WAIT`] after it, unless a prompt was reported since.
    pub fn submission_uncertain(&self) -> bool {
        let mut at = self
            .uncertain_at
            .lock()
            .expect("uncertain submission lock poisoned");
        match *at {
            Some((before, started))
                if self.prompts_sent() <= before
                    && started.elapsed() < crate::messaging::outbox::DRAFT_WAIT =>
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
        // Graceful teardown: the agent is asked to terminate and only killed if it does not. This
        // is not politeness — Grok's `Stop` hook fires with `reason: "shutdown"` on that request
        // and not at all on a kill, and repeatedly killing Claude Code mid-startup trips its own
        // renderer-fallback counter, which is machine-level state it keeps about itself.
        self.pty.stop(Polite::Terminate);
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

#[cfg(test)]
mod tests {
    use crate::platform::term::Input;

    #[test]
    fn a_submit_retry_waits_for_a_dialog_without_resetting_its_deadline() {
        let live = crate::test_support::fake_live_session(
            "unsent-dialog",
            crate::protocol::AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        );
        live.mark_unsent(0);
        *live.unsent_at.lock().unwrap() = Some((
            0,
            std::time::Instant::now() - crate::messaging::outbox::DRAFT_WAIT,
            true,
        ));
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
            crate::protocol::AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        );
        live.mark_unsent(0);
        assert!(!live.retry_unsent_submit(|| true).unwrap());
        *live.unsent_at.lock().unwrap() = Some((
            0,
            std::time::Instant::now() - crate::messaging::outbox::DRAFT_WAIT,
            true,
        ));
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
        *live.unsent_at.lock().unwrap() = Some((
            0,
            std::time::Instant::now() - crate::messaging::outbox::DRAFT_WAIT,
            true,
        ));
        live.note_prompt_sent(None);
        assert!(!live.retry_unsent_submit(|| true).unwrap());
        assert_eq!(live.input_writes(), before + 1);
    }

    /// A message the write left in its input box, its submit key pressed or held back, holds
    /// the queue as any unsent message does, but no retry presses the key once more, however long
    /// it waits: only a prompt the agent reports releases it.
    #[test]
    fn a_message_not_submitted_gets_no_further_enter() {
        let live = crate::test_support::fake_live_session(
            "unsent-not-submitted",
            crate::protocol::AgentId::CODEX,
            120,
            40,
            "sleep 30",
        );
        live.mark_not_submitted(0);
        *live.unsent_at.lock().unwrap() = Some((
            0,
            std::time::Instant::now() - crate::messaging::outbox::DRAFT_WAIT,
            false,
        ));
        let before = live.input_writes();
        assert!(!live.retry_unsent_submit(|| true).unwrap());
        live.schedule_unsent_submit_retry();
        assert!(!live
            .unsent_retry_scheduled
            .load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(live.input_writes(), before);
        assert!(live.unsent_waiting());

        live.note_prompt_sent(None);
        assert!(!live.unsent_waiting());
    }

    /// A prompt reported after its message was marked unsent, as a first hook started cold reports
    /// it, ends the hold on the queue and leaves the submit key unpressed.
    #[test]
    fn a_late_report_of_the_prompt_ends_the_unsent_hold_without_a_retry() {
        let live = crate::test_support::fake_live_session(
            "unsent-late",
            crate::protocol::AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        );
        live.mark_unsent(0);
        assert!(live.unsent_waiting());
        live.note_prompt_sent(Some("late"));
        assert!(!live.unsent_waiting());
        let before = live.input_writes();
        assert!(!live.retry_unsent_submit(|| true).unwrap());
        assert_eq!(live.input_writes(), before);
    }

    /// A submit-key retry whose write fails is told as a notice about the session, not as an error.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_submit_retry_is_announced_as_a_notice_about_the_session() {
        let (state, dir) = crate::test_support::app_state("submit-retry-failed");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::bare_session(
                "submit-retry-failed",
                crate::protocol::SessionKind::Project,
            ))
            .unwrap();
        #[cfg(unix)]
        let live = crate::test_support::fake_live_session(
            "submit-retry-failed",
            crate::protocol::AgentId::CLAUDE,
            120,
            40,
            "trap '' HUP; echo ready; exec sleep 30",
        );
        // On Windows the console's close is what a hang-up is, which ends a process once the host's
        // grace is over; this one holds on past it.
        #[cfg(windows)]
        let live = crate::test_support::fake_live_session_running(
            "submit-retry-failed",
            crate::protocol::AgentId::CLAUDE,
            120,
            40,
            &crate::test_support::fixture::Steps::new()
                .hold_on_close()
                .say("ready")
                .sleep(30)
                .program("hold-on-close"),
        );
        // The stand-in announces itself only once it ignores SIGHUP, so hanging up cannot end it.
        crate::terminal::pty::spawn_reader_thread((*live).clone(), 8 * 1024);
        let ready = crate::test_support::output_containing(&live, "ready");
        assert!(ready.contains("ready"), "{ready}");
        state.register_live(live.clone());
        let mut events = state.subscribe();
        live.hang_up();
        live.mark_unsent(0);
        *live.unsent_at.lock().unwrap() = Some((
            0,
            std::time::Instant::now() - crate::messaging::outbox::DRAFT_WAIT,
            true,
        ));
        live.schedule_unsent_submit_retry();
        let notice = tokio::time::timeout(crate::test_support::PATIENCE, async {
            loop {
                match events.recv().await.expect("the event stream ended") {
                    crate::protocol::Event::SessionNotice {
                        session,
                        code,
                        params,
                        ..
                    } => break (session, code, params),
                    crate::protocol::Event::Error { code, .. } => panic!("an error event {code}"),
                    _ => {}
                }
            }
        })
        .await
        .expect("no notice about the failed retry");
        assert_eq!(notice.0, "submit-retry-failed");
        assert_eq!(notice.1, crate::protocol::notice_code::SUBMIT_RETRY_FAILED);
        assert_eq!(
            notice.2.get("agent").map(String::as_str),
            Some("Claude Code")
        );
        assert!(!notice.2.get("detail").expect("a detail").is_empty());
    }

    #[test]
    fn uncertain_native_acceptance_cannot_hold_later_messages_forever() {
        let live = crate::test_support::fake_live_session(
            "uncertain-bounded",
            crate::protocol::AgentId::CODEX,
            120,
            40,
            "sleep 30",
        );
        live.mark_submission_uncertain(0);
        assert!(live.submission_uncertain());
        assert!(!live.unsent_waiting());
        *live.uncertain_at.lock().unwrap() = Some((
            0,
            std::time::Instant::now() - crate::messaging::outbox::DRAFT_WAIT,
        ));
        assert!(!live.submission_uncertain());
        assert_eq!(*live.uncertain_at.lock().unwrap(), None);
    }

    #[test]
    fn a_stale_empty_screen_cannot_send_keys_that_have_not_been_rendered() {
        use crate::protocol::AgentId;
        let live = crate::test_support::fake_live_session(
            "draft-gate",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.on_output(include_bytes!(
            "../../testdata/claude_input_placeholder.bin"
        ));
        assert_eq!(
            super::input_box::input_box(AgentId::CLAUDE, &live.screen_text_undimmed()).as_deref(),
            Some("")
        );
        live.write_terminal_input(b"unrendered draft").unwrap();
        let before = live.input_writes();
        let error = live
            .write_message_input(
                &[Input::Text("must not be written".into())],
                "program",
                false,
            )
            .unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(live.input_writes(), before);
        live.note_prompt_sent(None);
        assert!(
            live.write_message_input(&[Input::Text("still held".into())], "program", false)
                .is_err(),
            "a hook without a user submission candidate does not clear edits"
        );
        live.write_terminal_input(b"\rnext").unwrap();
        live.note_prompt_sent(Some("unrendered draft"));
        assert!(
            live.write_message_input(&[Input::Text("still held".into())], "program", false)
                .is_err(),
            "a hook for Enter cannot clear later input"
        );
        live.write_terminal_input(b"\r").unwrap();
        live.note_prompt_sent(Some("next"));
        live.write_message_input(
            &[Input::Text("now empty and acknowledged".into())],
            "program",
            false,
        )
        .unwrap();
    }

    #[test]
    fn a_known_draft_or_unknown_composer_prevents_a_message_for_all_three_agents() {
        use crate::protocol::AgentId;
        for (agent, draft) in [
            (
                AgentId::CLAUDE,
                &include_bytes!("../../testdata/claude_input_restored.bin")[..],
            ),
            (
                AgentId::GROK,
                &include_bytes!("../../testdata/grok_composer_draft.bin")[..],
            ),
            (
                AgentId::CODEX,
                &include_bytes!("../../testdata/codex_blank_first_draft.bin")[..],
            ),
        ] {
            let live =
                crate::test_support::fake_live_session("composer-gate", agent, 140, 45, "sleep 30")
                    .with_composer();
            assert!(live
                .write_message_input(&[Input::Text("unknown".into())], "unknown", false)
                .is_err());
            live.on_output(draft);
            let before = live.input_writes();
            assert!(live
                .write_message_input(&[Input::Text("draft".into())], "draft", false)
                .is_err());
            assert_eq!(live.input_writes(), before);
        }
    }

    /// Moves the latest key and the latest output back as far as the empty-box release waits on
    /// them: as if the agent had been still since well after it drew what the key did.
    fn settle(live: &super::LiveSession) {
        live.settle_input();
    }

    /// Draws `screen` as the whole of the agent's screen.
    fn draw(live: &super::LiveSession, screen: &[u8]) {
        let mut bytes = b"\x1b[2J\x1b[H".to_vec();
        bytes.extend_from_slice(screen);
        live.on_output(&bytes);
    }

    /// A capture's text as the bytes that draw it.
    fn drawn(capture: &str) -> Vec<u8> {
        capture.trim_end().replace('\n', "\r\n").into_bytes()
    }

    /// The screen of `agent` with its input box empty, as measured.
    fn empty_box(agent: crate::protocol::AgentId) -> Vec<u8> {
        use super::input_box::captures::{CLEARED, GROK_CLEARED};
        use crate::protocol::AgentId;
        match agent {
            AgentId::CLAUDE => drawn(CLEARED),
            AgentId::GROK => drawn(GROK_CLEARED),
            _ => include_bytes!("../../testdata/codex_fresh_empty.bin").to_vec(),
        }
    }

    /// Keys that leave the input box empty — a lone Esc, text typed and cleared again, text moved
    /// through and cut, a prompt sent whose hook named no text it was typed as — hold no
    /// message once the agent has drawn the box empty after them, for every agent.
    #[test]
    fn keys_that_leave_the_box_empty_hold_no_message_once_it_is_drawn_empty() {
        use crate::protocol::AgentId;
        for agent in [AgentId::CLAUDE, AgentId::GROK, AgentId::CODEX] {
            for keys in [&b"\x1b"[..], b"abc\x15", b"abc\x1b[D\x17", b"typed\r"] {
                let live = crate::test_support::fake_live_session(
                    "typed-then-empty",
                    agent,
                    140,
                    45,
                    "sleep 30",
                )
                .with_composer();
                draw(&live, &empty_box(agent));
                live.write_terminal_input(keys).unwrap();
                draw(&live, &empty_box(agent));
                assert_eq!(
                    live.input_box(&live.screen_text_undimmed()).as_deref(),
                    Some("")
                );
                settle(&live);
                let before = live.input_writes();
                live.write_message_input(&[Input::Text("peer".into())], "peer", false)
                    .unwrap_or_else(|error| panic!("{agent:?} after {keys:?}: {}", error.error));
                assert_eq!(live.input_writes(), before + 1);
            }
        }
    }

    /// The prompt Claude Code puts back in its box when the user's Esc cancels a turn holds a
    /// message as a draft does, until the user clears it and the box is drawn empty.
    #[test]
    fn a_prompt_an_escape_put_back_holds_a_message_until_it_is_cleared() {
        use crate::protocol::AgentId;
        let live = crate::test_support::fake_live_session(
            "restored-then-cleared",
            AgentId::CLAUDE,
            140,
            45,
            "sleep 30",
        )
        .with_composer();
        draw(&live, &empty_box(AgentId::CLAUDE));
        live.write_terminal_input(b"\x1b").unwrap();
        draw(
            &live,
            include_bytes!("../../testdata/claude_input_restored.bin"),
        );
        settle(&live);
        let before = live.input_writes();
        let error = live
            .write_message_input(&[Input::Text("peer".into())], "peer", false)
            .unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(live.input_writes(), before);
        live.write_terminal_input(b"\x15").unwrap();
        draw(&live, &empty_box(AgentId::CLAUDE));
        settle(&live);
        let before = live.input_writes();
        live.write_message_input(&[Input::Text("peer".into())], "peer", false)
            .unwrap();
        assert_eq!(live.input_writes(), before + 1);
    }

    /// A message queued while the user's keys hold the session is written by the session's watch
    /// at its next look once the box is drawn empty after them, not at its retry two seconds on.
    /// The clock is paused, so the watch's looks come in a few milliseconds of real time, while the
    /// retry waits two seconds of it.
    #[tokio::test(start_paused = true)]
    async fn the_watch_writes_a_message_held_for_typing_once_the_box_is_drawn_empty() {
        use super::input_box::captures::DRAFT;
        use crate::protocol::{AgentId, SessionStatus};
        let (state, dir) = crate::test_support::app_state("draft-release-watch");
        crate::state::test_support::insert_record(&state, &dir, "s", SessionStatus::Working);
        let live =
            crate::test_support::fake_live_session("s", AgentId::CLAUDE, 140, 45, "sleep 30")
                .with_composer();
        state.register_live(live.clone());
        draw(&live, &empty_box(AgentId::CLAUDE));
        live.write_terminal_input(b"draft text").unwrap();
        draw(&live, &drawn(DRAFT));
        assert_ne!(
            live.input_box(&live.screen_text_undimmed()).as_deref(),
            Some("")
        );
        state.queue_message("s", "peer message");
        crate::session::dialogs::watch(&state, &live);
        live.write_terminal_input(b"\x15").unwrap();
        draw(&live, &empty_box(AgentId::CLAUDE));
        settle(&live);
        // The release and the write it starts run in blocking tasks, which hold the clock still
        // until they are done, so each look the watch makes is over by the next one.
        for _ in 0..4 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if state.outbox.len("s") == 0 {
                break;
            }
        }
        assert_eq!(
            state.outbox.len("s"),
            0,
            "the box drawn empty did not release the message at the watch's next look"
        );
        assert!(!live.draft.lock().unwrap().holds());
    }

    /// A frame read in part can show the box cleared and not yet drawn again: inside an open
    /// synchronized update an empty box holds a message over a draft whose text the mirror cannot
    /// know, and once the update ends the box as drawn decides.
    #[test]
    fn an_empty_box_inside_a_frame_still_being_drawn_holds_a_message() {
        use super::input_box::captures::{CLEARED, DRAFT};
        use crate::protocol::AgentId;
        for ends_empty in [false, true] {
            let live = crate::test_support::fake_live_session(
                "torn-frame",
                AgentId::CLAUDE,
                140,
                45,
                "sleep 30",
            )
            .with_composer();
            draw(&live, &empty_box(AgentId::CLAUDE));
            // Text typed, then moved through: the mirror no longer knows what the box holds.
            live.write_terminal_input(b"draft text\x1b[D").unwrap();
            draw(&live, &drawn(DRAFT));
            let mut cleared = b"\x1b[?2026h\x1b[2J\x1b[H".to_vec();
            cleared.extend(drawn(CLEARED));
            live.on_output(&cleared);
            settle(&live);
            assert_eq!(
                live.input_box(&live.screen_text_undimmed()).as_deref(),
                Some("")
            );
            let before = live.input_writes();
            let error = live
                .write_message_input(&[Input::Text("peer".into())], "peer", false)
                .unwrap_err();
            assert_eq!(error.written, 0);
            assert_eq!(live.input_writes(), before);
            let mut rest = b"\x1b[2J\x1b[H".to_vec();
            rest.extend(drawn(if ends_empty { CLEARED } else { DRAFT }));
            rest.extend(b"\x1b[?2026l");
            live.on_output(&rest);
            settle(&live);
            let written = live.write_message_input(&[Input::Text("peer".into())], "peer", false);
            assert_eq!(written.is_ok(), ends_empty, "{written:?}");
        }
    }

    /// A key that may put text in the box the mirror cannot know — a prompt from history, a
    /// completion — holds a message over an empty box drawn after it, for every agent: the text it
    /// brings may be drawn later still.
    #[test]
    fn a_history_or_completion_key_holds_a_message_over_an_empty_box() {
        use crate::protocol::AgentId;
        for agent in [AgentId::CLAUDE, AgentId::GROK, AgentId::CODEX] {
            for keys in [&b"\x1b[A"[..], b"\t", b"\x1b[A\x15"] {
                let live = crate::test_support::fake_live_session(
                    "inserting-key",
                    agent,
                    140,
                    45,
                    "sleep 30",
                )
                .with_composer();
                draw(&live, &empty_box(agent));
                live.write_terminal_input(keys).unwrap();
                draw(&live, &empty_box(agent));
                settle(&live);
                let before = live.input_writes();
                let error = live
                    .write_message_input(&[Input::Text("peer".into())], "peer", false)
                    .unwrap_err();
                assert_eq!(error.written, 0, "{agent:?} after {keys:?}");
                assert!(
                    live.draft.lock().unwrap().holds(),
                    "{agent:?} after {keys:?}"
                );
                assert_eq!(live.input_writes(), before);
            }
        }
    }

    /// After Up and Ctrl-U a first message waits for the grace, and once it is written a second one
    /// on the same empty box goes in at once.
    #[test]
    fn a_message_written_after_a_history_key_frees_the_next_one() {
        use crate::protocol::AgentId;
        let live = crate::test_support::fake_live_session(
            "after-up-message",
            AgentId::CLAUDE,
            140,
            45,
            "sleep 30",
        )
        .with_composer();
        draw(&live, &empty_box(AgentId::CLAUDE));
        live.write_terminal_input(b"\x1b[A").unwrap();
        live.write_terminal_input(b"\x15").unwrap();
        draw(&live, &empty_box(AgentId::CLAUDE));
        settle(&live);
        let before = live.input_writes();
        assert!(live
            .write_message_input(&[Input::Text("first".into())], "first", false)
            .is_err());
        live.write_message_input(&[Input::Text("first".into())], "first", true)
            .unwrap();
        draw(&live, &empty_box(AgentId::CLAUDE));
        settle(&live);
        live.write_message_input(&[Input::Text("second".into())], "second", false)
            .unwrap();
        assert_eq!(live.input_writes(), before + 2);
    }

    /// An Esc typed while the hooks report a turn running is no Esc at the prompt, though the title
    /// still shows the prompt, as right after a submit: it waits for output like any other key.
    #[test]
    fn a_lone_escape_while_the_hooks_report_a_turn_waits_for_output() {
        use crate::protocol::{AgentId, SessionStatus};
        let (state, dir) = crate::test_support::app_state("escape-hooks-working");
        crate::state::test_support::insert_record(&state, &dir, "s", SessionStatus::Working);
        let live =
            crate::test_support::fake_live_session("s", AgentId::CLAUDE, 140, 45, "sleep 30")
                .with_composer();
        state.register_live(live.clone());
        draw(&live, &empty_box(AgentId::CLAUDE));
        live.write_terminal_input(b"\x1b").unwrap();
        settle(&live);
        let before = live.input_writes();
        assert!(live
            .write_message_input(&[Input::Text("peer".into())], "peer", false)
            .is_err());
        assert_eq!(live.input_writes(), before);
        draw(&live, &empty_box(AgentId::CLAUDE));
        settle(&live);
        live.write_message_input(&[Input::Text("peer".into())], "peer", false)
            .unwrap();
    }

    /// Past the draft's grace a box read empty off a frame still being drawn holds a message, and
    /// one read off the whole frame takes it.
    #[test]
    fn past_the_grace_an_empty_box_inside_a_frame_still_being_drawn_holds_a_message() {
        use super::input_box::captures::CLEARED;
        use crate::protocol::AgentId;
        let live = crate::test_support::fake_live_session(
            "grace-torn-frame",
            AgentId::CLAUDE,
            140,
            45,
            "sleep 30",
        )
        .with_composer();
        let mut cleared = b"\x1b[?2026h\x1b[2J\x1b[H".to_vec();
        cleared.extend(drawn(CLEARED));
        live.on_output(&cleared);
        settle(&live);
        let before = live.input_writes();
        assert!(live
            .write_message_input(&[Input::Text("peer".into())], "peer", true)
            .is_err());
        assert_eq!(live.input_writes(), before);
        live.on_output(b"\x1b[?2026l");
        settle(&live);
        live.write_message_input(&[Input::Text("peer".into())], "peer", true)
            .unwrap();
        assert_eq!(live.input_writes(), before + 1);
    }

    /// Past the draft's grace a message still waits while the box cannot be read after a key that
    /// may have brought back text the mirror cannot know, and goes in once the box can be read.
    #[test]
    fn past_the_grace_an_unreadable_box_after_a_history_key_holds_a_message() {
        use crate::protocol::AgentId;
        let live = crate::test_support::fake_live_session(
            "unreadable-after-up",
            AgentId::CLAUDE,
            140,
            45,
            "sleep 30",
        )
        .with_composer();
        draw(&live, &empty_box(AgentId::CLAUDE));
        live.write_terminal_input(b"\x1b[A").unwrap();
        draw(&live, b"a screen with no input box on it");
        settle(&live);
        let before = live.input_writes();
        let error = live
            .write_message_input(&[Input::Text("peer".into())], "peer", true)
            .unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(live.input_writes(), before);
        draw(&live, &empty_box(AgentId::CLAUDE));
        settle(&live);
        live.write_message_input(&[Input::Text("peer".into())], "peer", true)
            .unwrap();
        assert_eq!(live.input_writes(), before + 1);
    }

    /// A lone Esc at an empty box that Claude Code answers by printing nothing, as 2.1.322 at times
    /// does, holds no message once the Esc has settled; one typed while the title showed a turn
    /// running still waits for output, since the prompt the cancel puts back can come later.
    #[test]
    fn a_lone_escape_the_agent_printed_nothing_for_holds_no_message_at_the_prompt() {
        use crate::protocol::AgentId;
        for running in [false, true] {
            let live = crate::test_support::fake_live_session(
                "silent-escape",
                AgentId::CLAUDE,
                140,
                45,
                "sleep 30",
            )
            .with_composer();
            if running {
                live.on_output("\x1b]0;\u{25D0} Working\x07".as_bytes());
            }
            draw(&live, &empty_box(AgentId::CLAUDE));
            live.write_terminal_input(b"\x1b").unwrap();
            settle(&live);
            let before = live.input_writes();
            let written = live.write_message_input(&[Input::Text("peer".into())], "peer", false);
            assert_eq!(written.is_ok(), !running, "{written:?}");
            assert_eq!(live.input_writes(), before + u64::from(!running));
            if running {
                assert!(live.draft.lock().unwrap().holds());
            }
        }
    }

    /// An empty box read off a screen the agent has not drawn since the user's last key does not
    /// show what the key did, and one drawn empty although the user typed text and nothing that
    /// could have removed it may be a screen drawn before the text was: either holds a message.
    #[test]
    fn an_empty_box_that_may_predate_the_keys_holds_a_message() {
        use crate::protocol::AgentId;
        for (keys, redrawn) in [(&b"ab\x1b[D"[..], false), (b"draft", true)] {
            let live = crate::test_support::fake_live_session(
                "stale-empty-box",
                AgentId::CLAUDE,
                140,
                45,
                "sleep 30",
            )
            .with_composer();
            draw(&live, &empty_box(AgentId::CLAUDE));
            live.write_terminal_input(keys).unwrap();
            if redrawn {
                draw(&live, &empty_box(AgentId::CLAUDE));
            }
            settle(&live);
            let before = live.input_writes();
            let error = live
                .write_message_input(&[Input::Text("peer".into())], "peer", false)
                .unwrap_err();
            assert_eq!(error.written, 0, "{keys:?}");
            assert!(live.draft.lock().unwrap().holds(), "{keys:?}");
            assert_eq!(live.input_writes(), before);
        }
    }
}
