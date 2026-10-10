//! The daemon's shared state: the session records and their status transitions, the live sessions
//! and the claim that keeps two launches of the same session from racing, the per-session MCP
//! tokens, the turn bookkeeping a synthesised report rests on, a facade over the write queue in
//! `crate::outbox`, and each project's live git status together with the claim that keeps two
//! checks of one project from racing and the timestamp that keeps them from piling up across
//! several clients, each ended session's saved terminal output, and the requests for a console
//! session waiting for the user's answer (`crate::console_request`) together with the status
//! each of those sessions' agents is at while the request holds the record at waiting for the
//! user.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::broadcast;

use crate::outbox::{Drain, Outbox};
use crate::protocol::{
    error_code, notice_code, now_millis, Agent, AgentAvailability, Availability, CodedError, Event,
    GitStatus, Notice, Session, SessionStatus,
};
use crate::saved_output::SavedOutput;
use crate::session::LiveSession;
use crate::store::Store;

/// How often a session's process is polled for having exited. The agent's own exit (the user
/// typing `/exit`, a crash) has no other signal: `SessionEnd` hooks are not reliable for this,
/// since a killed process fires nothing at all.
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(500);

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
    /// Each project's live git status, broadcast as `project_git_status` on every change and
    /// replayed in a `snapshot`. Derived, never stored in SQLite — see `GitStatus`.
    git_statuses: RwLock<HashMap<String, GitStatus>>,
    /// The ids of the projects whose git status is being checked right now, so
    /// `refresh_git_status` does not start a second check for one already in flight — a client
    /// polling faster than the checks finish, or several clients watching the same console, must
    /// not pile up work. Claimed and released through [`AppState::claim_git_check`]. The value is
    /// whether a manual sync (`sync_project_git`) arrived while the check ran and is owed a rerun.
    git_checks: Mutex<HashMap<String, bool>>,
    /// When each project's last check completed, for the minimum-interval floor in
    /// `git_status::due_for_check` — a second guard next to `git_checks` above, needed because that
    /// one only stops two checks from overlapping and does nothing about several clients each
    /// polling on their own 5-minute phase. Entries are removed wherever the project's `GitStatus`
    /// is, by `remove_git_status`, so a reused project id never inherits a stale timestamp.
    git_check_completed: Mutex<HashMap<String, Instant>>,
    /// Each agent's availability and what its default account currently resolves to, derived once
    /// per daemon start (`crate::availability`) and broadcast by its own event on change; never a
    /// field of `Store`'s settings row, which only the user's own updates write. Always holds all
    /// three agents, each starting `NotDetermined` — the state every run begins in.
    agent_availability: RwLock<HashMap<Agent, AgentAvailability>>,
    /// Each ended session's last terminal output, written as its process ends and removed with its
    /// record.
    saved_output: SavedOutput,
    /// The bounds every connection's browse requests share: how many reads run at once, and how
    /// many bytes their replies may hold before they are written.
    pub browse_gate: Arc<crate::browse::lane::Gate>,
    /// The `request_console_session` calls waiting for the user's answer; see
    /// `crate::console_request`. Held in memory only: no caller outlives the daemon.
    pub console_requests: crate::console_request::ConsoleRequests,
    /// For each session with a console-session request waiting, the status its agent's own events
    /// say it has, while the record shows `WaitingUser` for the request. Kept so that the daemon's
    /// decisions about the agent (can a message be written, did a turn end) keep reading the agent
    /// rather than the hand, and so the status can be restored when the request ends. Always taken
    /// before `console_requests`, never after. Dropped with the session's other bookkeeping.
    request_hands: Mutex<HashMap<String, SessionStatus>>,
    events: broadcast::Sender<Event>,
    shutdown: tokio::sync::Notify,
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

impl AppState {
    pub fn new(
        store: Store,
        port: u16,
        self_exe: String,
        saved_output_dir: std::path::PathBuf,
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
            record_watch_generations: Mutex::new(HashMap::new()),
            next_record_watch_generation: std::sync::atomic::AtomicU64::new(0),
            record_offsets: Mutex::new(HashMap::new()),
            outbox: Outbox::default(),
            git_statuses: RwLock::new(HashMap::new()),
            git_checks: Mutex::new(HashMap::new()),
            git_check_completed: Mutex::new(HashMap::new()),
            agent_availability: RwLock::new(
                [Agent::Claude, Agent::Codex, Agent::Grok]
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
            browse_gate: Arc::new(crate::browse::lane::Gate::with_budget()),
            console_requests: Default::default(),
            events,
            shutdown: tokio::sync::Notify::new(),
        }
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

    pub fn broadcast(&self, event: Event) {
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
    pub fn begin_launch(self: &Arc<Self>, id: &str) -> Result<LaunchClaim> {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        if live.sessions.contains_key(id) {
            return Err(CodedError::raised(
                error_code::SESSION_ALREADY_RUNNING,
                "this session is already running",
                &[("session", id)],
            ));
        }
        if !live.launching.insert(id.to_string()) {
            return Err(CodedError::raised(
                error_code::SESSION_ALREADY_STARTING,
                "this session is already being started",
                &[("session", id)],
            ));
        }
        Ok(LaunchClaim {
            state: self.clone(),
            id: id.to_string(),
        })
    }

    /// Claims the right to relaunch this session for a switch of its account, which — unlike
    /// [`Self::begin_launch`] — may be taken while its process is still running, because ending
    /// that process is the switch's first step. It holds the same exclusion a launch does, for as
    /// long as the guard lives: the switch keeps it from before the process is ended until after
    /// the relaunched process has been judged to have come up or not, and any revert of the
    /// recorded account is done, so a resume, a second switch and a delete are all refused for
    /// that whole window — while the process is down, while it is being watched, and while the
    /// account is put back. Fails only when a launch, a resume or another switch of it is under way.
    pub fn begin_switch(self: &Arc<Self>, id: &str) -> Result<LaunchClaim> {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        if !live.launching.insert(id.to_string()) {
            return Err(CodedError::raised(
                error_code::SESSION_ALREADY_STARTING,
                "this session is already being started",
                &[("session", id)],
            ));
        }
        Ok(LaunchClaim {
            state: self.clone(),
            id: id.to_string(),
        })
    }

    fn release_launch(&self, id: &str) {
        self.live
            .write()
            .expect("live sessions lock poisoned")
            .launching
            .remove(id);
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

    /// Whether the session has a process right now, or is being launched, resumed or switched and
    /// so is about to. This is the line archiving a console session draws, and it is not the
    /// session's status: an interrupted session has none and an archived one may still be exiting
    /// (its record says archived, so callers ask about the ones that are not).
    pub fn has_process(&self, id: &str) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.sessions.contains_key(id) || live.launching.contains(id)
    }

    /// Whether a launch, resume or switch of the session is under way: its claim is held. Unlike
    /// [`Self::has_process`] this is false for a session whose process is up and whose claim is
    /// already released.
    pub fn is_launching(&self, id: &str) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.launching.contains(id)
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

    pub fn register_live(&self, session: Arc<LiveSession>) {
        session.enter_crash_cleanup();
        let mut live = self.live.write().expect("live sessions lock poisoned");
        live.sessions.insert(session.id.clone(), session);
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
            let dropped = self.outbox.forget(&session.id);
            if dropped > 0 {
                tracing::warn!(session = %session.id, dropped, "the session ended with messages still queued");
            }
        }
        drop(live);
        if is_current {
            // Its call has nobody left to return to.
            crate::console_request::withdraw_for_session(self, &session.id);
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
        self.broadcast(Event::SessionUpserted {
            session: session.clone(),
        });
    }

    /// Writes a session record and tells every client about it. A record deleted in the meantime
    /// is not written and not announced: publishing it would leave a ghost row in every client.
    pub fn save_session(&self, session: &Session) -> Result<()> {
        if self.store.update_session(session)? {
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
        }
        Ok(())
    }

    /// Deletes the session's record and its saved output if it is archived, and broadcasts
    /// `session_deleted` when it was. `false` means it was not archived, is gone already, or is
    /// being launched right now (a resume in flight). Checked and deleted under the live-sessions
    /// lock (`live`), so a resume cannot claim the session between the two. An archived session
    /// whose process is still exiting is deleted: its exit watcher finds no record and skips.
    pub fn delete_if_archived(&self, id: &str) -> Result<bool> {
        let live = self.live.read().expect("live sessions lock poisoned");
        if live.launching.contains(id) || !self.store.delete_session_if_archived(id)? {
            return Ok(false);
        }
        drop(live);
        self.forget_saved_output(id);
        self.forget_request_hand(id);
        self.broadcast(Event::SessionDeleted {
            session: id.to_string(),
        });
        Ok(true)
    }

    /// Applies a status reported by a hook, unless a waiting console-session request is holding the
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

    /// Raises the hand of a session whose console-session request has started waiting, remembering
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
        if self.console_requests.is_waiting_for(id) {
            return;
        }
        let Some(remembered) = hands.remove(id) else {
            return;
        };
        if remembered != SessionStatus::WaitingUser {
            if let Err(err) =
                self.apply_reported_status_if(id, SessionStatus::WaitingUser, remembered)
            {
                tracing::debug!(session = %id, %err, "lowering the console session request's hand failed");
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

    /// The status the session's agent is at, which is the record's except while a console-session
    /// request holds the hand up. Whatever the daemon decides about the agent itself reads this;
    /// the record is what the window shows.
    pub fn agent_status(&self, session: &Session) -> SessionStatus {
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

    /// Takes that credit back, for a report that turned out not to reach the console session. Left
    /// set, the session's stop would produce no synthesised report either and the console session
    /// would get nothing at all.
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
    pub fn queue_message(&self, id: &str, text: &str) {
        self.outbox.push(id, text);
    }

    /// Writes out whatever is queued for this session, and says what became of it.
    ///
    /// **Blocks** on the PTY write, so callers on an async path go through
    /// [`Self::spawn_flush_outbox`].
    pub fn flush_outbox(&self, id: &str, status: SessionStatus) -> Drain {
        // The gate is the session's hook-reported state, never the terminal: a write while a modal
        // dialog is up has its trailing Enter confirm whatever option is highlighted.
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
        // that answers it, its first hook or the end of the watch releases the queue.
        if live.trust.holds_writes() {
            return Drain::Pending;
        }
        let outcome = self.outbox.drain(&live);
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

    /// Throws away anything queued for a session that will never take it — a relaunch that failed,
    /// so nothing would ever release what was queued ahead of it.
    pub fn discard_outbox(&self, id: &str) {
        self.outbox.forget(id);
    }

    fn forget_session_bookkeeping(&self, id: &str) {
        self.revoke_mcp_tokens(id);
        self.turns.lock().expect("turn lock poisoned").remove(id);
        self.forget_request_hand(id);
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

    /// Keeps what this session's process printed last, now that it has exited. **Blocks** on the
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

    /// What this session's process printed last, if its output was kept.
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

    // -- git status ------------------------------------------------------------

    /// Claims the right to check this project's git status now, released by
    /// [`GitCheckClaim::release_unless_rerun_owed`] or, if the check panics, when the returned
    /// guard is dropped, so a wedged step cannot leave its project excluded from every later
    /// sweep for the rest of the daemon's life. `None` means a check for it is already running and
    /// the caller must not start another.
    pub fn claim_git_check(self: &Arc<Self>, project_id: &str) -> Option<GitCheckClaim> {
        self.claim_git_check_with(project_id, false)
    }

    /// As [`Self::claim_git_check`], for a manual sync: when a check is already running, nothing
    /// is claimed, but the running check's claim is marked as owing a forced rerun, which
    /// [`GitCheckClaim::release_unless_rerun_owed`] hands back to it. The marking and the claim
    /// share one lock, so a request cannot slip in between the owner's last look and its release.
    pub fn claim_git_check_forced(self: &Arc<Self>, project_id: &str) -> Option<GitCheckClaim> {
        self.claim_git_check_with(project_id, true)
    }

    fn claim_git_check_with(
        self: &Arc<Self>,
        project_id: &str,
        owe_rerun: bool,
    ) -> Option<GitCheckClaim> {
        let mut checks = self.git_checks.lock().expect("git check lock poisoned");
        if let Some(rerun_owed) = checks.get_mut(project_id) {
            *rerun_owed |= owe_rerun;
            return None;
        }
        checks.insert(project_id.to_string(), false);
        Some(GitCheckClaim {
            state: self.clone(),
            project_id: project_id.to_string(),
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
        self.git_statuses
            .write()
            .expect("git statuses lock poisoned")
            .insert(status.project.clone(), status.clone());
        self.broadcast(Event::ProjectGitStatus { status });
    }

    /// Every git status the daemon currently holds, for `Event::Snapshot`.
    pub fn git_statuses(&self) -> Vec<GitStatus> {
        self.git_statuses
            .read()
            .expect("git statuses lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// The git status the daemon currently holds for one project, if any.
    pub fn git_status(&self, project_id: &str) -> Option<GitStatus> {
        self.git_statuses
            .read()
            .expect("git statuses lock poisoned")
            .get(project_id)
            .cloned()
    }

    /// Drops a project's git status. Called when the project itself is removed, and when a whole
    /// console is — nothing else ever cleans up these entries, since the daemon keeps no timer of
    /// its own that would otherwise notice a project is gone.
    pub fn remove_git_status(&self, project_id: &str) {
        self.git_statuses
            .write()
            .expect("git statuses lock poisoned")
            .remove(project_id);
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
        [Agent::Claude, Agent::Codex, Agent::Grok]
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
}

/// The right to start a process for one session, released on drop, and only then: it outlives
/// `register_live`, so the holder decides how much of what follows a launch it covers.
pub struct LaunchClaim {
    state: Arc<AppState>,
    id: String,
}

impl Drop for LaunchClaim {
    fn drop(&mut self) {
        self.state.release_launch(&self.id);
    }
}

/// The right to check one project's git status right now, released by `release_unless_rerun_owed`
/// or, when a panic unwinds through the holder, on drop, which is what keeps a wedged step from
/// excluding its project from every later sweep for the rest of the daemon's life. See
/// `AppState::claim_git_check`.
pub struct GitCheckClaim {
    state: Arc<AppState>,
    project_id: String,
    released: bool,
}

impl GitCheckClaim {
    /// Releases the claim, unless a manual sync arrived while it was held: then the claim is kept,
    /// the owed rerun is cleared and `true` is returned, for the holder to run one more check that
    /// fast-forwards. Done under the lock `claim_git_check_forced` marks it under, so a request is
    /// either seen here or finds the project unclaimed.
    pub fn release_unless_rerun_owed(&mut self) -> bool {
        let mut checks = self
            .state
            .git_checks
            .lock()
            .expect("git check lock poisoned");
        if let Some(rerun_owed) = checks.get_mut(&self.project_id) {
            if std::mem::take(rerun_owed) {
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
            has_conversation: false,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// Stores a session record, with the console it belongs to.
    fn insert_record(state: &AppState, dir: &std::path::Path, id: &str, status: SessionStatus) {
        state
            .store
            .insert_console(&crate::protocol::Console {
                id: "console-1".to_string(),
                name: "Console".to_string(),
                workdir: dir.join("workdir").to_string_lossy().into_owned(),
                console_session_agent: Agent::Claude,
                default_agent: Agent::Claude,
                claude_account_id: None,
                codex_account_id: None,
                grok_account_id: None,
                icon: None,
                created_at: 0,
            })
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

    /// A check that panics must still release its project's claim, or a wedged step excludes that
    /// project from every later sweep for the rest of the daemon's life — `spawn_check` relies on
    /// unwinding through the claim's `Drop` rather than an explicit `end_git_check` call.
    #[test]
    fn a_git_check_claim_is_released_even_if_the_check_panics() {
        let (state, _dir) = app_state("state-git-claim-panic");
        let claim = state.claim_git_check("p1").expect("the first claim");
        assert!(state.claim_git_check("p1").is_none());

        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _claim = claim;
            panic!("simulated check failure");
        }));
        assert!(unwound.is_err());

        // The panic unwound through the claim's `Drop`, releasing it for the next sweep.
        assert!(state.claim_git_check("p1").is_some());
    }

    /// A manual sync that finds a check running must not be lost: it makes the holder run once
    /// more, once, and only then release.
    #[test]
    fn a_manual_sync_during_a_check_is_owed_one_rerun() {
        let (state, _dir) = app_state("state-git-claim-rerun");
        let mut claim = state.claim_git_check("p1").expect("the first claim");
        assert!(state.claim_git_check_forced("p1").is_none());
        assert!(state.claim_git_check_forced("p1").is_none());

        assert!(
            claim.release_unless_rerun_owed(),
            "the request is handed back"
        );
        assert!(
            state.claim_git_check("p1").is_none(),
            "the claim is still held for the rerun"
        );
        assert!(
            !claim.release_unless_rerun_owed(),
            "owed once, however many asked"
        );
        assert!(state.claim_git_check("p1").is_some(), "then it is released");
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
