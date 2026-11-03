//! When the readings are taken: the poller, a refresh on request or on demand, and the kicks that
//! start a window. It decides which accounts are due (the timing constants below, and the
//! scheduling methods of [`AccountQuotas`] in this file, which claim the due accounts and claim
//! each kick), runs the probe and the kick for each under the daemon-wide limit on CLI runs, and
//! records what they return in the registry `mod.rs` keeps.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use tokio::task::JoinSet;

use super::diagnostics::{log_stop, ProbeTrace};
use super::email::account_email;
use super::judgement::is_account_state;
use super::kick::{kick, KICK_TIMEOUT};
use super::probe::{config_dir_missing, failed, probe, PROBE_TIMEOUT};
use super::{retain_locked, window_id, AccountQuotas, Claim, Entry, Inner, Key, WindowId};
use crate::protocol::{
    quota_unavailable_code as code, AccountQuota, Agent, Availability, Event, QuotaUnavailable,
    QuotaWindowKind,
};
use crate::state::AppState;
use crate::time::now_millis;

/// How old an account's last attempt must be before it is read again.
pub(super) const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// How old the last attempt on an account holding no windows because it is not signed in or has no
/// quota to report must be for a client's request to read it before it is due. It may have
/// been signed in outside Octoboard since; this bounds how often a client opening a panel can
/// start its CLI.
pub(super) const MIN_REREAD_INTERVAL: Duration = Duration::from_secs(30);

/// How long after a held window's reset time the account is read again. The service is given a
/// moment to roll the window over, so that the reading taken shows the new window and not the old
/// one.
pub(super) const RESET_GRACE: Duration = Duration::from_secs(30);

/// How old an account's last attempt must be before a request for a refresh on demand
/// ([`refresh_on_demand`]) reads it again, whatever the poller's own interval: bounds how often
/// an agent asking can start a CLI for one account, and every attempt counts, the poller's
/// included.
const ON_DEMAND_INTERVAL: Duration = Duration::from_secs(60);

/// How often the poller looks for accounts that are due. Much shorter than [`POLL_INTERVAL`] so
/// that an account created, or an agent found available, after the poller's start is read within
/// moments rather than after a whole interval; only a due account costs a subprocess.
const POLL_TICK: Duration = Duration::from_secs(15);

/// How many consecutive failures keep stretching the wait before an account is tried again: the
/// wait doubles with each, so at most `POLL_INTERVAL * 2^3`.
const MAX_BACKOFF_DOUBLINGS: u32 = 3;

/// How many CLIs run at once across the whole daemon, a poll, any refresh requests and the kicks
/// together. Each is a whole agent process.
pub(super) const MAX_CONCURRENT_CLI_RUNS: usize = 4;

/// How long after an attempt an account is read again, given how many in a row ended unavailable.
fn wait_after(failures: u32) -> Duration {
    POLL_INTERVAL * 2u32.pow(failures.min(MAX_BACKOFF_DOUBLINGS))
}

/// Whether `quota` holds no windows because the account is not signed in or has no quota to
/// report, which a login done outside Octoboard could have changed.
fn may_have_signed_in_since(quota: &AccountQuota) -> bool {
    quota.windows.is_empty()
        && quota
            .unavailable
            .as_ref()
            .is_some_and(|u| u.code == code::NOT_SIGNED_IN || u.code == code::NO_WINDOWS)
}

/// Whether a window held for the entry's account reset after its last attempt, at least
/// [`RESET_GRACE`] before `wall` (wall-clock milliseconds). Windows kept out of date after a
/// failure count, as they are still held. A read that happens moves the attempt past the
/// reset, so it does not fire again.
fn reset_since_attempt(entry: &Entry, wall: i64) -> bool {
    let settled = wall.saturating_sub(RESET_GRACE.as_millis() as i64);
    entry.quota.windows.iter().any(|window| {
        window
            .resets_at
            .is_some_and(|at| at > entry.attempted_at && at <= settled)
    })
}

/// What started a refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Trigger {
    /// The poller, or a login that ended.
    Poll,
    /// A client asked, as it does on opening a panel that shows quotas.
    Request,
    /// A session asked for fresh readings, and is waiting for them: each account is read again
    /// once [`ON_DEMAND_INTERVAL`] has passed since its last attempt. Starts no quota window (see
    /// [`refresh_on_demand`]).
    Demand,
}

/// Which accounts a refresh covers; the default is all of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    pub agent: Option<Agent>,
    pub account: Option<String>,
    /// Leave out the accounts the user turned off.
    pub enabled_only: bool,
}

/// The accounts worth reading: the default account and every stored account of each agent whose
/// binary was found on the login shell's `PATH`.
fn targets(state: &AppState) -> anyhow::Result<Vec<Key>> {
    let accounts = state.store.list_accounts()?;
    let mut keys = Vec::new();
    for agent in Agent::ALL {
        if state.agent_availability_of(agent).availability != Availability::Available {
            continue;
        }
        keys.push(Key::of(agent, None, None));
        keys.extend(
            accounts
                .iter()
                .filter(|account| account.agent == agent)
                .map(|account| Key::of(agent, Some(&account.id), Some(&account.config_dir))),
        );
    }
    Ok(keys)
}

impl AccountQuotas {
    /// A claim on each of `selected` that is due at `now` and not already running. Due is: never
    /// read, made due by [`AccountQuotas::make_due`], last attempted longer ago than
    /// [`wait_after`] its consecutive failures, or holding a window that reset since its last
    /// attempt (see [`reset_since_attempt`]).
    #[cfg(test)]
    fn claim_due(&self, selected: Vec<Key>, now: Instant) -> Vec<Claim> {
        self.claim_due_at(selected, now, now_millis())
    }

    /// [`AccountQuotas::claim_due`] with `wall` as the wall-clock time in milliseconds.
    #[cfg(test)]
    fn claim_due_at(&self, selected: Vec<Key>, now: Instant, wall: i64) -> Vec<Claim> {
        self.claim_due_locked(&mut self.lock(), selected, now, wall, Trigger::Poll)
    }

    /// [`AccountQuotas::claim_due`] as a client's request would, which also claims an account
    /// holding no windows because it is not signed in or has no quota to report once
    /// [`MIN_REREAD_INTERVAL`] has passed.
    #[cfg(test)]
    fn claim_requested(&self, selected: Vec<Key>, now: Instant) -> Vec<Claim> {
        self.claim_due_locked(
            &mut self.lock(),
            selected,
            now,
            now_millis(),
            Trigger::Request,
        )
    }

    fn claim_due_locked(
        &self,
        inner: &mut Inner,
        selected: Vec<Key>,
        now: Instant,
        wall: i64,
        trigger: Trigger,
    ) -> Vec<Claim> {
        let mut claims = Vec::new();
        for key in selected {
            if inner.in_flight.contains_key(&key) {
                continue;
            }
            let is_due = inner.entries.get(&key).is_none_or(|entry| {
                let since = now.saturating_duration_since(entry.attempted);
                // Whatever else makes an account due, a session asking is held to the limit.
                if trigger == Trigger::Demand {
                    return since >= ON_DEMAND_INTERVAL;
                }
                entry.due
                    || since >= wait_after(entry.failures)
                    || reset_since_attempt(entry, wall)
                    || (trigger == Trigger::Request
                        && since >= MIN_REREAD_INTERVAL
                        && may_have_signed_in_since(&entry.quota))
            });
            if is_due {
                claims.push(self.claim_locked(inner, key, wall));
            }
        }
        claims
    }

    /// What `scope` selects among the accounts worth reading, claimed if due. The accounts are
    /// listed and the readings of any that are not among them dropped (an account that was
    /// deleted or repointed, or belongs to an agent that is not available), in the same lock as
    /// the claiming, so a refresh working from an older list can never cancel a claim another
    /// one just made.
    fn claim_scope(
        &self,
        state: &AppState,
        scope: &Scope,
        now: Instant,
        trigger: Trigger,
    ) -> anyhow::Result<Vec<Claim>> {
        let mut inner = self.lock();
        let all = targets(state)?;
        if retain_locked(&mut inner, &all) {
            state.broadcast(Event::AccountQuotasUpdated {
                account_quotas: inner.list(),
            });
        }
        let selected = all
            .into_iter()
            .filter(|key| scope.agent.is_none_or(|agent| key.agent == agent))
            .filter(|key| {
                scope
                    .account
                    .as_ref()
                    .is_none_or(|account| key.account.as_ref() == Some(account))
            })
            .filter(|key| {
                !scope.enabled_only
                    || !state
                        .store
                        .account_disabled(key.agent, key.account.as_deref())
                        .unwrap_or(false)
            })
            .collect();
        Ok(self.claim_due_locked(&mut inner, selected, now, now_millis(), trigger))
    }

    /// The claim for a kick of the account `key`, when its held reading calls for one: the setting
    /// is on, the account is not disabled, nothing is running for the account, the reading is a
    /// successful one and shows a window with no reset time, other than a model's own weekly one,
    /// that has not been kicked in its episode. Every such window then counts as kicked, as the one
    /// request is meant to start them all. None once the daemon is shutting down, with no window
    /// counted as kicked.
    fn claim_kick(&self, state: &AppState, key: &Key) -> Option<Claim> {
        if !self.kick_allowed(state, key) {
            return None;
        }
        let mut guard = self.lock();
        let inner = &mut *guard;
        let fresh = unkicked_windows(inner, key)?;
        inner.kicked.entry(key.clone()).or_default().extend(fresh);
        Some(self.claim_locked(inner, key.clone(), now_millis()))
    }

    /// Marks the account's reading due when [`Self::claim_kick`] would claim a kick for it, without
    /// claiming one: the next poll then reads it and kicks. For a reading that is not followed by
    /// a kick itself (see [`refresh_on_demand`]), so that the window the user's setting would
    /// start is not left unstarted until the account's next ordinary reading.
    fn make_due_for_kick(&self, state: &AppState, key: &Key) {
        if !self.kick_allowed(state, key) {
            return;
        }
        let mut inner = self.lock();
        if unkicked_windows(&inner, key).is_some() {
            if let Some(entry) = inner.entries.get_mut(key) {
                entry.due = true;
            }
        }
    }

    /// Whether the daemon, the user's setting and the account allow a kick at all: it is not
    /// shutting down, the setting that starts quota windows is on, and the account is not disabled.
    /// An account found not logged in needs no check of its own: its reading carries a reason it
    /// is unavailable, and `unkicked_windows` offers no window from such a reading.
    fn kick_allowed(&self, state: &AppState, key: &Key) -> bool {
        if self.stopping.load(Ordering::Relaxed) {
            return false;
        }
        match state.store.auto_start_quota_windows() {
            Ok(true) => {}
            Ok(false) => return false,
            Err(err) => {
                tracing::warn!(%err, "reading the setting that starts quota windows failed");
                return false;
            }
        }
        // A disabled account is left alone: no window is started on it.
        match state
            .store
            .account_disabled(key.agent, key.account.as_deref())
        {
            Ok(disabled) => !disabled,
            Err(err) => {
                tracing::warn!(%err, "reading whether the account is disabled failed");
                false
            }
        }
    }

    /// Whether the kick `claim` covers should go on to the reading after it: the daemon is not
    /// shutting down and the claim still counts, as a login that ended or a deleted account
    /// makes it not.
    fn kick_is_wanted(&self, claim: &Claim) -> bool {
        !self.stopping.load(Ordering::Relaxed)
            && self.lock().in_flight.get(&claim.key) == Some(&claim.token)
    }
}

/// The windows of the account `key` that a kick would start and that no kick has started in their
/// episode, or `None` when nothing is running for the account, its reading is not a successful one
/// or no window is such.
fn unkicked_windows(inner: &Inner, key: &Key) -> Option<Vec<WindowId>> {
    if inner.in_flight.contains_key(key) {
        return None;
    }
    let quota = &inner.entries.get(key)?.quota;
    if quota.unavailable.is_some() {
        return None;
    }
    let kicked = inner.kicked.get(key).map_or(&[][..], Vec::as_slice);
    let fresh: Vec<WindowId> = quota
        .windows
        .iter()
        .filter(|window| window.resets_at.is_none() && window.kind != QuotaWindowKind::WeeklyModel)
        .map(window_id)
        .filter(|id| !kicked.contains(id))
        .collect();
    (!fresh.is_empty()).then_some(fresh)
}

/// Starts the poller. It holds the state weakly, so a test or a shutdown can release it.
pub fn spawn(state: &Arc<AppState>) {
    let weak = Arc::downgrade(state);
    tokio::spawn(async move {
        loop {
            let Some(state) = weak.upgrade() else { return };
            refresh(&state, &Scope::default()).await;
            drop(state);
            tokio::time::sleep(POLL_TICK).await;
        }
    });
}

/// Where a probe or a kick gets the shell environment it runs in.
pub(crate) type ShellSource =
    Arc<dyn Fn() -> anyhow::Result<HashMap<String, String>> + Send + Sync>;

/// Reads the accounts of `scope` that are due and broadcasts after each. See [`refresh_with`].
pub async fn refresh(state: &Arc<AppState>, scope: &Scope) {
    let shell = Arc::new(crate::env_shell::cached_snapshot);
    refresh_with(state, scope, Trigger::Poll, shell).await;
}

/// [`refresh`] for a client's request: an account holding no windows because it is not signed in
/// or has no quota to report is read too, if its last attempt is older than [`MIN_REREAD_INTERVAL`].
pub async fn refresh_requested(state: &Arc<AppState>, scope: &Scope) {
    let shell = Arc::new(crate::env_shell::cached_snapshot);
    refresh_with(state, scope, Trigger::Request, shell).await;
}

/// [`refresh`] for a session that asked for fresh readings and waits for them: every account of
/// `scope` whose last attempt, the poller's included, is older than [`ON_DEMAND_INTERVAL`] is read
/// now, due or not; one read more recently keeps its reading. It never starts a quota window,
/// whatever the setting that starts them says: that spends quota, and is the user's setting to
/// act on at the poller's own reads, not something a session's request may set off.
pub(crate) async fn refresh_on_demand(state: &Arc<AppState>, scope: &Scope, shell: ShellSource) {
    refresh_with(state, scope, Trigger::Demand, shell).await;
}

/// Reads the accounts of `scope` that are due for `trigger`, as many at a time as the daemon-wide
/// limit allows, and broadcasts after each. Also drops the readings of accounts no longer
/// targeted. Returns when every reading it started is done; a kick one of them calls for, and
/// the reading after it, goes on in the background (see [`kick_and_read`]).
pub(super) async fn refresh_with(
    state: &Arc<AppState>,
    scope: &Scope,
    trigger: Trigger,
    shell: ShellSource,
) {
    let claims = match state
        .account_quotas
        .claim_scope(state, scope, Instant::now(), trigger)
    {
        Ok(claims) => claims,
        Err(err) => {
            tracing::warn!(%err, "listing the accounts to read quotas for failed");
            return;
        }
    };
    let mut readings = JoinSet::new();
    for claim in claims {
        let (state, shell) = (state.clone(), shell.clone());
        readings.spawn(async move {
            let kicks = trigger != Trigger::Demand;
            if let Some(kick) = read_account(&state, &shell, claim, kicks).await {
                tokio::spawn(async move { kick_and_read(&state, &shell, kick).await });
            }
        });
    }
    while readings.join_next().await.is_some() {}
}

/// Kicks the accounts whose held readings call for it, as many at a time as the daemon-wide limit
/// allows, and reads each again once its kick has ended. Starts the work in the background and
/// returns at once. Run when the setting that starts quota windows is turned on, or when an account
/// is turned back on, so that the readings already held count without waiting for the next one.
pub fn kick_held(state: &Arc<AppState>) {
    let state = state.clone();
    tokio::spawn(async move { kick_held_with(&state, held_shell()).await });
}

/// The shell environment the kicks of [`kick_held`] run in. A test has none: a kick under test
/// must not start the agent installed on the machine running it.
#[cfg(not(test))]
fn held_shell() -> ShellSource {
    Arc::new(crate::env_shell::cached_snapshot)
}

#[cfg(test)]
fn held_shell() -> ShellSource {
    Arc::new(|| anyhow::bail!("no shell environment under test"))
}

/// [`kick_held`] in the shell environment `shell` gives, returning when every kick and the reading
/// after it is done.
async fn kick_held_with(state: &Arc<AppState>, shell: ShellSource) {
    let keys: Vec<Key> = state
        .account_quotas
        .lock()
        .entries
        .keys()
        .cloned()
        .collect();
    let mut kicks = JoinSet::new();
    for key in keys {
        let Some(claim) = state.account_quotas.claim_kick(state, &key) else {
            continue;
        };
        let (state, shell) = (state.clone(), shell.clone());
        kicks.spawn(async move { kick_and_read(&state, &shell, claim).await });
    }
    while kicks.join_next().await.is_some() {}
}

/// Reads the account `claim` is on and records the reading. Returns the claim for a kick when the
/// reading calls for one (see [`AccountQuotas::claim_kick`](super::AccountQuotas::claim_kick)) and
/// `kicks` allows it. Holds a slot while the CLI runs.
async fn read_account(
    state: &Arc<AppState>,
    shell: &ShellSource,
    claim: Claim,
    kicks: bool,
) -> Option<Claim> {
    let _slot = state.account_quotas.slots.acquire().await;
    let key = claim.key.clone();
    let shell = shell.clone();
    let stopping = state.account_quotas.stopping.clone();
    let outcome = tokio::task::spawn_blocking(move || read(&key, &*shell, &stopping)).await;
    let (quota, trace) = outcome.unwrap_or_else(|err| {
        (
            unavailable_quota(
                &claim.key,
                QuotaUnavailable::new(
                    code::COMMAND_FAILED,
                    format!("reading the quota failed: {err}"),
                    &[
                        ("agent", claim.key.agent.label()),
                        ("detail", &err.to_string()),
                    ],
                ),
            ),
            ProbeTrace::default(),
        )
    });
    // A probe the daemon ended is logged even when the shutdown was the reason.
    log_stop(&claim.key, &trace);
    // A reading that the shutdown cut short says nothing about the account, so it is not recorded.
    if state.account_quotas.stopping.load(Ordering::Relaxed) {
        return None;
    }
    let key = claim.key.clone();
    drop(_slot);
    state.account_quotas.log_reading(&claim, &quota, &trace);
    let _ = finish_reading(state, claim, quota);
    if kicks {
        state.account_quotas.claim_kick(state, &key)
    } else {
        // Left to the poller, which reads it again within a tick and kicks as it always does.
        state.account_quotas.make_due_for_kick(state, &key);
        None
    }
}

fn finish_reading(
    state: &Arc<AppState>,
    claim: Claim,
    quota: AccountQuota,
) -> Option<tokio::task::JoinHandle<()>> {
    let (key, started_at) = (claim.key.clone(), claim.started_at);
    if state.account_quotas.finish(state, claim, quota) {
        state.after_quota_refresh(key.agent, key.account, key.config_dir, started_at)
    } else {
        None
    }
}

#[cfg(test)]
pub(crate) fn finish_probe_for_test(
    state: &Arc<AppState>,
    quota: AccountQuota,
    config_dir: Option<&str>,
) -> Option<tokio::task::JoinHandle<()>> {
    let claim = {
        let mut inner = state.account_quotas.lock();
        state.account_quotas.claim_locked(
            &mut inner,
            Key::of(quota.agent, quota.account.as_deref(), config_dir),
            now_millis(),
        )
    };
    finish_reading(state, claim, quota)
}

/// Carries a claim from a kick through the reading after it, and on to a further kick if that
/// reading still calls for one, until a reading calls for none. A step takes a slot of its own, so
/// the claim is held between them but a slot never waits on another.
async fn kick_and_read(state: &Arc<AppState>, shell: &ShellSource, claim: Claim) {
    let mut claim = claim;
    loop {
        {
            let _slot = state.account_quotas.slots.acquire().await;
            let key = claim.key.clone();
            let shell = shell.clone();
            let stopping = state.account_quotas.stopping.clone();
            let outcome =
                tokio::task::spawn_blocking(move || start_windows(&key, &*shell, &stopping)).await;
            if let Err(err) = outcome
                .map_err(anyhow::Error::from)
                .and_then(|result| result)
            {
                tracing::warn!(
                    agent = claim.key.agent.label(),
                    account = ?claim.key.account,
                    %err,
                    "starting the account's quota windows failed"
                );
            }
        }
        if !state.account_quotas.kick_is_wanted(&claim) {
            return;
        }
        match read_account(state, shell, claim, true).await {
            Some(next) => claim = next,
            None => return,
        }
    }
}

/// Reads one account's quota in the shell environment `shell` gives, with what the probe left to
/// log (see `diagnostics.rs`). Blocking: it runs a subprocess, which setting `cancel` stops.
fn read(
    key: &Key,
    shell: &(dyn Fn() -> anyhow::Result<HashMap<String, String>> + Send + Sync),
    cancel: &AtomicBool,
) -> (AccountQuota, ProbeTrace) {
    let mut trace = ProbeTrace::default();
    // Nothing is captured or started once the daemon is shutting down; the caller drops the result.
    if cancel.load(Ordering::Relaxed) {
        let quota = unavailable_quota(key, failed(key.agent, "was cancelled before it started"));
        return (quota, trace);
    }
    if let Some(dir) = missing_config_dir(key) {
        return (
            unavailable_quota(key, config_dir_missing(key.agent, dir)),
            trace,
        );
    }
    let shell_env = shell();
    let result = match &shell_env {
        Ok(shell_env) => probe(
            key.agent,
            shell_env,
            key.config_dir.as_deref().map(Path::new),
            PROBE_TIMEOUT,
            cancel,
            &mut trace,
        ),
        Err(err) => {
            let detail = format!("{err:#}");
            Err(QuotaUnavailable::new(
                code::SHELL_ENVIRONMENT_FAILED,
                format!("the shell environment could not be captured: {detail}"),
                &[("detail", &detail)],
            ))
        }
    };
    // An account that is not signed in has no email, whatever a leftover record says. Without a
    // shell environment an owned account's directory is still known, but the default account's
    // follows the user's own variables, so it gets none rather than a guess.
    let no_env = HashMap::new();
    let env = match &shell_env {
        Ok(env) => Some(env),
        Err(_) if key.config_dir.is_some() => Some(&no_env),
        Err(_) => None,
    };
    let email = match &result {
        Err(unavailable) if is_account_state(unavailable.code) => None,
        _ => env.and_then(|env| {
            account_email(key.agent, key.config_dir.as_deref().map(Path::new), env)
        }),
    };
    let quota = match result {
        Ok(reading) => AccountQuota {
            agent: key.agent,
            account: key.account.clone(),
            fetched_at: now_millis(),
            plan: reading.plan,
            email,
            windows: reading.windows,
            unavailable: None,
            reset_credits: reading.reset_credits,
        },
        Err(unavailable) => AccountQuota {
            email,
            ..unavailable_quota(key, unavailable)
        },
    };
    (quota, trace)
}

/// The account's configuration directory when it is gone. Looked for before anything is run:
/// Claude Code and Grok Build would create the directory, empty, and then call the account not
/// signed in, and a poll or a kick would keep bringing back one a user deleted. Only `NotFound`
/// says it is gone: a directory that cannot be looked at, or a regular file in its place, is left
/// to the CLI to report. The default account pins nothing, so Codex's own refusal of a missing
/// home is what tells that case (see `probe`).
fn missing_config_dir(key: &Key) -> Option<&str> {
    key.config_dir
        .as_deref()
        .filter(|dir| crate::accounts::shared_config::is_missing(Path::new(dir)))
}

/// Kicks one account in the shell environment `shell` gives, unless its directory is gone: the
/// reading after the kick records that. Blocking: it runs a subprocess. Setting `cancel` stops it,
/// and a kick begun with it set is refused before anything is run.
fn start_windows(
    key: &Key,
    shell: &(dyn Fn() -> anyhow::Result<HashMap<String, String>> + Send + Sync),
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("the kick was cancelled before it started");
    }
    if let Some(dir) = missing_config_dir(key) {
        anyhow::bail!("the account's configuration directory {dir} does not exist");
    }
    let shell_env = shell().context("the shell environment could not be captured")?;
    kick(
        key.agent,
        &shell_env,
        key.config_dir.as_deref().map(Path::new),
        KICK_TIMEOUT,
        cancel,
    )
}

fn unavailable_quota(key: &Key, unavailable: QuotaUnavailable) -> AccountQuota {
    AccountQuota {
        agent: key.agent,
        account: key.account.clone(),
        fetched_at: now_millis(),
        plan: None,
        email: None,
        windows: Vec::new(),
        unavailable: Some(unavailable),
        reset_credits: None,
    }
}

#[cfg(test)]
mod tests;
