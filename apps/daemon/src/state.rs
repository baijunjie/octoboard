//! The daemon's shared state: the session records and their status transitions, the live sessions
//! and the claim that keeps two launches of the same session from racing, the per-session MCP
//! tokens, the turn bookkeeping a synthesised report rests on, a facade over the write queue in
//! `crate::outbox`, and each project's live git status together with the claim that keeps two
//! checks of one project from racing and the timestamp that keeps them from piling up across
//! several clients.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::broadcast;

use crate::outbox::{Drain, Outbox};
use crate::protocol::{
    error_code, notice_code, now_millis, CodedError, Event, GitStatus, Notice, Session,
    SessionStatus,
};
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
    /// The generation each session's transcript watch (`crate::transcript`) is currently on.
    /// Starting a watch records a fresh value here; the watch keeps polling only while its own
    /// value is still the one recorded, which is what lets a newer watch for the same session
    /// retire an older one rather than race it. The counter it is drawn from is global, not
    /// per-session, because uniqueness is all that is asked of it.
    transcript_watch_generations: Mutex<HashMap<String, u64>>,
    next_transcript_watch_generation: std::sync::atomic::AtomicU64,
    /// The per-session write queues. Every message Octoboard sends into a running agent goes
    /// through them; see `crate::outbox`.
    outbox: Outbox,
    /// Each project's live git status, broadcast as `project_git_status` on every change and
    /// replayed in a `snapshot`. Derived, never stored in SQLite — see `GitStatus`.
    git_statuses: RwLock<HashMap<String, GitStatus>>,
    /// The ids of the projects whose git status is being checked right now, so
    /// `refresh_git_status` does not start a second check for one already in flight — a client
    /// polling faster than the checks finish, or several clients watching the same console, must
    /// not pile up work. Claimed and released through [`AppState::claim_git_check`].
    git_checks: Mutex<HashSet<String>>,
    /// When each project's last check completed, for the minimum-interval floor in
    /// `git_status::due_for_check` — a second guard next to `git_checks` above, needed because that
    /// one only stops two checks from overlapping and does nothing about several clients each
    /// polling on their own 5-minute phase. Entries are removed wherever the project's `GitStatus`
    /// is, by `remove_git_status`, so a reused project id never inherits a stale timestamp.
    git_check_completed: Mutex<HashMap<String, Instant>>,
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
    pub fn new(store: Store, port: u16, self_exe: String) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            store,
            port,
            self_exe,
            live: RwLock::new(LiveSessions::default()),
            mcp_tokens: RwLock::new(HashMap::new()),
            turns: Mutex::new(HashMap::new()),
            transcript_watch_generations: Mutex::new(HashMap::new()),
            next_transcript_watch_generation: std::sync::atomic::AtomicU64::new(0),
            outbox: Outbox::default(),
            git_statuses: RwLock::new(HashMap::new()),
            git_checks: Mutex::new(HashSet::new()),
            git_check_completed: Mutex::new(HashMap::new()),
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
    /// already being launched; the claim is released when the returned guard is dropped, which
    /// `register_live` does not do — the session is then in the map instead.
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

    /// Whether any session being launched or resumed right now matches. Its process is not yet
    /// registered, so it cannot be stopped; whatever it belongs to must not be deleted under it.
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
        cleanup::register(session.clone());
        let mut live = self.live.write().expect("live sessions lock poisoned");
        live.launching.remove(&session.id);
        live.sessions.insert(session.id.clone(), session);
    }

    /// Drops a session from the live map — but only if it is still *this* session. Comparing by id
    /// would let a finished session's exit watcher evict the live successor that reused its id.
    fn unregister_live(&self, session: &Arc<LiveSession>) {
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
        self.forget_session_bookkeeping(&session.id);
        cleanup::unregister(session);
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

    /// Deletes the session's record if it is archived, and broadcasts `session_deleted` when it
    /// was. `false` means it was not archived, is gone already, or is being launched right now (a
    /// resume in flight). Checked and deleted under the live-sessions lock (`live`), so a resume
    /// cannot claim the session between the two. An archived session whose process is still
    /// exiting is deleted: its exit watcher finds no record and skips.
    pub fn delete_if_archived(&self, id: &str) -> Result<bool> {
        let live = self.live.read().expect("live sessions lock poisoned");
        if live.launching.contains(id) || !self.store.delete_session_if_archived(id)? {
            return Ok(false);
        }
        drop(live);
        self.broadcast(Event::SessionDeleted {
            session: id.to_string(),
        });
        Ok(true)
    }

    /// Applies a status reported by a hook. Dormant statuses are never reached this way — a session
    /// becomes archived by the user's request and interrupted by its process going away, both of
    /// which are decided here rather than by the agent.
    pub fn apply_hook_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        let mut session = self.session_record(id)?;
        if session.status.is_dormant() || session.status == status {
            return Ok(());
        }
        session.status = status;
        self.save_session(&session)
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

    /// A turn has begun. This is also what re-arms the turn-end signal, so a session that reported
    /// in its previous turn is not credited for this one.
    pub fn turn_started(&self, id: &str) {
        let mut turns = self.turns.lock().expect("turn lock poisoned");
        let turn = turns.entry(id.to_string()).or_default();
        turn.open = true;
        turn.reported = false;
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
        self.transcript_watch_generations
            .lock()
            .expect("transcript watch lock poisoned")
            .remove(id);
    }

    // -- transcript watch bookkeeping -----------------------------------------

    /// Starts a new transcript watch for this session and returns the generation it owns.
    /// Recording it here retires whatever watch was running for the session before: its generation
    /// is no longer the one found under this id, so it stops at its next poll instead of racing the
    /// new one. See `crate::transcript::watch_for_rejection`.
    pub fn begin_transcript_watch(&self, id: &str) -> u64 {
        let generation = self
            .next_transcript_watch_generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.transcript_watch_generations
            .lock()
            .expect("transcript watch lock poisoned")
            .insert(id.to_string(), generation);
        generation
    }

    /// Whether `generation` is still this session's current transcript watch — false once a newer
    /// watch has taken over, or the session's bookkeeping has been dropped entirely.
    pub fn transcript_watch_current(&self, id: &str, generation: u64) -> bool {
        self.transcript_watch_generations
            .lock()
            .expect("transcript watch lock poisoned")
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
        // `terminate` waits out the graceful period, so it goes to a blocking thread.
        let handles: Vec<_> = sessions
            .into_iter()
            .map(|live| tokio::task::spawn_blocking(move || live.terminate()))
            .collect();
        for handle in handles {
            let _ = handle.await;
        }
    }

    // -- git status ------------------------------------------------------------

    /// Claims the right to check this project's git status now, released when the returned guard
    /// is dropped — including when the check panics, so a wedged step cannot leave its project
    /// excluded from every later sweep for the rest of the daemon's life. `None` means a check for
    /// it is already running and the caller must not start another.
    pub fn claim_git_check(self: &Arc<Self>, project_id: &str) -> Option<GitCheckClaim> {
        if !self
            .git_checks
            .lock()
            .expect("git check lock poisoned")
            .insert(project_id.to_string())
        {
            return None;
        }
        Some(GitCheckClaim {
            state: self.clone(),
            project_id: project_id.to_string(),
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
        match self.store.get_project(&status.project) {
            Ok(None) => {
                self.remove_git_status(&status.project);
                return;
            }
            // A store error says nothing about whether the project is actually gone, so this
            // fails open rather than risk dropping a status that is still perfectly valid.
            Ok(Some(_)) | Err(_) => {}
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
}

#[derive(Default)]
struct LiveSessions {
    sessions: HashMap<String, Arc<LiveSession>>,
    launching: HashSet<String>,
}

/// The right to start a process for one session, released on drop. Taken before the launch and
/// surrendered by `register_live` once the session is in the live map.
pub struct LaunchClaim {
    state: Arc<AppState>,
    id: String,
}

impl Drop for LaunchClaim {
    fn drop(&mut self) {
        self.state.release_launch(&self.id);
    }
}

/// The right to check one project's git status right now, released on drop — including by a panic
/// unwinding through it, which is what keeps a wedged step from excluding its project from every
/// later sweep for the rest of the daemon's life. See `AppState::claim_git_check`.
pub struct GitCheckClaim {
    state: Arc<AppState>,
    project_id: String,
}

impl Drop for GitCheckClaim {
    fn drop(&mut self) {
        self.state.release_git_check(&self.project_id);
    }
}

/// The last-resort cleanup path: a registry of running sessions that can be killed without taking
/// any lock that a panicking or signalled thread might already hold.
pub mod cleanup {
    use std::sync::{Arc, Mutex};

    use crate::session::LiveSession;

    static REGISTRY: Mutex<Vec<Arc<LiveSession>>> = Mutex::new(Vec::new());

    pub fn register(session: Arc<LiveSession>) {
        if let Ok(mut registry) = REGISTRY.lock() {
            registry.push(session);
        }
    }

    pub fn unregister(session: &Arc<LiveSession>) {
        if let Ok(mut registry) = REGISTRY.lock() {
            registry.retain(|registered| !Arc::ptr_eq(registered, session));
        }
    }

    /// Kills every registered session outright. Safe from a panic hook: it only reads a pid and an
    /// atomic flag, and the kill is gated on the child not having been reaped, so the pid cannot
    /// have been recycled into something unrelated.
    pub fn kill_all() {
        let sessions = match REGISTRY.lock() {
            Ok(registry) => registry.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        for session in sessions {
            session.kill_hard();
        }
    }
}

/// Installs a panic hook that kills every agent process before unwinding, so a daemon crash never
/// leaves agents running unattended — `portable-pty` calls `setsid()`, so they are not in the
/// daemon's process group and nothing else would reach them.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "panicking — killing every agent process first");
        cleanup::kill_all();
        default_hook(info);
    }));
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn app_state(name: &str) -> AppState {
        let dir = std::env::temp_dir().join(format!(
            "octoboardd-state-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("temporary directory");
        let store = Store::open(&dir.join("octoboard.db")).expect("store");
        AppState::new(store, 1234, "/opt/octoboardd".to_string())
    }

    /// A stand-in live session on a real PTY, with `/bin/sh` running `script` behind it instead of
    /// a real agent: enough for whatever the caller needs a `LiveSession` to write into, read from,
    /// or just stay alive on until dropped. Shared by `transcript.rs`, `reporting.rs` and
    /// `trust.rs`'s tests, which used to each keep a near-identical copy of this.
    pub(crate) fn fake_live_session(
        id: &str,
        agent: crate::protocol::Agent,
        cols: u16,
        rows: u16,
        script: &str,
    ) -> Arc<LiveSession> {
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};

        let pty = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("a PTY");
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", script]);
        let child = pty.slave.spawn_command(cmd).expect("the stand-in");
        let pid = child.process_id().expect("a pid");
        let fd = pty.master.as_raw_fd().expect("a descriptor");
        crate::ptyio::set_nonblocking(fd).expect("non-blocking");
        Arc::new(LiveSession::new(crate::session::NewSession {
            id: id.to_string(),
            agent,
            pid,
            fd,
            master: pty.master,
            child,
            scratch_dir: None,
            resolves_approvals_itself: false,
        }))
    }

    /// Only one report may come out of one turn, and only when the session did not report for
    /// itself. The repeat case is not hypothetical: Grok fires its `idle_prompt` backstop about a
    /// minute after every turn, whether or not a `Stop` already reported it.
    #[test]
    fn a_turn_can_only_be_closed_once() {
        let state = app_state("turns");
        // Nothing has started, so nothing is owed a report — a stop event arriving before any turn
        // start (an agent's own teardown, a slash command) must not produce one.
        assert_eq!(state.close_turn("s", false), TurnClose::NoTurn);

        state.turn_started("s");
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        assert_eq!(state.close_turn("s", false), TurnClose::NoTurn);

        state.turn_started("s");
        state.mark_reported("s");
        assert_eq!(state.close_turn("s", false), TurnClose::Reported);

        // A session credited for one turn is not credited for the next.
        state.turn_started("s");
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// A clock-attributed end left over from a finished turn must not close the turn running now,
    /// or the console session gets a report for work still in flight and the real end finds nothing
    /// open.
    #[test]
    fn a_clock_attributed_end_is_refused_while_the_open_turn_is_still_active() {
        let state = app_state("backstop");
        state.turn_started("s");
        assert_eq!(state.close_turn("s", true), TurnClose::NotThisTurn);
        // Still open, so its own end is still reportable.
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// The quiet gate alone is not enough: Grok emits nothing between `PreToolUse` and
    /// `PostToolUse`, so a tool call longer than the backstop delay leaves a working turn looking
    /// quiet and the previous turn's echo would close it. The echo has to be consumed instead.
    #[test]
    fn the_echo_of_an_ending_already_acted_on_is_consumed_not_acted_on() {
        let state = app_state("echo");
        state.turn_started("s");
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        assert!(state.echo_pending("s"));

        // The next turn begins and is still running when the previous turn's echo arrives.
        state.turn_started("s");
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
        let state = app_state("echo-idle");
        state.turn_started("s");
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
        let state = app_state("reported");
        state.turn_started("s");
        state.mark_reported("s");
        state.clear_reported("s");
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// A cancelled turn owes no report, but leaving it open is what lets a later backstop invent
    /// one for it.
    #[test]
    fn an_abandoned_turn_is_closed_without_owing_a_report() {
        let state = app_state("abandon");
        state.turn_started("s");
        state.abandon_turn("s");
        assert_eq!(state.close_turn("s", false), TurnClose::NoTurn);
    }

    /// A token is per launch: the previous process is gone, and a token that outlived it would let
    /// a stale child act on the session.
    #[test]
    fn issuing_a_token_retires_the_sessions_previous_one() {
        let state = app_state("tokens");
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
        let state = app_state("outbox");
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
        let state = Arc::new(app_state("git-claim-panic"));
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

    /// A status published for a project no longer in the store must be dropped rather than
    /// reinserted — the race `DeleteProject` leaves open when its check is already in flight, and
    /// the gap `DeleteConsole` leaves for every project of a deleted console.
    #[test]
    fn publishing_a_status_for_a_project_no_longer_in_the_store_drops_it() {
        let state = app_state("git-status-missing-project");
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
        let state = app_state("git-status-missing-project-completion");
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
