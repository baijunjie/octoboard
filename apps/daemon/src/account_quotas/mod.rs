//! How much of each account's plan quota is left, asked of the agent's own CLI. Octoboard never
//! reads an agent's credentials or calls its servers: the CLI, run the way a session of the
//! account would be, handles its own login, Keychain access and token refresh. What is asked and
//! how a reply is read is `probe.rs`'s; this module keeps the readings and decides when to take
//! them.
//!
//! Readings are runtime state: held in [`AccountQuotas`], never stored and never exported. A
//! poller started with the daemon keeps them fresh, and `refresh_account_quotas` runs the same
//! path on request. Both read an account whose last attempt is older than [`POLL_INTERVAL`],
//! stretched for an account that keeps failing, so a client that asks on every open costs next
//! to nothing; a request also reads some accounts sooner, as set out below, and a session that
//! asks for fresh readings over MCP has an account read again once a minute has passed since its
//! last attempt ([`refresh_on_demand`]). An account is also read once a window it holds has reset
//! since its last attempt, a short grace ([`RESET_GRACE`]) later, so a window that rolled over is
//! not shown as used up until the next interval. A reading
//! that fails over one that had windows keeps those windows, with the reason set beside them.
//!
//! An account whose configuration directory is missing or that is not signed in is a state of the
//! account, not a failure: it is reported as its own [`QuotaUnavailable`] code, drops the windows
//! it held, is not backed off, and is read again at once when its login ends
//! ([`AccountQuotas::make_due`]). An account the CLI has no quota to report for is not backed off
//! either, though it is not a state: a reading that had windows keeps them. A login done outside
//! Octoboard leaves no trace to react to, so a request from a client also reads an account
//! that is not signed in or has no quota to report, and holds no windows, once
//! [`MIN_REREAD_INTERVAL`] has passed since its last attempt. A missing configuration directory
//! is not read early: only a login or the ordinary interval brings it back.
//!
//! A window that has reset only begins counting at the account's next request, and until then the
//! agent reports it with no reset time. While the user's setting asks for it, a successful reading
//! that shows such a window is followed by a kick (`kick.rs`): one minimal request on the account,
//! and then a reading of it again. The kick and that reading run in the background, detached from
//! the refresh that found the window, so the other accounts' readings are not held up behind a
//! model round trip. A window is kicked once per episode, which ends only when a reading shows it
//! with a reset time again, so a window that a kick does not start (a credit pool with no period,
//! say) is not kicked over and over, and neither is one whose kick failed. A model's own weekly
//! window is never kicked: the request goes to one cheap model and cannot start another's. A kick
//! holds the account's claim and a slot like a reading does. A reading a session asked for over
//! MCP ([`refresh_on_demand`]) is the exception: it starts no kick itself, and marks an account
//! whose reading calls for one due instead, so that the poller's next tick reads it again and
//! kicks it, as it would have after any other reading.
//!
//! The module also decides whether a reading means the account is used up
//! ([`AccountQuotas::exhausted`]).

mod kick;
mod probe;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::protocol::{
    now_millis, quota_unavailable_code as code, AccountQuota, Agent, Availability, Event,
    QuotaUnavailable, QuotaWindow, QuotaWindowKind,
};
use crate::state::AppState;
use kick::{kick, KICK_TIMEOUT};
pub(crate) use probe::CODEX_APP_SERVER_ARGS;
use probe::{config_dir_missing, failed, probe, PROBE_TIMEOUT};

/// How old an account's last attempt must be before it is read again.
const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// How old the last attempt on an account holding no windows because it is not signed in or has no
/// quota to report must be for a client's request to read it before it is due. It may have
/// been signed in outside Octoboard since; this bounds how often a client opening a panel can
/// start its CLI.
const MIN_REREAD_INTERVAL: Duration = Duration::from_secs(30);

/// How long after a held window's reset time the account is read again. The service is given a
/// moment to roll the window over, so that the reading taken shows the new window and not the old
/// one.
const RESET_GRACE: Duration = Duration::from_secs(30);

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
const MAX_CONCURRENT_CLI_RUNS: usize = 4;

/// What a reading belongs to. The configuration directory is part of it so that an account
/// pointed at another directory never inherits the reading of its old one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    agent: Agent,
    /// The account's id; `None` for the default account.
    account: Option<String>,
    /// The account's directory; `None` for the default account, which pins nothing.
    config_dir: Option<String>,
}

impl Key {
    fn of(agent: Agent, account: Option<&str>, config_dir: Option<&str>) -> Self {
        Self {
            agent,
            account: account.map(str::to_string),
            config_dir: config_dir.map(str::to_string),
        }
    }
}

/// What names a window within an account: its kind and, for a model's own weekly window, the model.
type WindowId = (QuotaWindowKind, Option<String>);

fn window_id(window: &QuotaWindow) -> WindowId {
    (window.kind, window.label.clone())
}

struct Entry {
    quota: AccountQuota,
    attempted: Instant,
    /// The wall-clock time the last attempt began, in milliseconds since the epoch, to tell whether
    /// a window reset since. It is not the reading's `fetched_at`, which a failed attempt leaves as
    /// it was.
    attempted_at: i64,
    /// Consecutive attempts that failed to read the account.
    failures: u32,
    /// Read again at the next refresh whatever the wait: set by [`AccountQuotas::make_due`].
    due: bool,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<Key, Entry>,
    /// The claim (by its token) of each key whose probe or kick is running, so two requests
    /// arriving together run one.
    in_flight: HashMap<Key, u64>,
    next_token: u64,
    /// The windows of each account that have been kicked in an episode that has not ended: one
    /// that a reading shows with a reset time again is taken out.
    kicked: HashMap<Key, Vec<WindowId>>,
}

impl Inner {
    fn list(&self) -> Vec<AccountQuota> {
        let mut quotas: Vec<AccountQuota> = self
            .entries
            .values()
            .map(|entry| entry.quota.clone())
            .collect();
        quotas.sort_by_key(|quota| {
            (
                Agent::ALL.iter().position(|agent| *agent == quota.agent),
                quota.account.clone(),
            )
        });
        quotas
    }
}

/// The right to read or kick one account, held for as long as the CLI runs (a kick and the reading
/// after it under the one claim) and released on drop, so a run that panics or is abandoned never
/// leaves its account excluded from every later one. [`AccountQuotas::finish`] consumes it. A claim
/// whose account was forgotten, repointed or made due in the meantime no longer counts, and
/// whatever it finishes with is discarded.
struct Claim {
    key: Key,
    token: u64,
    /// The wall-clock time the attempt began, in milliseconds since the epoch: taken at the start
    /// rather than when the reading finishes, so a reset that lands while the CLI is answering still
    /// counts as after the attempt.
    started_at: i64,
    inner: Arc<Mutex<Inner>>,
}

impl Drop for Claim {
    fn drop(&mut self) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.in_flight.get(&self.key) == Some(&self.token) {
            inner.in_flight.remove(&self.key);
        }
    }
}

/// The readings taken lately. Every change to them is broadcast, whole, under the same lock that
/// makes it, so the order clients see is the order they happened in.
pub struct AccountQuotas {
    inner: Arc<Mutex<Inner>>,
    /// Bounds the CLI runs, probes and kicks, at once, whichever refresh started them.
    slots: Arc<Semaphore>,
    /// Set once the daemon is shutting down: a probe or kick running then is stopped, and no kick
    /// is claimed, nor any probe or kick started, after it.
    stopping: Arc<AtomicBool>,
}

impl Default for AccountQuotas {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            slots: Arc::new(Semaphore::new(MAX_CONCURRENT_CLI_RUNS)),
            stopping: Arc::default(),
        }
    }
}

impl AccountQuotas {
    /// Every reading held, by agent and then account with the default account first.
    pub fn list(&self) -> Vec<AccountQuota> {
        self.lock().list()
    }

    /// Whether the held reading of the account says its plan quota is used up at `now`; see
    /// [`exhausted_in`]. Reads what is held and never asks the CLI, so an account not read yet
    /// is usable.
    pub fn exhausted(
        &self,
        agent: Agent,
        account: Option<&str>,
        config_dir: Option<&str>,
        now: i64,
    ) -> Option<Exhausted> {
        exhausted_in(&self.lock().entries, agent, account, config_dir, now)
    }

    /// The held reading of the account under `config_dir`, if one is held. `None` for
    /// `config_dir` is the default account. A reading taken under another directory is not this
    /// account's.
    pub fn held(
        &self,
        agent: Agent,
        account: Option<&str>,
        config_dir: Option<&str>,
    ) -> Option<AccountQuota> {
        let key = Key::of(agent, account, config_dir);
        self.lock()
            .entries
            .get(&key)
            .map(|entry| entry.quota.clone())
    }

    /// Whether the held reading of the account says the agent is not signed in there
    /// (`quota_not_signed_in`) or that its directory does not exist yet
    /// (`quota_config_dir_missing`). Any other reason a reading is missing, a failed command or
    /// a timeout say, is a passing failure and is not this: it says nothing of the account.
    pub fn signed_out(
        &self,
        agent: Agent,
        account: Option<&str>,
        config_dir: Option<&str>,
    ) -> bool {
        self.held(agent, account, config_dir)
            .and_then(|quota| quota.unavailable)
            .is_some_and(|reason| {
                [code::NOT_SIGNED_IN, code::CONFIG_DIR_MISSING].contains(&reason.code.as_str())
            })
    }

    /// [`AccountQuotas::held`] and [`AccountQuotas::exhausted`] of the same account under one
    /// lock, so the reading and the judgement made from it cannot come from different moments.
    pub fn held_and_exhausted(
        &self,
        agent: Agent,
        account: Option<&str>,
        config_dir: Option<&str>,
        now: i64,
    ) -> (Option<AccountQuota>, Option<Exhausted>) {
        let inner = self.lock();
        let key = Key::of(agent, account, config_dir);
        (
            inner.entries.get(&key).map(|entry| entry.quota.clone()),
            exhausted_in(&inner.entries, agent, account, config_dir, now),
        )
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("account quotas lock poisoned")
    }

    /// Applies `change` and, when it says something changed, broadcasts the list.
    fn change(&self, state: &AppState, change: impl FnOnce(&mut Inner) -> bool) {
        let mut inner = self.lock();
        if change(&mut inner) {
            state.broadcast(Event::AccountQuotasUpdated {
                account_quotas: inner.list(),
            });
        }
    }

    fn claim_locked(&self, inner: &mut Inner, key: Key, wall: i64) -> Claim {
        inner.next_token += 1;
        inner.in_flight.insert(key.clone(), inner.next_token);
        Claim {
            key,
            token: inner.next_token,
            started_at: wall,
            inner: self.inner.clone(),
        }
    }

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
    /// holding no windows because it is not signed in or has no quota to report once [`MIN_REREAD_INTERVAL`] has passed.
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

    /// Records the outcome of the probe `claim` covered, unless the account was forgotten while
    /// it ran. A failure over a reading that had windows keeps that reading, with the reason set
    /// beside it, so a client can show it as stale; an account that never read keeps none. Only a
    /// failure counts towards the backoff: that the CLI has no quota to report is not one.
    fn finish(&self, state: &AppState, claim: Claim, mut quota: AccountQuota) {
        self.change(state, |inner| {
            if inner.in_flight.get(&claim.key) != Some(&claim.token) {
                return false;
            }
            inner.in_flight.remove(&claim.key);
            if quota.unavailable.is_none() {
                end_episodes(inner, &claim.key, &quota.windows);
            }
            let previous = inner.entries.get(&claim.key);
            let keeps_windows = quota
                .unavailable
                .as_ref()
                .filter(|u| !is_account_state(&u.code));
            let failures = if keeps_windows.is_some_and(|u| is_failure(&u.code)) {
                previous.map_or(0, |entry| entry.failures).saturating_add(1)
            } else {
                0
            };
            if let (Some(unavailable), Some(previous)) = (keeps_windows, previous) {
                if !previous.quota.windows.is_empty() {
                    quota = AccountQuota {
                        unavailable: Some(unavailable.clone()),
                        ..previous.quota.clone()
                    };
                }
            }
            inner.entries.insert(
                claim.key.clone(),
                Entry {
                    quota,
                    attempted: Instant::now(),
                    attempted_at: claim.started_at,
                    failures,
                    due: false,
                },
            );
            true
        });
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

    /// A probe's whole claim-and-finish for `key`, with `quota` as its outcome.
    #[cfg(test)]
    fn recorded(&self, state: &AppState, key: Key, quota: AccountQuota) {
        let claim = {
            let mut inner = self.lock();
            self.claim_locked(&mut inner, key, now_millis())
        };
        self.finish(state, claim, quota);
    }

    /// Puts `quota` in as a reading of its own account, as a finished probe would.
    #[cfg(test)]
    pub fn hold_for_test(&self, quota: &AccountQuota) {
        self.hold_under_for_test(quota, None);
    }

    /// [`AccountQuotas::hold_for_test`] for an account read under the directory `config_dir`.
    #[cfg(test)]
    pub fn hold_in_dir_for_test(&self, quota: &AccountQuota, config_dir: &str) {
        self.hold_under_for_test(quota, Some(config_dir));
    }

    #[cfg(test)]
    fn hold_under_for_test(&self, quota: &AccountQuota, config_dir: Option<&str>) {
        let key = Key::of(quota.agent, quota.account.as_deref(), config_dir);
        self.lock().entries.insert(
            key,
            Entry {
                quota: quota.clone(),
                attempted: Instant::now(),
                attempted_at: now_millis(),
                failures: 0,
                due: false,
            },
        );
    }

    /// Whether a window of the account has been kicked in an episode that has not ended.
    #[cfg(test)]
    pub fn is_kicked_for_test(&self, agent: Agent, account: Option<&str>) -> bool {
        self.lock()
            .kicked
            .keys()
            .any(|key| key.agent == agent && key.account.as_deref() == account)
    }

    /// Whether the held reading of the account is marked due.
    #[cfg(test)]
    pub fn is_due_for_test(&self, agent: Agent, account: Option<&str>) -> bool {
        self.lock().entries.iter().any(|(key, entry)| {
            key.agent == agent && key.account.as_deref() == account && entry.due
        })
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

    /// Makes the account's reading due and drops any claim on it, so that the next refresh reads it
    /// at once and a probe begun before a login ended cannot put its not-signed-in reading back
    /// over the new one. A kick running at the time is not stopped, so the account can be read
    /// while it still is. `account` is an account id, `None` for the default account.
    pub fn make_due(&self, agent: Agent, account: Option<&str>) {
        let mut inner = self.lock();
        let is_it = |key: &Key| key.agent == agent && key.account.as_deref() == account;
        inner.in_flight.retain(|key, _| !is_it(key));
        for (_, entry) in inner.entries.iter_mut().filter(|(key, _)| is_it(key)) {
            entry.due = true;
        }
    }

    /// Stops the probes and kicks running and keeps each kick from going on to the reading after
    /// it. A probe or kick would otherwise outlive a shutdown by up to [`PROBE_TIMEOUT`] or
    /// [`KICK_TIMEOUT`], its CLI with it.
    pub fn stop_cli_runs(&self) {
        self.stopping.store(true, Ordering::Relaxed);
    }

    /// Whether the kick `claim` covers should go on to the reading after it: the daemon is not
    /// shutting down and the claim still counts, as a login that ended or a deleted account
    /// makes it not.
    fn kick_is_wanted(&self, claim: &Claim) -> bool {
        !self.stopping.load(Ordering::Relaxed)
            && self.lock().in_flight.get(&claim.key) == Some(&claim.token)
    }

    /// Drops what is held for the account `id`, and any probe or kick running for it along with
    /// its episodes, so that a probe begun under the old directory cannot put its reading back
    /// and a kick is not followed by a reading.
    pub fn forget_account(&self, state: &AppState, id: &str) {
        self.change(state, |inner| {
            let before = inner.entries.len();
            inner
                .entries
                .retain(|key, _| key.account.as_deref() != Some(id));
            inner
                .in_flight
                .retain(|key, _| key.account.as_deref() != Some(id));
            inner
                .kicked
                .retain(|key, _| key.account.as_deref() != Some(id));
            inner.entries.len() != before
        });
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

/// The wire name of the session window, whichever agent it belongs to. Each agent's own interface
/// calls it something else ([`Exhausted::named_for`]).
const SESSION_WINDOW: &str = "5-hour";

/// The window that keeps an account from opening a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exhausted {
    /// The window's name in English, the same for every agent: `5-hour`, `Weekly`, `Monthly` or
    /// `Credits`, or the window's own label when it carries one.
    pub window: String,
    /// The window's kind, from which a client words it as the agent's own interface does.
    pub kind: QuotaWindowKind,
    /// The window's length, if known; what names a credit pool.
    pub window_minutes: Option<u32>,
    /// Whether the same reading also has a model's own weekly window, which makes an account-wide
    /// weekly window "Weekly · all models" in Claude Code's interface.
    pub has_model_weekly: bool,
    /// When it resets, in milliseconds since the epoch; `None` when the agent does not say.
    pub resets_at: Option<i64>,
}

impl Exhausted {
    /// The window's name for a sentence about `agent`'s account, as that agent's own interface
    /// calls it: Claude Code's `session` and `Weekly · all models` (beside a model's own weekly
    /// window), Codex's `5h`; any other name as `window` has it.
    pub fn named_for(&self, agent: Agent) -> &str {
        match (self.kind, agent) {
            (QuotaWindowKind::Session, Agent::Claude) => "session",
            (QuotaWindowKind::Session, _) => "5h",
            (QuotaWindowKind::Weekly, Agent::Claude) if self.has_model_weekly => {
                "Weekly · all models"
            }
            _ => &self.window,
        }
    }
}

/// The window of the account's reading that keeps it from opening a session at `now`, if any: a
/// session, weekly or credit-period window used up (100% or more) that has not reset yet or says
/// nothing of when it does. A model's own weekly window never counts, as the account still serves
/// the other models. A window whose reset time has passed is stale rather than exhausted. With
/// several, the one that resets last is reported, since the account is unusable until then (one
/// without a reset time counts as last). No reading, or one without windows, passes: an unknown
/// quota never refuses. A reading kept over a failed re-read is judged by the windows it kept.
/// The reading must be the account's under `config_dir`, as `Key` has it.
fn exhausted_in(
    entries: &HashMap<Key, Entry>,
    agent: Agent,
    account: Option<&str>,
    config_dir: Option<&str>,
    now: i64,
) -> Option<Exhausted> {
    let key = Key::of(agent, account, config_dir);
    let windows = &entries.get(&key)?.quota.windows;
    let has_model_weekly = windows
        .iter()
        .any(|window| window.kind == QuotaWindowKind::WeeklyModel);
    windows
        .iter()
        .filter(|window| {
            window.kind != QuotaWindowKind::WeeklyModel
                && window.used_percent >= 100.0
                && window.resets_at.is_none_or(|at| at > now)
        })
        .reduce(|latest, window| {
            let last = |window: &QuotaWindow| window.resets_at.unwrap_or(i64::MAX);
            if last(window) > last(latest) {
                window
            } else {
                latest
            }
        })
        .map(|window| Exhausted {
            window: window_name(window),
            kind: window.kind,
            window_minutes: window.window_minutes,
            has_model_weekly,
            resets_at: window.resets_at,
        })
}

/// What a window is called: by kind, with a credit pool named after its period (about a week,
/// about a month, or just credits). A window that carries its own label is named by it; the windows
/// that count today carry none, so that branch is a fallback.
fn window_name(window: &QuotaWindow) -> String {
    if let Some(label) = window.label.as_ref().filter(|label| !label.is_empty()) {
        return label.clone();
    }
    match window.kind {
        QuotaWindowKind::Session => SESSION_WINDOW,
        QuotaWindowKind::Weekly | QuotaWindowKind::WeeklyModel => "Weekly",
        QuotaWindowKind::CreditPeriod => match window.window_minutes {
            Some(minutes) if (6 * 24 * 60..=8 * 24 * 60).contains(&minutes) => "Weekly",
            Some(minutes) if (28 * 24 * 60..=31 * 24 * 60).contains(&minutes) => "Monthly",
            _ => "Credits",
        },
    }
    .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpiringWeekly {
    pub resets_at: i64,
    pub remaining: f64,
}

impl ExpiringWeekly {
    /// Whether this window ranks ahead of `other`: a sooner reset, then a larger remainder.
    /// An equal pair does not, so the earlier candidate stays.
    pub fn ranks_ahead_of(self, other: Self) -> bool {
        self.resets_at < other.resets_at
            || (self.resets_at == other.resets_at
                && self.remaining.total_cmp(&other.remaining).is_gt())
    }
}

/// A weekly window that still has quota and whose reset is still ahead.
/// Account-wide `Weekly`, or `CreditPeriod` whose `window_minutes` is in
/// `6 * 24 * 60..=8 * 24 * 60`. A model's own weekly window never counts.
/// `used_percent` must be finite and below 100. A reset that is missing or
/// not strictly after `now` does not rank. Among the windows of one account,
/// the soonest reset wins, so quota about to reset is spent first, then the
/// larger remaining (`(100 - used_percent).max(0)`).
pub fn expiring_weekly(windows: &[QuotaWindow], now: i64) -> Option<ExpiringWeekly> {
    let mut best: Option<ExpiringWeekly> = None;
    for window in windows {
        let weekly = match window.kind {
            QuotaWindowKind::Weekly => true,
            QuotaWindowKind::CreditPeriod => window
                .window_minutes
                .is_some_and(|minutes| (6 * 24 * 60..=8 * 24 * 60).contains(&minutes)),
            QuotaWindowKind::Session | QuotaWindowKind::WeeklyModel => false,
        };
        if !weekly || !window.used_percent.is_finite() || window.used_percent >= 100.0 {
            continue;
        }
        let Some(resets_at) = window.resets_at else {
            continue;
        };
        if resets_at <= now {
            continue;
        }
        let candidate = ExpiringWeekly {
            resets_at,
            remaining: window.remaining_percent(),
        };
        if best
            .as_ref()
            .is_none_or(|current| candidate.ranks_ahead_of(*current))
        {
            best = Some(candidate);
        }
    }
    best
}

/// Whether `code` is one of an account's states rather than a failure to read it.
fn is_account_state(code: &str) -> bool {
    code == self::code::CONFIG_DIR_MISSING || code == self::code::NOT_SIGNED_IN
}

/// Whether `code` is a failure to read the account, which backs it off: neither one of its states
/// nor the CLI having no quota to report.
fn is_failure(code: &str) -> bool {
    !is_account_state(code) && code != self::code::NO_WINDOWS
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

/// Ends the episode of each window of `key` that a reading shows with a reset time.
fn end_episodes(inner: &mut Inner, key: &Key, windows: &[QuotaWindow]) {
    let Some(kicked) = inner.kicked.get_mut(key) else {
        return;
    };
    kicked.retain(|id| {
        !windows
            .iter()
            .any(|window| window.resets_at.is_some() && window_id(window) == *id)
    });
    if kicked.is_empty() {
        inner.kicked.remove(key);
    }
}

/// Drops every reading, every probe in flight and every episode whose key is not in `wanted`; true
/// when a reading went.
fn retain_locked(inner: &mut Inner, wanted: &[Key]) -> bool {
    let before = inner.entries.len();
    inner.entries.retain(|key, _| wanted.contains(key));
    inner.in_flight.retain(|key, _| wanted.contains(key));
    inner.kicked.retain(|key, _| wanted.contains(key));
    inner.entries.len() != before
}

/// How long after an attempt an account is read again, given how many in a row ended unavailable.
fn wait_after(failures: u32) -> Duration {
    POLL_INTERVAL * 2u32.pow(failures.min(MAX_BACKOFF_DOUBLINGS))
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
enum Trigger {
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
async fn refresh_with(state: &Arc<AppState>, scope: &Scope, trigger: Trigger, shell: ShellSource) {
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
/// reading calls for one (see [`AccountQuotas::claim_kick`]) and `kicks` allows it. Holds a slot
/// while the CLI runs.
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
    // A reading that the shutdown cut short says nothing about the account, so it is not recorded.
    if state.account_quotas.stopping.load(Ordering::Relaxed) {
        return None;
    }
    let quota = outcome.unwrap_or_else(|err| {
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
        )
    });
    let key = claim.key.clone();
    state.account_quotas.finish(state, claim, quota);
    if kicks {
        state.account_quotas.claim_kick(state, &key)
    } else {
        // Left to the poller, which reads it again within a tick and kicks as it always does.
        state.account_quotas.make_due_for_kick(state, &key);
        None
    }
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

/// Reads one account's quota in the shell environment `shell` gives. Blocking: it runs a
/// subprocess, which setting `cancel` stops.
fn read(
    key: &Key,
    shell: &(dyn Fn() -> anyhow::Result<HashMap<String, String>> + Send + Sync),
    cancel: &AtomicBool,
) -> AccountQuota {
    // Nothing is captured or started once the daemon is shutting down; the caller drops the result.
    if cancel.load(Ordering::Relaxed) {
        return unavailable_quota(key, failed(key.agent, "was cancelled before it started"));
    }
    if let Some(dir) = missing_config_dir(key) {
        return unavailable_quota(key, config_dir_missing(key.agent, dir));
    }
    let result = match shell() {
        Ok(shell_env) => probe(
            key.agent,
            &shell_env,
            key.config_dir.as_deref().map(Path::new),
            PROBE_TIMEOUT,
            cancel,
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
    match result {
        Ok(reading) => AccountQuota {
            agent: key.agent,
            account: key.account.clone(),
            fetched_at: now_millis(),
            plan: reading.plan,
            windows: reading.windows,
            unavailable: None,
        },
        Err(unavailable) => unavailable_quota(key, unavailable),
    }
}

/// The account's configuration directory when it is gone. Looked for before anything is run:
/// Claude Code and Grok Build would create the directory, empty, and then call the account not
/// signed in, and a poll or a kick would keep bringing back one a user deleted. Only `NotFound`
/// says it is gone: a directory that cannot be looked at, or a regular file in its place, is left
/// to the CLI to report. The default account pins nothing, so Codex's own refusal of a missing
/// home is what tells that case (see `probe`).
fn missing_config_dir(key: &Key) -> Option<&str> {
    key.config_dir.as_deref().filter(|dir| {
        matches!(std::fs::metadata(dir), Err(err) if err.kind() == std::io::ErrorKind::NotFound)
    })
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
        windows: Vec::new(),
        unavailable: Some(unavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_made_due_is_claimed_at_once_whatever_the_clock_says() {
        let state = crate::test_support::app_state("quota-make-due").0;
        let quotas = &state.account_quotas;
        let codex = key(Agent::Codex, Some("a"));
        quotas.recorded(
            &state,
            codex.clone(),
            unavailable_quota(
                &codex,
                QuotaUnavailable::new(code::COMMAND_FAILED, "failed", &[]),
            ),
        );
        // It was just read, and failed, so it waits.
        let now = Instant::now();
        assert!(quotas.claim_due(vec![codex.clone()], now).is_empty());
        quotas.make_due(Agent::Codex, Some("a"));
        assert_eq!(quotas.claim_due(vec![codex.clone()], now).len(), 1);
    }

    #[test]
    fn the_wait_before_another_attempt_doubles_with_failures_up_to_a_cap() {
        assert_eq!(wait_after(0), POLL_INTERVAL);
        assert_eq!(wait_after(1), POLL_INTERVAL * 2);
        assert_eq!(wait_after(3), POLL_INTERVAL * 8);
        assert_eq!(wait_after(40), POLL_INTERVAL * 8);
    }

    #[test]
    fn an_account_is_due_once_a_held_window_has_reset_and_the_grace_has_passed() {
        let (state, _dir) = crate::test_support::app_state("quota-reset");
        let quotas = &state.account_quotas;
        let key = key(Agent::Codex, Some("a"));
        let mut held = quota_of(&key, false);
        held.windows = vec![window(QuotaWindowKind::Session, 100.0, Some(NOW))];
        quotas.recorded(&state, key.clone(), held);
        let set_attempt = |at: i64| {
            quotas.lock().entries.get_mut(&key).unwrap().attempted_at = at;
        };
        let grace = RESET_GRACE.as_millis() as i64;
        let now = Instant::now();
        let due = |wall: i64| quotas.claim_due_at(vec![key.clone()], now, wall).len();

        set_attempt(NOW - 1_000);
        assert_eq!(due(NOW + grace - 1), 0);
        assert_eq!(due(NOW + grace), 1);

        // A reset that came before the last attempt was already seen by it.
        set_attempt(NOW + 1);
        assert_eq!(due(NOW + grace + 1), 0);
    }

    #[test]
    fn a_failed_re_read_after_a_reset_falls_back_to_the_ordinary_wait() {
        let (state, _dir) = crate::test_support::app_state("quota-reset-failed");
        let quotas = &state.account_quotas;
        let key = key(Agent::Codex, Some("a"));
        let mut held = quota_of(&key, false);
        held.windows = vec![window(
            QuotaWindowKind::Session,
            100.0,
            Some(now_millis() - 60_000),
        )];
        quotas.recorded(&state, key.clone(), held);
        quotas.lock().entries.get_mut(&key).unwrap().attempted_at = now_millis() - 120_000;
        let claims = quotas.claim_due(vec![key.clone()], Instant::now());
        assert_eq!(claims.len(), 1);

        // The re-read fails, keeping the old windows: it began after the reset, so no refire.
        drop(claims);
        quotas.recorded(&state, key.clone(), quota_of(&key, true));
        assert!(quotas.claim_due(vec![key], Instant::now()).is_empty());
    }

    fn key(agent: Agent, account: Option<&str>) -> Key {
        Key {
            agent,
            account: account.map(str::to_string),
            config_dir: account.map(|id| format!("/accounts/{id}")),
        }
    }

    fn quota_of(key: &Key, failed: bool) -> AccountQuota {
        AccountQuota {
            agent: key.agent,
            account: key.account.clone(),
            fetched_at: 1,
            plan: None,
            windows: Vec::new(),
            unavailable: failed
                .then(|| QuotaUnavailable::new(code::TIMED_OUT, "no", &[("agent", "x")])),
        }
    }

    #[test]
    fn only_accounts_never_read_or_past_their_wait_are_due_and_a_running_one_is_not_claimed_twice()
    {
        let (state, _dir) = crate::test_support::app_state("quota-due");
        let quotas = AccountQuotas::default();
        let (fresh, failing, new) = (
            key(Agent::Claude, None),
            key(Agent::Codex, Some("a")),
            key(Agent::Grok, None),
        );
        quotas.recorded(&state, fresh.clone(), quota_of(&fresh, false));
        quotas.recorded(&state, failing.clone(), quota_of(&failing, true));
        let all = vec![fresh.clone(), failing.clone(), new.clone()];

        let due = |quotas: &AccountQuotas, now: Instant| -> Vec<Key> {
            let claims = quotas.claim_due(all.clone(), now);
            claims.iter().map(|claim| claim.key.clone()).collect()
        };
        let now = Instant::now();
        let first = quotas.claim_due(all.clone(), now);
        assert_eq!(first[0].key, new);
        assert_eq!(first.len(), 1);
        // Claimed once; not again while its probe runs.
        assert!(due(&quotas, now).is_empty());

        // After the interval the healthy account is due; the failing one waits twice as long.
        let later = now + POLL_INTERVAL + Duration::from_secs(1);
        let second = quotas.claim_due(all.clone(), later);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].key, fresh);
        // `fresh` is running now, so only the failing one can be claimed.
        let much_later = now + POLL_INTERVAL * 2 + Duration::from_secs(1);
        assert_eq!(due(&quotas, much_later), [failing]);

        // A claim that is dropped without finishing releases its account.
        drop(first);
        assert_eq!(due(&quotas, now).len(), 1);
    }

    #[test]
    fn a_probe_running_for_a_forgotten_or_repointed_account_cannot_put_its_reading_back() {
        let (state, _dir) = crate::test_support::app_state("quota-forgotten");
        let mut events = state.subscribe();
        let quotas = &state.account_quotas;
        let (deleted, repointed) = (key(Agent::Codex, Some("a")), key(Agent::Codex, Some("b")));
        let now = Instant::now();
        let claims = quotas.claim_due(vec![deleted.clone(), repointed.clone()], now);
        assert_eq!(claims.len(), 2);

        quotas.forget_account(&state, "a");
        // The repointed account's old key is no longer among the targets.
        quotas.change(&state, |inner| {
            retain_locked(inner, &[key(Agent::Codex, None)])
        });
        for claim in claims {
            let quota = quota_of(&claim.key, false);
            quotas.finish(&state, claim, quota);
        }
        assert!(quotas.list().is_empty());
        assert!(events.try_recv().is_err());
        // Nothing is left marked as running either.
        assert_eq!(quotas.claim_due(vec![deleted, repointed], now).len(), 2);
    }

    #[test]
    fn a_failure_keeps_the_last_good_windows_beside_the_reason_and_a_success_clears_it() {
        let (state, _dir) = crate::test_support::app_state("quota-stale");
        let key = key(Agent::Codex, None);
        let good = AccountQuota {
            fetched_at: 7,
            plan: Some("plus".to_string()),
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 15.0,
                resets_at: None,
                window_minutes: None,
            }],
            ..quota_of(&key, false)
        };
        // Never read: a failure holds no windows.
        state
            .account_quotas
            .recorded(&state, key.clone(), quota_of(&key, true));
        assert!(state.account_quotas.list()[0].windows.is_empty());

        state
            .account_quotas
            .recorded(&state, key.clone(), good.clone());
        state
            .account_quotas
            .recorded(&state, key.clone(), quota_of(&key, true));
        let held = &state.account_quotas.list()[0];
        assert_eq!(held.windows, good.windows);
        assert_eq!((held.fetched_at, held.plan.as_deref()), (7, Some("plus")));
        assert_eq!(
            held.unavailable.as_ref().map(|u| u.code.as_str()),
            Some(code::TIMED_OUT)
        );

        state
            .account_quotas
            .recorded(&state, key.clone(), good.clone());
        assert_eq!(state.account_quotas.list()[0], good);
    }

    fn no_windows(key: &Key) -> AccountQuota {
        AccountQuota {
            unavailable: Some(QuotaUnavailable::new(
                code::NO_WINDOWS,
                "none",
                &[("agent", "Claude Code")],
            )),
            ..quota_of(key, false)
        }
    }

    fn with_windows(key: &Key) -> AccountQuota {
        AccountQuota {
            fetched_at: 7,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 15.0,
                resets_at: None,
                window_minutes: None,
            }],
            ..quota_of(key, false)
        }
    }

    #[test]
    fn no_quota_to_report_is_not_backed_off_but_still_keeps_the_windows_it_held() {
        let (state, _dir) = crate::test_support::app_state("quota-no-windows");
        let quotas = &state.account_quotas;
        let key = key(Agent::Claude, None);
        for _ in 0..3 {
            quotas.recorded(&state, key.clone(), no_windows(&key));
        }
        let later = Instant::now() + POLL_INTERVAL + Duration::from_secs(1);
        assert_eq!(quotas.claim_due(vec![key.clone()], later).len(), 1);

        let good = with_windows(&key);
        quotas.recorded(&state, key.clone(), good.clone());
        quotas.recorded(&state, key.clone(), no_windows(&key));
        let held = &quotas.list()[0];
        assert_eq!(held.windows, good.windows);
        assert_eq!(
            held.unavailable.as_ref().map(|u| u.code.as_str()),
            Some(code::NO_WINDOWS)
        );
    }

    #[test]
    fn a_request_reads_an_account_that_may_have_signed_in_since_early_but_not_within_the_minimum() {
        let (state, _dir) = crate::test_support::app_state("quota-request-reread");
        let quotas = &state.account_quotas;
        let (signed_out, none, windows, failing, missing) = (
            key(Agent::Claude, None),
            key(Agent::Claude, Some("a")),
            key(Agent::Codex, None),
            key(Agent::Codex, Some("b")),
            key(Agent::Grok, None),
        );
        let not_signed_in = AccountQuota {
            unavailable: Some(QuotaUnavailable::new(code::NOT_SIGNED_IN, "no", &[])),
            ..quota_of(&signed_out, false)
        };
        quotas.recorded(&state, signed_out.clone(), not_signed_in);
        quotas.recorded(&state, none.clone(), no_windows(&none));
        // Stale windows beside a NO_WINDOWS reason: there is something to show.
        quotas.recorded(&state, windows.clone(), with_windows(&windows));
        quotas.recorded(&state, windows.clone(), no_windows(&windows));
        quotas.recorded(&state, failing.clone(), quota_of(&failing, true));
        let dir_missing = AccountQuota {
            unavailable: Some(QuotaUnavailable::new(code::CONFIG_DIR_MISSING, "gone", &[])),
            ..quota_of(&missing, false)
        };
        quotas.recorded(&state, missing.clone(), dir_missing);
        let all = vec![signed_out.clone(), none.clone(), windows, failing, missing];
        let keys = |claims: &[Claim]| -> Vec<Key> {
            claims.iter().map(|claim| claim.key.clone()).collect()
        };

        let now = Instant::now();
        // Just attempted: nothing is claimed, for a request either.
        assert!(quotas.claim_requested(all.clone(), now).is_empty());
        // Past the minimum but not the interval: only the accounts that may have signed in since, and only on request.
        let soon = now + MIN_REREAD_INTERVAL + Duration::from_secs(1);
        assert!(quotas.claim_due(all.clone(), soon).is_empty());
        let claimed = quotas.claim_requested(all.clone(), soon);
        assert_eq!(keys(&claimed), [signed_out, none]);
        // Those are running now, so a second request starts nothing.
        assert!(quotas.claim_requested(all, soon).is_empty());
    }

    #[test]
    fn every_change_is_broadcast_whole_and_a_forgotten_account_is_dropped() {
        let (state, _dir) = crate::test_support::app_state("quota-broadcast");
        let mut events = state.subscribe();
        let (default, owned) = (key(Agent::Claude, None), key(Agent::Claude, Some("a")));
        state
            .account_quotas
            .recorded(&state, owned.clone(), quota_of(&owned, false));
        state
            .account_quotas
            .recorded(&state, default.clone(), quota_of(&default, false));
        let mut last = None;
        while let Ok(event) = events.try_recv() {
            last = Some(event);
        }
        let Some(Event::AccountQuotasUpdated { account_quotas }) = last else {
            panic!("no update was broadcast");
        };
        // The default account first.
        let accounts: Vec<_> = account_quotas
            .iter()
            .map(|q| q.account.as_deref())
            .collect();
        assert_eq!(accounts, [None, Some("a")]);

        state.account_quotas.forget_account(&state, "a");
        let Ok(Event::AccountQuotasUpdated { account_quotas }) = events.try_recv() else {
            panic!("the drop was not broadcast");
        };
        assert_eq!(account_quotas.len(), 1);
        // Nothing to drop, nothing to say.
        state.account_quotas.forget_account(&state, "a");
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn readings_of_accounts_no_longer_targeted_are_dropped() {
        let (state, _dir) = crate::test_support::app_state("quota-retain");
        let (kept, gone) = (key(Agent::Codex, None), key(Agent::Codex, Some("a")));
        state
            .account_quotas
            .recorded(&state, kept.clone(), quota_of(&kept, false));
        state
            .account_quotas
            .recorded(&state, gone.clone(), quota_of(&gone, false));
        state.account_quotas.change(&state, |inner| {
            retain_locked(inner, std::slice::from_ref(&kept))
        });
        let held = state.account_quotas.list();
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].account, None);
    }

    #[test]
    fn an_account_state_drops_stale_windows_is_not_backed_off_and_a_login_makes_it_due() {
        let (state, _dir) = crate::test_support::app_state("quota-account-state");
        let key = key(Agent::Codex, Some("a"));
        let good = AccountQuota {
            fetched_at: 7,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 15.0,
                resets_at: None,
                window_minutes: None,
            }],
            ..quota_of(&key, false)
        };
        let signed_out = AccountQuota {
            unavailable: Some(QuotaUnavailable::new(
                code::NOT_SIGNED_IN,
                "no",
                &[("agent", "Codex")],
            )),
            ..quota_of(&key, false)
        };
        state.account_quotas.recorded(&state, key.clone(), good);
        state
            .account_quotas
            .recorded(&state, key.clone(), signed_out.clone());
        // Unlike a failure, the account's state replaces what it held.
        let held = state.account_quotas.list();
        assert!(held[0].windows.is_empty());
        assert_eq!(
            held[0].unavailable.as_ref().map(|u| u.code.as_str()),
            Some(code::NOT_SIGNED_IN)
        );
        // And is read again after the ordinary interval, however long it has been so.
        state
            .account_quotas
            .recorded(&state, key.clone(), signed_out);
        let later = Instant::now() + POLL_INTERVAL + Duration::from_secs(1);
        assert_eq!(
            state
                .account_quotas
                .claim_due(vec![key.clone()], later)
                .len(),
            1
        );

        // A login that ended makes it due at once, and a probe running meanwhile is discarded.
        let running = state
            .account_quotas
            .claim_due(vec![key.clone()], Instant::now() + POLL_INTERVAL * 100);
        assert_eq!(running.len(), 1);
        state.account_quotas.make_due(Agent::Codex, Some("a"));
        let again = state
            .account_quotas
            .claim_due(vec![key.clone()], Instant::now());
        assert_eq!(again.len(), 1);
        state.account_quotas.finish(
            &state,
            running.into_iter().next().unwrap(),
            quota_of(&key, false),
        );
        assert!(state.account_quotas.list()[0].unavailable.is_some());
    }

    /// A scope that leaves out the accounts the user turned off selects the others only, the
    /// default account among them, while the default scope selects every account.
    #[tokio::test]
    async fn a_scope_can_leave_out_the_accounts_the_user_turned_off() {
        let (state, dir) = crate::test_support::app_state("quota-scope-enabled");
        state.set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: Agent::Codex,
            availability: Availability::Available,
            default_account_dir: None,
        }]);
        for (id, disabled) in [("on", false), ("off", true)] {
            state
                .store
                .insert_account(&crate::protocol::Account {
                    id: id.to_string(),
                    agent: Agent::Codex,
                    name: id.to_string(),
                    config_dir: dir.join(id).to_string_lossy().into_owned(),
                    share_default_config: false,
                    share_default_config_incomplete: false,
                    disabled,
                })
                .unwrap();
        }
        let selected = |scope: Scope| {
            let mut accounts: Vec<Option<String>> = state
                .account_quotas
                .claim_scope(&state, &scope, Instant::now(), Trigger::Poll)
                .unwrap()
                .into_iter()
                .map(|claim| claim.key.account.clone())
                .collect();
            // Let the next selection claim them again.
            state.account_quotas.lock().in_flight.clear();
            accounts.sort();
            accounts
        };

        assert_eq!(
            selected(Scope::default()),
            [None, Some("off".to_string()), Some("on".to_string())]
        );
        assert_eq!(
            selected(Scope {
                enabled_only: true,
                ..Scope::default()
            }),
            [None, Some("on".to_string())]
        );
    }

    /// A missing directory is reported without running anything, and read for real once it exists.
    #[tokio::test]
    async fn a_missing_config_directory_is_a_state_found_before_the_cli_is_run() {
        let (state, dir) = crate::test_support::app_state("quota-missing-dir");
        let bin = dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(
            bin.join("codex.reply"),
            include_str!("../../testdata/codex_rate_limits_reply.json"),
        )
        .unwrap();
        let env = crate::test_support::fake_agent(
            &bin,
            "codex",
            &format!(
                "echo run >> '{d}/runs'\nwhile read line; do case \"$line\" in *rateLimits*) cat '{d}/codex.reply';; esac; done",
                d = bin.display()
            ),
        );
        let shell: ShellSource = Arc::new(move || Ok(env.clone()));
        state.set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: Agent::Codex,
            availability: Availability::Available,
            default_account_dir: None,
        }]);
        let config_dir = dir.join("codex-work");
        let account = crate::protocol::Account {
            id: "work".to_string(),
            agent: Agent::Codex,
            name: "Work".to_string(),
            config_dir: config_dir.to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        };
        state.store.insert_account(&account).unwrap();
        let scope = Scope {
            agent: Some(Agent::Codex),
            account: Some("work".to_string()),
            ..Scope::default()
        };

        refresh_with(&state, &scope, Trigger::Poll, shell.clone()).await;
        let held = state.account_quotas.list();
        let unavailable = held[0].unavailable.as_ref().expect("a state");
        assert_eq!(unavailable.code, code::CONFIG_DIR_MISSING);
        assert_eq!(
            unavailable.params.get("dir").map(String::as_str),
            config_dir.to_str()
        );
        assert!(!bin.join("runs").exists());
        assert!(!config_dir.exists());

        std::fs::create_dir(&config_dir).unwrap();
        state.account_quotas.make_due(Agent::Codex, Some("work"));
        refresh_with(&state, &scope, Trigger::Poll, shell).await;
        assert!(state.account_quotas.list()[0].unavailable.is_none());
        assert!(bin.join("runs").exists());
    }

    #[test]
    fn a_directory_that_cannot_be_judged_missing_is_left_to_the_cli() {
        let dir = crate::test_support::ScratchDir::new("quota-dir-is-a-file");
        let bin = dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let env = crate::test_support::fake_agent(
            &bin,
            "codex",
            &format!("echo run >> '{}/runs'; exit 1", bin.display()),
        );
        // A regular file where the directory should be is not `NotFound`.
        let file = dir.join("not-a-dir");
        std::fs::write(&file, "x").unwrap();
        let key = Key {
            agent: Agent::Codex,
            account: Some("a".to_string()),
            config_dir: Some(file.to_string_lossy().into_owned()),
        };
        let shell = move || Ok(env.clone());
        let quota = read(&key, &shell, &AtomicBool::new(false));
        assert_ne!(quota.unavailable.unwrap().code, code::CONFIG_DIR_MISSING);
        assert!(bin.join("runs").exists());
    }

    /// Runs `refresh_with` over fake Codex and Grok CLIs that count their runs under `dir`.
    #[tokio::test]
    async fn a_refresh_reads_what_its_scope_selects_once_and_announces_each_reading() {
        let (state, dir) = crate::test_support::app_state("quota-refresh");
        let bin = dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let reply = |name: &str, text: &str| std::fs::write(bin.join(name), text).unwrap();
        reply(
            "codex.reply",
            include_str!("../../testdata/codex_rate_limits_reply.json"),
        );
        reply(
            "grok.reply",
            include_str!("../../testdata/grok_billing_reply.json"),
        );
        let counting = |agent: &str| {
            format!(
                "echo run >> '{d}/{agent}.runs'\nwhile read line; do case \"$line\" in *rateLimits*|*billing*) cat '{d}/{agent}.reply';; esac; done",
                d = bin.display()
            )
        };
        crate::test_support::fake_agent(&bin, "codex", &counting("codex"));
        let env = crate::test_support::fake_agent(&bin, "grok", &counting("grok"));
        let runs = |agent: &str| {
            std::fs::read_to_string(bin.join(format!("{agent}.runs")))
                .map_or(0, |text| text.lines().count())
        };
        let shell: ShellSource = Arc::new(move || Ok(env.clone()));

        state.set_agent_availability(
            [Agent::Codex, Agent::Grok]
                .into_iter()
                .map(|agent| crate::protocol::AgentAvailability {
                    agent,
                    availability: Availability::Available,
                    default_account_dir: None,
                })
                .collect(),
        );
        let codex_work = dir.join("codex-work");
        std::fs::create_dir(&codex_work).unwrap();
        let account = crate::protocol::Account {
            id: "work".to_string(),
            agent: Agent::Codex,
            name: "Work".to_string(),
            config_dir: codex_work.to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        };
        state.store.insert_account(&account).unwrap();
        let mut events = state.subscribe();
        let held = |state: &Arc<AppState>| -> Vec<(Agent, Option<String>)> {
            state
                .account_quotas
                .list()
                .into_iter()
                .map(|quota| (quota.agent, quota.account))
                .collect()
        };

        // One account of one agent: only that one is read, and its reading is announced.
        let scope = Scope {
            agent: Some(Agent::Codex),
            account: Some("work".to_string()),
            ..Scope::default()
        };
        refresh_with(&state, &scope, Trigger::Poll, shell.clone()).await;
        assert_eq!(held(&state), [(Agent::Codex, Some("work".to_string()))]);
        assert_eq!((runs("codex"), runs("grok")), (1, 0));
        let mut announced = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let Event::AccountQuotasUpdated { account_quotas } = event {
                announced.push(account_quotas.len());
            }
        }
        assert_eq!(announced, [1]);

        // One agent: its default account joins; the other agent stays unread.
        let scope = Scope {
            agent: Some(Agent::Grok),
            account: None,
            ..Scope::default()
        };
        refresh_with(&state, &scope, Trigger::Poll, shell.clone()).await;
        assert_eq!((runs("codex"), runs("grok")), (1, 1));

        // Everything: only what was never read is read, and what is not available is left out.
        refresh_with(&state, &Scope::default(), Trigger::Poll, shell).await;
        assert_eq!((runs("codex"), runs("grok")), (2, 1));
        assert_eq!(
            held(&state),
            [
                (Agent::Codex, None),
                (Agent::Codex, Some("work".to_string())),
                (Agent::Grok, None),
            ]
        );
        assert!(state
            .account_quotas
            .list()
            .iter()
            .all(|quota| quota.unavailable.is_none()));
    }

    /// A default Codex account over a fake CLI: its quota requests are answered from `reply`
    /// and each run counted in `runs`; `exec` is a kick, logged with its arguments in `kicks`,
    /// failing while `kick-fails` exists, not ending while `kick-sleeps` exists, and otherwise
    /// leaving `after.reply` as the reply.
    struct KickFixture {
        state: Arc<AppState>,
        bin: std::path::PathBuf,
        shell: ShellSource,
        _dir: crate::test_support::ScratchDir,
    }

    /// Codex's reply with its 5-hour window started or not.
    fn codex_reply(started: bool) -> String {
        let reply = include_str!("../../testdata/codex_rate_limits_reply.json");
        if started {
            reply.to_string()
        } else {
            reply.replace(r#""resetsAt":1792261723"#, r#""resetsAt":null"#)
        }
    }

    impl KickFixture {
        fn new(label: &str, setting: bool) -> Self {
            let (state, dir) = crate::test_support::app_state(label);
            let bin = dir.join("bin");
            std::fs::create_dir(&bin).unwrap();
            let d = bin.display();
            let env = crate::test_support::fake_agent(
                &bin,
                "codex",
                &format!(
                    "case \"$1\" in\n\
                     exec) echo \"$*\" >> '{d}/kicks'; [ -e '{d}/kick-fails' ] && exit 1\n\
                     while [ -e '{d}/kick-sleeps' ]; do sleep 0.02; done\n\
                     cp '{d}/after.reply' '{d}/reply';;\n\
                     *) echo run >> '{d}/runs'\n\
                     while [ -e '{d}/probe-sleeps' ]; do sleep 0.02; done\n\
                     while read line; do case \"$line\" in *rateLimits*) cat '{d}/reply';; \
                     esac; done;;\n\
                     esac"
                ),
            );
            state.set_agent_availability(vec![crate::protocol::AgentAvailability {
                agent: Agent::Codex,
                availability: Availability::Available,
                default_account_dir: None,
            }]);
            state.store.set_auto_start_quota_windows(setting).unwrap();
            let fixture = Self {
                state,
                bin,
                shell: Arc::new(move || Ok(env.clone())),
                _dir: dir,
            };
            fixture.serve(&codex_reply(false));
            fixture.after_kick(&codex_reply(true));
            fixture
        }

        fn serve(&self, reply: &str) {
            std::fs::write(self.bin.join("reply"), reply).unwrap();
        }

        fn after_kick(&self, reply: &str) {
            std::fs::write(self.bin.join("after.reply"), reply).unwrap();
        }

        /// Makes the account's last attempt `age` old.
        fn age_attempt(&self, age: Duration) {
            let mut inner = self.state.account_quotas.lock();
            for entry in inner.entries.values_mut() {
                entry.attempted = Instant::now() - age;
            }
        }

        fn count(&self, file: &str) -> usize {
            std::fs::read_to_string(self.bin.join(file)).map_or(0, |text| text.lines().count())
        }

        /// A due reading of the account, with the kick it leads to and the reading after that,
        /// awaited.
        async fn refresh(&self) {
            self.state.account_quotas.make_due(Agent::Codex, None);
            refresh_with(
                &self.state,
                &Scope::default(),
                Trigger::Poll,
                self.shell.clone(),
            )
            .await;
            self.settled().await;
        }

        /// Waits for the work a refresh left in the background: the account is claimed for as long
        /// as a kick or the reading after it is running.
        async fn settled(&self) {
            let deadline = Instant::now() + crate::test_support::PATIENCE;
            while !self.state.account_quotas.lock().in_flight.is_empty() {
                assert!(Instant::now() < deadline, "the kick did not end");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        fn session_reset(&self) -> Option<i64> {
            let held = self.state.account_quotas.list();
            held[0].windows[0].resets_at
        }
    }

    /// A refresh on demand reads an account whose last attempt is over a minute old though it is
    /// not due, and starts no window itself, the setting that starts them being on; the poller's
    /// next tick does, without the account having to wait for its ordinary interval.
    #[tokio::test]
    async fn a_refresh_on_demand_rereads_an_account_and_leaves_the_kick_to_the_poller() {
        let fixture = KickFixture::new("quota-demand-reads", true);
        fixture.refresh().await;
        // The poll kicked it, as the setting asks; the reading of that is the baseline.
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
        fixture.serve(&codex_reply(false));
        fixture.age_attempt(Duration::from_secs(61));

        refresh_on_demand(&fixture.state, &Scope::default(), fixture.shell.clone()).await;
        fixture.settled().await;

        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (3, 1));
        assert_eq!(fixture.session_reset(), None);
        assert!(fixture
            .state
            .account_quotas
            .is_due_for_test(Agent::Codex, None));

        // The mark does not let a session's request past the limit: two more in a row read nothing.
        for _ in 0..2 {
            refresh_on_demand(&fixture.state, &Scope::default(), fixture.shell.clone()).await;
        }
        fixture.settled().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (3, 1));

        // The next poll tick reads the account again, nothing but the mark having made it due, and
        // kicks it.
        refresh_with(
            &fixture.state,
            &Scope::default(),
            Trigger::Poll,
            fixture.shell.clone(),
        )
        .await;
        fixture.settled().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (5, 2));
        assert_eq!(fixture.session_reset(), Some(1_792_261_723_000));
    }

    /// Within a minute of the last attempt the held reading stands, whoever took it.
    #[tokio::test]
    async fn a_refresh_on_demand_within_a_minute_of_an_attempt_keeps_the_reading() {
        let fixture = KickFixture::new("quota-demand-limited", false);
        fixture.refresh().await;
        assert_eq!(fixture.count("runs"), 1);

        fixture.age_attempt(Duration::from_secs(59));
        refresh_on_demand(&fixture.state, &Scope::default(), fixture.shell.clone()).await;
        assert_eq!(fixture.count("runs"), 1);

        fixture.age_attempt(Duration::from_secs(61));
        refresh_on_demand(&fixture.state, &Scope::default(), fixture.shell.clone()).await;
        assert_eq!(fixture.count("runs"), 2);
    }

    #[tokio::test]
    async fn a_window_with_no_reset_time_is_left_alone_while_the_setting_is_off() {
        let fixture = KickFixture::new("quota-kick-off", false);
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (1, 0));
        assert_eq!(fixture.session_reset(), None);
    }

    /// A reading is done once `refresh_with` returns: the kick it calls for goes on without it, so
    /// the readings of other accounts are not held up behind a model round trip.
    #[tokio::test]
    async fn a_refresh_returns_without_waiting_for_the_kick_it_starts() {
        let fixture = KickFixture::new("quota-kick-detached", true);
        std::fs::write(fixture.bin.join("kick-sleeps"), "").unwrap();
        fixture.state.account_quotas.make_due(Agent::Codex, None);
        refresh_with(
            &fixture.state,
            &Scope::default(),
            Trigger::Poll,
            fixture.shell.clone(),
        )
        .await;
        assert_eq!(fixture.session_reset(), None);
        assert!(!fixture.state.account_quotas.lock().in_flight.is_empty());
        std::fs::remove_file(fixture.bin.join("kick-sleeps")).unwrap();
        fixture.settled().await;
        assert_eq!(fixture.session_reset(), Some(1_792_261_723_000));
    }

    #[tokio::test]
    async fn a_window_with_no_reset_time_is_kicked_once_and_the_account_read_again() {
        let fixture = KickFixture::new("quota-kick-once", true);
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
        assert_eq!(fixture.session_reset(), Some(1_792_261_723_000));
        let kick = std::fs::read_to_string(fixture.bin.join("kicks")).unwrap();
        assert!(kick.starts_with("exec --ephemeral "), "{kick}");
    }

    #[tokio::test]
    async fn a_window_still_with_no_reset_time_after_a_kick_is_not_kicked_again() {
        let fixture = KickFixture::new("quota-kick-still-null", true);
        fixture.after_kick(&codex_reply(false));
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
        fixture.refresh().await;
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (4, 1));
        assert_eq!(fixture.session_reset(), None);
    }

    #[tokio::test]
    async fn a_failed_kick_is_not_retried_within_its_episode() {
        let fixture = KickFixture::new("quota-kick-fails", true);
        std::fs::write(fixture.bin.join("kick-fails"), "").unwrap();
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (3, 1));
    }

    #[tokio::test]
    async fn a_window_is_kicked_again_once_a_reading_has_shown_its_reset_time() {
        let fixture = KickFixture::new("quota-kick-again", true);
        fixture.after_kick(&codex_reply(false));
        fixture.refresh().await;
        assert_eq!(fixture.count("kicks"), 1);

        // A reading with the reset time ends the episode, so the same window going unstarted again
        // is a new one.
        fixture.serve(&codex_reply(true));
        fixture.refresh().await;
        assert_eq!(fixture.count("kicks"), 1);
        fixture.serve(&codex_reply(false));
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (5, 2));
    }

    /// A disabled account, the default one included, has no window started on it, and one that is
    /// turned back on is kicked again.
    #[tokio::test]
    async fn a_disabled_account_has_no_window_kicked() {
        let fixture = KickFixture::new("quota-kick-disabled", true);
        fixture
            .state
            .store
            .set_default_account_disabled(Agent::Codex, true)
            .unwrap();
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (1, 0));
        assert!(!fixture
            .state
            .account_quotas
            .is_kicked_for_test(Agent::Codex, None));

        fixture
            .state
            .store
            .set_default_account_disabled(Agent::Codex, false)
            .unwrap();
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (3, 1));
    }

    #[tokio::test]
    async fn turning_the_setting_on_kicks_the_readings_already_held() {
        let fixture = KickFixture::new("quota-kick-held", false);
        fixture.refresh().await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (1, 0));

        kick_held_with(&fixture.state, fixture.shell.clone()).await;
        assert_eq!(fixture.count("kicks"), 0);

        fixture
            .state
            .store
            .set_auto_start_quota_windows(true)
            .unwrap();
        kick_held_with(&fixture.state, fixture.shell.clone()).await;
        assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
        assert_eq!(fixture.session_reset(), Some(1_792_261_723_000));
    }

    /// Holds the default Codex account's unstarted window, claims its kick and runs
    /// [`kick_and_read`] on it in the background, with `kick-sleeps` keeping the kick going, until
    /// the fake CLI has been started for the kick.
    async fn kick_in_progress(fixture: &KickFixture) -> tokio::task::JoinHandle<()> {
        let key = key(Agent::Codex, None);
        let unstarted = AccountQuota {
            windows: vec![window(QuotaWindowKind::Session, 0.0, None)],
            ..quota_of(&key, false)
        };
        let quotas = &fixture.state.account_quotas;
        quotas.recorded(&fixture.state, key.clone(), unstarted);
        std::fs::write(fixture.bin.join("kick-sleeps"), "").unwrap();
        let claim = quotas.claim_kick(&fixture.state, &key).expect("a kick");
        let (state, shell) = (fixture.state.clone(), fixture.shell.clone());
        let running = tokio::spawn(async move { kick_and_read(&state, &shell, claim).await });
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while fixture.count("kicks") == 0 {
            assert!(Instant::now() < deadline, "the kick did not start");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        running
    }

    /// A kick whose account was made due (a login ended) while it ran has no claim any more, so
    /// the reading after it, which would be discarded, is not taken.
    #[tokio::test]
    async fn a_kick_whose_claim_was_dropped_is_not_followed_by_a_reading() {
        let fixture = KickFixture::new("quota-kick-dropped", true);
        let running = kick_in_progress(&fixture).await;
        fixture.state.account_quotas.make_due(Agent::Codex, None);
        std::fs::remove_file(fixture.bin.join("kick-sleeps")).unwrap();
        running.await.unwrap();
        assert_eq!(fixture.count("runs"), 0);
    }

    /// The daemon shutting down stops the CLI of a kick at once and takes no reading after it.
    #[tokio::test]
    async fn a_shutdown_stops_a_running_kick_and_the_reading_after_it() {
        let fixture = KickFixture::new("quota-kick-shutdown", true);
        let running = kick_in_progress(&fixture).await;
        fixture.state.account_quotas.stop_cli_runs();
        // The fake CLI is still told to sleep: only the cancel can end the kick.
        tokio::time::timeout(crate::test_support::PATIENCE, running)
            .await
            .expect("the kick was not stopped")
            .unwrap();
        assert_eq!(fixture.count("runs"), 0);
    }

    /// The daemon shutting down stops the CLI of a probe at once and records no reading for it.
    #[tokio::test]
    async fn a_shutdown_stops_a_running_probe_and_records_nothing() {
        let fixture = KickFixture::new("quota-probe-shutdown", false);
        // The fake CLI never answers while this exists: only the cancel can end the probe.
        std::fs::write(fixture.bin.join("probe-sleeps"), "").unwrap();
        let claim = fixture
            .state
            .account_quotas
            .claim_due(
                vec![key(Agent::Codex, None)],
                Instant::now() + POLL_INTERVAL,
            )
            .pop()
            .expect("a claim");
        let (state, shell) = (fixture.state.clone(), fixture.shell.clone());
        let running = tokio::spawn(async move { read_account(&state, &shell, claim, true).await });
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while fixture.count("runs") == 0 {
            assert!(Instant::now() < deadline, "the probe did not start");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        fixture.state.account_quotas.stop_cli_runs();
        let next = tokio::time::timeout(crate::test_support::PATIENCE, running)
            .await
            .expect("the probe was not stopped")
            .unwrap();
        assert!(next.is_none());
        assert!(fixture.state.account_quotas.lock().entries.is_empty());
    }

    /// A kick claimed or started once the daemon is shutting down is refused, so its CLI is never
    /// spawned, and no window counts as kicked.
    #[tokio::test]
    async fn no_kick_is_started_once_the_daemon_is_shutting_down() {
        let fixture = KickFixture::new("quota-kick-refused", true);
        let key = key(Agent::Codex, None);
        let unstarted = AccountQuota {
            windows: vec![window(QuotaWindowKind::Session, 0.0, None)],
            ..quota_of(&key, false)
        };
        let quotas = &fixture.state.account_quotas;
        quotas.recorded(&fixture.state, key.clone(), unstarted);
        quotas.stop_cli_runs();
        assert!(quotas.claim_kick(&fixture.state, &key).is_none());
        let shell_asked = AtomicBool::new(false);
        let shell = || {
            shell_asked.store(true, Ordering::Relaxed);
            (*fixture.shell)()
        };
        let err = start_windows(&key, &shell, &AtomicBool::new(true)).unwrap_err();
        assert!(err.to_string().contains("before it started"), "{err}");
        assert!(!shell_asked.load(Ordering::Relaxed));
        assert_eq!(fixture.count("kicks"), 0);
        // Nothing was counted as kicked, so the window is claimed once the flag is clear again.
        quotas.stopping.store(false, Ordering::Relaxed);
        assert!(quotas.claim_kick(&fixture.state, &key).is_some());
    }

    /// A kick would make the CLI recreate an account's directory the user deleted, so it is not
    /// run; the reading after it is what records the directory as missing.
    #[test]
    fn an_account_whose_directory_is_gone_is_not_kicked() {
        let fixture = KickFixture::new("quota-kick-dir-missing", true);
        let gone = fixture.bin.join("deleted-account");
        let key = Key::of(Agent::Codex, Some("a"), gone.to_str());
        let err = start_windows(&key, &*fixture.shell, &AtomicBool::new(false)).unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
        assert_eq!(fixture.count("kicks"), 0);
        assert!(matches!(
            read(&key, &*fixture.shell, &AtomicBool::new(false)).unavailable,
            Some(unavailable) if unavailable.code == code::CONFIG_DIR_MISSING
        ));
    }

    /// An account that is being read, whose reading failed or holds no windows, or that is gone
    /// is not kicked, and its episodes go with it.
    #[test]
    fn only_a_successful_reading_of_an_idle_account_with_an_unstarted_window_is_kicked() {
        let (state, _dir) = crate::test_support::app_state("quota-kick-claims");
        state.store.set_auto_start_quota_windows(true).unwrap();
        let quotas = AccountQuotas::default();
        let key = key(Agent::Codex, Some("a"));
        let unstarted = AccountQuota {
            windows: vec![window(QuotaWindowKind::Session, 0.0, None)],
            ..quota_of(&key, false)
        };

        assert!(quotas.claim_kick(&state, &key).is_none());
        quotas.recorded(&state, key.clone(), quota_of(&key, true));
        assert!(quotas.claim_kick(&state, &key).is_none());
        quotas.recorded(&state, key.clone(), quota_of(&key, false));
        assert!(quotas.claim_kick(&state, &key).is_none());

        quotas.recorded(&state, key.clone(), unstarted.clone());
        let running = quotas.claim_due(vec![key.clone()], Instant::now() + POLL_INTERVAL);
        assert!(quotas.claim_kick(&state, &key).is_none());
        drop(running);
        let claim = quotas.claim_kick(&state, &key).expect("a kick");
        assert!(quotas.claim_kick(&state, &key).is_none());
        drop(claim);
        assert!(quotas.claim_kick(&state, &key).is_none());

        quotas.forget_account(&state, "a");
        quotas.recorded(&state, key.clone(), unstarted);
        assert!(quotas.claim_kick(&state, &key).is_some());
    }

    /// A kick goes to one cheap model, so it cannot start the window of another model.
    #[test]
    fn a_models_own_weekly_window_with_no_reset_time_is_not_kicked() {
        let (state, _dir) = crate::test_support::app_state("quota-kick-weekly-model");
        state.store.set_auto_start_quota_windows(true).unwrap();
        let quotas = AccountQuotas::default();
        let key = key(Agent::Claude, None);
        let mut model = window(QuotaWindowKind::WeeklyModel, 0.0, None);
        model.label = Some("Fable".to_string());
        let mut reading = quota_of(&key, false);
        reading.windows = vec![model, window(QuotaWindowKind::Weekly, 40.0, Some(NOW))];
        quotas.recorded(&state, key.clone(), reading.clone());
        assert!(quotas.claim_kick(&state, &key).is_none());

        reading.windows[1] = window(QuotaWindowKind::Session, 0.0, None);
        quotas.recorded(&state, key.clone(), reading);
        assert!(quotas.claim_kick(&state, &key).is_some());
    }

    const NOW: i64 = 1_000_000;

    fn window(kind: QuotaWindowKind, used_percent: f64, resets_at: Option<i64>) -> QuotaWindow {
        QuotaWindow {
            kind,
            label: None,
            used_percent,
            resets_at,
            window_minutes: None,
        }
    }

    /// The readings of one Codex account `a` read under `/accounts/a`, holding `windows`.
    fn readings(windows: Vec<QuotaWindow>) -> HashMap<Key, Entry> {
        let key = key(Agent::Codex, Some("a"));
        let mut quota = quota_of(&key, false);
        quota.windows = windows;
        HashMap::from([(
            key,
            Entry {
                quota,
                attempted: Instant::now(),
                attempted_at: NOW,
                failures: 0,
                due: false,
            },
        )])
    }

    fn exhausted_of(windows: Vec<QuotaWindow>) -> Option<Exhausted> {
        exhausted_in(
            &readings(windows),
            Agent::Codex,
            Some("a"),
            Some("/accounts/a"),
            NOW,
        )
    }

    #[test]
    fn a_used_up_window_that_has_not_reset_keeps_the_account_from_opening_a_session() {
        let blocked = exhausted_of(vec![window(QuotaWindowKind::Session, 100.0, Some(NOW + 1))]);
        assert_eq!(
            blocked,
            Some(Exhausted {
                window: "5-hour".to_string(),
                kind: QuotaWindowKind::Session,
                window_minutes: None,
                has_model_weekly: false,
                resets_at: Some(NOW + 1)
            })
        );
        assert!(
            exhausted_of(vec![window(QuotaWindowKind::Weekly, 130.0, Some(NOW + 1))]).is_some()
        );
        assert!(
            exhausted_of(vec![window(QuotaWindowKind::Session, 99.9, Some(NOW + 1))]).is_none()
        );
    }

    #[test]
    fn a_window_that_has_reset_is_stale_and_one_with_no_reset_time_still_counts() {
        assert!(exhausted_of(vec![window(QuotaWindowKind::Session, 100.0, Some(NOW))]).is_none());
        assert!(
            exhausted_of(vec![window(QuotaWindowKind::Session, 100.0, Some(NOW - 1))]).is_none()
        );
        let blocked = exhausted_of(vec![window(QuotaWindowKind::Weekly, 100.0, None)]);
        assert_eq!(blocked.map(|b| b.resets_at), Some(None));
    }

    #[test]
    fn a_model_window_never_counts_and_a_credit_pool_does() {
        let model = QuotaWindow {
            label: Some("Fable".to_string()),
            ..window(QuotaWindowKind::WeeklyModel, 100.0, Some(NOW + 1))
        };
        assert!(exhausted_of(vec![model]).is_none());
        let pool = QuotaWindow {
            window_minutes: Some(30 * 24 * 60),
            ..window(QuotaWindowKind::CreditPeriod, 100.0, Some(NOW + 1))
        };
        assert!(exhausted_of(vec![pool]).is_some());
    }

    #[test]
    fn the_window_that_resets_last_is_the_one_reported() {
        let blocked = exhausted_of(vec![
            window(QuotaWindowKind::Session, 100.0, Some(NOW + 10)),
            window(QuotaWindowKind::Weekly, 100.0, Some(NOW + 20)),
            window(QuotaWindowKind::CreditPeriod, 50.0, Some(NOW + 30)),
        ]);
        assert_eq!(
            blocked,
            Some(Exhausted {
                window: "Weekly".to_string(),
                kind: QuotaWindowKind::Weekly,
                window_minutes: None,
                has_model_weekly: false,
                resets_at: Some(NOW + 20)
            })
        );
        // No reset time counts as last, whatever the order.
        for windows in [
            vec![
                window(QuotaWindowKind::Session, 100.0, None),
                window(QuotaWindowKind::Weekly, 100.0, Some(NOW + 20)),
            ],
            vec![
                window(QuotaWindowKind::Weekly, 100.0, Some(NOW + 20)),
                window(QuotaWindowKind::Session, 100.0, None),
            ],
        ] {
            assert_eq!(
                exhausted_of(windows),
                Some(Exhausted {
                    window: "5-hour".to_string(),
                    kind: QuotaWindowKind::Session,
                    window_minutes: None,
                    has_model_weekly: false,
                    resets_at: None
                })
            );
        }
    }

    #[test]
    fn an_unknown_quota_or_one_of_another_directory_passes() {
        let held = readings(vec![window(QuotaWindowKind::Session, 100.0, None)]);
        let asked = |agent, account, dir| exhausted_in(&held, agent, account, dir, NOW);
        assert!(asked(Agent::Codex, Some("a"), Some("/accounts/a")).is_some());
        // No reading for the account, and one taken under another directory.
        assert!(asked(Agent::Codex, Some("b"), Some("/accounts/b")).is_none());
        assert!(asked(Agent::Codex, Some("a"), Some("/accounts/other")).is_none());
        // A reading without windows.
        assert!(exhausted_of(Vec::new()).is_none());
    }

    #[test]
    fn windows_kept_over_a_failed_re_read_still_count() {
        let mut held = readings(vec![window(QuotaWindowKind::Session, 100.0, Some(NOW + 1))]);
        for entry in held.values_mut() {
            entry.quota.unavailable = Some(QuotaUnavailable::new(code::TIMED_OUT, "no", &[]));
        }
        let asked = exhausted_in(&held, Agent::Codex, Some("a"), Some("/accounts/a"), NOW);
        assert!(asked.is_some());
    }

    fn credit(minutes: u32, used_percent: f64, resets_at: Option<i64>) -> QuotaWindow {
        QuotaWindow {
            window_minutes: Some(minutes),
            ..window(QuotaWindowKind::CreditPeriod, used_percent, resets_at)
        }
    }

    /// The soonest reset is spent first; only a tie looks at how much is left.
    #[test]
    fn the_soonest_reset_beats_a_larger_remainder_and_an_equal_reset_prefers_more_remaining() {
        let soonest = expiring_weekly(
            &[
                window(QuotaWindowKind::Weekly, 10.0, Some(NOW + 100)),
                window(QuotaWindowKind::Weekly, 80.0, Some(NOW + 10)),
            ],
            NOW,
        );
        assert_eq!(
            soonest,
            Some(ExpiringWeekly {
                resets_at: NOW + 10,
                remaining: 20.0
            })
        );
        let fuller = expiring_weekly(
            &[
                window(QuotaWindowKind::Weekly, 50.0, Some(NOW + 10)),
                window(QuotaWindowKind::Weekly, 10.0, Some(NOW + 10)),
            ],
            NOW,
        );
        assert_eq!(
            fuller,
            Some(ExpiringWeekly {
                resets_at: NOW + 10,
                remaining: 90.0
            })
        );
    }

    /// A model window, a session window and a monthly credit pool are not the account's weekly
    /// quota, even when they reset sooner than one that is.
    #[test]
    fn a_window_that_is_not_an_account_wide_weekly_one_does_not_rank() {
        let model = QuotaWindow {
            label: Some("Fable".to_string()),
            ..window(QuotaWindowKind::WeeklyModel, 10.0, Some(NOW + 1))
        };
        let session = window(QuotaWindowKind::Session, 10.0, Some(NOW + 1));
        let month = credit(30 * 24 * 60, 10.0, Some(NOW + 1));
        assert!(expiring_weekly(std::slice::from_ref(&model), NOW).is_none());
        assert!(expiring_weekly(std::slice::from_ref(&session), NOW).is_none());
        assert!(expiring_weekly(std::slice::from_ref(&month), NOW).is_none());
        assert!(expiring_weekly(&[], NOW).is_none());
        for minutes in [28 * 24 * 60, 31 * 24 * 60] {
            assert!(
                expiring_weekly(&[credit(minutes, 10.0, Some(NOW + 1))], NOW).is_none(),
                "{minutes}"
            );
        }
        let ranked = expiring_weekly(
            &[
                model,
                session,
                month,
                window(QuotaWindowKind::Weekly, 40.0, Some(NOW + 50)),
            ],
            NOW,
        );
        assert_eq!(
            ranked,
            Some(ExpiringWeekly {
                resets_at: NOW + 50,
                remaining: 60.0
            })
        );
    }

    #[test]
    fn a_used_up_non_finite_or_passed_weekly_window_does_not_rank() {
        let ignored = [
            window(QuotaWindowKind::Weekly, 100.0, Some(NOW + 1)),
            window(QuotaWindowKind::Weekly, 130.0, Some(NOW + 1)),
            window(QuotaWindowKind::Weekly, f64::NAN, Some(NOW + 1)),
            window(QuotaWindowKind::Weekly, f64::INFINITY, Some(NOW + 1)),
            window(QuotaWindowKind::Weekly, f64::NEG_INFINITY, Some(NOW + 1)),
            window(QuotaWindowKind::Weekly, 10.0, Some(NOW)),
            window(QuotaWindowKind::Weekly, 10.0, Some(NOW - 1)),
            window(QuotaWindowKind::Weekly, 10.0, None),
        ];
        for window in ignored {
            assert!(expiring_weekly(&[window], NOW).is_none());
        }
        let ranked = expiring_weekly(
            &[
                window(QuotaWindowKind::Weekly, 100.0, Some(NOW + 1)),
                window(QuotaWindowKind::Weekly, f64::NAN, Some(NOW + 1)),
                window(QuotaWindowKind::Weekly, 10.0, Some(NOW)),
                window(QuotaWindowKind::Weekly, 10.0, None),
                window(QuotaWindowKind::Weekly, 25.0, Some(NOW + 20)),
            ],
            NOW,
        );
        assert_eq!(
            ranked,
            Some(ExpiringWeekly {
                resets_at: NOW + 20,
                remaining: 75.0
            })
        );
    }

    #[test]
    fn a_credit_period_inside_the_week_range_ranks() {
        let week = credit(7 * 24 * 60, 25.0, Some(NOW + 5));
        assert_eq!(
            expiring_weekly(&[week], NOW),
            Some(ExpiringWeekly {
                resets_at: NOW + 5,
                remaining: 75.0
            })
        );
        for minutes in [6 * 24 * 60, 8 * 24 * 60] {
            assert!(
                expiring_weekly(&[credit(minutes, 10.0, Some(NOW + 1))], NOW).is_some(),
                "{minutes}"
            );
        }
        for minutes in [6 * 24 * 60 - 1, 8 * 24 * 60 + 1] {
            assert!(
                expiring_weekly(&[credit(minutes, 10.0, Some(NOW + 1))], NOW).is_none(),
                "{minutes}"
            );
        }
    }

    #[test]
    fn credit_pools_are_named_after_their_period() {
        let named = |minutes| {
            window_name(&QuotaWindow {
                window_minutes: minutes,
                ..window(QuotaWindowKind::CreditPeriod, 100.0, None)
            })
        };
        assert_eq!(named(Some(7 * 24 * 60)), "Weekly");
        assert_eq!(named(Some(30 * 24 * 60)), "Monthly");
        assert_eq!(named(Some(10 * 24 * 60)), "Credits");
        assert_eq!(named(None), "Credits");
    }

    #[test]
    fn the_session_window_is_named_as_each_agents_own_interface_names_it() {
        let exhausted = |kind| {
            exhausted_of(vec![window(kind, 100.0, Some(NOW + 1))]).expect("a used-up window")
        };
        let session = exhausted(QuotaWindowKind::Session);
        assert_eq!(session.window, "5-hour");
        assert_eq!(session.named_for(Agent::Claude), "session");
        assert_eq!(session.named_for(Agent::Codex), "5h");
        assert_eq!(
            exhausted(QuotaWindowKind::Weekly).named_for(Agent::Claude),
            "Weekly"
        );
    }

    #[test]
    fn a_weekly_window_beside_a_models_own_is_for_all_models_in_claude_codes_words() {
        let blocked = exhausted_of(vec![
            window(QuotaWindowKind::Weekly, 100.0, Some(NOW + 1)),
            window(QuotaWindowKind::WeeklyModel, 10.0, Some(NOW + 1)),
        ])
        .expect("a used-up window");
        assert!(blocked.has_model_weekly);
        assert_eq!(blocked.kind, QuotaWindowKind::Weekly);
        assert_eq!(blocked.named_for(Agent::Claude), "Weekly · all models");
        assert_eq!(blocked.named_for(Agent::Codex), "Weekly");
    }
}
