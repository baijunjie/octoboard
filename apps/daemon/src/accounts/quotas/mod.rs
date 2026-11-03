//! How much of each account's plan quota is left, asked of the agent's own CLI. Octoboard never
//! uses an agent's credentials or calls its servers: the CLI, run the way a session of the
//! account would be, handles its own login, Keychain access and token refresh. What is asked and
//! how a reply is read is `probe.rs`'s; this module keeps the readings, and `poll.rs` decides when
//! to take them. Each reading also carries the email the account is signed in with, which
//! `email.rs` takes from the agent's sign-in record whether or not the quota could be read.
//! What the daemon's log is told of the readings (sign-in changes, failures, probes Octoboard
//! ended) is `diagnostics.rs`'s, with redaction of anything the agent printed.
//!
//! Readings are runtime state: held in [`AccountQuotas`], never stored and never exported. A poller
//! started with the daemon keeps them fresh, and `refresh_account_quotas` runs the same path on
//! request. Both read an account whose last attempt is older than
//! [`POLL_INTERVAL`](poll::POLL_INTERVAL), stretched for an account that keeps failing, so a client
//! that asks on every open costs next to nothing; a request also reads some accounts sooner, as set
//! out below, and a session that asks for fresh readings over MCP has an account read again once a
//! minute has passed since its last attempt ([`refresh_on_demand`]). An account is also read once a
//! window it holds has reset since its last attempt, a short grace
//! ([`RESET_GRACE`](poll::RESET_GRACE)) later, so a window that rolled over is not shown as used up
//! until the next interval. A reading that fails over one that had windows keeps those windows,
//! with the reason set beside them.
//!
//! An account whose configuration directory is missing or that is not signed in is a state of the
//! account, not a failure: it is reported as its own
//! [`QuotaUnavailable`](crate::protocol::QuotaUnavailable) code, drops the windows it held, is not
//! backed off, and is read again at once when its login ends ([`AccountQuotas::make_due`]). An
//! account the CLI has no quota to report for is not backed off either, though it is not a state: a
//! reading that had windows keeps them. A login done outside Octoboard leaves no trace to react to,
//! so a request from a client also reads an account that is not signed in or has no quota to
//! report, and holds no windows, once [`MIN_REREAD_INTERVAL`](poll::MIN_REREAD_INTERVAL) has passed
//! since its last attempt. A missing configuration directory is not read early: only a login or the
//! ordinary interval brings it back.
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
//! A reading of a provider that has them also carries the account's rate limit reset credits. Using
//! one (`reset_credit.rs`, `use_credit.rs`) is only ever the user's own request
//! ([`use_reset_credit`]): nothing in the poller, the kicks or the automatic account switch calls
//! it.
//!
//! The module also decides whether a reading means the account is used up
//! ([`AccountQuotas::exhausted`], judged in `judgement.rs`). When the readings are taken is
//! `poll.rs`'s: what makes an account due, the claims on the accounts that are, and the claim for
//! a kick are all decided there, over the registry kept here.

pub(crate) mod diagnostics;
pub(crate) mod email;
mod judgement;
pub(crate) mod kick;
mod poll;
pub(crate) mod probe;
pub(crate) mod reset_credit;
mod use_credit;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::Semaphore;

use crate::protocol::{
    quota_unavailable_code as code, AccountQuota, AgentId, Event, QuotaWindow, QuotaWindowKind,
};
use crate::state::AppState;
#[cfg(test)]
use crate::time::now_millis;
use judgement::{exhausted_in, is_account_state, is_failure};
pub use judgement::{preferred_signal, AccountSignal, Exhausted};
#[cfg(test)]
pub(crate) use poll::finish_probe_for_test;
use poll::MAX_CONCURRENT_CLI_RUNS;
pub use poll::{kick_held, refresh, refresh_requested, spawn, Scope};
pub(crate) use poll::{refresh_on_demand, ShellSource};
pub(crate) use reset_credit::CONSUME_TIMEOUT as RESET_CREDIT_TIMEOUT;
pub(crate) use use_credit::use_reset_credit;

/// What a reading belongs to. The configuration directory is part of it so that an account
/// pointed at another directory never inherits the reading of its old one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    agent: AgentId,
    /// The account's id; `None` for the default account.
    account: Option<String>,
    /// The account's directory; `None` for the default account, which pins nothing.
    config_dir: Option<String>,
}

impl Key {
    fn of(agent: AgentId, account: Option<&str>, config_dir: Option<&str>) -> Self {
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
    /// What the daemon's log has been told of each account's readings, so that only a change is
    /// written (see `diagnostics.rs`).
    observed: HashMap<Key, diagnostics::Observed>,
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
                AgentId::ALL.iter().position(|agent| *agent == quota.agent),
                quota.account.clone(),
            )
        });
        quotas
    }
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
    inner.observed.retain(|key, _| wanted.contains(key));
    inner.entries.len() != before
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
    /// The accounts a reset credit is being used on, so that two requests (another window's, a
    /// second click) never use two credits.
    resetting: Arc<Mutex<HashSet<Key>>>,
}

impl Default for AccountQuotas {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            slots: Arc::new(Semaphore::new(MAX_CONCURRENT_CLI_RUNS)),
            stopping: Arc::default(),
            resetting: Arc::default(),
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
        agent: AgentId,
        account: Option<&str>,
        config_dir: Option<&str>,
        now: i64,
    ) -> Option<Exhausted> {
        exhausted_in(&self.lock().entries, agent, account, config_dir, now)
    }

    /// Recovery requires an actual successful probe after the failed turn, with every
    /// account-wide window below its limit even when an old reset time has passed.
    pub(crate) fn verified_available(
        &self,
        agent: AgentId,
        account: Option<&str>,
        config_dir: Option<&str>,
        started_at: i64,
    ) -> bool {
        let inner = self.lock();
        inner
            .entries
            .get(&Key::of(agent, account, config_dir))
            .is_some_and(|entry| {
                let mut windows = entry
                    .quota
                    .windows
                    .iter()
                    .filter(|window| window.kind != QuotaWindowKind::WeeklyModel)
                    .peekable();
                entry.attempted_at >= started_at
                    && entry.quota.unavailable.is_none()
                    && windows.peek().is_some()
                    && windows.all(|window| {
                        window.used_percent.is_finite() && window.used_percent < 100.0
                    })
            })
    }

    /// The held reading of the account under `config_dir`, if one is held. `None` for
    /// `config_dir` is the default account. A reading taken under another directory is not this
    /// account's.
    pub fn held(
        &self,
        agent: AgentId,
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
        agent: AgentId,
        account: Option<&str>,
        config_dir: Option<&str>,
    ) -> bool {
        self.held(agent, account, config_dir)
            .and_then(|quota| quota.unavailable)
            .is_some_and(|reason| {
                [code::NOT_SIGNED_IN, code::CONFIG_DIR_MISSING].contains(&reason.code)
            })
    }

    /// [`AccountQuotas::held`] and [`AccountQuotas::exhausted`] of the same account under one
    /// lock, so the reading and the judgement made from it cannot come from different moments.
    pub fn held_and_exhausted(
        &self,
        agent: AgentId,
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

    /// Logs what the reading `quota`, taken by the probe `trace` describes under `claim`, changed
    /// in the account's sign-in or in whether it can be read (see `diagnostics.rs`), unless the
    /// claim no longer counts. Written after the lock is released.
    fn log_reading(&self, claim: &Claim, quota: &AccountQuota, trace: &diagnostics::ProbeTrace) {
        let events = {
            let mut inner = self.lock();
            if inner.in_flight.get(&claim.key) != Some(&claim.token) {
                return;
            }
            let observed = inner.observed.entry(claim.key.clone()).or_default();
            diagnostics::advance(observed, &diagnostics::classify(quota))
        };
        diagnostics::log_events(&claim.key, quota, trace, &events);
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

    /// Records the outcome of the probe `claim` covered, unless the account was forgotten while
    /// it ran. A failure over a reading that had windows keeps its windows and plan, with the
    /// reason set beside them and the email of the newer attempt, so a client can show it as
    /// stale; an account that never read keeps none. Only a failure counts towards the backoff:
    /// that the CLI has no quota to report is not one.
    fn finish(&self, state: &AppState, claim: Claim, mut quota: AccountQuota) -> bool {
        let mut recorded = false;
        self.change(state, |inner| {
            if inner.in_flight.get(&claim.key) != Some(&claim.token) {
                return false;
            }
            inner.in_flight.remove(&claim.key);
            recorded = true;
            if quota.unavailable.is_none() {
                end_episodes(inner, &claim.key, &quota.windows);
            }
            let previous = inner.entries.get(&claim.key);
            let keeps_windows = quota
                .unavailable
                .as_ref()
                .filter(|u| !is_account_state(u.code));
            let failures = if keeps_windows.is_some_and(|u| is_failure(u.code)) {
                previous.map_or(0, |entry| entry.failures).saturating_add(1)
            } else {
                0
            };
            if let (Some(unavailable), Some(previous)) = (keeps_windows, previous) {
                if !previous.quota.windows.is_empty() {
                    quota = AccountQuota {
                        unavailable: Some(unavailable.clone()),
                        email: quota.email,
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
        recorded
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
    pub fn is_kicked_for_test(&self, agent: AgentId, account: Option<&str>) -> bool {
        self.lock()
            .kicked
            .keys()
            .any(|key| key.agent == agent && key.account.as_deref() == account)
    }

    /// Whether the held reading of the account is marked due.
    #[cfg(test)]
    pub fn is_due_for_test(&self, agent: AgentId, account: Option<&str>) -> bool {
        self.lock().entries.iter().any(|(key, entry)| {
            key.agent == agent && key.account.as_deref() == account && entry.due
        })
    }

    /// Makes the account's reading due and drops any claim on it, so that the next refresh reads it
    /// at once and a probe begun before a login ended cannot put its not-signed-in reading back
    /// over the new one. A kick running at the time is not stopped, so the account can be read
    /// while it still is. `account` is an account id, `None` for the default account.
    pub fn make_due(&self, agent: AgentId, account: Option<&str>) {
        let mut inner = self.lock();
        let is_it = |key: &Key| key.agent == agent && key.account.as_deref() == account;
        inner.in_flight.retain(|key, _| !is_it(key));
        for (_, entry) in inner.entries.iter_mut().filter(|(key, _)| is_it(key)) {
            entry.due = true;
        }
    }

    /// Stops the probes and kicks running and keeps each kick from going on to the reading after
    /// it. A probe or kick would otherwise outlive a shutdown by up to
    /// [`PROBE_TIMEOUT`](probe::PROBE_TIMEOUT) or [`KICK_TIMEOUT`](kick::KICK_TIMEOUT), its CLI
    /// with it.
    pub fn stop_cli_runs(&self) {
        self.stopping.store(true, Ordering::Relaxed);
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
            inner
                .observed
                .retain(|key, _| key.account.as_deref() != Some(id));
            inner.entries.len() != before
        });
    }
}

#[cfg(test)]
mod tests;
