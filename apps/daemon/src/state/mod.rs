//! The daemon's shared state, [`AppState`]: the store and the registries that live beside it. Each
//! concern has a file of its own holding its types and its methods as a further `impl AppState`
//! block; this file holds the struct, its construction and the methods no one concern owns.
//!
//! - [`sessions`]: the session records' writes, each of which writes only its own columns (see
//!   `crate::store`), and the removal of a record; [`status`], [`titles`]: the status transitions
//!   made through them, the status and title shown.
//! - [`live`], [`claims`], [`sharing_lock`], [`process_exit`]: the live sessions, the claims that
//!   keep two launches of one session, an archive or deletion and a lock change, or two of the
//!   operations that must have a session to themselves from racing, the locks of an account's
//!   change of sharing, and the stop of every session at shutdown with the output saved as each
//!   process ends.
//! - [`mcp_tokens`], [`turns`], [`left_for_limit`]: the per-session MCP tokens, the turn
//!   bookkeeping a synthesised report rests on, and the accounts a session left for a usage limit.
//! - [`auto_switch`]: the hooks through which lower layers reach the automatic account switch.
//! - [`requests`]: the hands a waiting session request (`crate::session_request`) raises and the
//!   status each such session's agent is at meanwhile.
//! - [`background`], [`record_watch`]: each session's background tasks, default effort and record
//!   watch.
//! - [`git_status`]: each project's live git status, with the claim that keeps two checks of one
//!   project from racing and the timestamp that keeps them from piling up across several clients.
//! - [`agent_availability`]: each agent's availability; [`forget`]: what is dropped where a record
//!   is removed.
//! - [`bindings`], [`refusals`]: free functions rather than `AppState` methods: the reads of which
//!   sessions are bound to which, and the refusals every operation on a session shares.
//!
//! The write queues' methods on `AppState` (queuing, flushing, holds) are in
//! `crate::messaging::write_queue`.
//!
//! What is held in memory for a session, project or console is dropped where its record is
//! removed, the records the store removes by cascade included, except for what a later pass drops
//! by itself (the default efforts, which the record collector trims to the sessions on record).
//!
//! The types other modules name are re-exported here, so they are reached as `crate::state::X`.

mod agent_availability;
mod auto_switch;
mod background;
pub(crate) mod bindings;
mod claims;
mod forget;
mod git_status;
mod late_turn_end;
mod left_for_limit;
mod live;
mod mcp_tokens;
mod process_exit;
mod record_watch;
mod recoveries;
pub(crate) mod refusals;
mod requests;
pub(crate) mod session_as_left;
mod sessions;
mod sharing_lock;
mod sharing_status;
mod status;
#[cfg(test)]
pub(crate) mod test_support;
mod titles;
mod turns;

pub use crate::messaging::write_queue::{OutboxHold, WriteHold};
pub(crate) use auto_switch::AutoSwitchHooks;
#[cfg(test)]
pub(crate) use background::EMPTY_TASKS_DELAY;
pub(crate) use claims::session_archived;
pub use claims::{ClaimKind, LaunchClaim};
pub use git_status::GitCheckClaim;
pub use live::Running;
pub use sessions::Deletion;
pub use sharing_lock::AccountSharingLock;
pub use turns::{LastReport, TurnClose};

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::broadcast;

use crate::blocking_pass::StalledPasses;
use crate::messaging::outbox::Outbox;
use crate::protocol::{
    error_code, AgentAvailability, AgentId, Availability, CodedError, Event, SessionStatus,
};
use crate::store::Store;
use crate::terminal::saved_output::SavedOutput;

use self::background::BackgroundWork;
use self::git_status::{GitCheck, GitStatuses};
use self::left_for_limit::AccountLeft;
use self::live::LiveSessions;
use self::sharing_status::SharingStatus;
use self::turns::TurnState;

/// How long a message written to an idle session is given to be reported sent before it is taken
/// to be sitting unsent in the input box. A prompt's `UserPromptSubmit` normally arrives well
/// within a second of the Enter that sent it.
const SEND_WAIT: Duration = Duration::from_secs(3);

pub struct AppState {
    pub store: Store,
    pub port: u16,
    /// The secret every connection to the daemon has to carry (see `crate::server::access`).
    /// Generated at start and kept in memory; handed to the clients the daemon starts the way it
    /// hands them the port, so it is also in their arguments and environment and in the owner-only
    /// files of a session's scratch directory (the hook script, Grok's `config.toml`), which go
    /// with that directory.
    pub key: String,
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
    /// The accounts Octoboard moved each session off because a turn stopped on their usage limit,
    /// and when (`crate::coordinator::auto_switch`), so that it does not move the session straight
    /// back to one.
    left_for_limit: Mutex<HashMap<String, Vec<AccountLeft>>>,
    pub(crate) quota_recovery: crate::coordinator::auto_switch::RecoveryState,
    /// The waits to retry Codex turns that stopped at their model's capacity
    /// (`crate::messaging::capacity_retry`).
    pub(crate) capacity_retries: crate::messaging::capacity_retry::CapacityRetries,
    /// How the lower layers reach the automatic account switch (see [`auto_switch`]); none until
    /// installed.
    auto_switch: std::sync::OnceLock<AutoSwitchHooks>,
    /// How long a restart of a session (`crate::coordinator::restart`) waits for the process it
    /// ended to be recorded down, in place of the switch's generous wait
    /// (`crate::coordinator::launch::SWITCH_STOP_TIMEOUT`).
    #[cfg(test)]
    pub(crate) restart_stop_timeout: Mutex<Option<std::time::Duration>>,
    /// The generation each session's record watch (`crate::records::watch`, run for Claude Code by
    /// `crate::adapter::claude::transcript_watch` and for Codex by
    /// `crate::adapter::codex::rollout_watch`) is currently on. Starting a watch records a fresh
    /// value here; the watch keeps polling only while its own value is still the one recorded,
    /// which is what lets a newer watch for the same session retire an older one rather than race
    /// it. The counter it is drawn from is global, not per-session, because uniqueness is all that
    /// is asked of it.
    record_watch_generations: Mutex<HashMap<String, u64>>,
    next_record_watch_generation: std::sync::atomic::AtomicU64,
    /// How far into its record file each session's carried-offset watches have read
    /// (`crate::records::watch::Start::Carried`), with the file's path. A new turn's watch resumes
    /// from there rather than rescanning the whole file, which on a long session runs to
    /// megabytes; what an earlier watch read it has already judged.
    record_offsets: Mutex<HashMap<String, (std::path::PathBuf, u64)>>,
    /// The per-session write queues. Every message Octoboard sends into a running agent goes
    /// through them; see `crate::messaging::outbox`.
    pub(crate) outbox: Outbox,
    /// The hook events received and not yet applied, per session (`crate::hooks::queue`).
    pub hook_queues: crate::hooks::queue::HookQueues,
    /// Each project's live git status, broadcast as `project_git_status` on every change and
    /// replayed in a `snapshot`, with each project's in-flight push marker overlaying its activity
    /// while the push runs and excluding a second push of that project.
    /// Derived, never stored in SQLite — see `GitStatus`.
    git_statuses: RwLock<GitStatuses>,
    /// The ids of the projects whose git status is being checked right now, so
    /// `refresh_git_status` does not start a second check for one already in flight — a client
    /// polling faster than the checks finish, or several clients watching the same console, must
    /// not pile up work. Claimed and released through [`AppState::claim_git_check_queued`]; see
    /// [`GitCheck`] for what an entry carries.
    git_checks: Mutex<HashMap<String, GitCheck>>,
    /// When each project's last check completed, for the minimum-interval floor in
    /// `crate::project_git::status::due_for_check` — a second guard next to `git_checks` above,
    /// needed because that one only stops two checks from overlapping and does nothing about
    /// several clients each polling on their own 5-minute phase. Entries are removed wherever the
    /// project's `GitStatus` is, by `remove_git_status`, so a reused project id never inherits a
    /// stale timestamp. Each carries the wall-clock time too, in milliseconds since the epoch: the
    /// `Instant` stops while the machine sleeps, which suits the floor but not telling an agent how
    /// long ago the check was.
    git_check_completed: Mutex<HashMap<String, (Instant, i64)>>,
    /// Per repository, the gate every write to its branch origins takes and when rows were last
    /// added (`crate::branch_origin`).
    pub branch_origins: crate::branch_origin::Gates,
    /// The removal of the branches and worktrees bound to sessions archived or deleted: its gate
    /// per repository.
    pub branch_cleanups: crate::branch_cleanup::Cleanups,
    /// What requests reach the host through where a test stands in: `git`, the Trash.
    pub host_tools: crate::host_tools::HostTools,
    /// How long a message written to an idle Claude Code session is given to be reported sent, or
    /// `None` for no check at all; see `HookPrompts`. A seam for tests, whose stand-in sessions
    /// run no hooks and so never report sending anything.
    pub send_wait: Mutex<Option<Duration>>,
    pub record_hints: crate::records::Hints,
    /// The context window each Claude Code session's status line last reported.
    pub context_windows: crate::records::context::ReportedWindows,
    /// The repositories' refs under watch, so a commit made outside the daemon is re-read
    /// (`crate::project_git::watch`).
    pub git_watch: crate::project_git::watch::Watch,
    /// The repositories a session's git has reported moving refs of, with their re-read due or
    /// running (`crate::project_git::ref_moves`).
    pub ref_moves: crate::project_git::ref_moves::RefMoves,
    /// Each agent's availability and what its default account currently resolves to, derived once
    /// per daemon start (`crate::agents::availability`) and broadcast by its own event on change;
    /// never a field of `Store`'s settings row, which only the user's own updates write. Always
    /// holds all three agents, each starting `NotDetermined` — the state every run begins in.
    agent_availability: RwLock<HashMap<AgentId, AgentAvailability>>,
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
    /// The model lists read lately, per agent and account directory (`crate::agents::models`).
    pub agent_models: crate::agents::models::ModelCache,
    /// The plan quota last read for each account (`crate::accounts::quotas`). Held in memory only.
    pub account_quotas: crate::accounts::quotas::AccountQuotas,
    /// The logins running now, one per account (`crate::accounts::login`). Held in memory only.
    pub account_logins: crate::accounts::login::AccountLogins,
    /// The accounts a `delete_account` that moves the directory to the Trash is running for, each
    /// with the number of such removals; see [`AppState::begin_account_removal`]. Held in memory
    /// only. A plain mutex, never held across an `.await`.
    accounts_being_removed: Mutex<HashMap<String, (crate::protocol::Account, usize)>>,
    /// What the accounts' changes of sharing of the default configuration hold: one lock per
    /// agent over the default directory (`crate::accounts::shared_config`). Held in memory only.
    pub account_sharing: crate::accounts::shared_config::Sharing,
    /// Each account's sharing state as clients were last told it, with its change's progress and
    /// last failure (`sharing_status.rs`). Held in memory only.
    sharing_status: SharingStatus,
    /// The shell terminals running, or exited and not yet closed (`crate::terminal::shell`). Held
    /// in memory only: each is a process of this daemon's.
    pub shell_terminals: crate::terminal::shell::ShellTerminals,
    /// The paths looked at on the file system whose look outran its deadline and is still running,
    /// stuck once it has run long enough (`crate::hostfs::place::looked_within`). Held in memory
    /// only.
    pub directory_looks: StalledPasses<std::path::PathBuf>,
    /// The roots of each project a path link was last checked against, reused for a few seconds
    /// (`crate::path_links`). Held in memory only. Dropped with their project.
    pub path_link_roots: crate::path_links::RootsCache,
    /// For each session with a session request waiting, the status its agent's own events
    /// say it has, while the record shows `WaitingUser` for the request. Kept so that the daemon's
    /// decisions about the agent (can a message be written, did a turn end) keep reading the agent
    /// rather than the hand, and so the status can be restored when the request ends. Always taken
    /// before `session_requests`, never after. Dropped with the session's other bookkeeping.
    request_hands: Mutex<HashMap<String, SessionStatus>>,
    /// For each session whose process ended in the middle of a turn, that turn, so that its own
    /// normal end posted after the process had gone can still clear the paused flag the end set
    /// (`late_turn_end`). Held in memory only: such an end comes from the ended process's own
    /// hook forwarder, seconds after it, and a later daemon has a key that forwarder does not
    /// carry. Not dropped with the session's other bookkeeping, which goes with the process;
    /// dropped with the record.
    ended_turns: Mutex<HashMap<String, late_turn_end::EndedTurn>>,
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
    ///
    /// [`EMPTY_TASKS_DELAY`]: background::EMPTY_TASKS_DELAY
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
    /// Set once [`Self::stop_all_sessions`] has begun: no claim is granted and no process stays
    /// registered from then on, so a launch that was under way cannot start an agent after the
    /// stop.
    shutting_down: AtomicBool,
    /// The sessions an `interrupt_session` is sending keys into now (`crate::session::interrupt`),
    /// so two at once cannot send a second interrupt key. Held in memory only.
    interrupts: Mutex<HashSet<String>>,
    /// Per console, the lock that serialises associating projects with it
    /// ([`Self::claim_project_association`]). Held in memory only. A console's entry is removed
    /// when its last claim is dropped.
    project_associations: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Per console, the look at its projects' directories that outran its deadline and whose
    /// blocking thread is still stuck (`crate::coordinator::projects`). Held in memory only.
    pub stalled_directory_passes: StalledPasses<String>,
    /// The removal inspections' refusals that a path that could not be resolved decided, by token
    /// (`crate::removal_safety::project`). Held in memory only. Dropped with their project.
    pub unresolved_removals: crate::removal_safety::project::UnresolvedRemovals,
    /// The processes each agent launch started outside its terminal session, stopped when the
    /// launch ends (`crate::processes::ledger`).
    pub process_ledger: Arc<crate::processes::ledger::ProcessLedger>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: Store,
        port: u16,
        key: String,
        self_exe: String,
        saved_output_dir: std::path::PathBuf,
        consoles_dir: std::path::PathBuf,
        handoff_dir: std::path::PathBuf,
        process_ledger: Arc<crate::processes::ledger::ProcessLedger>,
    ) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            store,
            port,
            key,
            self_exe,
            live: RwLock::new(LiveSessions::default()),
            mcp_tokens: RwLock::new(HashMap::new()),
            turns: Mutex::new(HashMap::new()),
            left_for_limit: Mutex::new(HashMap::new()),
            quota_recovery: Default::default(),
            capacity_retries: Default::default(),
            auto_switch: Default::default(),
            #[cfg(test)]
            restart_stop_timeout: Mutex::new(None),
            request_hands: Mutex::new(HashMap::new()),
            ended_turns: Mutex::new(HashMap::new()),
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
            ref_moves: Default::default(),
            agent_availability: RwLock::new(
                AgentId::ALL
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
            accounts_being_removed: Default::default(),
            account_sharing: Default::default(),
            sharing_status: Default::default(),
            shell_terminals: Default::default(),
            directory_looks: Default::default(),
            path_link_roots: Default::default(),
            events,
            account_writes: Mutex::new(()),
            session_publication: Mutex::new(()),
            session_removals: Mutex::new(0),
            shutdown: tokio::sync::Notify::new(),
            shutting_down: AtomicBool::new(false),
            interrupts: Mutex::new(HashSet::new()),
            project_associations: Mutex::new(HashMap::new()),
            stalled_directory_passes: Default::default(),
            unresolved_removals: Default::default(),
            process_ledger,
        }
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

    /// Whether the daemon has begun to shut down, after which nothing is started.
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }

    fn refuse_while_shutting_down(&self) -> Result<()> {
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(CodedError::raised(
                error_code::DAEMON_SHUTTING_DOWN,
                "the daemon is shutting down, so nothing is started",
                &[],
            ));
        }
        Ok(())
    }
}

/// Broadcasts the current settings record — the pattern every settings-shaped mutation (the
/// trusted directories, `auto_pull_repositories` and the accounts) follows.
pub(crate) fn broadcast_settings(state: &AppState) -> Result<()> {
    state.broadcast(Event::SettingsUpdated {
        settings: state.store.get_settings()?,
    });
    Ok(())
}
