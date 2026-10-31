use super::*;

use crate::protocol::{
    quota_unavailable_code as code, AccountQuota, Agent, Event, QuotaUnavailable, QuotaWindow,
    QuotaWindowKind,
};

pub(super) fn key(agent: Agent, account: Option<&str>) -> Key {
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
    let key = key(Agent::Codex, None);
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
