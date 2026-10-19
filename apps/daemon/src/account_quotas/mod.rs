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
//! to nothing; a request also reads some accounts sooner, as set out below. A reading that fails
//! over one that had windows keeps those windows, with the reason set beside them.
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

mod probe;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::protocol::{
    now_millis, quota_unavailable_code as code, AccountQuota, Agent, Availability, Event,
    QuotaUnavailable,
};
use crate::state::AppState;
pub(crate) use probe::CODEX_APP_SERVER_ARGS;
use probe::{config_dir_missing, probe, PROBE_TIMEOUT};

/// How old an account's last attempt must be before it is read again.
const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// How old the last attempt on an account holding no windows because it is not signed in or has no
/// quota to report must be for a client's request to read it before it is due. It may have
/// been signed in outside Octoboard since; this bounds how often a client opening a panel can
/// start its CLI.
const MIN_REREAD_INTERVAL: Duration = Duration::from_secs(30);

/// How often the poller looks for accounts that are due. Much shorter than [`POLL_INTERVAL`] so
/// that an account created, or an agent found available, after the poller's start is read within
/// moments rather than after a whole interval; only a due account costs a subprocess.
const POLL_TICK: Duration = Duration::from_secs(15);

/// How many consecutive failures keep stretching the wait before an account is tried again: the
/// wait doubles with each, so at most `POLL_INTERVAL * 2^3`.
const MAX_BACKOFF_DOUBLINGS: u32 = 3;

/// How many CLIs run at once across the whole daemon, a poll and any refresh requests together.
/// Each is a whole agent process.
const MAX_CONCURRENT_PROBES: usize = 4;

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

struct Entry {
    quota: AccountQuota,
    attempted: Instant,
    /// Consecutive attempts that failed to read the account.
    failures: u32,
    /// Read again at the next refresh whatever the wait: set by [`AccountQuotas::make_due`].
    due: bool,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<Key, Entry>,
    /// The claim (by its token) of each key whose probe is running, so two requests arriving
    /// together run one.
    in_flight: HashMap<Key, u64>,
    next_token: u64,
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

/// The right to probe one account, held for as long as its probe runs and released on drop, so a
/// probe that panics or is abandoned never leaves its account excluded from every later one.
/// [`AccountQuotas::finish`] consumes it. A claim whose account was forgotten or repointed in the
/// meantime no longer counts, and whatever it finishes with is discarded.
struct Claim {
    key: Key,
    token: u64,
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
    /// Bounds the probes running at once, whichever refresh started them.
    slots: Arc<Semaphore>,
}

impl Default for AccountQuotas {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            slots: Arc::new(Semaphore::new(MAX_CONCURRENT_PROBES)),
        }
    }
}

impl AccountQuotas {
    /// Every reading held, by agent and then account with the default account first.
    pub fn list(&self) -> Vec<AccountQuota> {
        self.lock().list()
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

    fn claim_locked(&self, inner: &mut Inner, key: Key) -> Claim {
        inner.next_token += 1;
        inner.in_flight.insert(key.clone(), inner.next_token);
        Claim {
            key,
            token: inner.next_token,
            inner: self.inner.clone(),
        }
    }

    /// A claim on each of `selected` that is due at `now` and not already running. Due is: never
    /// read, made due by [`AccountQuotas::make_due`], or last attempted longer ago than
    /// [`wait_after`] its consecutive failures.
    #[cfg(test)]
    fn claim_due(&self, selected: Vec<Key>, now: Instant) -> Vec<Claim> {
        self.claim_due_locked(&mut self.lock(), selected, now, Trigger::Poll)
    }

    /// [`AccountQuotas::claim_due`] as a client's request would, which also claims an account
    /// holding no windows because it is not signed in or has no quota to report once [`MIN_REREAD_INTERVAL`] has passed.
    #[cfg(test)]
    fn claim_requested(&self, selected: Vec<Key>, now: Instant) -> Vec<Claim> {
        self.claim_due_locked(&mut self.lock(), selected, now, Trigger::Request)
    }

    fn claim_due_locked(
        &self,
        inner: &mut Inner,
        selected: Vec<Key>,
        now: Instant,
        trigger: Trigger,
    ) -> Vec<Claim> {
        let mut claims = Vec::new();
        for key in selected {
            if inner.in_flight.contains_key(&key) {
                continue;
            }
            let is_due = inner.entries.get(&key).is_none_or(|entry| {
                let since = now.saturating_duration_since(entry.attempted);
                entry.due
                    || since >= wait_after(entry.failures)
                    || (trigger == Trigger::Request
                        && since >= MIN_REREAD_INTERVAL
                        && may_have_signed_in_since(&entry.quota))
            });
            if is_due {
                claims.push(self.claim_locked(inner, key));
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
                    failures,
                    due: false,
                },
            );
            true
        });
    }

    /// A probe's whole claim-and-finish for `key`, with `quota` as its outcome.
    #[cfg(test)]
    fn recorded(&self, state: &AppState, key: Key, quota: AccountQuota) {
        let claim = {
            let mut inner = self.lock();
            self.claim_locked(&mut inner, key)
        };
        self.finish(state, claim, quota);
    }

    /// Puts `quota` in as a reading of its own account, as a finished probe would.
    #[cfg(test)]
    pub fn hold_for_test(&self, quota: &AccountQuota) {
        let key = Key {
            agent: quota.agent,
            account: quota.account.clone(),
            config_dir: None,
        };
        self.lock().entries.insert(
            key,
            Entry {
                quota: quota.clone(),
                attempted: Instant::now(),
                failures: 0,
                due: false,
            },
        );
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
            .collect();
        Ok(self.claim_due_locked(&mut inner, selected, now, trigger))
    }

    /// Makes the account's reading due and drops any probe running for it, so that the next
    /// refresh reads it at once and a probe begun before a login ended cannot put its not-signed-in
    /// reading back over the new one. `account` is an account id, `None` for the default account.
    pub fn make_due(&self, agent: Agent, account: Option<&str>) {
        let mut inner = self.lock();
        let is_it = |key: &Key| key.agent == agent && key.account.as_deref() == account;
        inner.in_flight.retain(|key, _| !is_it(key));
        for (_, entry) in inner.entries.iter_mut().filter(|(key, _)| is_it(key)) {
            entry.due = true;
        }
    }

    /// Drops what is held for the account `id`, and any probe running for it, so that a probe
    /// begun under the old directory cannot put its reading back.
    pub fn forget_account(&self, state: &AppState, id: &str) {
        self.change(state, |inner| {
            let before = inner.entries.len();
            inner
                .entries
                .retain(|key, _| key.account.as_deref() != Some(id));
            inner
                .in_flight
                .retain(|key, _| key.account.as_deref() != Some(id));
            inner.entries.len() != before
        });
    }
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

/// Drops every reading, and every probe in flight, whose key is not in `wanted`; true when a
/// reading went.
fn retain_locked(inner: &mut Inner, wanted: &[Key]) -> bool {
    let before = inner.entries.len();
    inner.entries.retain(|key, _| wanted.contains(key));
    inner.in_flight.retain(|key, _| wanted.contains(key));
    inner.entries.len() != before
}

/// How long after an attempt an account is read again, given how many in a row ended unavailable.
fn wait_after(failures: u32) -> Duration {
    POLL_INTERVAL * 2u32.pow(failures.min(MAX_BACKOFF_DOUBLINGS))
}

/// What started a refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    /// The poller, or a login that ended.
    Poll,
    /// A client asked, as it does on opening a panel that shows quotas.
    Request,
}

/// Which accounts a refresh covers; the default is all of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    pub agent: Option<Agent>,
    pub account: Option<String>,
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
        keys.push(Key {
            agent,
            account: None,
            config_dir: None,
        });
        keys.extend(
            accounts
                .iter()
                .filter(|account| account.agent == agent)
                .map(|account| Key {
                    agent,
                    account: Some(account.id.clone()),
                    config_dir: Some(account.config_dir.clone()),
                }),
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

/// Where a probe gets the shell environment it runs in.
type ShellSource = Arc<dyn Fn() -> anyhow::Result<HashMap<String, String>> + Send + Sync>;

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

/// Reads the accounts of `scope` that are due for `trigger`, as many at a time as the daemon-wide
/// limit allows, and broadcasts after each. Also drops the readings of accounts no longer
/// targeted. Returns when every reading it started is done.
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
        let slots = state.account_quotas.slots.clone();
        readings.spawn(async move {
            let _slot = slots.acquire_owned().await;
            let key = claim.key.clone();
            let outcome = tokio::task::spawn_blocking(move || read(&key, &*shell)).await;
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
            state.account_quotas.finish(&state, claim, quota);
        });
    }
    while readings.join_next().await.is_some() {}
}

/// Reads one account's quota in the shell environment `shell` gives. Blocking: it runs a
/// subprocess.
fn read(
    key: &Key,
    shell: &(dyn Fn() -> anyhow::Result<HashMap<String, String>> + Send + Sync),
) -> AccountQuota {
    // Looked for before anything is run: Claude Code and Grok Build would create the directory,
    // empty, and then call the account not signed in, and a poll would keep bringing back one a
    // user deleted. Only `NotFound` says it is gone: a directory that cannot be looked at, or a
    // regular file in its place, is left to the CLI to report. The default account pins nothing,
    // so Codex's own refusal of a missing home is what tells that case (see `probe`).
    if let Some(dir) = key.config_dir.as_deref().filter(|dir| {
        matches!(std::fs::metadata(dir), Err(err) if err.kind() == std::io::ErrorKind::NotFound)
    }) {
        return unavailable_quota(key, config_dir_missing(key.agent, dir));
    }
    let result = match shell() {
        Ok(shell_env) => probe(
            key.agent,
            &shell_env,
            key.config_dir.as_deref().map(Path::new),
            PROBE_TIMEOUT,
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
    use crate::protocol::{QuotaWindow, QuotaWindowKind};

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
            approval_reviewer: None,
            approval_mode: None,
        };
        state.store.insert_account(&account).unwrap();
        let scope = Scope {
            agent: Some(Agent::Codex),
            account: Some("work".to_string()),
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
        let quota = read(&key, &shell);
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
            approval_reviewer: None,
            approval_mode: None,
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
}
