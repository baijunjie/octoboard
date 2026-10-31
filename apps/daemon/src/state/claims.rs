//! The claims on a session: launching or resuming its process, the exclusive claim of a switch,
//! a handoff or a binding change, an archive or deletion's claim against lock changes, and an
//! interrupt's claim, each released when its guard drops.

use std::sync::Arc;

use anyhow::Result;

use super::sharing_lock::locked_over;
use super::AppState;
use crate::protocol::{error_code, CodedError, Session};

/// A session's claim for one interrupt, taken by [`AppState::claim_interrupt`].
pub struct InterruptClaim {
    state: Arc<AppState>,
    id: String,
}

impl Drop for InterruptClaim {
    fn drop(&mut self) {
        self.state
            .interrupts
            .lock()
            .expect("interrupt lock poisoned")
            .remove(&self.id);
    }
}

/// What a session's claim is held for (see [`AppState::claim_kind`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    /// Launching or resuming its process.
    Launch,
    /// An account switch, a handoff, a handover of branches or a binding change.
    Switch,
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
    pub(super) state: Arc<AppState>,
    pub(super) id: String,
    pub(super) watches_launch: bool,
}

impl LaunchClaim {
    /// Whether the holder watches the process it launches under the claim come up and reports the
    /// outcome itself, so the exit watcher does not tell the owner that it ended on its own
    /// (`LiveSession::watch_under_claim`).
    pub fn watches_launch(&self) -> bool {
        self.watches_launch
    }
}

impl Drop for LaunchClaim {
    fn drop(&mut self) {
        self.state.release_launch(&self.id);
    }
}

impl AppState {
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
        if locked {
            if !self.store.set_session_locked_unless_archived(id)? {
                return Err(crate::coordinator::lifecycle::session_archived(id));
            }
        } else {
            self.store.set_session_locked(id, false)?;
        }
        Ok(())
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
        self.quota_recovery.cancel(id);
        self.refuse_while_shutting_down()?;
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
            watches_launch: false,
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
        self.refuse_while_shutting_down()?;
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
            watches_launch: false,
        })
    }

    /// [`Self::begin_switch`] for an account switch: the session is also shown as switching its
    /// account to every client until the claim is released. `watches_launch` is for Octoboard's own
    /// switch and the restore after it, which watch the process they launch come up and report the
    /// outcome themselves (see [`LaunchClaim::watches_launch`]); a switch someone asked for leaves
    /// the process to the exit watcher, which tells the owner of an end nobody asked for.
    pub fn begin_account_switch(
        self: &Arc<Self>,
        id: &str,
        watches_launch: bool,
    ) -> Result<LaunchClaim> {
        let mut claim = self.begin_switch(id)?;
        claim.watches_launch = watches_launch;
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

    /// Marks a session whose claim from [`Self::begin_switch`] the caller holds as restarted for a
    /// change of its model: until the claim is released every client is shown it `starting` (see
    /// [`Self::shown_status`]), so nothing offers to resume it while its process is down. Marked
    /// once the change has the claim, so a change that is then refused shows it starting only
    /// for as long as the checks take.
    pub fn mark_model_change(&self, id: &str) {
        let marked = {
            let mut live = self.live.write().expect("live sessions lock poisoned");
            live.switching.contains(id) && live.model_changes.insert(id.to_string())
        };
        if marked {
            self.publish_session_by_id(id);
        }
    }

    /// How long a change of a session's model waits for the process it ended to be recorded down:
    /// [`crate::coordinator::launch::SWITCH_STOP_TIMEOUT`], shortened by a test.
    pub(crate) fn model_change_stop_timeout(&self) -> std::time::Duration {
        #[cfg(test)]
        if let Some(shorter) = *self.model_change_stop_timeout.lock().unwrap() {
            return shorter;
        }
        crate::coordinator::launch::SWITCH_STOP_TIMEOUT
    }

    /// Whether a change of its model is restarting the session (see [`Self::mark_model_change`]).
    pub fn is_changing_model(&self, id: &str) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.model_changes.contains(id)
    }

    fn release_launch(&self, id: &str) {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        live.launching.remove(id);
        live.switching.remove(id);
        let was_switching_account = live.account_switches.remove(id);
        let was_changing_model = live.model_changes.remove(id);
        drop(live);
        if was_switching_account || was_changing_model {
            self.publish_session_by_id(id);
        }
        crate::shared_config::publish(self);
    }

    /// Claims the session for one interrupt, or `None` while another holds it. The claim ends when
    /// the guard drops, a panic included.
    pub fn claim_interrupt(self: &Arc<Self>, id: &str) -> Option<InterruptClaim> {
        self.quota_recovery.cancel(id);
        let mut interrupts = self.interrupts.lock().expect("interrupt lock poisoned");
        interrupts.insert(id.to_string()).then(|| InterruptClaim {
            state: self.clone(),
            id: id.to_string(),
        })
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

    /// Gives the exit watcher back the end of the session's process, which a launch under a claim
    /// that watches it (`LaunchClaim::watches_launch`) held: for a launch that failed after the
    /// process was registered, which nothing will see come up.
    pub fn release_launch_watch(&self, id: &str) {
        if let Some(live) = self.live_session(id) {
            live.release_watch();
        }
    }
}
