//! Automatic switching and continuation of turns stopped by an account's usage limit.
//! Waits are scoped to the failed turn and live process; successful quota probes reconsider
//! only those waits. For the lifecycle and recovery rules see
//! docs/product/automatic-account-switch.md's "Automatic account switch" section.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::account_switch::{self, NoLongerDue, SwitchOnlyIf};
use super::launch::LaunchEdit;
use super::{open, restart};
use crate::messaging::delivery::{self, WhenBlocked};
use crate::messaging::owner_notices;
use crate::messaging::texts::NOTICE_PREFIX;
use crate::protocol::{
    error_code, error_parts, notice_code, Agent, CodedError, Notice, Session, SessionStatus,
    DEFAULT_ACCOUNT_NAME,
};
use crate::session::term::{Launch, LaunchRequest};
use crate::state::session_as_left::SessionAsLeft;
use crate::state::{AppState, AutoSwitchHooks};

/// How long after moving a session off an account for its usage limit the session is not moved
/// back onto that account.
const LEFT_FOR_LIMIT_FOR: Duration = Duration::from_secs(30 * 60);

/// How soon a used-up quota window must reset for a session to wait on its account instead of
/// being moved. A switch relaunches the agent and carries the conversation over, which costs more
/// than waiting a few minutes for the account to come back.
const HOLD_FOR_RESET: Duration = Duration::from_secs(5 * 60);

/// How long past the reset a held session waits for its own account to be read as recovered before
/// it is judged as any other: the grace before the account is read again, a tick of the poller and
/// the reading itself, which can take up to a minute and queue behind the readings of other
/// accounts.
const HOLD_SLACK: Duration = Duration::from_secs(3 * 60);

/// How long a failed turn waits for another claim on its session to end before it is judged. A
/// switch keeps its claim past the relaunch until the process has stayed up a few seconds, and the
/// turn it continues may fail within that time.
const CLAIM_WAIT: Duration = Duration::from_secs(15);

/// Runtime waits belong to the failed turn and process, never to an idle session in general.
pub(crate) struct RecoveryState {
    inner: std::sync::Mutex<RecoveryInner>,
    slots: tokio::sync::Semaphore,
    #[cfg(test)]
    launch: std::sync::Mutex<Option<RecoveryLaunch>>,
}

impl Default for RecoveryState {
    fn default() -> Self {
        Self {
            inner: Default::default(),
            slots: tokio::sync::Semaphore::new(4),
            #[cfg(test)]
            launch: Default::default(),
        }
    }
}

#[cfg(test)]
#[derive(Clone)]
struct RecoveryLaunch {
    env: HashMap<String, String>,
    launch: Arc<dyn Fn(LaunchRequest) -> Result<Launch> + Send + Sync>,
}

/// The next [`PendingRecovery::seq`].
static NEXT_TRIGGER: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct RecoveryInner {
    generations: HashMap<String, Arc<()>>,
    newest: HashMap<String, u64>,
    waiting: HashMap<String, PendingRecovery>,
}

#[derive(Clone)]
pub(crate) struct PendingRecovery {
    session: SessionAsLeft,
    generation: Arc<()>,
    /// Tells this trigger from another of the same session (the clones of one share it), which the
    /// generation does not: a session's later failed turn replaces the trigger parked for an
    /// earlier one.
    token: Arc<()>,
    since: i64,
    /// Orders the triggers of a session by when they were captured, which `since` cannot where two
    /// fall in the same millisecond.
    seq: u64,
    verified: Option<(Option<String>, Option<String>, i64)>,
    /// While set and not yet passed (milliseconds since the epoch), the trigger is held for its
    /// own account's reset and only a reading of that account resumes it.
    hold_until: Option<i64>,
}

impl RecoveryState {
    pub fn cancel(&self, id: &str) {
        let mut inner = self.inner.lock().expect("quota recovery lock poisoned");
        inner.generations.remove(id);
        inner.newest.remove(id);
        inner.waiting.remove(id);
    }

    /// Has the session `id` waiting for quota, as a failed turn on a usage limit leaves it.
    #[cfg(test)]
    pub(crate) fn wait_for_test(&self, state: &Arc<AppState>, id: &str) {
        if let Some(trigger) = self.capture(state, id).ok().flatten() {
            self.inner
                .lock()
                .expect("quota recovery lock poisoned")
                .waiting
                .insert(id.to_string(), trigger);
        }
    }

    /// Whether the session `id` is waiting for quota (see [`Self::wait_for_test`]).
    #[cfg(test)]
    pub(crate) fn is_waiting_for_test(&self, id: &str) -> bool {
        let inner = self.inner.lock().expect("quota recovery lock poisoned");
        inner.waiting.contains_key(id)
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock().expect("quota recovery lock poisoned");
        inner.waiting.clear();
        inner.generations.clear();
        inner.newest.clear();
    }

    fn capture(&self, state: &Arc<AppState>, id: &str) -> Result<Option<PendingRecovery>> {
        let (generation, seq) = {
            let mut inner = self.inner.lock().expect("quota recovery lock poisoned");
            let generation = inner.generations.entry(id.to_string()).or_default().clone();
            let seq = NEXT_TRIGGER.fetch_add(1, Ordering::Relaxed);
            inner.newest.insert(id.to_string(), seq);
            (generation, seq)
        };
        let session = match SessionAsLeft::capture(state, id) {
            Ok(Some(session)) => session,
            Ok(None) => {
                self.cancel(id);
                return Ok(None);
            }
            Err(err) => {
                self.cancel(id);
                return Err(err);
            }
        };
        Ok(Some(PendingRecovery {
            session,
            generation,
            token: Arc::new(()),
            since: crate::time::now_millis(),
            seq,
            verified: None,
            hold_until: None,
        }))
    }
}

impl PendingRecovery {
    /// A held account's successful reading can arrive during switch preparation, after its
    /// callback found no parked trigger. It must still take precedence before anything is stopped.
    pub(crate) fn own_account_recovered(&self, state: &Arc<AppState>) -> Result<bool> {
        if self.hold_until.is_none() {
            return Ok(false);
        }
        let account = self.session.account_id.as_deref();
        let dir = state.store.account_dir(&self.session.account_id)?;
        Ok(state.account_quotas.verified_available(
            self.session.agent,
            account,
            dir.as_deref(),
            self.since + 1,
        ) && crate::accounts::choice::account_can_open(state, self.session.agent, account)?
            && !account.is_some_and(|id| state.sharing_locked(id).is_some()))
    }

    pub(crate) fn permits_current_account(&self) -> bool {
        self.verified.is_some()
    }

    pub(crate) fn still_due(&self, state: &Arc<AppState>) -> bool {
        let id = &self.session.id;
        state.store.auto_switch_on_usage_limit().unwrap_or(false)
            && !state.is_shutting_down()
            && {
                let inner = state
                    .quota_recovery
                    .inner
                    .lock()
                    .expect("quota recovery lock poisoned");
                inner
                    .generations
                    .get(id)
                    .is_some_and(|generation| Arc::ptr_eq(generation, &self.generation))
                    && inner.newest.get(id) == Some(&self.seq)
            }
            && self.session.unchanged(state).is_some_and(|session| {
                matches!(
                    state.status_for_switch(&session),
                    SessionStatus::Working | SessionStatus::Idle
                )
            })
            && self.verified.as_ref().is_none_or(|(account, dir, at)| {
                state.store.account_dir(account).ok() == Some(dir.clone())
                    && state.account_quotas.verified_available(
                        self.session.agent,
                        account.as_deref(),
                        dir.as_deref(),
                        *at,
                    )
            })
    }
}

/// The shell environment snapshot and the launch that a recovery run in the background uses: the
/// real ones, or in a test the stand-ins its fixture installed (none installed, none returned).
// Opaque callbacks keep test launch injection statically dispatched without boxing.
#[allow(clippy::type_complexity)]
fn recovery_env(
    state: &Arc<AppState>,
) -> Option<(
    impl Fn() -> Result<HashMap<String, String>> + Clone + Send + 'static,
    impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
)> {
    #[cfg(not(test))]
    {
        let _ = state;
        Some((
            crate::env_shell::launch_snapshot,
            crate::session::term::launch,
        ))
    }
    #[cfg(test)]
    {
        let hook = state.quota_recovery.launch.lock().unwrap().clone()?;
        Some((
            move || Ok(hook.env.clone()),
            move |request| (hook.launch)(request),
        ))
    }
}

/// The entry points the lower layers reach through `AppState`, for it to install where it is built.
pub(crate) fn hooks() -> AutoSwitchHooks {
    AutoSwitchHooks {
        after_quota_refresh,
        after_failed_turn,
    }
}

/// Only a newly recorded, successful probe is a reason to reconsider the waits.
fn after_quota_refresh(
    state: &Arc<AppState>,
    agent: Agent,
    account: Option<String>,
    config_dir: Option<String>,
    started_at: i64,
) -> Option<tokio::task::JoinHandle<()>> {
    let (snapshot, launch) = recovery_env(state)?;
    let state = state.clone();
    Some(tokio::spawn(async move {
        let _slot = state.quota_recovery.slots.acquire().await;
        recover_after_refresh(
            &state, agent, account, config_dir, started_at, snapshot, launch,
        )
        .await;
    }))
}

async fn recover_after_refresh(
    state: &Arc<AppState>,
    agent: Agent,
    account: Option<String>,
    config_dir: Option<String>,
    started_at: i64,
    snapshot: impl Fn() -> Result<HashMap<String, String>> + Clone + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) {
    if !state.store.auto_switch_on_usage_limit().unwrap_or(false) {
        state.quota_recovery.clear();
        return;
    }
    if !state.account_quotas.verified_available(
        agent,
        account.as_deref(),
        config_dir.as_deref(),
        started_at,
    ) {
        return;
    }
    let candidates: Vec<_> = state
        .quota_recovery
        .inner
        .lock()
        .expect("quota recovery lock poisoned")
        .waiting
        .values()
        .filter(|wait| {
            wait.session.agent == agent
                && started_at > wait.since
                && wait.hold_until.is_none_or(|until| {
                    until <= crate::time::now_millis() || wait.session.account_id == account
                })
        })
        .cloned()
        .collect();
    for mut wait in candidates {
        let id = wait.session.id.clone();
        // Claim before inspecting launch state: another callback may already be restarting it.
        if !claim(state, &wait) {
            continue;
        }
        if !wait.still_due(state) {
            continue;
        }
        if state.is_launching(&id) {
            park(state, &wait.session.id.clone(), wait);
            continue;
        }
        if switchable_session(state, &id).ok().flatten().is_none() {
            continue;
        }
        if !crate::accounts::choice::account_can_open(state, agent, account.as_deref())
            .unwrap_or(false)
            || account
                .as_deref()
                .is_some_and(|id| state.sharing_locked(id).is_some())
        {
            park(state, &wait.session.id.clone(), wait);
            continue;
        }
        wait.verified = Some((account.clone(), config_dir.clone(), started_at));
        let result = switch_to_with_recovery(
            state,
            &wait.session,
            account.clone(),
            snapshot.clone(),
            launch.clone(),
            Some(&wait),
        )
        .await;
        if let Err(err) = result {
            if !is_quiet(&err) && !state.is_shutting_down() {
                tracing::warn!(session = %id, %err, "resuming a session after quota recovery failed");
            }
        }
        // A refusal before stopping leaves this exact failed turn eligible for a later probe.
        wait.verified = None;
        if wait.still_due(state) {
            park(state, &wait.session.id.clone(), wait);
        }
    }
}

/// Takes `wait` out of the waits if it is the trigger waiting for its session, not one parked over
/// it since; true when it was.
fn claim(state: &Arc<AppState>, wait: &PendingRecovery) -> bool {
    let mut inner = state
        .quota_recovery
        .inner
        .lock()
        .expect("quota recovery lock poisoned");
    let is_waiting = inner
        .waiting
        .get(&wait.session.id)
        .is_some_and(|pending| Arc::ptr_eq(&pending.token, &wait.token));
    if is_waiting {
        inner.waiting.remove(&wait.session.id);
    }
    is_waiting
}

/// Puts a trigger taken out to be judged back among the waits, unless a newer one was captured:
/// whether it was put back. A newer trigger supersedes the older one even while it is claimed and
/// the waiting slot is empty.
fn park(state: &Arc<AppState>, id: &str, wait: PendingRecovery) -> bool {
    let mut inner = state
        .quota_recovery
        .inner
        .lock()
        .expect("quota recovery lock poisoned");
    if inner
        .generations
        .get(id)
        .is_some_and(|generation| Arc::ptr_eq(generation, &wait.generation))
        && inner.newest.get(id) == Some(&wait.seq)
    {
        inner.waiting.insert(id.to_string(), wait);
        return true;
    }
    false
}

/// The sessions [`after_failed_turn`] was asked to switch under test.
#[cfg(test)]
pub(crate) static ASKED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The sessions whose switch [`after_failed_turn`] answers as having moved them under test, each
/// once the test sends on the channel kept for it; a session not named here is answered as not
/// moved at once.
#[cfg(test)]
pub(crate) static MOVES: std::sync::Mutex<Vec<(String, tokio::sync::oneshot::Receiver<bool>)>> =
    std::sync::Mutex::new(Vec::new());

/// Moves the session `id` to another account if its turn that has just failed stopped on the
/// usage limit and the setting is on. Runs in the background, off the turn's path; the task it
/// runs on ends in whether the session was moved, or resumed on its own account, which its owner
/// is then told of. `None` when the setting is off, and nothing is judged.
fn after_failed_turn(state: &Arc<AppState>, id: &str) -> Option<tokio::task::JoinHandle<bool>> {
    match state.store.auto_switch_on_usage_limit() {
        Ok(true) => {}
        Ok(false) => return None,
        Err(err) => {
            tracing::warn!(%err, "reading whether sessions are switched automatically failed");
            return None;
        }
    }
    let trigger = state.quota_recovery.capture(state, id).ok().flatten();
    // The switch itself starts an agent, which a test cannot; it only sees that one was asked for.
    #[cfg(test)]
    {
        let _ = trigger;
        ASKED.lock().unwrap().push(id.to_string());
        let mut moves = MOVES.lock().unwrap();
        let answer = moves
            .iter()
            .position(|(session, _)| session == id)
            .map(|index| moves.remove(index).1);
        Some(tokio::spawn(async move {
            match answer {
                Some(answer) => answer.await.unwrap_or(false),
                None => false,
            }
        }))
    }
    #[cfg(not(test))]
    {
        let (state, id) = (state.clone(), id.to_string());
        Some(tokio::spawn(async move {
            switch_after_failed_turn_captured(
                &state,
                &id,
                trigger,
                crate::env_shell::launch_snapshot,
                crate::session::term::launch,
            )
            .await
        }))
    }
}

/// [`after_failed_turn`], waiting for the switch, with the shell environment `snapshot` gives and
/// the processes `launch` starts: `env_shell::launch_snapshot` and `term::launch` outside the
/// tests, which cannot start an agent.
#[cfg(test)]
async fn switch_after_failed_turn(
    state: &Arc<AppState>,
    id: &str,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) -> bool {
    let trigger = state.quota_recovery.capture(state, id).ok().flatten();
    switch_after_failed_turn_captured(state, id, trigger, snapshot, launch).await
}

/// Judges the failed turn `trigger` stands for, as [`try_switch_captured`] does; whether the
/// session was moved, or resumed on its own account.
async fn switch_after_failed_turn_captured(
    state: &Arc<AppState>,
    id: &str,
    trigger: Option<PendingRecovery>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) -> bool {
    match try_switch_captured(state, id, trigger, snapshot, launch).await {
        Ok(moved) => moved,
        // Archived or deleted while it was being switched, or the daemon shutting down.
        Err(err) if is_quiet(&err) || state.is_shutting_down() => false,
        Err(err) => {
            tracing::warn!(
                session = %id,
                err = %format!("{err:#}"),
                "switching a session off the account whose usage limit it reached failed"
            );
            false
        }
    }
}

/// [`try_switch_captured`] with the trigger captured now.
#[cfg(test)]
async fn try_switch(
    state: &Arc<AppState>,
    id: &str,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) -> Result<bool> {
    let trigger = state.quota_recovery.capture(state, id)?;
    try_switch_captured(state, id, trigger, snapshot, launch).await
}

/// Switches the session `id` to the account a new session would be given, if the setting is on,
/// it is in a state to be switched and an account qualifies, and tells those concerned. Nothing
/// when not. The turn's stop on the usage limit is waited for here, ahead of any claim: a failed
/// turn that did not stop on it (an overload, a network error) must not leave the session shown as
/// switching and refuse everything else meanwhile. With no target, the same confirmation authorizes
/// a wait for a fresh quota probe; so does a blocking window of the session's own account that
/// resets within [`HOLD_FOR_RESET`], though that wait is held for the account itself until
/// [`HOLD_SLACK`] after the reset (see [`end_hold`]). A claim held on the session is
/// waited for first ([`wait_for_claim`]); a trigger from a turn on one account is dropped if the
/// session was moved to another during that wait. What was decided on the records read here is
/// held to under the switch's claim as well: the account the limit was found on (the session is
/// left alone if it changed during the wait), that it has a process, that it does not wait for the
/// user, and that its last turn stopped on the usage limit. `trigger` is the one captured when the
/// turn failed. Whether the session was moved, or resumed on its own account.
async fn try_switch_captured(
    state: &Arc<AppState>,
    id: &str,
    trigger: Option<PendingRecovery>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) -> Result<bool> {
    if !state.store.auto_switch_on_usage_limit()? {
        return Ok(false);
    }
    let Some(trigger) = trigger else {
        return Ok(false);
    };
    // The account the turn that triggered this ran on, which the wait for a claim may change.
    let Some(triggered_on) = state
        .store
        .get_session(id)?
        .map(|session| session.account_id)
    else {
        return Ok(false);
    };
    wait_for_claim(state, id).await;
    let Some(session) = switchable_session(state, id)? else {
        return Ok(false);
    };
    if session.account_id != triggered_on {
        return Ok(false);
    }
    // The snapshot is taken for the record's sake only where finding it needs one (the default
    // account has no directory of its own); the switch is given it in place of the original, which
    // is spent. Otherwise the switch takes the original itself.
    type Snapshot = Box<dyn FnOnce() -> Result<HashMap<String, String>> + Send>;
    let (shell_env, snapshot): (_, Snapshot) = if restart::record_needs_shell_env(&session) {
        let env = tokio::task::spawn_blocking(snapshot)
            .await?
            .context("snapshotting the user's shell environment")?;
        (Some(env.clone()), Box::new(move || Ok(env)))
    } else {
        (None, Box::new(snapshot))
    };
    if !account_switch::wait_for_usage_limit(&session, shell_env).await? {
        return Ok(false);
    }
    // The wait took time: what the session is doing is read again, and the account the limit was
    // found on must be the one it is on still.
    let Some(waited) = switchable_session(state, id)? else {
        return Ok(false);
    };
    if waited.account_id != session.account_id || !trigger.still_due(state) {
        return Ok(false);
    }
    // A trigger that has been held once is judged as any other when its hold ends.
    if trigger.hold_until.is_none() {
        if let Some(until) = hold_deadline(state, &waited)? {
            let token = trigger.token.clone();
            let held = PendingRecovery {
                hold_until: Some(until),
                ..trigger
            };
            if park(state, id, held) {
                schedule_end_hold(state, id, token, until);
            }
            return Ok(false);
        }
    }
    // A hold that has ended was for the session's own account. Its reading may have landed while the
    // waits above ran, after the timer found nothing to claim, so the account is judged again here,
    // ahead of any other, and not moved off a quota that has just recovered.
    if trigger.own_account_recovered(state)? {
        let account = waited.account_id.clone();
        let dir = state.store.account_dir(&account)?;
        let mut continuing = trigger.clone();
        continuing.verified = Some((account.clone(), dir, trigger.since));
        let result =
            switch_to_with_recovery(state, &waited, account, snapshot, launch, Some(&continuing))
                .await;
        // A refusal before stopping leaves this failed turn eligible for a later probe.
        if trigger.still_due(state) {
            park(state, &trigger.session.id.clone(), trigger);
        }
        return result;
    }
    let Some(target) = pick_target(state, &waited)? else {
        park(state, id, trigger);
        return Ok(false);
    };
    switch_to_with_recovery(state, &waited, target, snapshot, launch, Some(&trigger)).await
}

/// When a trigger held for the reset of `reset_at` stops being held: [`HOLD_SLACK`] after it.
fn hold_end(reset_at: i64) -> i64 {
    reset_at + HOLD_SLACK.as_millis() as i64
}

/// Whether a window that resets at `reset_at` is close enough at `now` to wait for.
fn within_hold(reset_at: i64, now: i64) -> bool {
    reset_at - now <= HOLD_FOR_RESET.as_millis() as i64
}

/// When the hold ends, if the window that keeps the account `session` is on from running a turn
/// resets within [`HOLD_FOR_RESET`], going by the reading held. A missing reading, one that failed
/// to be read again (its windows are out of date, so it is not trusted to say when the account
/// recovers), an account that is not used up in it, and a window that does not say when it resets
/// are none of them a reason to wait.
fn hold_deadline(state: &Arc<AppState>, session: &Session) -> Result<Option<i64>> {
    let config_dir = state.store.account_dir(&session.account_id)?;
    let account = session.account_id.as_deref();
    let readable = state
        .account_quotas
        .held(session.agent, account, config_dir.as_deref())
        .is_some_and(|quota| quota.unavailable.is_none());
    if !readable {
        return Ok(None);
    }
    let now = crate::time::now_millis();
    Ok(state
        .account_quotas
        .exhausted(session.agent, account, config_dir.as_deref(), now)
        .and_then(|exhausted| exhausted.resets_at)
        .filter(|reset_at| within_hold(*reset_at, now))
        .map(hold_end))
}

/// Runs [`end_hold`] for the trigger `token` of the session `id` at `until`, the deadline it was
/// held to. A plain function, so that the spawned future does not make the switch's own future
/// depend on itself.
fn schedule_end_hold(state: &Arc<AppState>, id: &str, token: Arc<()>, until: i64) {
    let (state, id) = (state.clone(), id.to_string());
    let delay = Duration::from_millis((until - crate::time::now_millis()).max(0) as u64);
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        end_hold(&state, &id, &token, until).await;
    });
}

/// Ends the hold of the trigger `token` of the session `id`, held until `until`, once that
/// deadline has passed without its own account having been read as recovered, and judges it as an
/// ordinary failed turn, except that its own account, if it has been read as recovered since, is
/// continued on first: moved to another account if one qualifies, else left waiting for any
/// account's recovery. Nothing unless that very trigger is still waiting and held to that very
/// deadline: it may have recovered, been cancelled, or been replaced by a later one.
async fn end_hold(state: &Arc<AppState>, id: &str, token: &Arc<()>, until: i64) {
    let trigger = {
        let mut inner = state
            .quota_recovery
            .inner
            .lock()
            .expect("quota recovery lock poisoned");
        let held = inner
            .waiting
            .get(id)
            .is_some_and(|wait| Arc::ptr_eq(&wait.token, token) && wait.hold_until == Some(until));
        if held {
            inner.waiting.remove(id)
        } else {
            None
        }
    };
    let Some(mut trigger) = trigger else {
        return;
    };
    // Held no longer, but not to be held again.
    trigger.hold_until = Some(crate::time::now_millis());
    let Some((snapshot, launch)) = recovery_env(state) else {
        park(state, &trigger.session.id.clone(), trigger);
        return;
    };
    switch_after_failed_turn_captured(state, id, Some(trigger), snapshot, launch).await;
}

/// Waits up to [`CLAIM_WAIT`] for the claim on the session `id` to be released, so that a turn that
/// failed under one (the continued turn of a switch that has just relaunched it) is judged once
/// the session is free, and not dropped. A claim still held then leaves the session to be refused
/// as busy.
async fn wait_for_claim(state: &Arc<AppState>, id: &str) {
    let deadline = Instant::now() + CLAIM_WAIT;
    while state.is_launching(id) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The account `session` would be moved to, if any qualifies: the one a new session would be given,
/// without the account it is on and the ones it was moved off for a limit lately. The outer
/// `Option` is whether one qualifies, the inner one the account (`None` the default account).
fn pick_target(state: &Arc<AppState>, session: &Session) -> Result<Option<Option<String>>> {
    let mut except = state.recently_left_for_limit(&session.id, LEFT_FOR_LIMIT_FOR);
    except.push(session.account_id.clone());
    crate::accounts::choice::preferred_weekly_account_except(state, session.agent, &except, true)
}

/// The session `id` as it stands if it is in a state to be switched now: a process at its prompt or
/// in a turn, not waiting for the user and not under a launch. `None` for one deleted since.
fn switchable_session(state: &Arc<AppState>, id: &str) -> Result<Option<Session>> {
    // Deleted since: nothing left to switch.
    let Some(session) = state.store.get_session(id)? else {
        return Ok(None);
    };
    let switchable = matches!(
        state.status_for_switch(&session),
        SessionStatus::Working | SessionStatus::Idle
    ) && state.live_session(id).is_some()
        && !state.is_launching(id);
    Ok(switchable.then_some(session))
}

/// Switches `session` to `target`, the account picked for it, then tells those concerned; on a
/// failure after its process was ended, brings it back up on the account it had.
#[cfg(test)]
async fn switch_to(
    state: &Arc<AppState>,
    session: &Session,
    target: Option<String>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) -> Result<()> {
    switch_to_with_recovery(state, session, target, snapshot, launch, None)
        .await
        .map(|_| ())
}

/// What the tests' `switch_to` does, for the wait `recovery` when one triggered it; whether the
/// session was moved, or resumed on its own account.
async fn switch_to_with_recovery(
    state: &Arc<AppState>,
    session: &Session,
    target: Option<String>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
    recovery: Option<&PendingRecovery>,
) -> Result<bool> {
    let id = session.id.as_str();
    let same_account = session.account_id == target;
    let from = account_name(state, session, session.account_id.as_deref());
    let to = account_name(state, session, target.as_deref());
    let note = if same_account {
        format!("{NOTICE_PREFIX}: your account's usage quota has recovered.")
    } else {
        note_to_session(session.agent, &from, &to)
    };
    let only_if = SwitchOnlyIf {
        recovery,
        on_account: Some(session.account_id.as_deref()),
        not_waiting: true,
        running: true,
        usage_limited: true,
        watch_launch: true,
        note: Some(&note),
    };
    let switched = match account_switch::switch_session_account_if(
        state,
        id,
        Some(target.clone()),
        None,
        only_if,
        None,
        LaunchEdit::default(),
        snapshot,
        launch.clone(),
    )
    .await
    {
        Ok(switched) => switched,
        Err(failed) => {
            let err = failed.error;
            if !failed.ended_process {
                if !same_account {
                    if let Some(wait) = recovery.filter(|wait| wait.still_due(state)) {
                        if wait.own_account_recovered(state)? {
                            // Its probe may have found nothing while this run held the trigger.
                            // Schedule it again after parking, so the recovered account resumes it.
                            park(state, &wait.session.id.clone(), wait.clone());
                            after_quota_refresh(
                                state,
                                session.agent,
                                session.account_id.clone(),
                                state.store.account_dir(&session.account_id)?,
                                wait.since + 1,
                            );
                        }
                    }
                }
                // The session is as it was (or, for a process that did not stop in time, may still
                // run): a refusal of the time is no error, any other failure is logged.
                return if is_wrong_time(&err) {
                    Ok(false)
                } else {
                    Err(err)
                };
            }
            if is_quiet(&err) {
                return Err(err);
            }
            // The restore would be refused, and the switch is neither told nor logged.
            if state.is_shutting_down() {
                return Ok(false);
            }
            tracing::warn!(
                session = %id,
                err = %format!("{err:#}"),
                "switching a session off the account whose usage limit it reached failed after its \
                 process was ended; bringing it back up on the account it had"
            );
            return restore(state, session, target.as_deref(), launch)
                .await
                .map(|()| false);
        }
    };
    let account_switch::Switched { continued, ended } = switched;
    let moved = state.session_record(id)?;
    if same_account {
        let (owner_state, moved) = (state.clone(), moved.clone());
        tokio::task::spawn_blocking(move || {
            owner_notices::tell_owner_it_resumed_on_recovery(&owner_state, &moved, continued)
        })
        .await?;
        // The recovery has no message of its own to carry what the restart ended.
        if let Some(ended) = ended {
            restart::tell_what_a_restart_ended(state, id, ended).await;
        }
        return Ok(true);
    }
    // The default account has no id, so its param is left out.
    let mut params = vec![
        ("agent", moved.agent.label()),
        ("from", from.as_str()),
        ("account", to.as_str()),
        ("session", moved.id.as_str()),
    ];
    params.extend(session.account_id.as_deref().map(|id| ("from_id", id)));
    params.extend(moved.account_id.as_deref().map(|id| ("account_id", id)));
    state.broadcast(
        Notice::new(
            notice_code::ACCOUNT_SWITCHED_AUTOMATICALLY,
            format!(
                "{from} ({}) reached its usage limit, so {} switched this session to {to}.",
                moved.agent.label(),
                crate::APP_NAME
            ),
            &params,
        )
        .about(&moved.id),
    );
    let state = state.clone();
    // Delivery writes into the sessions' PTYs, which blocks.
    tokio::task::spawn_blocking(move || {
        tell_sessions(
            &state,
            &moved,
            &from,
            &to,
            &note,
            continued,
            ended.as_deref(),
        )
    })
    .await?;
    Ok(true)
}

/// Brings `session` back up on the account it had after a switch to `target` failed past ending its
/// process, and tells the owner and the user only if that fails too. Somebody having brought it
/// back meanwhile, the user having archived or deleted it, or the daemon shutting down, is nothing
/// to tell.
async fn restore(
    state: &Arc<AppState>,
    session: &Session,
    target: Option<&str>,
    launch: impl FnOnce(LaunchRequest) -> Result<Launch> + Send + 'static,
) -> Result<()> {
    match account_switch::restore_after_failed_switch(state, session, target, launch).await {
        Ok(()) => Ok(()),
        Err(err) if is_quiet(&err) || is_already_up(&err) || state.is_shutting_down() => Ok(()),
        Err(err) => {
            if target == session.account_id.as_deref() {
                tell_recovery_failed(state, session, &err).await;
            } else {
                tell_switch_failed(state, session, target, &err).await;
            }
            Err(err)
        }
    }
}

async fn tell_recovery_failed(state: &Arc<AppState>, session: &Session, err: &anyhow::Error) {
    open::report_failed_launch(state, session, err);
    let (state, session, reason) = (state.clone(), session.clone(), format!("{err:#}"));
    let _ = tokio::task::spawn_blocking(move || {
        owner_notices::tell_owner_the_recovery_failed(&state, &session, &reason)
    })
    .await;
}

/// What the account `id` of `session`'s agent is called (`None` the default account), for a
/// notice. A name that cannot be read is replaced by the account's id, or by the default account's
/// standard name, so that a notice of what happened is not lost for it.
fn account_name(state: &Arc<AppState>, session: &Session, id: Option<&str>) -> String {
    state
        .store
        .account_display_name(session.agent, id)
        .unwrap_or_else(|lookup| {
            tracing::warn!(session = %session.id, %lookup, "reading an account name failed");
            fallback_account_name(id)
        })
}

/// What stands in for the name of the account `id` (`None` the default account).
fn fallback_account_name(id: Option<&str>) -> String {
    id.map_or_else(|| DEFAULT_ACCOUNT_NAME.to_string(), str::to_string)
}

/// Tells the user and the owner that the switch of `session` to `target` failed after its process
/// was ended and that bringing it back on its account failed with `err`.
async fn tell_switch_failed(
    state: &Arc<AppState>,
    session: &Session,
    target: Option<&str>,
    err: &anyhow::Error,
) {
    let (reason_code, failure, reason) = error_parts(err);
    let (from, to) = (
        account_name(state, session, session.account_id.as_deref()),
        account_name(state, session, target),
    );
    let mut params: Vec<(&str, &str)> = vec![
        ("agent", session.agent.label()),
        ("from", from.as_str()),
        ("account", to.as_str()),
        ("reason", reason.as_str()),
        ("reason_code", reason_code.as_str()),
        ("session", session.id.as_str()),
    ];
    params.extend(session.account_id.as_deref().map(|id| ("from_id", id)));
    params.extend(target.map(|id| ("account_id", id)));
    // The failure's own params come after ours and only where ours have no param of the name, the
    // ids of the default account included, which are left out.
    let ours = ["from_id", "account_id"];
    let others: Vec<_> = failure
        .iter()
        .filter(|(name, _)| {
            !params.iter().any(|(own, _)| own == name) && !ours.contains(&name.as_str())
        })
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    params.extend(others);
    state.broadcast(
        Notice::new(
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED,
            format!(
                "{from} ({}) reached its usage limit, and {} could neither switch this session to \
                 {to} nor bring it back up on {from}: {reason}",
                session.agent.label(),
                crate::APP_NAME
            ),
            &params,
        )
        .about(&session.id),
    );
    let (state, session, reason) = (state.clone(), session.clone(), format!("{err:#}"));
    // Delivery writes into the owner's PTY, which blocks.
    let _ = tokio::task::spawn_blocking(move || {
        owner_notices::tell_owner_the_switch_failed(&state, &session, &reason)
    })
    .await;
}

/// Whether the switch failed for a reason nobody is told of and that is not logged: the session was
/// deleted or archived before or while it ran, which is the user's word on the matter, or the
/// daemon is shutting down.
fn is_quiet(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>().is_some_and(|coded| {
        [
            error_code::UNKNOWN_SESSION,
            error_code::SESSION_ARCHIVED,
            error_code::DAEMON_SHUTTING_DOWN,
        ]
        .contains(&coded.code)
    })
}

/// Whether restoring found the session running already: the user or an owner resumed it meanwhile.
/// A session another claim still held when the restore gave up waiting is not: it is down, and
/// that is told.
fn is_already_up(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::SESSION_ALREADY_RUNNING)
}

/// Whether a refused switch only found the time wrong, which is no error: the session was busy or
/// had changed, its turn was not (or no longer) one the limit stopped, or the account picked for it
/// had been turned off, logged out, used up or removed since. Only for a refusal that left the
/// session as it was.
fn is_wrong_time(err: &anyhow::Error) -> bool {
    err.is::<NoLongerDue>()
        || err.downcast_ref::<CodedError>().is_some_and(|coded| {
            [
                error_code::SESSION_ALREADY_STARTING,
                error_code::ACCOUNT_SHARING_IN_PROGRESS,
                error_code::ACCOUNT_DISABLED,
                error_code::ACCOUNT_QUOTA_EXHAUSTED,
                error_code::ACCOUNT_NOT_SIGNED_IN,
                error_code::UNKNOWN_ACCOUNT,
            ]
            .contains(&coded.code)
        })
}

/// What the switched session is told, put ahead of the prompt it is relaunched with to carry on:
/// a message written into it would open a turn of its own, which its owner would be sent a report
/// of.
fn note_to_session(agent: Agent, from: &str, to: &str) -> String {
    format!(
        "{}, so {} switched you to `{to}`.",
        note_opening(agent, from),
        crate::APP_NAME
    )
}

/// How [`note_to_session`] begins, which, as written, credits the turn the note opens: credited by
/// the notice prefix alone, the turn of whichever notice reached the session first would spend it.
fn note_opening(agent: Agent, from: &str) -> String {
    format!(
        "{NOTICE_PREFIX}: your {} account `{from}` reached its usage limit",
        agent.label()
    )
}

/// Tells the switched session, when its relaunch did not carry the note, and its owner if it has
/// one, what became of it. What the switch ended of what the session had started (`ended`, for a
/// session that was not continued) goes in the same message as the note. A message to a session
/// that cannot take one now is queued, and one for an owner that is interrupted is kept for its
/// resume (see `owner_notices::tell_owner`); one that cannot be delivered at all is logged and
/// goes no further.
fn tell_sessions(
    state: &Arc<AppState>,
    moved: &Session,
    from: &str,
    to: &str,
    note: &str,
    continued: bool,
    ended: Option<&str>,
) {
    if !continued {
        // The turn this opens is the notice's own, which needs no report back. Credited as written:
        // the write strips what an account name may carry (a direction mark, a joiner), and the
        // prompt the turn reports is the stripped text.
        let opening = crate::session::term::stripped(&note_opening(moved.agent, from));
        state.credit_turn_opened_by(&moved.id, opening.clone());
        let ended = ended.map(|ended| format!(" {ended}")).unwrap_or_default();
        let own = format!(
            "{note}{ended} Carry on with what you were doing, or wait for your next instruction \
             if you had none."
        );
        if !tell(state, &moved.id, &own) {
            state.clear_turn_credit(&moved.id, &opening);
        }
    }
    owner_notices::tell_owner_it_was_switched(state, moved, from, to, continued);
}

/// Writes `text` into the session `id`, queued until it can take it. Whether it was accepted.
fn tell(state: &Arc<AppState>, id: &str, text: &str) -> bool {
    match delivery::write_message(state, id, text, WhenBlocked::Queue) {
        Ok(_) => true,
        Err(err) => {
            tracing::debug!(session = %id, %err, "telling a session of its switch failed");
            false
        }
    }
}

#[cfg(test)]
mod tests;
