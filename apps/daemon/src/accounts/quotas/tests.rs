use super::*;

use crate::protocol::{
    quota_unavailable_code as code, AccountQuota, AgentId, Event, QuotaUnavailable, QuotaWindow,
    QuotaWindowKind,
};

pub(super) fn key(agent: AgentId, account: Option<&str>) -> Key {
    Key {
        agent,
        account: account.map(str::to_string),
        config_dir: account.map(|id| format!("/accounts/{id}")),
    }
}

pub(super) fn quota_of(key: &Key, failed: bool) -> AccountQuota {
    AccountQuota {
        reset_credits: None,
        agent: key.agent,
        account: key.account.clone(),
        fetched_at: 1,
        plan: None,
        email: None,
        windows: Vec::new(),
        unavailable: failed
            .then(|| QuotaUnavailable::new(code::TIMED_OUT, "no", &[("agent", "x")])),
    }
}

#[test]
fn a_failure_keeps_the_last_good_windows_beside_the_reason_and_a_success_clears_it() {
    let (state, _dir) = crate::test_support::app_state("quota-stale");
    let key = key(AgentId::CODEX, None);
    let good = AccountQuota {
        fetched_at: 7,
        plan: Some("plus".to_string()),
        email: Some("old@example.com".to_string()),
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
    // The email is the newer attempt's, whatever the failure kept of the older reading.
    state.account_quotas.recorded(
        &state,
        key.clone(),
        AccountQuota {
            email: Some("new@example.com".to_string()),
            ..quota_of(&key, true)
        },
    );
    let held = &state.account_quotas.list()[0];
    assert_eq!(held.email.as_deref(), Some("new@example.com"));
    assert_eq!(held.windows, good.windows);
    assert_eq!((held.fetched_at, held.plan.as_deref()), (7, Some("plus")));
    assert_eq!(
        held.unavailable.as_ref().map(|u| u.code),
        Some(code::TIMED_OUT)
    );

    state
        .account_quotas
        .recorded(&state, key.clone(), good.clone());
    assert_eq!(state.account_quotas.list()[0], good);
}

#[test]
fn every_change_is_broadcast_whole_and_a_forgotten_account_is_dropped() {
    let (state, _dir) = crate::test_support::app_state("quota-broadcast");
    let mut events = state.subscribe();
    let (default, owned) = (key(AgentId::CLAUDE, None), key(AgentId::CLAUDE, Some("a")));
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
    let (kept, gone) = (key(AgentId::CODEX, None), key(AgentId::CODEX, Some("a")));
    state
        .account_quotas
        .recorded(&state, kept.clone(), quota_of(&kept, false));
    state
        .account_quotas
        .recorded(&state, gone.clone(), quota_of(&gone, false));
    for key in [&kept, &gone] {
        state
            .account_quotas
            .lock()
            .observed
            .insert(key.clone(), diagnostics::Observed::default());
    }
    state.account_quotas.change(&state, |inner| {
        retain_locked(inner, std::slice::from_ref(&kept))
    });
    let held = state.account_quotas.list();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].account, None);
    let observed: Vec<Key> = state
        .account_quotas
        .lock()
        .observed
        .keys()
        .cloned()
        .collect();
    assert_eq!(observed, [kept]);
}

#[test]
fn a_superseded_claim_tells_the_log_nothing() {
    let (state, _dir) = crate::test_support::app_state("quota-log-claim");
    let key = key(AgentId::CODEX, Some("a"));
    let claim = {
        let mut inner = state.account_quotas.lock();
        state
            .account_quotas
            .claim_locked(&mut inner, key.clone(), 1)
    };
    state
        .account_quotas
        .make_due(key.agent, key.account.as_deref());
    let signed_out = AccountQuota {
        unavailable: Some(QuotaUnavailable::new(code::NOT_SIGNED_IN, "no", &[])),
        ..quota_of(&key, false)
    };
    state
        .account_quotas
        .log_reading(&claim, &signed_out, &diagnostics::ProbeTrace::default());
    assert!(state.account_quotas.lock().observed.is_empty());
}

#[test]
fn forgetting_an_account_forgets_what_the_log_was_told_of_it() {
    let (state, _dir) = crate::test_support::app_state("quota-forget-observed");
    let (kept, gone) = (key(AgentId::CODEX, None), key(AgentId::CODEX, Some("a")));
    for key in [&kept, &gone] {
        state
            .account_quotas
            .lock()
            .observed
            .insert(key.clone(), diagnostics::Observed::default());
    }
    state.account_quotas.forget_account(&state, "a");
    let observed: Vec<Key> = state
        .account_quotas
        .lock()
        .observed
        .keys()
        .cloned()
        .collect();
    assert_eq!(observed, [kept]);
}

/// A successful reading of `key` finished at `now` (milliseconds since the epoch), holding one
/// session window that resets at `resets_at`.
pub(super) fn finish_session_reading(
    state: &AppState,
    key: &Key,
    resets_at: Option<i64>,
    now: i64,
) {
    let quota = AccountQuota {
        windows: vec![window(QuotaWindowKind::Session, 0.0, resets_at)],
        ..quota_of(key, false)
    };
    finish_reading(state, key, quota, now);
}

/// [`AccountQuotas::finish_at`] on a fresh claim of `key`, which must be recorded.
pub(super) fn finish_reading(state: &AppState, key: &Key, quota: AccountQuota, now: i64) {
    let quotas = &state.account_quotas;
    let claim = {
        let mut inner = quotas.lock();
        quotas.claim_locked(&mut inner, key.clone(), now)
    };
    assert!(quotas.finish_at(state, claim, quota, now));
}

/// The reset time the held session window of the only account has.
pub(super) fn held_session_reset(state: &AppState) -> Option<i64> {
    state.account_quotas.list()[0].windows[0].resets_at
}

/// A session window's length, as the 5-hour window Codex holds.
pub(super) const SESSION_MS: i64 = 5 * 60 * 60 * 1000;
/// When the readings of the observation tests start.
pub(super) const START: i64 = 1_800_000_000_000;

/// A reset that follows the clock, as Codex gives for a window no request has started, is held
/// with no reset time; once the reset stands still, as after a kick has started the window, the
/// reading shows it.
#[test]
fn a_reset_that_follows_the_clock_is_held_without_a_reset_time_until_it_stands_still() {
    let (state, _dir) = crate::test_support::app_state("quota-observe-slide");
    let key = key(AgentId::CODEX, None);
    let reset = START + SESSION_MS;
    finish_session_reading(&state, &key, Some(reset), START);
    assert_eq!(held_session_reset(&state), Some(reset));

    let slid = reset + observation::MIN_OBSERVATION;
    finish_session_reading(
        &state,
        &key,
        Some(slid),
        START + observation::MIN_OBSERVATION,
    );
    assert_eq!(held_session_reset(&state), None);

    finish_session_reading(
        &state,
        &key,
        Some(slid),
        START + 2 * observation::MIN_OBSERVATION,
    );
    assert_eq!(held_session_reset(&state), Some(slid));
}

/// A failed reading carries no windows to judge and takes none of the tracking away: the next
/// good reading is still judged against the last good one.
#[test]
fn a_failed_reading_leaves_what_was_tracked_alone() {
    let (state, _dir) = crate::test_support::app_state("quota-observe-failed");
    let key = key(AgentId::CODEX, None);
    let reset = START + SESSION_MS;
    finish_session_reading(&state, &key, Some(reset), START);
    finish_reading(
        &state,
        &key,
        quota_of(&key, true),
        START + observation::MIN_OBSERVATION,
    );

    let two_minutes_on = 2 * observation::MIN_OBSERVATION;
    finish_session_reading(
        &state,
        &key,
        Some(reset + two_minutes_on),
        START + two_minutes_on,
    );
    assert_eq!(held_session_reset(&state), None);
}

/// A good reading with no windows leaves nothing tracked for the account.
#[test]
fn a_reading_without_windows_clears_what_was_tracked() {
    let (state, _dir) = crate::test_support::app_state("quota-observe-empty");
    let key = key(AgentId::CODEX, None);
    finish_session_reading(&state, &key, Some(START + SESSION_MS), START);
    assert!(state.account_quotas.lock().resets_seen.contains_key(&key));

    finish_reading(
        &state,
        &key,
        quota_of(&key, false),
        START + observation::MIN_OBSERVATION,
    );
    assert!(!state.account_quotas.lock().resets_seen.contains_key(&key));
}

/// A forgotten account is observed from scratch: the first reading after it counts, whatever the
/// reset's movement since the last one before.
#[test]
fn forgetting_an_account_forgets_what_was_tracked_of_it() {
    let (state, _dir) = crate::test_support::app_state("quota-observe-forget");
    let (kept, gone) = (key(AgentId::CODEX, None), key(AgentId::CODEX, Some("a")));
    let reset = START + SESSION_MS;
    for key in [&kept, &gone] {
        finish_session_reading(&state, key, Some(reset), START);
    }
    state.account_quotas.forget_account(&state, "a");
    let tracked: Vec<Key> = state
        .account_quotas
        .lock()
        .resets_seen
        .keys()
        .cloned()
        .collect();
    assert_eq!(tracked, [kept]);

    let slid = reset + observation::MIN_OBSERVATION;
    finish_session_reading(
        &state,
        &gone,
        Some(slid),
        START + observation::MIN_OBSERVATION,
    );
    let held = state.account_quotas.list();
    let gone_window = &held
        .iter()
        .find(|q| q.account.as_deref() == Some("a"))
        .unwrap()
        .windows[0];
    assert_eq!(gone_window.resets_at, Some(slid));
}

/// Accounts no longer targeted lose what was tracked of them with their readings.
#[test]
fn accounts_no_longer_targeted_lose_what_was_tracked_of_them() {
    let (state, _dir) = crate::test_support::app_state("quota-observe-retain");
    let (kept, gone) = (key(AgentId::CODEX, None), key(AgentId::CODEX, Some("a")));
    for key in [&kept, &gone] {
        finish_session_reading(&state, key, Some(START + SESSION_MS), START);
    }
    state.account_quotas.change(&state, |inner| {
        retain_locked(inner, std::slice::from_ref(&kept))
    });
    let tracked: Vec<Key> = state
        .account_quotas
        .lock()
        .resets_seen
        .keys()
        .cloned()
        .collect();
    assert_eq!(tracked, [kept]);
}

pub(super) const NOW: i64 = 1_000_000;

pub(super) fn window(
    kind: QuotaWindowKind,
    used_percent: f64,
    resets_at: Option<i64>,
) -> QuotaWindow {
    QuotaWindow {
        kind,
        label: None,
        used_percent,
        resets_at,
        window_minutes: None,
    }
}
