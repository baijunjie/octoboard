//! Shared daemon state, and the session status transitions that go with it.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::sync::broadcast;

use crate::protocol::{error_code, now_millis, CodedError, Event, Session, SessionStatus};
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
    events: broadcast::Sender<Event>,
    shutdown: tokio::sync::Notify,
}

impl AppState {
    pub fn new(store: Store, port: u16, self_exe: String) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            store,
            port,
            self_exe,
            live: RwLock::new(LiveSessions::default()),
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
            ));
        }
        if !live.launching.insert(id.to_string()) {
            return Err(CodedError::raised(
                error_code::SESSION_ALREADY_RUNNING,
                "this session is already being started",
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

    /// Whether any session of this console (or of one of its projects) is still running. Deleting a
    /// console or a project with live sessions is refused rather than silently killing them.
    pub fn has_live_sessions_where(&self, matches: impl Fn(&Session) -> bool) -> Result<bool> {
        for live in self.live_sessions() {
            if let Some(record) = self.store.get_session(&live.id)? {
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
        }
        drop(live);
        cleanup::unregister(session);
    }

    /// Reads a session record, failing with a message the UI can show when it is gone.
    pub fn session_record(&self, id: &str) -> Result<Session> {
        self.store
            .get_session(id)?
            .ok_or_else(|| anyhow!("unknown session {id}"))
    }

    pub fn publish_session(&self, session: &Session) {
        self.broadcast(Event::SessionUpserted {
            session: session.clone(),
        });
    }

    /// Writes a session record and tells every client about it.
    pub fn save_session(&self, session: &Session) -> Result<()> {
        self.store.update_session(session)?;
        self.publish_session(session);
        Ok(())
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
            match state.session_record(&live.id) {
                Ok(mut session) => {
                    if session.status != SessionStatus::Archived {
                        session.status = SessionStatus::Interrupted;
                    }
                    session.ended_at = Some(now_millis());
                    if let Err(err) = state.save_session(&session) {
                        tracing::warn!(session = %live.id, %err, "recording the session's exit failed");
                    }
                }
                Err(err) => {
                    tracing::warn!(session = %live.id, %err, "the exited session has no record")
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
