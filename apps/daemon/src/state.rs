//! The daemon's shared state: the session records and their status transitions, the live sessions
//! and the claim that keeps two launches of the same session from racing, the per-session MCP
//! tokens, the turn bookkeeping a synthesised report rests on, a facade over the write queue in
//! `crate::outbox`, and each project's live git status together with the claim that keeps two
//! checks of one project from racing and the timestamp that keeps them from piling up across
//! several clients, each ended session's saved terminal output, and the session requests waiting
//! for the user's answer (`crate::session_request`) together with the status
//! each of those sessions' agents is at while the request holds the record at waiting for the
//! user, and the shell terminals (`crate::shell_terminal`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::broadcast;

use crate::agent_dialogs::Sighting;
use crate::hooks::BackgroundTasksChange;
use crate::outbox::{Drain, Outbox, PromptSignal, Ticket};
use crate::protocol::{
    error_code, notice_code, now_millis, Agent, AgentAvailability, Availability, BackgroundTask,
    CodedError, Event, GitActivity, GitStatus, Notice, Session, SessionStatus,
};
use crate::saved_output::SavedOutput;
use crate::session::LiveSession;
use crate::store::Store;

/// How often a session's process is polled for having exited. The agent's own exit (the user
/// typing `/exit`, a crash) has no other signal: `SessionEnd` hooks are not reliable for this,
/// since a killed process fires nothing at all.
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// How long a subagent's stop that leaves no background work stands before the list is emptied.
/// A subagent handing over to the next one stops and starts within the same instant, and the list
/// must not publish empty in between.
pub(crate) const EMPTY_TASKS_DELAY: Duration = Duration::from_secs(2);

/// What is known of one session's background work.
#[derive(Default)]
struct BackgroundWork {
    tasks: Vec<BackgroundTask>,
    /// The descriptions seen for each subagent id. A `SubagentStart` carries none, and a subagent
    /// that resumes is listed again without one until its next stop lists it.
    descriptions: HashMap<String, String>,
    /// When each subagent's `SubagentStart` last arrived, kept for [`EMPTY_TASKS_DELAY`]. With
    /// async command hooks the events can arrive out of order, so a `SubagentStop` may list a
    /// registry from before a start that has already been applied.
    started: HashMap<String, tokio::time::Instant>,
    /// The ticket of the emptying of the list that is waiting out [`EMPTY_TASKS_DELAY`]. A
    /// `Stop`, a `StopFailure` and a `SubagentStop` that leaves something running take it back; a
    /// `SubagentStart` applies the emptying at once, before adding itself; a completion
    /// notification does neither.
    pending_empty: Option<u64>,
}

impl BackgroundWork {
    /// Applies one change. Returns the ticket when the change empties the list, which is not done
    /// yet: it takes effect through [`Self::expire`] once the caller has waited out
    /// [`EMPTY_TASKS_DELAY`], unless the ticket was taken back by then.
    fn apply(
        &mut self,
        change: BackgroundTasksChange,
        now: tokio::time::Instant,
        ticket: u64,
    ) -> Option<u64> {
        self.started
            .retain(|_, at| now.duration_since(*at) < EMPTY_TASKS_DELAY);
        match change {
            BackgroundTasksChange::Replace(list) => {
                self.pending_empty = None;
                self.started.clear();
                self.remember_descriptions(&list);
                self.tasks = list;
            }
            BackgroundTasksChange::Finished(ids) => {
                self.tasks.retain(|task| !ids.contains(&task.id));
                self.started.retain(|id, _| !ids.contains(id));
            }
            BackgroundTasksChange::SubagentStopped { id, tasks: listed } => {
                self.remember_descriptions(&listed);
                self.started.remove(&id);
                let mut rest: Vec<_> = listed.into_iter().filter(|task| task.id != id).collect();
                // A start that arrived just before this stop is running even if the registry
                // the stop carries predates it.
                let recent: Vec<_> = self.started.keys().cloned().collect();
                for started in recent {
                    if !rest.iter().any(|task| task.id == started) {
                        rest.push(self.subagent_task(started));
                    }
                }
                if rest.is_empty() {
                    self.pending_empty = Some(ticket);
                    return Some(ticket);
                }
                self.pending_empty = None;
                self.tasks = rest;
            }
            BackgroundTasksChange::SubagentStarted(id) => {
                // The stop it follows left nothing else running, so its deferred emptying takes
                // effect now, before this subagent is added.
                if self.pending_empty.take().is_some() {
                    self.tasks.clear();
                }
                self.started.insert(id.clone(), now);
                if !self.tasks.iter().any(|task| task.id == id) {
                    self.tasks.push(self.subagent_task(id));
                }
            }
        }
        None
    }

    /// Empties the list if `ticket` is still the one waiting. Whether the list changed.
    fn expire(&mut self, ticket: u64) -> bool {
        if self.pending_empty != Some(ticket) {
            return false;
        }
        self.pending_empty = None;
        !std::mem::take(&mut self.tasks).is_empty()
    }

    /// Whether there is nothing left at all, descriptions included, to keep a map entry for.
    fn is_empty(&self) -> bool {
        self.tasks.is_empty() && self.pending_empty.is_none() && self.descriptions.is_empty()
    }

    fn subagent_task(&self, id: String) -> BackgroundTask {
        BackgroundTask {
            description: self.descriptions.get(&id).cloned(),
            id,
            kind: Some("subagent".to_string()),
        }
    }

    fn remember_descriptions(&mut self, tasks: &[BackgroundTask]) {
        for task in tasks {
            if let (Some("subagent"), Some(description)) = (task.kind.as_deref(), &task.description)
            {
                self.descriptions
                    .insert(task.id.clone(), description.clone());
            }
        }
    }
}

pub struct AppState {
    pub store: Store,
    pub port: u16,
    pub self_exe: String,
    /// The running sessions, plus the ids currently being launched. One lock over both, because
    /// "is this session already running" has to be answered and acted on without a gap: a second
    /// `resume_session` arriving while the first is still forking would otherwise start a second
    /// process for the same session, and the first one's exit watcher would then evict the live
    /// one and leave an agent running that nothing tracks.
    live: RwLock<LiveSessions>,
    /// MCP token to session id. A token is issued per launch and dropped when the process goes
    /// away, so the MCP surface is reachable only by the children the daemon itself started.
    mcp_tokens: RwLock<HashMap<String, String>>,
    /// Per-session turn bookkeeping, which is what makes a synthesised report possible without
    /// correlating turn ids. Entries are added on a turn start and removed when the session's
    /// process goes away.
    turns: Mutex<HashMap<String, TurnState>>,
    /// The generation each session's record watch (`crate::record_watch`, run for Claude Code by
    /// `crate::transcript` and for Codex by `crate::rollout`) is currently on.
    /// Starting a watch records a fresh value here; the watch keeps polling only while its own
    /// value is still the one recorded, which is what lets a newer watch for the same session
    /// retire an older one rather than race it. The counter it is drawn from is global, not
    /// per-session, because uniqueness is all that is asked of it.
    record_watch_generations: Mutex<HashMap<String, u64>>,
    next_record_watch_generation: std::sync::atomic::AtomicU64,
    /// How far into its record file each session's carried-offset watches have read
    /// (`crate::record_watch::Start::Carried`), with the file's path. A new turn's watch resumes
    /// from there rather than rescanning the whole file, which on a long session runs to
    /// megabytes; what an earlier watch read it has already judged.
    record_offsets: Mutex<HashMap<String, (std::path::PathBuf, u64)>>,
    /// The per-session write queues. Every message Octoboard sends into a running agent goes
    /// through them; see `crate::outbox`.
    outbox: Outbox,
    /// The hook events received and not yet applied, per session (`crate::hook_queue`).
    pub hook_queues: crate::hook_queue::HookQueues,
    /// Each project's live git status, broadcast as `project_git_status` on every change and
    /// replayed in a `snapshot`, with a counter overlaying its activity while pushes run.
    /// Derived, never stored in SQLite — see `GitStatus`.
    git_statuses: RwLock<GitStatuses>,
    /// The ids of the projects whose git status is being checked right now, so
    /// `refresh_git_status` does not start a second check for one already in flight — a client
    /// polling faster than the checks finish, or several clients watching the same console, must
    /// not pile up work. Claimed and released through [`AppState::claim_git_check_queued`]; see
    /// [`GitCheck`] for what an entry carries.
    git_checks: Mutex<HashMap<String, GitCheck>>,
    /// When each project's last check completed, for the minimum-interval floor in
    /// `git_status::due_for_check` — a second guard next to `git_checks` above, needed because that
    /// one only stops two checks from overlapping and does nothing about several clients each
    /// polling on their own 5-minute phase. Entries are removed wherever the project's `GitStatus`
    /// is, by `remove_git_status`, so a reused project id never inherits a stale timestamp.
    git_check_completed: Mutex<HashMap<String, Instant>>,
    /// Per repository, the gate every write to its branch origins takes and when rows were last
    /// added (`crate::branch_origin`).
    pub branch_origins: crate::branch_origin::Gates,
    /// The removal of the branches and worktrees bound to sessions archived or deleted: its gate
    /// per repository.
    pub branch_cleanups: crate::branch_origin::cleanup::Cleanups,
    /// What requests reach the host through where a test stands in: `git`, the Trash.
    pub host_tools: crate::host_tools::HostTools,
    /// How long a message written to an idle Claude Code session is given to be reported sent, or
    /// `None` for no check at all; see [`HookPrompts`]. A seam for tests, whose stand-in sessions
    /// run no hooks and so never report sending anything.
    pub send_wait: Mutex<Option<Duration>>,
    pub record_hints: crate::agent_records::Hints,
    /// The context window each Claude Code session's status line last reported.
    pub context_windows: crate::record_context::ReportedWindows,
    /// The repositories' refs under watch, so a commit made outside the daemon is re-read
    /// (`crate::git_watch`).
    pub git_watch: crate::git_watch::Watch,
    /// Each agent's availability and what its default account currently resolves to, derived once
    /// per daemon start (`crate::availability`) and broadcast by its own event on change; never a
    /// field of `Store`'s settings row, which only the user's own updates write. Always holds all
    /// three agents, each starting `NotDetermined` — the state every run begins in.
    agent_availability: RwLock<HashMap<Agent, AgentAvailability>>,
    /// Each ended session's last terminal output, written as its process ends and removed with its
    /// record.
    saved_output: SavedOutput,
    /// Parent of the consoles' working directories; see [`AppState::console_workdir`].
    consoles_dir: std::path::PathBuf,
    /// Where the handoff packages are kept, one file per receiving session; see `crate::handoff`.
    handoff_dir: std::path::PathBuf,
    /// The bounds every connection's browse requests share: how many reads run at once, and how
    /// many bytes their replies may hold before they are written.
    pub browse_gate: Arc<crate::browse::lane::Gate>,
    /// The `request_session` and `request_resume_session` calls waiting for the user's answer; see
    /// `crate::session_request`.
    /// Held in memory only: no caller outlives the daemon.
    pub session_requests: crate::session_request::SessionRequests,
    /// The model lists read lately, per agent and account directory (`crate::agent_models`).
    pub agent_models: crate::agent_models::ModelCache,
    /// The plan quota last read for each account (`crate::account_quotas`). Held in memory only.
    pub account_quotas: crate::account_quotas::AccountQuotas,
    /// The logins running now, one per account (`crate::account_login`). Held in memory only.
    pub account_logins: crate::account_login::AccountLogins,
    /// What the accounts' changes of sharing of the default configuration hold: one lock per
    /// agent over the default directory, and each account's progress and last failure
    /// (`crate::shared_config`). Held in memory only.
    pub account_sharing: crate::shared_config::Sharing,
    /// The shell terminals running, or exited and not yet closed (`crate::shell_terminal`). Held in
    /// memory only: each is a process of this daemon's.
    pub shell_terminals: crate::shell_terminal::ShellTerminals,
    /// For each session with a session request waiting, the status its agent's own events
    /// say it has, while the record shows `WaitingUser` for the request. Kept so that the daemon's
    /// decisions about the agent (can a message be written, did a turn end) keep reading the agent
    /// rather than the hand, and so the status can be restored when the request ends. Always taken
    /// before `session_requests`, never after. Dropped with the session's other bookkeeping.
    request_hands: Mutex<HashMap<String, SessionStatus>>,
    /// The background work each session's agent last said it left running
    /// (`Session::background_tasks`). Held in memory only: the work belongs to the agent process
    /// and goes with it, so nothing is worth keeping across a daemon restart. Dropped with the
    /// session's other bookkeeping.
    background_tasks: Mutex<HashMap<String, BackgroundWork>>,
    /// The effort each session's agent runs it at by itself (`Session::default_effort`), kept by
    /// the record collector for the sessions that have one. Held in memory only: it is read off
    /// the agent's own files again within a pass of the collector after a restart.
    default_efforts: Mutex<HashMap<String, String>>,
    /// Numbers the emptyings of a session's background task list that wait out
    /// [`EMPTY_TASKS_DELAY`], so one that was taken back is told from the one that replaced it.
    next_tasks_ticket: std::sync::atomic::AtomicU64,
    events: broadcast::Sender<Event>,
    /// Held by `create_account` and `update_account` from their uniqueness checks to their write,
    /// so two requests cannot both pass the checks and then both write. A plain mutex and never
    /// held across an `.await`; see [`AppState::lock_account_writes`].
    account_writes: Mutex<()>,
    /// Serializes session readback with event sends, including deletions, so a stale reader
    /// cannot announce its snapshot after a newer title or removal.
    session_publication: Mutex<()>,
    /// Lock changes and removal claims share this gate, so a lock cannot be accepted halfway
    /// through a cascade or while a project directory is being moved to the Trash.
    session_removals: Mutex<usize>,
    shutdown: tokio::sync::Notify,
}

/// What a session's claim is held for (see [`AppState::claim_kind`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    /// Launching or resuming its process.
    Launch,
    /// An account switch, a handoff, a handover of branches or a binding change.
    Switch,
}

/// Why nothing is written into a running session for now; see [`AppState::write_hold`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteHold {
    /// The agent has not yet sent the signal that it is past its startup screens: its first hook,
    /// or for Codex its first terminal title.
    Starting,
    /// The agent's folder-trust confirmation is up and has not been answered.
    TrustConfirmation,
    /// The agent is showing a dialog only the user can answer, which a message's paste and Enter
    /// would answer destructively; see [`crate::agent_dialogs`].
    AgentDialog(crate::agent_dialogs::Dialog),
    /// Codex's terminal title says it is waiting on the user.
    AgentAction,
    /// The agent stopped on an error and may be showing a prompt only the user can answer; held for
    /// [`crate::session::AGENT_ERROR_SETTLE`] or until a prompt is sent there, whichever is first.
    AgentError,
    /// The session is being handed off to another agent.
    Handoff,
    /// The session is being restarted for a change of its account's sharing, and is resumed
    /// afterwards.
    SharingChange,
    /// A message written into it was not sent and is still in its input box.
    UnsentMessage,
}

impl WriteHold {
    pub fn code(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::TrustConfirmation => "trust_confirmation",
            Self::AgentDialog(_) => "agent_dialog",
            Self::AgentAction => "agent_action",
            Self::AgentError => "agent_error",
            Self::Handoff => "handoff",
            Self::SharingChange => "sharing_change",
            Self::UnsentMessage => "unsent_message",
        }
    }

    /// Which dialog, for [`Self::AgentDialog`].
    pub fn dialog(self) -> Option<&'static str> {
        match self {
            Self::AgentDialog(dialog) => Some(dialog.code),
            _ => None,
        }
    }
}

/// What the daemon remembers about a session's current turn.
#[derive(Default)]
struct TurnState {
    /// A turn has started and no turn-end signal has been read for it yet. Turn ends are only
    /// acted on while this holds, which is what keeps a turn-end signal that arrives twice from
    /// being read as two turns ending.
    open: bool,
    /// The session called `report` during this turn, so its stop needs no synthesised report.
    reported: bool,
    /// When this turn last showed any sign of life. A clock-attributed turn end (Grok's
    /// `idle_prompt`) can only belong to a turn that has been quiet at least as long as that
    /// signal's own delay; without this, one left over from a finished turn would close the turn
    /// running now and hand the console session a report for work still in flight.
    last_event_at: i64,
    /// A turn of this session has ended through a signal that names its own turn, and Grok's
    /// clock-attributed echo of that ending has not been seen yet. The quiet gate alone is not
    /// enough: Grok emits nothing between `PreToolUse` and `PostToolUse`, so a tool call that runs
    /// longer than the backstop delay — a build, a test run — leaves a *working* turn looking
    /// quiet, and the previous turn's echo would then close it. The echo is consumed rather than
    /// acted on.
    echo_pending: bool,
    /// Turns not started yet that are credited as reported, because what starts each is something
    /// of Octoboard's own that needs no report back: the turn whose prompt opens with that text
    /// (`credit_turn_opened_by`). Each is spent by that turn's start.
    credits: Vec<String>,
}

/// What closing a turn amounted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnClose {
    /// The turn is closed and nobody reported for it, so a report has to be synthesised.
    OwesReport,
    /// The turn is closed and the session reported for itself.
    Reported,
    /// No turn was open. The signal is a repeat, or belongs to something Octoboard never saw start.
    NoTurn,
    /// A clock-attributed signal that cannot belong to the turn currently open, so nothing was
    /// closed — and nothing it says about the session may be believed either.
    NotThisTurn,
}

/// A session's queue held by [`AppState::hold_outbox`]. While it lives the queue is kept, whatever
/// becomes of the session's process. Dropping it ends the hold, and where the queue goes depends on
/// whether the holder has named a successor ([`Self::forward_to`]): to the successor, in order,
/// or, without one, to the session itself if it is still running and nowhere if it is not, since a
/// session without a process takes nothing and would leave the queue stranded. A queue dropped for
/// that is announced as `handoff_messages_dropped`, or as `sharing_messages_dropped` for a hold taken
/// for a sharing change ([`AppState::hold_outbox_for_sharing`]): these are messages held back, never
/// ones part-written into a terminal, so the notice says nothing of an input line.
pub struct OutboxHold {
    state: Arc<AppState>,
    id: String,
    /// Held for a change of an account's sharing, not a handoff; only the notice for a drop differs.
    for_sharing: bool,
    successor: std::sync::Mutex<Option<String>>,
}

impl OutboxHold {
    /// From now on whatever is queued for the held session goes to `successor` when the hold ends.
    pub fn forward_to(&self, successor: &str) {
        *self.successor.lock().expect("hold lock poisoned") = Some(successor.to_string());
    }
}

impl Drop for OutboxHold {
    fn drop(&mut self) {
        let successor = self.successor.lock().expect("hold lock poisoned").take();
        let target = match &successor {
            Some(successor) => {
                for text in self.state.outbox.take(&self.id) {
                    self.state.outbox.push(successor, &text);
                }
                successor
            }
            None => &self.id,
        };
        self.state.outbox.release_hold(&self.id);
        let record = self.state.store.get_session(target).ok().flatten();
        // Judged by the process as well as the status: between a process leaving the live map and
        // the exit being saved the record still reads running.
        let writable = record
            .filter(|record| !record.status.is_dormant())
            .filter(|_| self.state.live_session(target).is_some());
        let Some(record) = writable else {
            // A successor being launched is no process yet but will be: a resume saves it running
            // and queues its instruction first. The launch flushes what is queued, and
            // `unregister_live` forgets it if the launch fails.
            if successor.is_some() && self.state.is_launching(target) {
                return;
            }
            // Nothing to write to, and a queue left in the map would be replayed on a later resume.
            self.state.drop_held_queue(target, self.for_sharing);
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let (state, id) = (self.state.clone(), target.clone());
            let status = state.agent_status(&record);
            runtime.spawn_blocking(move || state.flush_outbox(&id, status));
        }
    }
}

/// How long a message written to an idle session is given to be reported sent before it is taken
/// to be sitting unsent in the input box. A prompt's `UserPromptSubmit` normally arrives well
/// within a second of the Enter that sent it.
const SEND_WAIT: Duration = Duration::from_secs(3);

/// A running session's prompts as its hooks report them, for the outbox to tell a message the agent
/// sent from one it left in the input box.
struct HookPrompts<'a> {
    state: &'a AppState,
    live: &'a LiveSession,
}

impl HookPrompts<'_> {
    fn wait(&self) -> Option<Duration> {
        *self
            .state
            .send_wait
            .lock()
            .expect("send wait lock poisoned")
    }
}

impl PromptSignal for HookPrompts<'_> {
    fn sent(&self) -> u64 {
        self.live.prompts_sent()
    }

    /// Claude Code alone, the one agent seen to take a write and not send it. Read now rather than
    /// taken from the status the drain was started with, which can be stale by the time a message
    /// reaches the front: a write that lands mid-turn is queued by the agent and sent only when
    /// the turn ends, which no short wait would see.
    fn expects_send(&self) -> bool {
        self.wait().is_some()
            && self.live.agent == Agent::Claude
            && self
                .state
                .session_record(&self.live.id)
                .is_ok_and(|record| self.state.agent_status(&record) == SessionStatus::Idle)
    }

    fn wait_past(&self, before: u64) -> bool {
        let deadline = Instant::now() + self.wait().unwrap_or_default();
        while self.live.prompts_sent() <= before {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }

    fn mark_unsent(&self, before: u64) {
        self.live.mark_unsent(before);
    }

    fn unsent_waiting(&self) -> bool {
        self.live.unsent_waiting()
    }

    fn sighting(&self) -> Sighting {
        self.live.look_for_dialog()
    }

    fn watch_screen(&self, on: bool) {
        self.live.watch_screen(on);
    }

    fn trust_holds_writes(&self) -> bool {
        self.live.trust.holds_writes()
    }
}

impl AppState {
    pub fn new(
        store: Store,
        port: u16,
        self_exe: String,
        saved_output_dir: std::path::PathBuf,
        consoles_dir: std::path::PathBuf,
        handoff_dir: std::path::PathBuf,
    ) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            store,
            port,
            self_exe,
            live: RwLock::new(LiveSessions::default()),
            mcp_tokens: RwLock::new(HashMap::new()),
            turns: Mutex::new(HashMap::new()),
            request_hands: Mutex::new(HashMap::new()),
            background_tasks: Mutex::new(HashMap::new()),
            default_efforts: Mutex::new(HashMap::new()),
            next_tasks_ticket: std::sync::atomic::AtomicU64::new(0),
            record_watch_generations: Mutex::new(HashMap::new()),
            next_record_watch_generation: std::sync::atomic::AtomicU64::new(0),
            record_offsets: Mutex::new(HashMap::new()),
            outbox: Outbox::default(),
            hook_queues: Default::default(),
            git_statuses: RwLock::new(GitStatuses::default()),
            git_checks: Mutex::new(HashMap::new()),
            git_check_completed: Mutex::new(HashMap::new()),
            branch_origins: Default::default(),
            branch_cleanups: Default::default(),
            host_tools: Default::default(),
            send_wait: Mutex::new(Some(SEND_WAIT)),
            record_hints: Default::default(),
            context_windows: Default::default(),
            git_watch: Default::default(),
            agent_availability: RwLock::new(
                Agent::ALL
                    .into_iter()
                    .map(|agent| {
                        (
                            agent,
                            AgentAvailability {
                                agent,
                                availability: Availability::NotDetermined,
                                default_account_dir: None,
                            },
                        )
                    })
                    .collect(),
            ),
            saved_output: SavedOutput::new(saved_output_dir),
            consoles_dir,
            handoff_dir,
            browse_gate: Arc::new(crate::browse::lane::Gate::with_budget()),
            session_requests: Default::default(),
            agent_models: Default::default(),
            account_quotas: Default::default(),
            account_logins: Default::default(),
            account_sharing: Default::default(),
            shell_terminals: Default::default(),
            events,
            account_writes: Mutex::new(()),
            session_publication: Mutex::new(()),
            session_removals: Mutex::new(0),
            shutdown: tokio::sync::Notify::new(),
        }
    }

    pub fn claim_session_removal(self: &Arc<Self>) -> SessionRemovalClaim {
        *self.session_removals.lock().unwrap() += 1;
        SessionRemovalClaim {
            state: self.clone(),
        }
    }

    pub fn set_session_locked(&self, id: &str, locked: bool) -> Result<()> {
        let removals = self.session_removals.lock().unwrap();
        if *removals != 0 {
            return Err(CodedError::raised(
                error_code::SESSION_LOCK_BUSY,
                "sessions are being archived or deleted; try changing the lock again when that operation finishes",
                &[("session", id)],
            ));
        }
        self.store.set_session_locked(id, locked)
    }

    /// Where the handoff packages are kept; see `crate::handoff`.
    pub fn handoff_dir(&self) -> &std::path::Path {
        &self.handoff_dir
    }

    /// The working directory a console of this id gets, under the directory the daemon was started
    /// with for them rather than a fixed one, so a test points it at a scratch directory.
    pub fn console_workdir(&self, console_id: &str) -> std::path::PathBuf {
        self.consoles_dir.join(console_id)
    }

    /// Asks the daemon to shut down. The application sends this once the user has confirmed the
    /// exit; `main` waits on it.
    pub fn request_shutdown(&self) {
        self.shutdown.notify_waiters();
    }

    pub async fn await_shutdown(&self) {
        self.shutdown.notified().await;
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// The critical section of an account write that depends on what the other accounts hold (a
    /// name or a directory that must stay unique): take it before the checks and keep the guard
    /// until the write is done. Synchronous code only: the guard must not live across an `.await`.
    pub fn lock_account_writes(&self) -> std::sync::MutexGuard<'_, ()> {
        self.account_writes
            .lock()
            .expect("account writes lock poisoned")
    }

    pub fn broadcast(&self, event: Event) {
        let _publication = self
            .session_publication
            .lock()
            .expect("session publication lock poisoned");
        self.broadcast_locked(event);
    }

    fn broadcast_locked(&self, event: Event) {
        // No subscribers is the normal state while the application is not running.
        let _ = self.events.send(event);
    }

    pub fn live_session(&self, id: &str) -> Option<Arc<LiveSession>> {
        self.live
            .read()
            .expect("live sessions lock poisoned")
            .sessions
            .get(id)
            .cloned()
    }

    /// Claims the right to launch a process for this session. Fails if one is already running or
    /// already being launched; the claim is released only when the returned guard is dropped, and
    /// registering the process as live does not release it — the holder drops it once whatever it
    /// does around the launch is done.
    ///
    /// Refused too while the session's account is locked for a change of its sharing (see
    /// [`Self::lock_account_sharing`]): judged under the same lock as the claim, so a change
    /// either sees the claim or the launch sees the change.
    pub fn begin_launch(self: &Arc<Self>, id: &str) -> Result<LaunchClaim> {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        if live.sessions.contains_key(id) {
            return Err(CodedError::raised(
                error_code::SESSION_ALREADY_RUNNING,
                "this session is already running",
                &[("session", id)],
            ));
        }
        if !live.sharing_locks.is_empty() {
            if let Some(session) = self.store.get_session(id)? {
                if let Some(entry) = locked_over(&live, &session) {
                    return Err(crate::shared_config::sharing_in_progress(&entry.account));
                }
            }
        }
        if !live.launching.insert(id.to_string()) {
            return Err(CodedError::raised(
                error_code::SESSION_ALREADY_STARTING,
                "this session is already being started",
                &[("session", id)],
            ));
        }
        drop(live);
        crate::shared_config::publish(self);
        Ok(LaunchClaim {
            state: self.clone(),
            id: id.to_string(),
        })
    }

    /// Claims the session exclusively, as [`Self::begin_launch`] does but without needing its
    /// process down: it may be taken while the process is still running. It is the claim of
    /// every operation that must have the session to itself while it runs — an account switch,
    /// which ends the process as its first step, a handoff, a handover of branches, and a binding
    /// change — and it is held for as long as the guard lives, so a resume, a launch, a delete and
    /// another holder are all refused for that whole window. Fails only when a launch or another
    /// holder has it.
    pub fn begin_switch(self: &Arc<Self>, id: &str) -> Result<LaunchClaim> {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        if !live.launching.insert(id.to_string()) {
            return Err(CodedError::raised(
                error_code::SESSION_ALREADY_STARTING,
                "this session is already being started",
                &[("session", id)],
            ));
        }
        live.switching.insert(id.to_string());
        drop(live);
        crate::shared_config::publish(self);
        Ok(LaunchClaim {
            state: self.clone(),
            id: id.to_string(),
        })
    }

    /// [`Self::begin_switch`] for an account switch: the session is also shown as switching its
    /// account to every client until the claim is released.
    pub fn begin_account_switch(self: &Arc<Self>, id: &str) -> Result<LaunchClaim> {
        let claim = self.begin_switch(id)?;
        self.live
            .write()
            .expect("live sessions lock poisoned")
            .account_switches
            .insert(id.to_string());
        self.publish_session_by_id(id);
        Ok(claim)
    }

    /// Whether an account switch holds the session's claim (see [`Self::begin_account_switch`]).
    pub fn is_switching_account(&self, id: &str) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.account_switches.contains(id)
    }

    fn release_launch(&self, id: &str) {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        live.launching.remove(id);
        live.switching.remove(id);
        let was_switching_account = live.account_switches.remove(id);
        drop(live);
        if was_switching_account {
            self.publish_session_by_id(id);
        }
        crate::shared_config::publish(self);
    }

    /// Locks `account` for a change of its sharing, in one step with `admit`, which is shown
    /// whether the account is locked already and the sessions running or holding a claim, and
    /// refuses by answering an error: no claim can be taken or let go in between, and from then on
    /// [`Self::begin_launch`] refuses every session of the account until the answer is dropped.
    pub fn lock_account_sharing<T>(
        self: &Arc<Self>,
        account: &crate::protocol::Account,
        admit: impl FnOnce(bool, &[Running]) -> Result<T>,
    ) -> Result<(T, AccountSharingLock)> {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        let running = self.running_of(&live)?;
        let admitted = admit(live.sharing_locks.contains_key(&account.id), &running)?;
        live.sharing_locks.insert(
            account.id.clone(),
            SharingLockEntry {
                account: account.clone(),
                restarting: HashSet::new(),
            },
        );
        Ok((
            admitted,
            AccountSharingLock {
                state: self.clone(),
                account: account.id.clone(),
                claims: HashMap::new(),
                holds: HashMap::new(),
            },
        ))
    }

    /// The account locked for a change of its sharing that `session` runs under, if any, unless
    /// the session is one that change restarts.
    pub fn sharing_lock_over(&self, session: &Session) -> Option<crate::protocol::Account> {
        let live = self.live.read().expect("live sessions lock poisoned");
        locked_over(&live, session)
            .filter(|entry| !entry.restarting.contains(&session.id))
            .map(|entry| entry.account.clone())
    }

    /// The account `id` while it is locked for a change of its sharing.
    pub fn sharing_locked(&self, id: &str) -> Option<crate::protocol::Account> {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.sharing_locks
            .get(id)
            .map(|entry| entry.account.clone())
    }

    /// The sessions running or holding a claim, each once, as a change of sharing sees them.
    pub fn running_sessions(&self) -> Result<Vec<Running>> {
        let live = self.live.read().expect("live sessions lock poisoned");
        self.running_of(&live)
    }

    fn running_of(&self, live: &LiveSessions) -> Result<Vec<Running>> {
        let ids: HashSet<&String> = live.sessions.keys().chain(&live.launching).collect();
        let mut running = Vec::new();
        for id in ids {
            let Some(session) = self.store.get_session(id)? else {
                continue;
            };
            let restarting = live
                .sharing_locks
                .values()
                .any(|entry| entry.restarting.contains(id));
            let working = !restarting
                && (live.launching.contains(id)
                    || live
                        .sessions
                        .get(id)
                        .is_none_or(|process| !self.is_idle_with(&session, process)));
            running.push(Running {
                session,
                working,
                restarting,
            });
        }
        Ok(running)
    }

    /// Whether `session` is idle: its process is at the prompt, waiting for the user's next
    /// message, with nothing on screen only the user can answer, no message of Octoboard's
    /// queued, being written or left unsent in its input box, and nothing left running in the
    /// background.
    fn is_idle_with(&self, session: &Session, process: &LiveSession) -> bool {
        session.status == SessionStatus::Idle
            && !process.waits_for_user()
            && !process.unsent_waiting()
            && !self.outbox.is_pending(&session.id)
            && self.background_tasks_of(&session.id).is_empty()
    }

    /// Whether `session`, its stored record, is working (`Some(true)`), idle (`Some(false)`), or has
    /// no process, for telling whether its account's sharing state changed.
    pub(crate) fn activity_of(&self, session: &Session) -> Option<bool> {
        self.live_session(&session.id)
            .map(|process| !self.is_idle_with(session, &process))
    }

    pub fn live_sessions(&self) -> Vec<Arc<LiveSession>> {
        self.live
            .read()
            .expect("live sessions lock poisoned")
            .sessions
            .values()
            .cloned()
            .collect()
    }

    /// Whether the session has a process right now, or its claim is held. This is the line
    /// archiving a console session draws, and it is not the session's status: an interrupted
    /// session has none and an archived one may still be exiting (its record says archived, so
    /// callers ask about the ones that are not).
    pub fn has_process(&self, id: &str) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.sessions.contains_key(id) || live.launching.contains(id)
    }

    /// Whether the session's claim is held (a launch, resume, switch, handoff, handover of branches or
    /// binding change is under way). Unlike [`Self::has_process`] this is false for a session whose process is up
    /// and whose claim is already released.
    pub fn is_launching(&self, id: &str) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.launching.contains(id)
    }

    /// The kind of claim held on the session, if any, read under one lock so a claim released
    /// between two reads cannot make one kind look like the other.
    pub fn claim_kind(&self, id: &str) -> Option<ClaimKind> {
        let live = self.live.read().expect("live sessions lock poisoned");
        if !live.launching.contains(id) {
            return None;
        }
        Some(if live.switching.contains(id) {
            ClaimKind::Switch
        } else {
            ClaimKind::Launch
        })
    }

    /// Whether the session is held by a launch claim proper, one for launching or resuming its
    /// process: in `launching` and not in `switching`, read under one lock so a claim released
    /// between two reads cannot make one kind look like the other.
    pub fn is_held_by_launch(&self, id: &str) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.launching.contains(id) && !live.switching.contains(id)
    }

    /// Whether any session matching is running or still being launched or resumed. Deleting a
    /// console or a project in that state is refused rather than silently killing them.
    pub fn has_live_sessions_where(&self, matches: impl Fn(&Session) -> bool) -> Result<bool> {
        for live in self.live_sessions() {
            if let Some(record) = self.store.get_session(&live.id)? {
                if matches(&record) {
                    return Ok(true);
                }
            }
        }
        self.has_launching_sessions_where(matches)
    }

    /// Whether any session holding a launch claim right now matches: being launched, resumed or
    /// switched, until its holder lets go. Its process may not be registered yet, or may be
    /// registered with the holder still judging it, so it cannot be relied on to stop; whatever it
    /// belongs to must not be deleted under it.
    pub fn has_launching_sessions_where(&self, matches: impl Fn(&Session) -> bool) -> Result<bool> {
        let launching: Vec<String> = self
            .live
            .read()
            .expect("live sessions lock poisoned")
            .launching
            .iter()
            .cloned()
            .collect();
        for id in launching {
            if let Some(record) = self.store.get_session(&id)? {
                if matches(&record) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub fn register_live(self: &Arc<Self>, session: Arc<LiveSession>) {
        session.enter_crash_cleanup();
        let mut live = self.live.write().expect("live sessions lock poisoned");
        live.sessions.insert(session.id.clone(), session.clone());
        drop(live);
        session.observe_titles(self);
    }

    /// The live lock keeps registration/relaunch from replacing the process between checking its
    /// identity and storing its title. Only this column is written: status and user names may move
    /// concurrently. A deleted record is never recreated. An agent re-sends its title as often as
    /// its spinner turns, so the title is always stored but published only when it changes what
    /// clients are shown. Returns whether a visible action prompt has just been answered, so
    /// the observer can release queued messages without writing on the PTY reader thread.
    pub(crate) fn record_terminal_title(&self, session: &LiveSession, title: Option<&str>) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        if !live
            .sessions
            .get(&session.id)
            .is_some_and(|current| std::ptr::eq(current.as_ref(), session))
        {
            return false;
        }
        let names = std::cell::OnceCell::new();
        let before = match self.store.get_session(&session.id) {
            Ok(Some(record)) => self.resolved_title(&record, &names),
            Ok(None) => return false,
            Err(err) => {
                tracing::warn!(session = %session.id, %err, "reading the session for its terminal title failed");
                return false;
            }
        };
        let action_changed = session.observe_action_title(title);
        let stored = self.store.set_terminal_title(&session.id, title);
        drop(live);
        match stored {
            Ok(Some(record))
                if action_changed || self.resolved_title(&record, &names) != before =>
            {
                self.publish_session(&record)
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(session = %session.id, %err, "storing the terminal title failed")
            }
        }
        action_changed && !session.needs_user_action()
    }

    /// The title a client is shown for the session. `names` holds the project's names once asked.
    fn resolved_title(
        &self,
        session: &Session,
        names: &std::cell::OnceCell<Vec<String>>,
    ) -> Option<String> {
        crate::session_title::resolve(
            session.agent,
            session.terminal_title.as_deref(),
            &session.record_titles,
            || {
                names
                    .get_or_init(|| {
                        let path = match &session.project_id {
                            Some(project) => self
                                .store
                                .get_project(project)
                                .ok()
                                .flatten()
                                .map(|p| p.path),
                            None => self
                                .store
                                .get_console(&session.console_id)
                                .ok()
                                .flatten()
                                .map(|console| console.workdir),
                        };
                        path.map(|path| {
                            crate::session_title::project_names(std::path::Path::new(&path))
                        })
                        .unwrap_or_default()
                    })
                    .clone()
            },
        )
    }

    /// The name the session list shows for the session: its task title and created name joined by
    /// a fullwidth bar, or the created name alone when there is no task title or it repeats the
    /// name. Clients compose the same from the record they receive; see "Displayed names" in
    /// docs/product/session-titles.md.
    pub(crate) fn display_title(&self, session: &Session) -> String {
        match self.resolved_title(session, &std::cell::OnceCell::new()) {
            Some(task) if !task.trim().is_empty() && task.trim() != session.title.trim() => {
                format!("{}｜{}", task.trim(), session.title)
            }
            _ => session.title.clone(),
        }
    }

    /// The status a client is shown for a session: the stored one, or waiting for the user while
    /// its running process is held up by something only the user can answer that no hook reports,
    /// or starting while a change of its account's sharing has it ended and is to resume it.
    pub(crate) fn shown_status(&self, session: &Session) -> SessionStatus {
        let waiting = !session.status.is_dormant()
            && self
                .live_session(&session.id)
                .is_some_and(|live| live.waits_for_user());
        if waiting {
            SessionStatus::WaitingUser
        } else if session.status == SessionStatus::Interrupted && self.is_restarting(&session.id) {
            // Ended by a change of its account's sharing, which resumes it once it has ended.
            SessionStatus::Starting
        } else {
            session.status
        }
    }

    /// The session as a client receives it, with its resolved title and live user interactions.
    /// These leave the stored agent status intact so answering restores it even before a hook.
    pub(crate) fn wire_session(&self, mut session: Session) -> Session {
        session.status = self.shown_status(&session);
        session.background_tasks = self.shown_background_tasks(&session);
        session.switching_account = self.is_switching_account(&session.id);
        session.default_effort = self
            .default_efforts
            .lock()
            .expect("default efforts lock poisoned")
            .get(&session.id)
            .cloned();
        session.terminal_title = self.resolved_title(&session, &std::cell::OnceCell::new());
        session
    }

    /// Applies what a hook event says about the session's background tasks, and tells every client
    /// when the list changed while it is shown. A subagent's stop that leaves the list empty only
    /// takes effect after [`EMPTY_TASKS_DELAY`], unless something else changes the list by then.
    ///
    /// A subagent's start or stop counts only while the session is idle: inside a turn the turn's
    /// own `Stop` lists everything anyway, and a turn cancelled without a `Stop` would leave a
    /// foreground subagent in the list.
    pub fn apply_background_tasks_change(
        self: &Arc<Self>,
        id: &str,
        change: BackgroundTasksChange,
    ) {
        if matches!(
            change,
            BackgroundTasksChange::SubagentStarted(_)
                | BackgroundTasksChange::SubagentStopped { .. }
        ) && !self.is_idle(id)
        {
            return;
        }
        let (changed, deferred) = {
            let mut all = self
                .background_tasks
                .lock()
                .expect("background tasks lock poisoned");
            let work = all.entry(id.to_string()).or_default();
            let before = work.tasks.clone();
            let ticket = self
                .next_tasks_ticket
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let deferred = work.apply(change, tokio::time::Instant::now(), ticket);
            let changed = work.tasks != before;
            if work.is_empty() {
                all.remove(id);
            }
            (changed, deferred)
        };
        if let Some(ticket) = deferred {
            let state = self.clone();
            let id = id.to_string();
            tokio::spawn(async move {
                tokio::time::sleep(EMPTY_TASKS_DELAY).await;
                state.empty_background_tasks_if_pending(&id, ticket);
            });
        }
        if changed {
            self.publish_if_idle(id);
        }
    }

    /// Empties the list once a subagent's stop has stood, unless something took its ticket back.
    fn empty_background_tasks_if_pending(self: &Arc<Self>, id: &str, ticket: u64) {
        let changed = {
            let mut all = self
                .background_tasks
                .lock()
                .expect("background tasks lock poisoned");
            let changed = all.get_mut(id).is_some_and(|work| work.expire(ticket));
            if all.get(id).is_some_and(BackgroundWork::is_empty) {
                all.remove(id);
            }
            changed
        };
        if changed {
            self.publish_if_idle(id);
        }
    }

    /// Whether the session's agent is idle: the record's status, except that a session request
    /// holds the record at waiting for the user while the agent stays at what it was.
    fn is_idle(&self, id: &str) -> bool {
        let Ok(Some(session)) = self.store.get_session(id) else {
            return false;
        };
        let hand = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned")
            .get(id)
            .copied();
        hand.unwrap_or(session.status) == SessionStatus::Idle
    }

    fn publish_if_idle(&self, id: &str) {
        match self.store.get_session(id) {
            Ok(Some(session)) if session.status == SessionStatus::Idle => {
                self.publish_session(&session)
            }
            Ok(_) => {}
            Err(err) => {
                tracing::debug!(session = %id, %err, "reading the session for its background tasks failed")
            }
        }
    }

    /// Records the effort the session's agent runs it at by itself, and says whether that changed
    /// what a client is shown; the caller publishes the session when it did.
    pub fn set_default_effort(&self, id: &str, effort: Option<String>) -> bool {
        let mut all = self
            .default_efforts
            .lock()
            .expect("default efforts lock poisoned");
        let changed = all.get(id) != effort.as_ref();
        match effort {
            Some(effort) => all.insert(id.to_string(), effort),
            None => all.remove(id),
        };
        changed
    }

    /// Forgets the default effort of every session not in `ids`, the sessions that exist.
    pub fn retain_default_efforts(&self, ids: &HashSet<&str>) {
        self.default_efforts
            .lock()
            .expect("default efforts lock poisoned")
            .retain(|id, _| ids.contains(id.as_str()));
    }

    /// The background tasks a client is shown for the session: its list while it is idle, where
    /// work still running is told apart from nothing left, and none for any other status.
    pub(crate) fn shown_background_tasks(&self, session: &Session) -> Vec<BackgroundTask> {
        if session.status == SessionStatus::Idle {
            self.background_tasks_of(&session.id)
        } else {
            Vec::new()
        }
    }

    /// The background work the agent last listed for the session; empty when none, or when the
    /// session has not been seen to leave any. Whether it is worth showing depends on the status:
    /// see [`Self::shown_background_tasks`].
    pub fn background_tasks_of(&self, id: &str) -> Vec<BackgroundTask> {
        self.background_tasks
            .lock()
            .expect("background tasks lock poisoned")
            .get(id)
            .map(|work| work.tasks.clone())
            .unwrap_or_default()
    }

    /// Drops a session from the live map — but only if it is still *this* session. Comparing by id
    /// would let a finished session's exit watcher evict the live successor that reused its id.
    fn unregister_live(self: &Arc<Self>, session: &Arc<LiveSession>) {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        let is_current = live
            .sessions
            .get(&session.id)
            .is_some_and(|current| Arc::ptr_eq(current, session));
        if is_current {
            live.sessions.remove(&session.id);
            // Under the same lock as the removal: a relaunch claiming this id the moment it leaves
            // the map would otherwise queue an instruction that this cleanup then threw away.
            // A held queue is its holder's to forward or discard.
            let dropped = if self.outbox.is_held(&session.id) {
                0
            } else {
                self.outbox.forget(&session.id)
            };
            if dropped > 0 {
                tracing::warn!(session = %session.id, dropped, "the session ended with messages still queued");
            }
        }
        drop(live);
        if is_current {
            // Its call has nobody left to return to.
            crate::session_request::withdraw_for_session(self, &session.id);
            crate::shared_config::publish(self);
        }
        self.forget_session_bookkeeping(&session.id);
    }

    /// Reads a session record, failing with a message the UI can show when it is gone.
    pub fn session_record(&self, id: &str) -> Result<Session> {
        self.store
            .get_session(id)?
            .ok_or_else(|| CodedError::unknown_session(id))
    }

    pub fn publish_session(&self, session: &Session) {
        self.publish_session_by_id(&session.id);
    }

    /// Publishes the session the store holds under `id`, as [`Self::publish_session`] does.
    pub fn publish_session_by_id(&self, id: &str) {
        {
            let _publication = self
                .session_publication
                .lock()
                .expect("session publication lock poisoned");
            // Partial store writes preserve columns owned by other paths. Always publish the
            // stored row so a previously read record cannot blank a terminal title learned in the
            // meantime.
            match self.store.get_session(id) {
                Ok(Some(session)) => self.broadcast_locked(Event::SessionUpserted {
                    session: self.wire_session(session),
                }),
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(session = %id, %err, "reading the session for publication failed")
                }
            }
        }
        if let Ok(Some(session)) = self.store.get_session(id) {
            crate::shared_config::note_session(self, &session);
        }
    }

    /// Whether a change of sharing has claimed `id` to restart it.
    fn is_restarting(&self, id: &str) -> bool {
        self.live
            .read()
            .expect("live sessions lock poisoned")
            .sharing_locks
            .values()
            .any(|entry| entry.restarting.contains(id))
    }

    /// Writes a session record and tells every client about it. A record deleted in the meantime
    /// is not written and not announced: publishing it would leave a ghost row in every client.
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let live = session
            .status
            .is_dormant()
            .then(|| self.live_session(&session.id))
            .flatten();
        // Titles and new control snapshots can publish the row independently, so freeze before
        // the database exposes dormancy, and undo that freeze if the write fails.
        let update = || self.store.update_session(session);
        let updated = match live {
            Some(live) => live.freeze_output_for(update)?,
            None => update()?,
        };
        if updated {
            self.publish_session(session);
        }
        Ok(())
    }

    /// Records the account a session runs under and its directory, and tells every client. A
    /// record deleted in the meantime is not written and not announced, as with [`Self::save_session`].
    pub fn set_session_account(
        &self,
        id: &str,
        account_id: Option<&str>,
        config_dir: Option<&str>,
    ) -> Result<()> {
        if let Some(session) = self.store.set_session_account(id, account_id, config_dir)? {
            self.publish_session(&session);
            crate::shared_config::publish(self);
        }
        Ok(())
    }

    /// Deletes the session's record and its saved output if it is archived, and broadcasts
    /// `session_deleted` when it was. `false` means it was not archived, is gone already, or is
    /// being launched right now (a resume in flight). Checked and deleted under the live-sessions
    /// lock (`live`), so a resume cannot claim the session between the two. An archived session
    /// whose process is still exiting is deleted: its exit watcher finds no record and skips.
    pub fn delete_if_archived(&self, id: &str) -> Result<bool> {
        let deletion = self.delete_where(id, |store| store.delete_session_if_archived(id))?;
        Ok(matches!(deletion, Deletion::Deleted { .. }))
    }

    /// Deletes the session's record and its saved output if it is unused (see
    /// `Store::delete_session_if_unused`), and broadcasts `session_deleted` when it was. It is
    /// refused when the session is in use, is gone already, or is being launched right now. The
    /// returned session carries the native identity captured atomically with the deletion.
    pub fn delete_if_unused(&self, id: &str) -> Result<Deletion> {
        self.delete_where(id, |store| store.delete_session_if_unused(id))
    }

    fn delete_where(
        &self,
        id: &str,
        delete: impl FnOnce(&Store) -> Result<Option<Session>>,
    ) -> Result<Deletion> {
        let live = self.live.read().expect("live sessions lock poisoned");
        if live.launching.contains(id) {
            return Ok(Deletion::Refused);
        }
        let Some(session) = delete(&self.store)? else {
            return Ok(Deletion::Refused);
        };
        let process = live.sessions.get(id).cloned();
        drop(live);
        self.forget_deleted(id);
        Ok(Deletion::Deleted {
            process,
            session: Box::new(session),
        })
    }

    /// Deletes the record of a session whose launch failed and tells every client it is gone. The
    /// launch claim is the caller's to hold until this has returned, so nothing else acts on the
    /// session meanwhile. The record goes whatever is bound to it, which is why a session still
    /// starting is never accepted as an owner (`coordinator::resolve_bound_to`): nothing of it ran,
    /// so a conversation that binding meant to keep never existed.
    pub fn discard_session(&self, id: &str) -> Result<()> {
        self.store.delete_session(id)?;
        self.forget_deleted(id);
        Ok(())
    }

    /// What follows a session's record being deleted: dropping what the daemon holds for it and
    /// telling every client.
    fn forget_deleted(&self, id: &str) {
        self.forget_saved_output(id);
        crate::shared_config::forget_session(self, id);
        self.forget_handoff_package(id);
        self.forget_request_hand(id);
        self.broadcast(Event::SessionDeleted {
            session: id.to_string(),
        });
    }

    /// Applies a status reported by a hook, unless a waiting session request is holding the
    /// hand up: then the status is only remembered, as what the agent's events say, and the record
    /// keeps `WaitingUser`. Dormant statuses are never reached this way — a session becomes
    /// archived by the user's request and interrupted by its process going away, both of which are
    /// decided here rather than by the agent.
    pub fn apply_hook_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        // Held across the write, so a request opening at the same moment cannot have its hand
        // lowered by a status read before it.
        let mut hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        let status = if let Some(remembered) = hands.get_mut(id) {
            *remembered = status;
            SessionStatus::WaitingUser
        } else {
            status
        };
        let mut session = self.session_record(id)?;
        if session.status.is_dormant() || session.status == status {
            return Ok(());
        }
        session.status = status;
        self.save_session(&session)
    }

    /// Raises the hand of a session whose request for a session has started waiting, remembering
    /// the status its agent is at. A session with a request already held keeps what it remembered:
    /// the request replacing another one is the same wait. A dormant record is left alone, as in
    /// [`Self::apply_hook_status`].
    pub fn raise_request_hand(&self, id: &str) -> Result<()> {
        let mut hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        let mut session = self.session_record(id)?;
        if session.status.is_dormant() {
            return Ok(());
        }
        hands.entry(id.to_string()).or_insert(session.status);
        if session.status != SessionStatus::WaitingUser {
            session.status = SessionStatus::WaitingUser;
            self.save_session(&session)?;
        }
        Ok(())
    }

    /// Lowers the hand [`Self::raise_request_hand`] raised, to the status the agent's events have
    /// left it at since, unless another request of the session is still waiting. Conditional on
    /// the hand still being up, so a session that has since been interrupted stays so.
    pub fn lower_request_hand(self: &Arc<Self>, id: &str) {
        let mut hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        if self.session_requests.is_waiting_for(id) {
            return;
        }
        let Some(remembered) = hands.remove(id) else {
            return;
        };
        if remembered != SessionStatus::WaitingUser {
            if let Err(err) =
                self.apply_reported_status_if(id, SessionStatus::WaitingUser, remembered)
            {
                tracing::debug!(session = %id, %err, "lowering the session request's hand failed");
                return;
            }
        }
        // Released only now: while it is held, an `agent_status` reader cannot see "no hand" while
        // the record still says `WaitingUser`.
        drop(hands);
        // A message queued while the hand was up is still queued, and no hook event is coming to
        // release it.
        self.spawn_flush_outbox(id, remembered);
    }

    /// Drops what [`Self::raise_request_hand`] remembered for a session that is going away.
    pub fn forget_request_hand(&self, id: &str) {
        self.request_hands
            .lock()
            .expect("request hands lock poisoned")
            .remove(id);
    }

    /// The status the session's agent is at, which is the record's except while a session
    /// request holds the hand up or a visible action prompt overrides the hooks. Whatever the
    /// daemon decides about the agent itself reads this; clients receive `wire_session`.
    pub fn agent_status(&self, session: &Session) -> SessionStatus {
        if !session.status.is_dormant()
            && self
                .live_session(&session.id)
                .is_some_and(|live| live.needs_user_action())
        {
            return SessionStatus::WaitingUser;
        }
        self.request_hands
            .lock()
            .expect("request hands lock poisoned")
            .get(&session.id)
            .copied()
            .unwrap_or(session.status)
    }

    /// [`Self::apply_reported_status_if`] for a reporter that watches the agent's own status: with
    /// a hand held and the agent remembered at `expected`, the new status replaces what is
    /// remembered and the record keeps the request's `WaitingUser`. Otherwise it is the record that
    /// is `expected` to be moved. Says whether anything moved.
    pub fn report_agent_status_if(
        &self,
        id: &str,
        expected: SessionStatus,
        status: SessionStatus,
    ) -> Result<bool> {
        // Held across the fall-through, for the reason [`Self::apply_hook_status`] holds it.
        let mut hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        match hands.get_mut(id) {
            Some(remembered) if *remembered == expected => {
                *remembered = status;
                Ok(true)
            }
            Some(_) => Ok(false),
            None => self.apply_reported_status_if(id, expected, status),
        }
    }

    /// Applies a status on behalf of a reporter whose conclusion only holds while the session is
    /// still `expected`, and says whether it moved. The check and the write are one step, so a
    /// conclusion that has gone stale cannot overwrite what moved the session on.
    ///
    /// A hook event needs none of this and cannot use it: it reports an event that happened, which
    /// is true whatever the session's status is, so it has no status to expect.
    pub fn apply_reported_status_if(
        &self,
        id: &str,
        expected: SessionStatus,
        status: SessionStatus,
    ) -> Result<bool> {
        // `expected` is never dormant — the statuses a reporter waits on all mean a process is
        // running — which is what lets the conditional write stand in for the dormancy guard
        // [`Self::apply_hook_status`] needs. `status` must not be dormant either: a session becomes
        // dormant together with its `ended_at`, and this writes the status column alone.
        debug_assert!(!expected.is_dormant());
        debug_assert!(!status.is_dormant());
        match self.store.update_session_status_if(id, expected, status)? {
            Some(session) => {
                self.publish_session(&session);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    // -- MCP tokens ----------------------------------------------------------

    /// Issues the token one session's MCP server authenticates with. A fresh one per launch: the
    /// previous process is gone, and a token that outlived it would let a stale child act on the
    /// session the id was reused for.
    pub fn issue_mcp_token(&self, session_id: &str) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let mut tokens = self.mcp_tokens.write().expect("mcp token lock poisoned");
        tokens.retain(|_, owner| owner != session_id);
        tokens.insert(token.clone(), session_id.to_string());
        token
    }

    /// Retires this session's token without waiting for its process to go away. Used where no
    /// process ever started, so nothing will ever observe the exit that would otherwise retire it.
    pub fn revoke_mcp_tokens(&self, session_id: &str) {
        self.mcp_tokens
            .write()
            .expect("mcp token lock poisoned")
            .retain(|_, owner| owner != session_id);
    }

    /// The session a token belongs to, which is how an MCP call is attributed. The caller never
    /// names its own session: a child that rewrote the argument would otherwise act on another.
    pub fn session_for_mcp_token(&self, token: &str) -> Option<String> {
        self.mcp_tokens
            .read()
            .expect("mcp token lock poisoned")
            .get(token)
            .cloned()
    }

    // -- turn bookkeeping ----------------------------------------------------

    /// A turn has begun, with `prompt` the text that started it where the agent reports one. This
    /// is also what re-arms the turn-end signal, so a session that reported in its previous turn is
    /// not credited for this one.
    ///
    /// A credit given ahead of it covers this turn only if `prompt` opens with the credited text,
    /// and otherwise waits for the turn that does: the next turn to start is not always the one
    /// credited, since a backgrounded tool call's result arrives as a turn of its own and nothing
    /// orders it against a message written into the session. With no prompt to go by, the oldest
    /// credit is spent by this turn.
    pub fn turn_started(&self, id: &str, prompt: Option<&str>) {
        let mut turns = self.turns.lock().expect("turn lock poisoned");
        let turn = turns.entry(id.to_string()).or_default();
        turn.open = true;
        let credit = match prompt {
            Some(prompt) => turn
                .credits
                .iter()
                .position(|opening| prompt.starts_with(opening.as_str())),
            None => (!turn.credits.is_empty()).then_some(0),
        };
        turn.reported = credit.is_some();
        if let Some(credit) = credit {
            turn.credits.remove(credit);
        }
        turn.last_event_at = now_millis();
    }

    /// Whether a clock-attributed turn end would be consumed as an echo of an earlier ending rather
    /// than acted on. Nothing in the daemon has to ask — `close_turn` decides it — but the rule is
    /// subtle enough that its test says so directly rather than inferring it.
    #[cfg(test)]
    pub fn echo_pending(&self, id: &str) -> bool {
        self.turns
            .lock()
            .expect("turn lock poisoned")
            .get(id)
            .is_some_and(|turn| turn.echo_pending)
    }

    /// This session is still working on its open turn. Anything that is not a turn ending counts,
    /// and the point is only the clock: a turn producing events cannot be the one a backstop that
    /// fires a minute after a turn ends is talking about.
    pub fn touch_turn(&self, id: &str) {
        if let Some(turn) = self.turns.lock().expect("turn lock poisoned").get_mut(id) {
            turn.last_event_at = now_millis();
        }
    }

    /// The turn ended without owing anything — the user cancelled it. Closing it matters even
    /// though no report follows: left open, a later clock-attributed end would find it and invent
    /// one.
    pub fn abandon_turn(&self, id: &str) {
        if let Some(turn) = self.turns.lock().expect("turn lock poisoned").get_mut(id) {
            turn.open = false;
            turn.reported = false;
            turn.last_event_at = now_millis();
            // Grok's backstop fires after a cancelled turn too.
            turn.echo_pending = true;
        }
    }

    /// The session reported during its current turn, so its stop needs nothing synthesised.
    pub fn mark_reported(&self, id: &str) {
        self.set_reported(id, true);
    }

    /// Credits as reported the turn something of Octoboard's own will start, one the session has
    /// nothing to report back on and whose prompt opens with `opening`. The credit is spent by that
    /// turn's start (see `turn_started`) and otherwise goes with the session's bookkeeping when its
    /// process ends. Crediting the same turn again changes nothing.
    pub fn credit_turn_opened_by(&self, id: &str, opening: String) {
        let mut turns = self.turns.lock().expect("turn lock poisoned");
        let credits = &mut turns.entry(id.to_string()).or_default().credits;
        if !credits.contains(&opening) {
            credits.push(opening);
        }
    }

    /// Takes the credit for the turn opening with `opening` back, for something that will not
    /// start one after all.
    pub fn clear_turn_credit(&self, id: &str, opening: &str) {
        if let Some(turn) = self.turns.lock().expect("turn lock poisoned").get_mut(id) {
            turn.credits.retain(|credit| credit != opening);
        }
    }

    /// Takes that credit back, for a report that turned out not to reach its owner. Left set, the
    /// session's stop would produce no synthesised report either and the owner would get nothing.
    pub fn clear_reported(&self, id: &str) {
        self.set_reported(id, false);
    }

    fn set_reported(&self, id: &str, reported: bool) {
        self.turns
            .lock()
            .expect("turn lock poisoned")
            .entry(id.to_string())
            .or_default()
            .reported = reported;
    }

    /// Closes the open turn and says what that amounted to. `clock_attributed` marks a signal that
    /// names no turn of its own, which may only be read as ending a turn that has already gone
    /// quiet.
    pub fn close_turn(&self, id: &str, clock_attributed: bool) -> TurnClose {
        let mut turns = self.turns.lock().expect("turn lock poisoned");
        let Some(turn) = turns.get_mut(id) else {
            return TurnClose::NoTurn;
        };
        if !turn.open {
            // Each ending arms exactly one echo, and this spends it whether or not a turn is open —
            // which is the usual case: the echo arrives about a minute later, and a session that has
            // not been given more work is still idle by then. Left armed, it would be spent on the
            // next turn instead, and that turn might be one Grok reports no stop event for at all,
            // whose echo is its only turn-end signal.
            if clock_attributed {
                turn.echo_pending = false;
            }
            return TurnClose::NoTurn;
        }
        if clock_attributed {
            // An ending that named its own turn has already been acted on, and this is its echo.
            if turn.echo_pending {
                turn.echo_pending = false;
                return TurnClose::NotThisTurn;
            }
            if now_millis().saturating_sub(turn.last_event_at)
                < crate::hooks::BACKSTOP_QUIET.as_millis() as i64
            {
                return TurnClose::NotThisTurn;
            }
        }
        let reported = turn.reported;
        turn.open = false;
        turn.reported = false;
        turn.last_event_at = now_millis();
        // Only a signal that named its own turn leaves an echo behind; the echo itself does not.
        turn.echo_pending = !clock_attributed;
        if reported {
            TurnClose::Reported
        } else {
            TurnClose::OwesReport
        }
    }

    // -- the write queue -----------------------------------------------------

    /// Accepts a message for a session, to be written as soon as it can take one. Queued rather
    /// than written even when the session looks ready, so order is structural rather than something
    /// each caller has to get right.
    pub fn queue_message(&self, id: &str, text: &str) -> Ticket {
        let ticket = self.outbox.push(id, text);
        self.note_activity(id);
        ticket
    }

    /// [`Self::queue_message`] for a session a change of its account's sharing is restarting, and
    /// `None` for any other. The check and the push are one step under the `live` lock, which
    /// [`AccountSharingLock::end_restart`] takes to let the session go before it ends the hold, so
    /// a message is never pushed after the hold has dropped what it held.
    pub(crate) fn queue_if_restarting(&self, id: &str, text: &str) -> Option<Ticket> {
        let ticket = {
            let live = self.live.read().expect("live sessions lock poisoned");
            live.sharing_locks
                .values()
                .any(|entry| entry.restarting.contains(id))
                .then(|| self.outbox.push(id, text))
        }?;
        // After the lock: it reads the live sessions again.
        self.note_activity(id);
        Some(ticket)
    }

    /// Tells the session's account a message queued, written or left unsent may have changed
    /// whether the session is working.
    fn note_activity(&self, id: &str) {
        if let Ok(Some(session)) = self.store.get_session(id) {
            crate::shared_config::note_session(self, &session);
        }
    }

    /// Writes out whatever is queued for this session, and says what became of it.
    ///
    /// **Blocks** on the PTY write and on the wait for an idle Claude Code session to send what was
    /// written — twice over for a message a dialog discarded, which is written again in the same
    /// call — so callers on an async path go through [`Self::spawn_flush_outbox`].
    pub fn flush_outbox(&self, id: &str, status: SessionStatus) -> Drain {
        self.flush_outbox_once(id, status, true)
    }

    /// [`Self::flush_outbox`]; `again` says whether a message a dialog discarded may be written
    /// again here, which is allowed once per call so that a dialog every write brings back cannot
    /// keep the caller waiting.
    fn flush_outbox_once(&self, id: &str, status: SessionStatus, again: bool) -> Drain {
        // Each check below leaves the queue pending, and nothing retries a pending queue by itself:
        // whatever lifts a hold — a hook, a screen watch seeing the screen go, a timer, the person
        // answering in the terminal — has to call for a drain on every path it can lift by, and a
        // hold must never wait on an event that may not come. The same goes for a message put back
        // on the queue while no drain is running.
        //
        // A write while a modal dialog is up has its trailing Enter confirm whatever option is
        // highlighted. Check both the caller's hook state and the live interaction below.
        if !matches!(status, SessionStatus::Working | SessionStatus::Idle) {
            return Drain::Pending;
        }
        // And never a session on its way out. Archiving records the status before the process is
        // gone, so a hook from the dying process can arrive reporting work while the live entry it
        // would be drained through is still registered. Nothing is waiting on this answer: a sender
        // is refused before it ever queues for a dormant session.
        match self.store.get_session(id) {
            Ok(Some(record)) if !record.status.is_dormant() => {}
            _ => return Drain::Pending,
        }
        let Some(live) = self.live_session(id) else {
            return Drain::Pending;
        };
        // Nor while the agent may be showing its trust screen, which the paste's trailing Enter
        // would accept (Codex, Grok Build) or decline (Claude Code); see `crate::trust`. The press
        // that answers it, its first hook, the end of the watch or, for an agent whose hook may
        // never come, its fallback releases the queue.
        if live.trust.holds_writes() || live.needs_user_action() {
            return Drain::Pending;
        }
        // Nor while it shows a dialog of its own, looked for now rather than taken from the watch,
        // which polls: a dialog that came up since its last look would be answered by the paste.
        // The drain looks again before each message.
        if live.look_for_dialog().current.is_some() {
            return Drain::Pending;
        }
        // Nor just after an error the agent stopped on, which can leave a prompt that no hook
        // announces and the screen may not show yet; for `AGENT_ERROR_SETTLE`, or until a prompt
        // is sent.
        if live.agent_error() {
            return Drain::Pending;
        }
        let outcome = self.outbox.drain(
            &live,
            &HookPrompts {
                state: self,
                live: &live,
            },
        );
        let still_running = self
            .live_session(id)
            .is_some_and(|current| Arc::ptr_eq(&current, &live));
        if matches!(outcome, Drain::Unsent(_)) && still_running {
            // The sender may be told, but only the user can see the input box and send what is in
            // it; and whoever sent a message queued behind it is told nothing at all.
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_NOT_SENT,
                    format!(
                        "{} wrote a message into this session, but {} did not send it: it is \
                         waiting in the session's input box. Check it and send it; nothing more \
                         is written into the session until a prompt is sent.",
                        crate::APP_NAME,
                        live.agent.label()
                    ),
                    &[("agent", live.agent.label())],
                )
                .about(id),
            );
        }
        if matches!(outcome, Drain::Diverted(_)) && still_running {
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_DIVERTED_BY_VIEW,
                    format!(
                        "{} was showing a view of its background agents while {} wrote a message \
                         into this session, so the message went there rather than to the \
                         conversation. Check the session.",
                        live.agent.label(),
                        crate::APP_NAME
                    ),
                    &[("agent", live.agent.label())],
                )
                .about(id),
            );
        }
        if matches!(outcome, Drain::Discarded(_)) && still_running {
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_DISCARDED_BY_DIALOG,
                    format!(
                        "{} showed a dialog while {} wrote a message into this session and \
                         discarded the message. {} will write it again once the dialog has been \
                         answered.",
                        live.agent.label(),
                        crate::APP_NAME,
                        crate::APP_NAME
                    ),
                    &[("agent", live.agent.label())],
                )
                .about(id),
            );
            // The dialog has usually been answered by the message's own Enter, and taken down,
            // within the write, and answering it runs no hook: with no drain under way any more
            // and no dialog left for the watch to see go, nothing else would write the message
            // again. One still up is left to the watch, which drains when it goes. What the second
            // drain came to is what the sender is told, except a pending one, which does not say
            // what was written: the first outcome still holds for whatever went in before the
            // discarded message. A second discard by a dialog gone before anything looks waits for
            // the next drain.
            if again && live.look_for_dialog().current.is_none() {
                if let Ok(record) = self.session_record(id) {
                    return match self.flush_outbox_once(id, self.agent_status(&record), false) {
                        Drain::Pending => outcome,
                        other => other,
                    };
                }
            }
        }
        if outcome == Drain::Lost {
            // Nobody is waiting on a return value for most of these drains, and a message Octoboard
            // accepted and then could not deliver is the user's business — the more so because the
            // session's input line is the thing left in a state only they can see.
            self.broadcast(
                Notice::new(
                    notice_code::QUEUED_MESSAGES_DROPPED,
                    format!(
                        "{} dropped what it had queued for this session: {}",
                        crate::APP_NAME,
                        crate::term::FRAGMENT_HAZARD
                    ),
                    &[],
                )
                .about(id),
            );
        }
        self.note_activity(id);
        outcome
    }

    /// Drains this session's queue off the runtime. Used wherever releasing a message is a side
    /// effect of something else — a status change reported by a hook — and the caller must not wait
    /// for a PTY write: every agent gives its hooks a few seconds and renders a timed-out one to
    /// the user.
    pub fn spawn_flush_outbox(self: &Arc<Self>, id: &str, status: SessionStatus) {
        let state = self.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || state.flush_outbox(&id, status));
    }

    /// Holds a session's queue for as long as the guard lives: messages for it are accepted and
    /// queued, none is written. A handoff holds the original's, so that nothing reaches a session
    /// whose work is being moved; see [`OutboxHold`].
    pub fn hold_outbox(self: &Arc<Self>, id: &str) -> OutboxHold {
        self.hold_outbox_for(id, false)
    }

    /// [`Self::hold_outbox`] for a session a change of its account's sharing restarts; the only
    /// difference is the notice for messages dropped when it is not relaunched.
    pub fn hold_outbox_for_sharing(self: &Arc<Self>, id: &str) -> OutboxHold {
        self.hold_outbox_for(id, true)
    }

    fn hold_outbox_for(self: &Arc<Self>, id: &str, for_sharing: bool) -> OutboxHold {
        self.outbox.hold(id, for_sharing);
        OutboxHold {
            state: self.clone(),
            id: id.to_string(),
            for_sharing,
            successor: std::sync::Mutex::new(None),
        }
    }

    /// Forgets what is queued for `id`, a session that will never take it, and says so as a
    /// `sharing_messages_dropped` notice when `for_sharing` or `handoff_messages_dropped` otherwise,
    /// when anything was queued.
    fn drop_held_queue(&self, id: &str, for_sharing: bool) {
        let dropped = self.outbox.forget(id);
        if dropped == 0 {
            return;
        }
        let (code, during, why) = if for_sharing {
            (
                notice_code::SHARING_MESSAGES_DROPPED,
                "while its account's sharing changed",
                "the session was not restarted",
            )
        } else {
            (
                notice_code::HANDOFF_MESSAGES_DROPPED,
                "during a handoff",
                "the session has no process to receive them",
            )
        };
        self.broadcast(
            Notice::new(
                code,
                format!(
                    "{} dropped {dropped} message(s) held back for this session {during}, \
                     because {why}",
                    crate::APP_NAME
                ),
                &[],
            )
            .about(id),
        );
    }

    /// Removes and returns what is queued for a session, oldest first.
    pub fn take_queued(&self, id: &str) -> Vec<String> {
        self.outbox.take(id)
    }

    /// How many messages are queued for a session and not yet written into it.
    pub fn queued_count(&self, id: &str) -> usize {
        self.outbox.len(id)
    }

    /// What keeps [`Self::flush_outbox`] from writing into a running session apart from the status
    /// a hook reported, which a caller reads off the record: a session can read as idle and still
    /// hold everything queued for it. `None` for one with no process running, or nothing holding
    /// it.
    pub fn write_hold(&self, id: &str) -> Option<WriteHold> {
        let live = self.live_session(id)?;
        if live.trust.holds_writes() {
            return Some(if live.trust.waiting() {
                WriteHold::TrustConfirmation
            } else {
                WriteHold::Starting
            });
        }
        if let Some(dialog) = live.look_for_dialog().current {
            Some(WriteHold::AgentDialog(dialog))
        } else if live.needs_user_action() {
            Some(WriteHold::AgentAction)
        } else if live.agent_error() {
            Some(WriteHold::AgentError)
        } else if self.outbox.is_held_for_sharing(id) {
            Some(WriteHold::SharingChange)
        } else if self.outbox.is_held(id) {
            Some(WriteHold::Handoff)
        } else if live.unsent_waiting() {
            Some(WriteHold::UnsentMessage)
        } else {
            None
        }
    }

    /// Throws away anything queued for a session that will never take it — a relaunch that failed,
    /// so nothing would ever release what was queued ahead of it.
    pub fn discard_outbox(&self, id: &str) {
        self.outbox.forget(id);
    }

    fn forget_session_bookkeeping(&self, id: &str) {
        self.revoke_mcp_tokens(id);
        self.turns.lock().expect("turn lock poisoned").remove(id);
        self.forget_request_hand(id);
        self.background_tasks
            .lock()
            .expect("background tasks lock poisoned")
            .remove(id);
        self.record_watch_generations
            .lock()
            .expect("record watch lock poisoned")
            .remove(id);
        self.record_offsets
            .lock()
            .expect("record offset lock poisoned")
            .remove(id);
    }

    // -- record watch bookkeeping ---------------------------------------------

    /// Starts a new record watch for this session and returns the generation it owns.
    /// Recording it here retires whatever watch was running for the session before: its generation
    /// is no longer the one found under this id, so it stops at its next poll instead of racing the
    /// new one. See `crate::record_watch::spawn`.
    pub fn begin_record_watch(&self, id: &str) -> u64 {
        let generation = self
            .next_record_watch_generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.record_watch_generations
            .lock()
            .expect("record watch lock poisoned")
            .insert(id.to_string(), generation);
        generation
    }

    /// Where a carried-offset watch of the record at `path` may start reading: past what an earlier
    /// watch of the same file already read, or zero for a file not seen before.
    pub fn record_offset(&self, id: &str, path: &std::path::Path) -> u64 {
        self.record_offsets
            .lock()
            .expect("record offset lock poisoned")
            .get(id)
            .filter(|(seen, _)| seen == path)
            .map_or(0, |(_, offset)| *offset)
    }

    /// Records how far a carried-offset watch has read into the record at `path`.
    pub fn note_record_offset(&self, id: &str, path: &std::path::Path, offset: u64) {
        self.record_offsets
            .lock()
            .expect("record offset lock poisoned")
            .insert(id.to_string(), (path.to_path_buf(), offset));
    }

    /// Retires the session's record watch, if any, because the turn it was watching is over: a
    /// new turn is about to open, and a watch left current could close that one instead.
    pub fn end_record_watch(&self, id: &str) {
        self.record_watch_generations
            .lock()
            .expect("record watch lock poisoned")
            .remove(id);
    }

    /// Whether `generation` is still this session's current record watch — false once a newer
    /// watch has taken over, or the session's bookkeeping has been dropped entirely.
    pub fn record_watch_current(&self, id: &str, generation: u64) -> bool {
        self.record_watch_generations
            .lock()
            .expect("record watch lock poisoned")
            .get(id)
            == Some(&generation)
    }

    /// Runs `write` only while `generation` is still this session's current record watch, and says
    /// what it returned, or `None` for a watch retired or superseded meanwhile. The bookkeeping
    /// stays locked across `write`, so a hook starting or ending a watch waits for it instead of
    /// slipping in between the check and the write. That lock is never taken while any other lock
    /// is held, so `write` may take any lock but this one; it must not start or end a watch.
    pub fn while_record_watch_current<R>(
        &self,
        id: &str,
        generation: u64,
        write: impl FnOnce() -> R,
    ) -> Option<R> {
        let generations = self
            .record_watch_generations
            .lock()
            .expect("record watch lock poisoned");
        (generations.get(id) == Some(&generation)).then(write)
    }

    /// Records that the session has had a turn, so a later resume reopens the stored conversation
    /// instead of failing on an id the agent never wrote.
    pub fn mark_conversation_started(&self, id: &str) -> Result<()> {
        let mut session = self.session_record(id)?;
        if session.has_conversation {
            return Ok(());
        }
        session.has_conversation = true;
        self.save_session(&session)
    }

    /// Forgets the conversation of a session whose agent no longer has it, so that its next launch
    /// opens a fresh one, as for a session that has had no turn. The new conversation's id is
    /// recorded by its first hook, as for any session.
    pub fn forget_conversation(&self, id: &str) -> Result<()> {
        let mut session = self.session_record(id)?;
        session.has_conversation = false;
        session.agent_session_id = None;
        self.save_session(&session)
    }

    pub fn set_agent_session_id(&self, id: &str, agent_session_id: &str) -> Result<()> {
        let mut session = self.session_record(id)?;
        if session.agent_session_id.as_deref() == Some(agent_session_id) {
            return Ok(());
        }
        session.agent_session_id = Some(agent_session_id.to_string());
        self.save_session(&session)
    }

    /// Watches one session's process and records where it ended up. A session whose status was
    /// already set to archived was ended on purpose; anything else that stops is interrupted, which
    /// is the status a click resumes from.
    pub fn watch_exit(self: &Arc<Self>, live: Arc<LiveSession>) {
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(EXIT_POLL_INTERVAL).await;
                if live.poll_exit() {
                    break;
                }
            }
            // Saved before the live entry goes, so a terminal client attaching meanwhile finds
            // either the process or what it left, never neither.
            let saving = {
                let (state, live) = (state.clone(), live.clone());
                tokio::task::spawn_blocking(move || state.save_output(&live))
            };
            let _ = saving.await;
            state.unregister_live(&live);
            match state.store.get_session(&live.id) {
                Ok(Some(mut session)) => {
                    if session.status != SessionStatus::Archived {
                        session.status = SessionStatus::Interrupted;
                    }
                    session.ended_at = Some(now_millis());
                    if let Err(err) = state.save_session(&session) {
                        tracing::warn!(session = %live.id, %err, "recording the session's exit failed");
                    }
                }
                // Deleted while its process was still exiting, which is allowed.
                Ok(None) => {
                    tracing::debug!(session = %live.id, "the session was deleted while exiting")
                }
                Err(err) => {
                    tracing::warn!(session = %live.id, %err, "reading the exited session failed")
                }
            }
        });
    }

    /// Ends every running session, leaving each one interrupted and resumable. Used by the exit
    /// flow and by a signal: the daemon owns the agent processes, so none of them may outlive it.
    pub async fn stop_all_sessions(self: &Arc<Self>) {
        let sessions = self.live_sessions();
        for live in &sessions {
            match self.session_record(&live.id) {
                Ok(mut session) => {
                    if session.status != SessionStatus::Archived {
                        session.status = SessionStatus::Interrupted;
                        session.ended_at = Some(now_millis());
                        if let Err(err) = self.save_session(&session) {
                            tracing::warn!(session = %live.id, %err, "marking the session interrupted failed");
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(session = %live.id, %err, "stopping a session with no record")
                }
            }
        }
        // `terminate` waits out the graceful period, so it goes to a blocking thread. The output is
        // saved here rather than left to each exit watcher, which the daemon may not outlive.
        let handles: Vec<_> = sessions
            .into_iter()
            .map(|live| {
                let state = self.clone();
                tokio::task::spawn_blocking(move || {
                    live.terminate();
                    state.save_output(&live);
                })
            })
            .collect();
        for handle in handles {
            let _ = handle.await;
        }
    }

    // -- saved output ------------------------------------------------------------

    /// Keeps the frozen or final parsed screen after exit. **Blocks** on the
    /// file and on the PTY reader (see `LiveSession::keep_final_output`).
    fn save_output(&self, live: &LiveSession) {
        live.keep_final_output(|output| {
            self.saved_output.write(&live.id, output);
            // A record deleted while its process was exiting may have had its file removed before
            // the write above. Looked at after writing, and the deletion removes the file after
            // the record, so whichever comes second leaves no file behind.
            if let Ok(None) = self.store.get_session(&live.id) {
                self.saved_output.remove(&live.id);
            }
        });
    }

    /// The terminal preview saved for this session, if one was kept.
    pub fn saved_output(&self, id: &str) -> Option<Vec<u8>> {
        self.saved_output.read(id)
    }

    /// Removes the saved output of every session the store has no record of, and any file left
    /// half-written. Run once at startup, before anything is served.
    pub fn sweep_saved_output(&self) -> Result<()> {
        let ids: std::collections::HashSet<String> = self
            .store
            .list_sessions()?
            .into_iter()
            .map(|session| session.id)
            .collect();
        self.saved_output.sweep(|id| ids.contains(id));
        Ok(())
    }

    /// Drops a session's saved output. Called after its record is deleted, never before: see
    /// [`Self::save_output`]. Also when a new process takes the session over, whose output replaces
    /// it.
    pub fn forget_saved_output(&self, id: &str) {
        self.saved_output.remove(id);
    }

    /// Removes the handoff package written for a session, which is named by the session's id. Called
    /// wherever a record is deleted; a session that was never a receiving one has none. A failure
    /// is logged: the record is gone either way.
    pub fn forget_handoff_package(&self, id: &str) {
        if let Err(err) = crate::handoff::remove(self, id) {
            tracing::warn!(session = %id, %err, "removing a handoff package failed");
        }
    }

    // -- git status ------------------------------------------------------------

    /// Claims the right to check this project's git status, for a check that waits for a place
    /// before it runs (a sweep's, or `git_status::pull_behind_projects`'s). Released by
    /// [`GitCheckClaim::release_unless_rerun_owed`] or, if the check panics, when the returned
    /// guard is dropped, so a wedged step cannot leave its project excluded from every later
    /// sweep for the rest of the daemon's life. `None` means a check for it is already running and
    /// the caller must not start another. The claim is held from the moment it is queued, so
    /// another sweep does not queue the project twice, and a manual pull that finds it still
    /// waiting takes it over instead of waiting behind the sweep (see [`GitCheckClaim::start`]).
    pub fn claim_git_check_queued(self: &Arc<Self>, project_id: &str) -> Option<GitCheckClaim> {
        self.claim_git_check_with(project_id, GitCheckRequest::Queued)
    }

    /// As [`Self::claim_git_check_queued`], for a manual pull. When a check is already running, nothing
    /// is claimed, but the running check's claim is marked as owing a forced rerun, which
    /// [`GitCheckClaim::release_unless_rerun_owed`] hands back to it. When the check is still
    /// queued, nothing is claimed either: the queued claim is marked and woken, and runs forced
    /// at once. The marking and the claim share one lock, so a request cannot slip in between the
    /// owner's last look and its release.
    pub fn claim_git_check_forced(self: &Arc<Self>, project_id: &str) -> Option<GitCheckClaim> {
        self.claim_git_check_with(project_id, GitCheckRequest::Forced)
    }

    fn claim_git_check_with(
        self: &Arc<Self>,
        project_id: &str,
        request: GitCheckRequest,
    ) -> Option<GitCheckClaim> {
        let mut checks = self.git_checks.lock().expect("git check lock poisoned");
        if let Some(check) = checks.get_mut(project_id) {
            if request == GitCheckRequest::Forced {
                check.rerun_owed = true;
                if check.queued {
                    check.promote.notify_one();
                }
            }
            return None;
        }
        let promote = Arc::new(tokio::sync::Notify::new());
        checks.insert(
            project_id.to_string(),
            GitCheck {
                queued: request == GitCheckRequest::Queued,
                rerun_owed: false,
                promote: promote.clone(),
            },
        );
        Some(GitCheckClaim {
            state: self.clone(),
            project_id: project_id.to_string(),
            promote,
            released: false,
        })
    }

    fn release_git_check(&self, project_id: &str) {
        self.git_checks
            .lock()
            .expect("git check lock poisoned")
            .remove(project_id);
    }

    /// Records a project's current git status and broadcasts it — unless the project is no longer
    /// in the store, in which case the entry is dropped instead of reinserted. This is what covers
    /// both a project removed while its check was in flight and a whole console deleted out from
    /// under one: either way nothing resurrects a status for a project that no longer exists, and
    /// `PROTOCOL.md` promises no `project_git_status` for one either.
    pub fn publish_git_status(&self, status: GitStatus) {
        // A store error says nothing about whether the project is actually gone, so this fails
        // open rather than risk dropping a status that is still perfectly valid.
        if let Ok(None) = self.store.get_project(&status.project) {
            self.remove_git_status(&status.project);
            return;
        }
        let mut statuses = self
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        statuses
            .statuses
            .insert(status.project.clone(), status.clone());
        self.broadcast(Event::ProjectGitStatus {
            status: statuses.visible(status),
        });
    }

    /// Records a local re-read of the branch header and broadcasts the result. Only what the read
    /// produced is merged into the held status, under the lock: a read that finished after an
    /// overlapping check or pull published must not bring back that check's older `error` or end
    /// its `activity`. A failed read records its own `error` and leaves the header as it was. As
    /// with [`Self::publish_git_status`], a project no longer in the store is dropped instead.
    pub(crate) fn publish_git_reread(
        &self,
        project_id: &str,
        header: Result<crate::git_status::BranchHeader, String>,
    ) {
        if let Ok(None) = self.store.get_project(project_id) {
            self.remove_git_status(project_id);
            return;
        }
        let mut statuses = self
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        let status = statuses
            .statuses
            .entry(project_id.to_string())
            .or_insert_with(|| blank_git_status(project_id));
        status.repository = true;
        match header {
            Ok(header) => crate::git_status::apply_header(status, header),
            Err(err) => status.error = Some(err),
        }
        let status = status.clone();
        self.broadcast(Event::ProjectGitStatus {
            status: statuses.visible(status),
        });
    }

    /// Every git status the daemon currently holds, for `Event::Snapshot`.
    pub fn git_statuses(&self) -> Vec<GitStatus> {
        let statuses = self
            .git_statuses
            .read()
            .expect("git statuses lock poisoned");
        statuses
            .statuses
            .values()
            .cloned()
            .map(|status| statuses.visible(status))
            .collect()
    }

    /// Marks one push in flight, without excluding another push or a background check. The
    /// guard restores the latest underlying check activity on every exit, including a failure.
    pub(crate) fn begin_git_push(&self, project_id: &str) -> GitPushActivity<'_> {
        if let Ok(None) = self.store.get_project(project_id) {
            return GitPushActivity {
                state: self,
                project_id: project_id.to_string(),
            };
        }
        let mut statuses = self
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        *statuses.pushes.entry(project_id.to_string()).or_default() += 1;
        let status = statuses
            .statuses
            .entry(project_id.to_string())
            .or_insert_with(|| blank_git_status(project_id));
        status.repository = true;
        let status = status.clone();
        self.broadcast(Event::ProjectGitStatus {
            status: statuses.visible(status),
        });
        GitPushActivity {
            state: self,
            project_id: project_id.to_string(),
        }
    }

    /// The underlying check status for one project, without the push overlay.
    pub fn git_status(&self, project_id: &str) -> Option<GitStatus> {
        self.git_statuses
            .read()
            .expect("git statuses lock poisoned")
            .statuses
            .get(project_id)
            .cloned()
    }

    /// Drops a project's git status. Called when the project itself is removed, and when a whole
    /// console is — nothing else ever cleans up these entries, since the daemon keeps no timer of
    /// its own that would otherwise notice a project is gone.
    pub fn remove_git_status(&self, project_id: &str) {
        let mut statuses = self
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        statuses.statuses.remove(project_id);
        statuses.pushes.remove(project_id);
        drop(statuses);
        self.git_check_completed
            .lock()
            .expect("git check completed lock poisoned")
            .remove(project_id);
    }

    /// When this project's check last completed, for `git_status::due_for_check`. `None` means it
    /// has never been checked, which that function always treats as due.
    pub fn git_check_completed_at(&self, project_id: &str) -> Option<Instant> {
        self.git_check_completed
            .lock()
            .expect("git check completed lock poisoned")
            .get(project_id)
            .copied()
    }

    /// Records that this project's check just completed, now.
    pub fn record_git_check_completed(&self, project_id: &str) {
        self.git_check_completed
            .lock()
            .expect("git check completed lock poisoned")
            .insert(project_id.to_string(), Instant::now());
    }

    // -- agent availability ---------------------------------------------------

    /// Every agent's current availability, for `Event::Snapshot` — always three entries, each
    /// `NotDetermined` until `crate::availability`'s one-time determination for this run lands.
    pub fn agent_availability(&self) -> Vec<AgentAvailability> {
        let map = self
            .agent_availability
            .read()
            .expect("agent availability lock poisoned");
        Agent::ALL
            .into_iter()
            .map(|agent| map[&agent].clone())
            .collect()
    }

    /// One agent's current availability, for the launch refusal in `coordinator::open_session`.
    pub fn agent_availability_of(&self, agent: Agent) -> AgentAvailability {
        self.agent_availability
            .read()
            .expect("agent availability lock poisoned")
            .get(&agent)
            .cloned()
            .expect("every agent has an entry from AppState::new")
    }

    /// Merges the given entries into every agent's availability and broadcasts the whole,
    /// ordered, three-entry result — never the argument itself, which may name only some agents.
    /// Called once per daemon start, by `crate::availability::spawn_determine` once its one-time
    /// snapshot lands — never again afterwards, since nothing here re-determines it during a run.
    pub fn set_agent_availability(&self, agent_availability: Vec<AgentAvailability>) {
        {
            let mut map = self
                .agent_availability
                .write()
                .expect("agent availability lock poisoned");
            for entry in agent_availability {
                map.insert(entry.agent, entry);
            }
        }
        self.broadcast(Event::AgentAvailabilityUpdated {
            agent_availability: self.agent_availability(),
        });
    }
}

#[derive(Default)]
struct LiveSessions {
    sessions: HashMap<String, Arc<LiveSession>>,
    launching: HashSet<String>,
    /// The sessions in `launching` whose claim came from [`AppState::begin_switch`], so is not a
    /// launch of their process.
    switching: HashSet<String>,
    /// The sessions in `switching` held by an account switch, which clients are shown
    /// (`Session::switching_account`).
    account_switches: HashSet<String>,
    /// The accounts locked for a change of their sharing, by id.
    sharing_locks: HashMap<String, SharingLockEntry>,
}

struct SharingLockEntry {
    /// The account as it was when locked.
    account: crate::protocol::Account,
    /// The sessions the change restarts, until each is resumed or let go.
    restarting: HashSet<String>,
}

/// A session running or holding a claim, as a change of sharing sees it.
pub struct Running {
    pub session: Session,
    /// Working, which refuses a change of its account's sharing: not idle (see
    /// [`AppState::activity_of`]), or claimed by anything but such a change.
    pub working: bool,
    /// Claimed by a change of its account's sharing, which restarts it.
    pub restarting: bool,
}

/// The account locked for a change of its sharing that `session` runs under, if any.
fn locked_over<'a>(live: &'a LiveSessions, session: &Session) -> Option<&'a SharingLockEntry> {
    live.sharing_locks
        .values()
        .find(|entry| crate::shared_config::runs_under(&entry.account, session))
}

/// An account locked for a change of its sharing (see [`AppState::lock_account_sharing`]), until
/// dropped, with the claims on the idle sessions the change ends and resumes: nothing else can
/// launch, resume, switch or delete them meanwhile, and a client is shown them `starting` rather
/// than interrupted while their process is down (see [`AppState::shown_status`]).
pub struct AccountSharingLock {
    state: Arc<AppState>,
    account: String,
    claims: HashMap<String, LaunchClaim>,
    /// The claimed sessions' queues, held from the claim until the session is let go: a message
    /// sent to one meanwhile is not written into a process about to be ended, nor forgotten when
    /// it ends, and is left for the resumed process (see [`Self::release_queue_for_relaunch`]), or
    /// dropped with a notice for a session not resumed (see [`OutboxHold`]).
    holds: HashMap<String, OutboxHold>,
}

impl AccountSharingLock {
    /// Claims the sessions `choose` names, idle ones of the account, for the change to restart, in
    /// one step with `choose`'s look at the sessions running or holding a claim, which refuses by
    /// answering an error.
    pub fn claim_idle(
        &mut self,
        choose: impl FnOnce(&[Running]) -> Result<Vec<String>>,
    ) -> Result<()> {
        let mut live = self
            .state
            .live
            .write()
            .expect("live sessions lock poisoned");
        let running = self.state.running_of(&live)?;
        let ids = choose(&running)?;
        let mut claimed = HashSet::new();
        for id in ids {
            // Not claimed already: a claimed session is never idle.
            if live.launching.insert(id.clone()) {
                claimed.insert(id.clone());
                self.holds
                    .insert(id.clone(), self.state.hold_outbox_for_sharing(&id));
                self.claims.insert(
                    id.clone(),
                    LaunchClaim {
                        state: self.state.clone(),
                        id,
                    },
                );
            }
        }
        if let Some(entry) = live.sharing_locks.get_mut(&self.account) {
            entry.restarting.extend(claimed);
        }
        Ok(())
    }

    /// The sessions the change restarts that are still claimed for it.
    pub fn restarting(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.claims.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// The claim on `id`, one of [`Self::restarting`].
    pub fn claim(&self, id: &str) -> Option<&LaunchClaim> {
        self.claims.get(id)
    }

    /// Ends the hold on `id`'s queue ahead of its relaunch, leaving what is queued for the launch's
    /// own triggers to write once the agent can take it (its first hook, its trust screen
    /// released), as a resume does: the session is still claimed and dormant, which the hold's end
    /// keeps a queue for.
    pub fn release_queue_for_relaunch(&mut self, id: &str) {
        if let Some(hold) = self.holds.remove(id) {
            hold.forward_to(id);
        }
    }

    /// Lets `id` go: it is no longer restarted, and every client is told its own status again.
    pub fn end_restart(&mut self, id: &str) {
        if let Some(entry) = self
            .state
            .live
            .write()
            .expect("live sessions lock poisoned")
            .sharing_locks
            .get_mut(&self.account)
        {
            entry.restarting.remove(id);
        }
        drop(self.claims.remove(id));
        drop(self.holds.remove(id));
        // A message queued after the queue was handed to the launch and before the session was let
        // go has had no drain to write it, its hooks may all have come already, or, when the launch
        // failed, nobody to drop it.
        if self.state.queued_count(id) > 0 {
            if self.state.has_process(id) {
                if let (Ok(Some(session)), Ok(_)) = (
                    self.state.store.get_session(id),
                    tokio::runtime::Handle::try_current(),
                ) {
                    self.state
                        .spawn_flush_outbox(id, self.state.agent_status(&session));
                }
            } else if !self.state.is_launching(id) {
                self.state.drop_held_queue(id, true);
            }
        }
        if let Ok(Some(session)) = self.state.store.get_session(id) {
            self.state.publish_session(&session);
        }
    }
}

impl Drop for AccountSharingLock {
    fn drop(&mut self) {
        for id in self.restarting() {
            self.end_restart(&id);
        }
        self.state
            .live
            .write()
            .expect("live sessions lock poisoned")
            .sharing_locks
            .remove(&self.account);
    }
}

/// What a delete of a session came to.
pub enum Deletion {
    Refused,
    /// The record is gone. `process` is the session's process, read under the lock that keeps a
    /// resume from registering a new one, for the caller to end. `session` is the row returned by
    /// the delete statement, including native identity changes made since an earlier read.
    Deleted {
        process: Option<Arc<LiveSession>>,
        session: Box<Session>,
    },
}

/// Keeps lock changes out of a removal's preflight and mutations without holding a mutex
/// across awaits. Nested removals each keep their own claim until their caller has finished.
pub struct SessionRemovalClaim {
    state: Arc<AppState>,
}

impl Drop for SessionRemovalClaim {
    fn drop(&mut self) {
        *self.state.session_removals.lock().unwrap() -= 1;
    }
}

/// The exclusive claim on one session — to start a process for it, or (from
/// [`AppState::begin_switch`]) to act on it while its process runs — released on drop, and only
/// then: it outlives `register_live`, so the holder decides how much of what follows it covers.
pub struct LaunchClaim {
    state: Arc<AppState>,
    id: String,
}

impl Drop for LaunchClaim {
    fn drop(&mut self) {
        self.state.release_launch(&self.id);
    }
}

fn blank_git_status(project_id: &str) -> GitStatus {
    GitStatus {
        project: project_id.to_string(),
        repository: true,
        branch: None,
        detached: false,
        upstream: None,
        ahead: 0,
        behind: 0,
        activity: GitActivity::Idle,
        error: None,
    }
}

/// The base check status and its push overlay share a lock so an overlapping publication cannot
/// briefly clear the pushing glyph, and broadcasts retain the same order as cached snapshots.
#[derive(Default)]
struct GitStatuses {
    statuses: HashMap<String, GitStatus>,
    pushes: HashMap<String, usize>,
}

impl GitStatuses {
    fn visible(&self, mut status: GitStatus) -> GitStatus {
        if status.repository && self.pushes.contains_key(&status.project) {
            status.activity = GitActivity::Pushing;
        }
        status
    }
}

pub(crate) struct GitPushActivity<'a> {
    state: &'a AppState,
    project_id: String,
}

impl Drop for GitPushActivity<'_> {
    fn drop(&mut self) {
        let mut statuses = self
            .state
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        let Some(count) = statuses.pushes.get_mut(&self.project_id) else {
            return;
        };
        *count -= 1;
        if *count == 0 {
            statuses.pushes.remove(&self.project_id);
        }
        if let Some(status) = statuses.statuses.get(&self.project_id) {
            self.state.broadcast(Event::ProjectGitStatus {
                status: statuses.visible(status.clone()),
            });
        }
    }
}

/// What the in-flight map holds for one project's claim.
struct GitCheck {
    /// Still waiting for a place to run, as a sweep's check does; false once it runs.
    queued: bool,
    /// A manual pull (`pull_project_branch`) arrived while the check was queued or running: a
    /// queued check then runs forced from the start, a running one is owed one forced rerun.
    rerun_owed: bool,
    /// Woken when a manual pull finds the check queued.
    promote: Arc<tokio::sync::Notify>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GitCheckRequest {
    Queued,
    Forced,
}

/// The right to check one project's git status right now, released by `release_unless_rerun_owed`
/// or, when a panic unwinds through the holder, on drop, which is what keeps a wedged step from
/// excluding its project from every later sweep for the rest of the daemon's life. See
/// `AppState::claim_git_check_queued`.
pub struct GitCheckClaim {
    state: Arc<AppState>,
    project_id: String,
    promote: Arc<tokio::sync::Notify>,
    released: bool,
}

impl GitCheckClaim {
    /// Resolves when a manual pull finds this claim still queued, for the holder to stop waiting
    /// for a place and run at once.
    pub async fn promoted(&self) {
        self.promote.notified().await;
    }

    /// Marks a queued claim as running, for the holder that has stopped waiting, and returns
    /// whether a manual pull arrived meanwhile, in which case the check runs forced and owes no
    /// rerun for that request. Done under the lock the request is marked under, so one arriving
    /// after this is owed a rerun instead.
    pub fn start(&mut self) -> bool {
        let mut checks = self
            .state
            .git_checks
            .lock()
            .expect("git check lock poisoned");
        let Some(check) = checks.get_mut(&self.project_id) else {
            return false;
        };
        check.queued = false;
        std::mem::take(&mut check.rerun_owed)
    }

    /// Releases the claim, unless a manual pull arrived while it was held: then the claim is kept,
    /// the owed rerun is cleared and `true` is returned, for the holder to run one more check that
    /// fast-forwards. Done under the lock `claim_git_check_forced` marks it under, so a request is
    /// either seen here or finds the project unclaimed.
    pub fn release_unless_rerun_owed(&mut self) -> bool {
        let mut checks = self
            .state
            .git_checks
            .lock()
            .expect("git check lock poisoned");
        if let Some(check) = checks.get_mut(&self.project_id) {
            if std::mem::take(&mut check.rerun_owed) {
                return true;
            }
        }
        checks.remove(&self.project_id);
        self.released = true;
        false
    }
}

impl Drop for GitCheckClaim {
    fn drop(&mut self) {
        if !self.released {
            self.state.release_git_check(&self.project_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Origin, Role};
    use crate::session::spawn_reader_thread;
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::{app_state, fake_live_session, PATIENCE};

    fn session_record(id: &str, status: SessionStatus) -> Session {
        Session {
            id: id.to_string(),
            agent: Agent::Claude,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: None,
            host_id: LOCAL_HOST_ID.to_string(),
            role: Role::Project,
            origin: Origin::User,
            title: "Session".to_string(),
            status,
            terminal_title: None,
            record_titles: Default::default(),
            background_tasks: Vec::new(),
            switching_account: false,
            default_effort: None,
            context: None,
            observed_model: None,
            observed_effort: None,
            model: None,
            effort: None,
            has_conversation: false,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            locked: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// Stores a session record, with the console it belongs to.
    fn insert_record(state: &AppState, dir: &std::path::Path, id: &str, status: SessionStatus) {
        state
            .store
            .insert_console(&crate::test_support::console(dir))
            .unwrap();
        state
            .store
            .insert_session(&session_record(id, status))
            .unwrap();
    }

    /// Waits, up to `PATIENCE`, for the exit watcher to let go of the session.
    async fn until_unregistered(state: &AppState, id: &str) {
        let deadline = Instant::now() + PATIENCE;
        while state.live_session(id).is_some() {
            assert!(Instant::now() < deadline, "the exit was never recorded");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn task(id: &str) -> BackgroundTask {
        BackgroundTask {
            id: id.to_string(),
            kind: Some("shell".to_string()),
            description: None,
        }
    }

    fn subagent(id: &str) -> BackgroundTask {
        BackgroundTask {
            id: id.to_string(),
            kind: Some("subagent".to_string()),
            description: None,
        }
    }

    fn stopped(id: &str, listed: &[&str]) -> BackgroundTasksChange {
        BackgroundTasksChange::SubagentStopped {
            id: id.to_string(),
            tasks: listed.iter().map(|id| subagent(id)).collect(),
        }
    }

    fn ids(work: &BackgroundWork) -> Vec<&str> {
        work.tasks.iter().map(|task| task.id.as_str()).collect()
    }

    /// A stop that leaves nothing running defers the emptying; a start applies it at once and brings
    /// only the new subagent, where a notification does not touch it.
    #[test]
    fn a_stop_that_leaves_nothing_defers_the_emptying() {
        let now = tokio::time::Instant::now();
        let mut work = BackgroundWork::default();
        work.apply(BackgroundTasksChange::SubagentStarted("a".into()), now, 0);
        work.apply(BackgroundTasksChange::SubagentStarted("b".into()), now, 1);
        assert_eq!(work.apply(stopped("a", &["a", "b"]), now, 2), None);
        assert_eq!(ids(&work), ["b"]);

        assert_eq!(work.apply(stopped("b", &["b"]), now, 3), Some(3));
        assert_eq!(ids(&work), ["b"]);
        // A notification does not take the ticket back.
        work.apply(BackgroundTasksChange::Finished(vec!["x".into()]), now, 4);
        assert_eq!(work.pending_empty, Some(3));
        // A newer ticket replaces it, and the older one then does nothing.
        assert_eq!(work.apply(stopped("b", &["b"]), now, 5), Some(5));
        assert!(!work.expire(3));
        assert!(work.expire(5));
        assert!(work.tasks.is_empty() && work.is_empty());

        work.apply(stopped("b", &["b"]), now, 6);
        work.apply(BackgroundTasksChange::SubagentStarted("a".into()), now, 7);
        assert_eq!(ids(&work), ["a"]);
        assert!(!work.expire(6));
    }

    /// With async command hooks a start can arrive before the stop whose registry predates it:
    /// the subagent that started within the delay survives that stop.
    #[test]
    fn a_start_just_before_a_stop_survives_it() {
        let now = tokio::time::Instant::now();
        let mut work = BackgroundWork::default();
        work.apply(BackgroundTasksChange::SubagentStarted("b".into()), now, 0);
        work.apply(BackgroundTasksChange::SubagentStarted("a".into()), now, 1);
        let later = now + Duration::from_millis(500);
        assert_eq!(work.apply(stopped("b", &["b"]), later, 2), None);
        assert_eq!(ids(&work), ["a"]);
        // The start has aged out by the time of a later stop.
        let much_later = now + EMPTY_TASKS_DELAY * 2;
        assert_eq!(work.apply(stopped("a", &["a"]), much_later, 3), Some(3));
        // A subagent that stopped itself is not brought back by a later stop.
        let mut work = BackgroundWork::default();
        work.apply(BackgroundTasksChange::SubagentStarted("x".into()), now, 0);
        work.apply(stopped("x", &["x"]), now, 1);
        assert_eq!(work.apply(stopped("y", &["y"]), now, 2), Some(2));
    }

    /// Only a subagent's description is remembered for its next start.
    #[test]
    fn a_subagents_description_comes_back_on_its_next_start() {
        let now = tokio::time::Instant::now();
        let mut work = BackgroundWork::default();
        let described = |id: &str, kind: &str| BackgroundTask {
            id: id.to_string(),
            kind: Some(kind.to_string()),
            description: Some("probe".to_string()),
        };
        work.apply(
            BackgroundTasksChange::Replace(vec![
                described("a", "subagent"),
                described("s", "shell"),
            ]),
            now,
            0,
        );
        work.apply(BackgroundTasksChange::Replace(Vec::new()), now, 1);
        work.apply(BackgroundTasksChange::SubagentStarted("a".into()), now, 2);
        assert_eq!(work.tasks[0].description.as_deref(), Some("probe"));
        assert!(!work.descriptions.contains_key("s"));
    }

    /// A session request holds the record at waiting for the user while its agent stays idle: its
    /// subagents still count, and a working one's do not.
    #[tokio::test]
    async fn a_session_request_does_not_make_an_idle_agent_busy() {
        let (state, dir) = app_state("background-tasks-request-hand");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        state.raise_request_hand("s").unwrap();
        assert!(state.is_idle("s"));
        let started = BackgroundTasksChange::SubagentStarted("a".to_string());
        state.apply_background_tasks_change("s", started);
        assert_eq!(state.background_tasks_of("s"), vec![subagent("a")]);

        state
            .store
            .insert_session(&session_record("w", SessionStatus::Working))
            .unwrap();
        state.raise_request_hand("w").unwrap();
        assert!(!state.is_idle("w"));
    }

    /// An idle session shows what it has left running, and only while it is idle: the tasks are
    /// tracked beside the status, so a message still goes in like it does for any idle session.
    #[test]
    fn background_tasks_show_on_an_idle_session_until_they_finish_or_the_session_goes() {
        let (state, dir) = app_state("background-tasks");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let mut events = state.subscribe();

        let both = vec![task("a"), task("b")];
        state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(both.clone()));
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("a new list must be published")
        };
        assert_eq!(session.background_tasks, both);
        // The same list again says nothing new.
        state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(both));
        assert!(events.try_recv().is_err());

        // A hook's idle changes nothing about them.
        state.apply_hook_status("s", SessionStatus::Idle).unwrap();
        let record = state.session_record("s").unwrap();
        assert_eq!(state.wire_session(record).background_tasks.len(), 2);

        state.apply_background_tasks_change(
            "s",
            BackgroundTasksChange::Finished(vec!["a".to_string()]),
        );
        let record = state.session_record("s").unwrap();
        assert_eq!(state.wire_session(record).background_tasks, vec![task("b")]);

        // Working again: the session is not idle, so nothing is shown, but the list is kept for
        // the next idle.
        state
            .apply_hook_status("s", SessionStatus::Working)
            .unwrap();
        let record = state.session_record("s").unwrap();
        assert!(state.wire_session(record).background_tasks.is_empty());
        state.apply_hook_status("s", SessionStatus::Idle).unwrap();
        let record = state.session_record("s").unwrap();
        assert_eq!(state.wire_session(record).background_tasks, vec![task("b")]);

        // An empty list, as a `Stop` with nothing left running brings, clears them.
        state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(Vec::new()));
        let record = state.session_record("s").unwrap();
        assert!(state.wire_session(record).background_tasks.is_empty());

        // The session going away takes them with it.
        state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("c")]));
        state.forget_session_bookkeeping("s");
        assert!(state.background_tasks_of("s").is_empty());
    }

    #[test]
    fn codex_manual_approval_overrides_auto_review_and_concurrent_hook_statuses() {
        let (state, dir) = app_state("codex-manual-approval");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let mut record = state.session_record("s").unwrap();
        record.agent = Agent::Codex;
        state.save_session(&record).unwrap();
        let live = fake_live_session("s", Agent::Codex, 80, 24, "sleep 30").with_auto_review();
        assert!(live.resolves_approvals_itself);
        state.register_live(live.clone());
        live.on_output(include_bytes!("../testdata/codex_working_title.bin"));
        let previous_title = state
            .wire_session(state.session_record("s").unwrap())
            .terminal_title;
        let mut events = state.subscribe();
        let approval = include_bytes!("../testdata/codex_approval_title.bin");
        for byte in approval {
            live.on_output(std::slice::from_ref(byte));
        }
        assert_eq!(state.agent_status(&record), SessionStatus::WaitingUser);
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("the approval must publish its hand even when the task title stays the same")
        };
        assert_eq!(session.status, SessionStatus::WaitingUser);
        assert_eq!(session.terminal_title, previous_title);
        for status in [SessionStatus::Idle, SessionStatus::Working] {
            state.apply_hook_status("s", status).unwrap();
            let record = state.session_record("s").unwrap();
            assert_eq!(state.agent_status(&record), SessionStatus::WaitingUser);
            assert_eq!(
                state.wire_session(record).status,
                SessionStatus::WaitingUser
            );
        }
        live.on_output(b"\x1b]0;[ . ] Action Required | octoboard\x07");
        assert!(
            live.needs_user_action(),
            "the blinking title must keep the hand up"
        );
        state.raise_request_hand("s").unwrap();
        live.on_output(include_bytes!("../testdata/codex_working_title.bin"));
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::WaitingUser
        );
        assert_eq!(
            state.agent_status(&state.session_record("s").unwrap()),
            SessionStatus::Working
        );
        state.forget_request_hand("s");
        // Dormancy is written by the coordinator, never by hooks.
        let mut record = state.session_record("s").unwrap();
        record.status = SessionStatus::Interrupted;
        state.save_session(&record).unwrap();
        live.on_output(approval);
        assert_eq!(
            state.wire_session(record.clone()).status,
            SessionStatus::Interrupted
        );
        assert_eq!(state.agent_status(&record), SessionStatus::Interrupted);
    }

    #[tokio::test]
    async fn answering_a_codex_manual_approval_releases_queued_messages_off_the_reader() {
        let (state, dir) = app_state("codex-approval-outbox");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", Agent::Codex, 80, 24, "sleep 30").with_auto_review();
        state.register_live(live.clone());
        live.on_output(include_bytes!("../testdata/codex_working_title.bin"));
        assert!(
            !live.needs_user_action(),
            "ordinary auto-review work raises no hand"
        );
        live.on_output(include_bytes!("../testdata/codex_approval_title.bin"));
        state.queue_message("s", "Queued while waiting");
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Pending
        );
        assert_eq!(state.outbox.len("s"), 1);
        let reader = live.clone();
        std::thread::spawn(move || {
            reader.on_output(include_bytes!("../testdata/codex_working_title.bin"));
        })
        .join()
        .unwrap();
        let deadline = Instant::now() + PATIENCE;
        while state.outbox.len("s") != 0 {
            assert!(
                Instant::now() < deadline,
                "answering must release the message without another hook"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!live.needs_user_action());
    }

    const LAUNCH_DIALOG: &str = "  Resume this conversation?\r\n  \u{276f} 1. Resume\r\n    2. Start a new conversation\r\n  Enter to confirm \u{b7} Esc to resume\r\n";
    const CLEAR_SCREEN: &str = "\x1b[2J\x1b[H";

    /// A dialog on the screen holds the writes, shows the session as waiting for the user, and the
    /// queue goes in once the screen shows none.
    #[test]
    fn an_agent_dialog_on_the_screen_holds_writes_until_it_is_gone() {
        let (state, dir) = app_state("state-agent-dialog");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        assert_eq!(state.write_hold("s"), None);

        live.on_output(LAUNCH_DIALOG.as_bytes());
        let hold = state.write_hold("s").expect("held");
        assert_eq!(hold.code(), "agent_dialog");
        assert_eq!(hold.dialog(), Some("claude_resume_at_launch"));
        assert_eq!(
            state
                .wire_session(state.session_record("s").unwrap())
                .status,
            SessionStatus::WaitingUser
        );

        state.queue_message("s", "go on");
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Pending);
        assert_eq!(state.outbox.len("s"), 1);

        live.on_output(CLEAR_SCREEN.as_bytes());
        assert_eq!(state.write_hold("s"), None);
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Clear);
        assert_eq!(state.outbox.len("s"), 0);
    }

    /// The watch publishes the session when a dialog comes and goes, and writes out what waited.
    #[tokio::test]
    async fn the_dialog_watch_raises_the_hand_and_releases_the_queue() {
        let (state, dir) = app_state("state-agent-dialog-watch");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        crate::agent_dialogs::watch(&state, &live);
        let mut events = state.subscribe();

        live.on_output(LAUNCH_DIALOG.as_bytes());
        let deadline = Instant::now() + PATIENCE;
        loop {
            match events.try_recv() {
                Ok(Event::SessionUpserted { session }) => {
                    assert_eq!(session.status, SessionStatus::WaitingUser);
                    break;
                }
                _ => {
                    assert!(Instant::now() < deadline, "the hand was not raised");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }

        state.queue_message("s", "go on");
        live.on_output(CLEAR_SCREEN.as_bytes());
        let deadline = Instant::now() + PATIENCE;
        while state.outbox.len("s") != 0 {
            assert!(Instant::now() < deadline, "the queue was not released");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// A launch dialog that comes up as a message is written discards it: the message is queued
    /// again, no unsent mark holds the queue, and the user is told what happened.
    #[test]
    fn a_message_a_launch_dialog_discarded_is_queued_again_and_announced() {
        let (state, dir) = app_state("state-agent-dialog-discard");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::from_secs(2));
        let mut events = state.subscribe();
        state.queue_message("s", "go on");
        state.queue_message("s", "and then");

        let screen = live.clone();
        let dialog = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            screen.on_output(LAUNCH_DIALOG.as_bytes());
        });
        let outcome = state.flush_outbox("s", SessionStatus::Idle);
        dialog.join().unwrap();

        assert_eq!(outcome, Drain::Discarded(0));
        assert_eq!(state.outbox.len("s"), 2);
        assert!(!live.unsent_waiting());
        assert!(matches!(
            state.write_hold("s"),
            Some(WriteHold::AgentDialog(_))
        ));
        let mut announced = None;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionNotice {
                session,
                code,
                params,
                ..
            } = event
            {
                announced = Some((session, code, params));
            }
        }
        let (session, code, params) = announced.expect("a notice");
        assert_eq!(
            (session.as_str(), code.as_str()),
            ("s", "message_discarded_by_dialog")
        );
        assert_eq!(params.get("agent").map(String::as_str), Some("Claude Code"));
    }

    /// The dialog a write closes within milliseconds is gone before anything looks at the screen
    /// after the write, and the message is still known to have been discarded: the screen is looked
    /// at on every chunk of output while the write is waited on. With the dialog gone, the message
    /// is written again at once.
    #[test]
    fn a_dialog_drawn_and_closed_inside_the_write_still_discards_the_message() {
        let (state, dir) = app_state("state-agent-dialog-closed");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::from_secs(1));
        state.queue_message("s", "go on");

        let screen = live.clone();
        let dialog = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            screen.on_output(LAUNCH_DIALOG.as_bytes());
            screen.on_output(CLEAR_SCREEN.as_bytes());
        });
        let outcome = state.flush_outbox("s", SessionStatus::Idle);
        dialog.join().unwrap();

        // Written again, and what the second write came to is the answer: the stand-in sends no
        // prompt, so this time it is left unsent.
        assert_eq!(outcome, Drain::Unsent(0));
        assert_eq!(state.outbox.len("s"), 0);
        assert!(live.unsent_waiting());
        assert_eq!(state.write_hold("s"), Some(WriteHold::UnsentMessage));
        assert_eq!(live.look_for_dialog().current, None);
    }

    /// A view of background agents takes the message it is written while it comes up: the message
    /// is not queued again and holds nothing, the sender is told it did not reach the conversation,
    /// and the user is told where it went.
    #[test]
    fn a_message_a_view_of_background_agents_took_is_announced_and_not_queued_again() {
        let (state, dir) = app_state("state-agent-view-diverted");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        *state.send_wait.lock().unwrap() = Some(Duration::from_secs(1));
        let mut events = state.subscribe();
        state.queue_message("s", "go on");
        state.queue_message("s", "and then");

        let screen = live.clone();
        let view = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            screen.on_output("  \u{25ef} main\r\n".as_bytes());
        });
        let outcome = state.flush_outbox("s", SessionStatus::Idle);
        view.join().unwrap();

        assert_eq!(outcome, Drain::Diverted(0));
        assert_eq!(state.outbox.len("s"), 1);
        assert!(!live.unsent_waiting());
        assert!(matches!(
            state.write_hold("s"),
            Some(WriteHold::AgentDialog(_))
        ));
        let mut announced = None;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionNotice { session, code, .. } = event {
                announced = Some((session, code));
            }
        }
        let (session, code) = announced.expect("a notice");
        assert_eq!(
            (session.as_str(), code.as_str()),
            ("s", "message_diverted_by_view")
        );
    }

    #[test]
    fn terminal_titles_reach_unattached_sessions_without_stale_processes_or_records_winning() {
        let (state, dir) = app_state("state-terminal-title");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let original = state.session_record("s").unwrap();
        let first = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        let mut events = state.subscribe();
        let output = b"before\x1b]2;First task\x07after";
        first.on_output(output);
        assert!(
            events.try_recv().is_err(),
            "no record update before registration"
        );
        state.register_live(first.clone());
        assert_eq!(first.recent_output(1024), output, "raw output is unchanged");
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("expected a title update")
        };
        assert_eq!(session.terminal_title.as_deref(), Some("First task"));
        first.on_output(b"\x1b]2;First task\x07");
        assert!(
            events.try_recv().is_err(),
            "equal titles are not announced again"
        );
        state.save_session(&original).unwrap();
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("expected the saved session")
        };
        assert_eq!(
            session.terminal_title.as_deref(),
            Some("First task"),
            "a stale caller cannot blank the title"
        );
        let successor = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(successor.clone());
        first.on_output(b"\x1b]2;Stale task\x07");
        assert!(events.try_recv().is_err());
        assert_eq!(
            state.session_record("s").unwrap().terminal_title.as_deref(),
            Some("First task")
        );
        successor.on_output(b"\x1b]2;Next task\x1b\\");
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("expected successor title")
        };
        assert_eq!(session.terminal_title.as_deref(), Some("Next task"));
        let mut archived = state.session_record("s").unwrap();
        archived.status = SessionStatus::Archived;
        state.save_session(&archived).unwrap();
        events.try_recv().unwrap();
        for cleanup in [b"\x1b]2;\x07".as_slice(), b"\x1b]2;octoboard\x07"] {
            successor.on_output(cleanup);
            assert!(
                events.try_recv().is_err(),
                "shutdown titles are not published"
            );
            assert_eq!(
                state.session_record("s").unwrap().terminal_title.as_deref(),
                Some("Next task"),
                "archiving preserves the active title through process cleanup"
            );
        }
        assert!(state.delete_if_archived("s").unwrap());
        assert!(matches!(
            events.try_recv().unwrap(),
            Event::SessionDeleted { .. }
        ));
        successor.on_output(b"\x1b]2;Deleted task\x07");
        assert!(events.try_recv().is_err());
        assert!(state.store.get_session("s").unwrap().is_none());
    }

    #[test]
    fn decorations_that_leave_the_resolved_title_alone_are_not_published() {
        let (state, dir) = app_state("state-title-decoration");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        let mut events = state.subscribe();
        live.on_output(b"\x1b]2;\xe2\x9c\xb3 Claude Code\x07");
        assert!(events.try_recv().is_err(), "the placeholder names nothing");
        live.on_output("\x1b]2;◐ Task\x07".as_bytes());
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("expected a title update")
        };
        assert_eq!(session.terminal_title.as_deref(), Some("Task"));
        live.on_output("\x1b]2;✳ Task\x07".as_bytes());
        assert!(events.try_recv().is_err(), "a new glyph is not a new title");
        assert_eq!(
            state.session_record("s").unwrap().terminal_title.as_deref(),
            Some("Task"),
            "the title is stored without its leading glyph"
        );
    }

    /// The output is saved before the live entry goes, so the moment the session stops being
    /// running there is already something to show for it.
    #[tokio::test]
    async fn a_process_that_ends_leaves_its_last_output_behind() {
        let (state, dir) = app_state("state-output-kept-on-exit");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "printf 'last words'; sleep 0.2");
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        state.watch_exit(live.clone());

        until_unregistered(&state, "s").await;
        let saved = state.saved_output("s").expect("the output was kept");
        assert!(String::from_utf8_lossy(&saved).contains("last words"));
    }

    #[tokio::test]
    async fn a_dormant_status_publishes_only_after_its_screen_is_frozen() {
        for status in [SessionStatus::Archived, SessionStatus::Interrupted] {
            let (state, dir) = app_state("state-output-frozen-before-publication");
            insert_record(&state, &dir, "s", SessionStatus::Working);
            let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
            state.register_live(live.clone());
            live.on_output(b"\x1b[?1049h\x1b[Hconversation");
            let mut events = state.subscribe();
            let mut record = state.session_record("s").unwrap();
            record.status = status;
            state.save_session(&record).unwrap();
            assert!(matches!(
                events.try_recv(),
                Ok(Event::SessionUpserted { .. })
            ));
            let frozen = live
                .frozen_output()
                .expect("published status has a snapshot");
            live.on_output(b"\x1b[?1049l\x1b[2J\x1b[Hexit message");
            state.save_output(&live);
            assert_eq!(state.saved_output("s"), Some(frozen));
            assert!(live.write_input(b"should stay archived").is_err());
        }
    }

    /// The daemon does not wait for its exit watchers on the way out, so a shutdown keeps the
    /// output itself.
    #[tokio::test]
    async fn stopping_every_session_keeps_each_ones_output() {
        let (state, dir) = app_state("state-output-kept-on-shutdown");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "printf 'still here'; sleep 30");
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let deadline = Instant::now() + PATIENCE;
        while live.recent_output(64).is_empty() {
            assert!(Instant::now() < deadline, "the stand-in printed nothing");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        state.stop_all_sessions().await;
        let saved = state.saved_output("s").expect("the output was kept");
        assert!(String::from_utf8_lossy(&saved).contains("still here"));
    }

    /// A record deleted while its process is still exiting, which is allowed, gets no output saved
    /// after it.
    #[tokio::test]
    async fn a_session_deleted_while_exiting_leaves_no_output_behind() {
        let (state, dir) = app_state("state-output-deleted-while-exiting");
        insert_record(&state, &dir, "s", SessionStatus::Archived);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "printf 'gone'; sleep 0.2");
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        assert!(state.delete_if_archived("s").unwrap());
        state.watch_exit(live.clone());

        until_unregistered(&state, "s").await;
        assert_eq!(state.saved_output("s"), None);
        assert!(!dir.join("output").join("s").exists());
    }

    /// Only one report may come out of one turn, and only when the session did not report for
    /// itself. The repeat case is not hypothetical: Grok fires its `idle_prompt` backstop about a
    /// minute after every turn, whether or not a `Stop` already reported it.
    /// A carried-offset watch resumes where the session's previous watch of the same file got to,
    /// and a different file starts again from zero.
    #[test]
    fn a_record_offset_is_carried_for_the_same_file_and_not_for_another() {
        let (state, _dir) = app_state("state-record-offset");
        let path = std::path::Path::new("/rollout-a.jsonl");
        assert_eq!(state.record_offset("s", path), 0);
        state.note_record_offset("s", path, 120);
        assert_eq!(state.record_offset("s", path), 120);
        assert_eq!(
            state.record_offset("s", std::path::Path::new("/rollout-b.jsonl")),
            0
        );
    }

    #[test]
    fn a_turn_can_only_be_closed_once() {
        let (state, _dir) = app_state("state-turns");
        // Nothing has started, so nothing is owed a report — a stop event arriving before any turn
        // start (an agent's own teardown, a slash command) must not produce one.
        assert_eq!(state.close_turn("s", false), TurnClose::NoTurn);

        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        assert_eq!(state.close_turn("s", false), TurnClose::NoTurn);

        state.turn_started("s", None);
        state.mark_reported("s");
        assert_eq!(state.close_turn("s", false), TurnClose::Reported);

        // A session credited for one turn is not credited for the next.
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// A credit given ahead of a turn covers the turn its message starts, and only that one: the
    /// turn open when it is given keeps its own account, and a turn something else starts first
    /// does not spend it.
    #[test]
    fn a_credit_is_spent_by_the_turn_its_message_starts() {
        let (state, _dir) = app_state("state-turn-credit");
        state.turn_started("s", Some("work"));
        state.credit_turn_opened_by("s", "Message from Octoboard".to_string());
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        state.turn_started("s", Some("<task-notification>"));
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        state.turn_started("s", Some("Message from Octoboard about it"));
        assert_eq!(state.close_turn("s", false), TurnClose::Reported);
        state.turn_started("s", Some("Message from Octoboard about it"));
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// Where the agent's prompt is not read, there is nothing to tell the credited turn by, so the
    /// next turn to start is the one credited.
    #[test]
    fn without_a_prompt_the_next_turn_spends_the_credit() {
        let (state, _dir) = app_state("state-turn-credit-fallback");
        state.credit_turn_opened_by("s", "Message from Octoboard".to_string());
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::Reported);
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// A clock-attributed end left over from a finished turn must not close the turn running now,
    /// or the console session gets a report for work still in flight and the real end finds nothing
    /// open.
    #[test]
    fn a_clock_attributed_end_is_refused_while_the_open_turn_is_still_active() {
        let (state, _dir) = app_state("state-backstop");
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", true), TurnClose::NotThisTurn);
        // Still open, so its own end is still reportable.
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// The quiet gate alone is not enough: Grok emits nothing between `PreToolUse` and
    /// `PostToolUse`, so a tool call longer than the backstop delay leaves a working turn looking
    /// quiet and the previous turn's echo would close it. The echo has to be consumed instead.
    #[test]
    fn the_echo_of_an_ending_already_acted_on_is_consumed_not_acted_on() {
        let (state, _dir) = app_state("state-echo");
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        assert!(state.echo_pending("s"));

        // The next turn begins and is still running when the previous turn's echo arrives.
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", true), TurnClose::NotThisTurn);
        assert!(!state.echo_pending("s"));
        // And this turn's own ending is still reportable.
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// The echo usually arrives with no turn open at all — a minute later, with the session still
    /// idle. It has to be spent there too: left armed, it would be spent on the next turn instead,
    /// and that turn may be one Grok reports no stop event for, whose echo is its only ending.
    #[test]
    fn an_echo_that_arrives_with_no_turn_open_is_still_spent() {
        let (state, _dir) = app_state("state-echo-idle");
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        // The echo of that ending, with nothing open.
        assert_eq!(state.close_turn("s", true), TurnClose::NoTurn);
        assert!(!state.echo_pending("s"));
    }

    /// A report that never reached the console session must not leave the session looking as
    /// though it reported, or its stop produces no synthesised report either and the console
    /// session gets nothing at all.
    #[test]
    fn credit_for_reporting_can_be_taken_back() {
        let (state, _dir) = app_state("state-reported");
        state.turn_started("s", None);
        state.mark_reported("s");
        state.clear_reported("s");
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// A cancelled turn owes no report, but leaving it open is what lets a later backstop invent
    /// one for it.
    #[test]
    fn an_abandoned_turn_is_closed_without_owing_a_report() {
        let (state, _dir) = app_state("state-abandon");
        state.turn_started("s", None);
        state.abandon_turn("s");
        assert_eq!(state.close_turn("s", false), TurnClose::NoTurn);
    }

    /// A token is per launch: the previous process is gone, and a token that outlived it would let
    /// a stale child act on the session.
    #[test]
    fn issuing_a_token_retires_the_sessions_previous_one() {
        let (state, _dir) = app_state("state-tokens");
        let first = state.issue_mcp_token("s");
        assert_eq!(state.session_for_mcp_token(&first).as_deref(), Some("s"));

        let second = state.issue_mcp_token("s");
        assert_ne!(first, second);
        assert!(state.session_for_mcp_token(&first).is_none());
        assert_eq!(state.session_for_mcp_token(&second).as_deref(), Some("s"));
    }

    /// A flush has nothing to write to while no process is registered, and must not take the queue
    /// apart on the way to finding that out. The queue's own properties are `crate::outbox`'s to
    /// test; what matters here is that the status gate and the live-session check come first.
    #[test]
    fn a_flush_with_nowhere_to_write_leaves_the_queue_alone() {
        let (state, _dir) = app_state("state-outbox");
        state.queue_message("s", "first");
        state.queue_message("s", "second");

        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Pending);
        assert_eq!(
            state.flush_outbox("s", SessionStatus::WaitingUser),
            Drain::Pending
        );
        assert_eq!(state.outbox.len("s"), 2);

        // A relaunch that failed has nothing left that would ever release these.
        state.discard_outbox("s");
        assert_eq!(state.outbox.len("s"), 0);
    }

    /// A handoff holds the queue of the session it moves. The queue is not forgotten when that
    /// session's process ends meanwhile; it goes to the successor the holder names, and is
    /// discarded, not stranded, when the session has nowhere to be written and no successor.
    #[test]
    fn a_held_queue_outlives_its_sessions_process_and_goes_where_its_holder_says() {
        let (state, dir) = app_state("state-held-queue");
        insert_record(&state, &dir, "t", SessionStatus::Idle);
        let successor = crate::test_support::idle_stand_in("t");
        state.register_live(successor.clone());
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let hold = state.hold_outbox("s");
        state.queue_message("s", "one");
        state.unregister_live(&live);
        assert_eq!(state.outbox.len("s"), 1);

        // No runtime here, so the successor's queue is left for a flush to write.
        hold.forward_to("t");
        drop(hold);
        assert_eq!((state.outbox.len("s"), state.outbox.len("t")), (0, 1));

        let hold = state.hold_outbox("u");
        state.queue_message("u", "two");
        drop(hold);
        assert_eq!(state.outbox.len("u"), 0);

        // A successor with no process takes nothing either, and keeps nothing for a later resume.
        let mut events = state.subscribe();
        let hold = state.hold_outbox("v");
        state.queue_message("v", "three");
        hold.forward_to("w");
        drop(hold);
        assert_eq!((state.outbox.len("v"), state.outbox.len("w")), (0, 0));
        match events.try_recv().expect("a notice") {
            Event::SessionNotice { session, code, .. } => {
                assert_eq!(
                    (session.as_str(), code.as_str()),
                    ("w", "handoff_messages_dropped")
                );
            }
            other => panic!("expected a notice, got {other:?}"),
        }

        // A hold taken for a sharing change says so when it drops what it held.
        let mut events = state.subscribe();
        let hold = state.hold_outbox_for_sharing("z");
        state.queue_message("z", "five");
        drop(hold);
        assert_eq!(state.outbox.len("z"), 0);
        match events.try_recv().expect("a notice") {
            Event::SessionNotice { session, code, .. } => {
                assert_eq!(
                    (session.as_str(), code.as_str()),
                    ("z", "sharing_messages_dropped")
                );
            }
            other => panic!("expected a notice, got {other:?}"),
        }

        // Unless it is being launched: the launch will flush what is queued for it.
        state
            .store
            .insert_session(&session_record("x", SessionStatus::Idle))
            .unwrap();
        let claim = state.begin_launch("x").unwrap();
        let hold = state.hold_outbox("y");
        state.queue_message("y", "four");
        hold.forward_to("x");
        drop(hold);
        assert_eq!(state.outbox.len("x"), 1);
        drop(claim);
    }

    /// A check that panics must still release its project's claim, or a wedged step excludes that
    /// project from every later sweep for the rest of the daemon's life — `spawn_check` relies on
    /// unwinding through the claim's `Drop` rather than an explicit `end_git_check` call.
    #[test]
    fn a_git_check_claim_is_released_even_if_the_check_panics() {
        let (state, _dir) = app_state("state-git-claim-panic");
        let claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert!(state.claim_git_check_queued("p1").is_none());

        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _claim = claim;
            panic!("simulated check failure");
        }));
        assert!(unwound.is_err());

        // The panic unwound through the claim's `Drop`, releasing it for the next sweep.
        assert!(state.claim_git_check_queued("p1").is_some());
    }

    /// A manual pull that finds a check running must not be lost: it makes the holder run once
    /// more, once, and only then release.
    #[test]
    fn a_manual_pull_during_a_check_is_owed_one_rerun() {
        let (state, _dir) = app_state("state-git-claim-rerun");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert!(state.claim_git_check_forced("p1").is_none());
        assert!(state.claim_git_check_forced("p1").is_none());

        assert!(
            claim.release_unless_rerun_owed(),
            "the request is handed back"
        );
        assert!(
            state.claim_git_check_queued("p1").is_none(),
            "the claim is still held for the rerun"
        );
        assert!(
            !claim.release_unless_rerun_owed(),
            "owed once, however many asked"
        );
        assert!(
            state.claim_git_check_queued("p1").is_some(),
            "then it is released"
        );
    }

    /// A manual pull that finds a sweep's check still waiting for a place takes it over: the
    /// waiting claim is woken and starts forced, and nothing more is owed afterwards.
    #[tokio::test]
    async fn a_manual_pull_promotes_a_queued_check() {
        let (state, _dir) = app_state("state-git-claim-queued");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert!(state.claim_git_check_queued("p1").is_none());
        assert!(state.claim_git_check_forced("p1").is_none());

        tokio::time::timeout(crate::test_support::PATIENCE, claim.promoted())
            .await
            .expect("the queued claim is woken");
        assert!(claim.start(), "it runs forced");
        assert!(
            !claim.release_unless_rerun_owed(),
            "the request is not owed a second run"
        );
        assert!(state.claim_git_check_queued("p1").is_some());
    }

    /// A manual pull arriving after the queued check started is owed a rerun, as for any running
    /// check.
    #[test]
    fn a_manual_pull_after_a_queued_check_started_is_owed_a_rerun() {
        let (state, _dir) = app_state("state-git-claim-started");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert!(!claim.start(), "nothing asked yet");
        assert!(state.claim_git_check_forced("p1").is_none());
        assert!(claim.release_unless_rerun_owed());
    }

    fn git_project(state: &AppState, dir: &std::path::Path) -> crate::protocol::Project {
        insert_record(state, dir, "s", SessionStatus::Idle);
        crate::test_support::insert_project(state, dir)
    }

    fn git_reading() -> GitStatus {
        GitStatus {
            project: "p1".to_string(),
            repository: true,
            branch: Some("main".to_string()),
            detached: false,
            upstream: Some("origin/main".to_string()),
            ahead: 2,
            behind: 1,
            activity: GitActivity::Idle,
            error: Some("fetch failed".to_string()),
        }
    }

    #[test]
    fn overlapping_pushes_overlay_checks_until_the_last_push_ends() {
        let (state, dir) = app_state("state-git-push-overlap");
        git_project(&state, &dir);
        state.publish_git_status(git_reading());
        let mut first_window = state.subscribe();
        let mut second_window = state.subscribe();
        let first = state.begin_git_push("p1");
        let second = state.begin_git_push("p1");
        state.publish_git_status(GitStatus {
            activity: GitActivity::Checking,
            ..git_reading()
        });
        state.publish_git_reread(
            "p1",
            Ok(crate::git_status::BranchHeader {
                branch: Some("main".to_string()),
                upstream: Some("origin/main".to_string()),
                ahead: 0,
                behind: 1,
                ..Default::default()
            }),
        );
        drop(first);
        let status = state.git_statuses().pop().unwrap();
        assert_eq!(status.activity, GitActivity::Pushing);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!((status.ahead, status.behind), (0, 1));
        assert_eq!(status.error.as_deref(), Some("fetch failed"));
        drop(second);
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Checking);
        for window in [&mut first_window, &mut second_window] {
            for expected in [
                GitActivity::Pushing,
                GitActivity::Pushing,
                GitActivity::Pushing,
                GitActivity::Pushing,
                GitActivity::Pushing,
                GitActivity::Checking,
            ] {
                let Event::ProjectGitStatus { status } = window.try_recv().unwrap() else {
                    panic!("git status event")
                };
                assert_eq!(status.activity, expected);
            }
            assert!(window.try_recv().is_err());
        }
    }

    #[test]
    fn a_push_guard_restores_the_latest_status_even_on_unwind() {
        let (state, dir) = app_state("state-git-push-unwind");
        git_project(&state, &dir);
        state.publish_git_status(git_reading());
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _push = state.begin_git_push("p1");
            state.publish_git_status(GitStatus {
                activity: GitActivity::Pulling,
                ..git_reading()
            });
            panic!("simulated push panic");
        }));
        assert!(unwound.is_err());
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Pulling);
    }

    #[test]
    fn push_success_and_failure_both_clear_the_in_flight_activity() {
        let (state, dir) = app_state("state-git-push-outcomes");
        let repo = crate::test_support::repo_with("state-git-push-repo", &[(b"file", b"one")]);
        let project = git_project(&state, &repo);
        let remote = dir.join("remote.git");
        crate::test_support::git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        state.publish_git_status(git_reading());
        let mut events = state.subscribe();
        assert!(crate::git_push::push_current_branch(&state, &project).is_err());
        // The header re-read first, ahead of the push being marked in flight.
        for expected in [GitActivity::Idle, GitActivity::Pushing, GitActivity::Idle] {
            let Event::ProjectGitStatus { status } = events.try_recv().unwrap() else {
                panic!("git status event")
            };
            assert_eq!(status.activity, expected);
        }
        assert_eq!(
            state.git_statuses()[0].error.as_deref(),
            Some("fetch failed")
        );
        crate::test_support::git(&dir, &["init", "--bare", remote.to_str().unwrap()]);
        crate::git_push::push_current_branch(&state, &project).unwrap();
        for expected in [
            GitActivity::Idle,
            GitActivity::Pushing,
            GitActivity::Pushing,
            GitActivity::Idle,
        ] {
            let Event::ProjectGitStatus { status } = events.try_recv().unwrap() else {
                panic!("git status event")
            };
            assert_eq!(status.activity, expected);
        }
        let status = state.git_statuses().pop().unwrap();
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (0, 0));
        assert_eq!(status.error.as_deref(), Some("fetch failed"));
    }

    #[test]
    fn a_push_finishing_after_its_project_is_removed_does_not_restore_a_status() {
        let (state, dir) = app_state("state-git-push-removed");
        git_project(&state, &dir);
        let push = state.begin_git_push("p1");
        state.remove_git_status("p1");
        let mut events = state.subscribe();
        drop(push);
        assert!(state.git_statuses().is_empty());
        assert!(events.try_recv().is_err());
        let _missing = state.begin_git_push("ghost");
        assert!(state.git_statuses().is_empty());
    }

    /// A status published for a project no longer in the store must be dropped rather than
    /// reinserted — the race `DeleteProject` leaves open when its check is already in flight, and
    /// the gap `DeleteConsole` leaves for every project of a deleted console.
    #[test]
    fn publishing_a_status_for_a_project_no_longer_in_the_store_drops_it() {
        let (state, _dir) = app_state("state-git-status-missing-project");
        state.publish_git_status(GitStatus {
            project: "ghost".to_string(),
            repository: true,
            branch: Some("main".to_string()),
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            activity: crate::protocol::GitActivity::Idle,
            error: None,
        });
        assert!(state.git_statuses().is_empty());
    }

    /// A completion recorded for a project, then dropped by the publish that follows it because
    /// the project is gone, must stay dropped — `record_git_check_completed` running first (as
    /// `git_status::check_project` does at both of its exit points) and then losing to
    /// `remove_git_status` is the order that matters; recording it after the publish would
    /// resurrect the very entry `remove_git_status` just cleared, with nothing left to ever remove
    /// it again.
    #[test]
    fn a_completion_recorded_then_published_for_a_gone_project_leaves_no_entry() {
        let (state, _dir) = app_state("state-git-status-missing-project-completion");
        state.record_git_check_completed("ghost");
        state.publish_git_status(GitStatus {
            project: "ghost".to_string(),
            repository: false,
            branch: None,
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            activity: crate::protocol::GitActivity::Idle,
            error: None,
        });
        assert!(state.git_statuses().is_empty());
        assert!(state.git_check_completed_at("ghost").is_none());
    }
}
