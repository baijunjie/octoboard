use super::*;
use std::collections::HashMap;
use std::time::Instant;

use crate::accounts::quotas::tests::{key, quota_of, window, NOW};
use crate::protocol::{
    quota_unavailable_code as code, AgentId, QuotaUnavailable, QuotaWindow, QuotaWindowKind,
};

/// The readings of one Codex account `a` read under `/accounts/a`, holding `windows`.
fn readings(windows: Vec<QuotaWindow>) -> HashMap<Key, Entry> {
    let key = key(AgentId::CODEX, Some("a"));
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
        AgentId::CODEX,
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
    assert!(exhausted_of(vec![window(QuotaWindowKind::Weekly, 130.0, Some(NOW + 1))]).is_some());
    assert!(exhausted_of(vec![window(QuotaWindowKind::Session, 99.9, Some(NOW + 1))]).is_none());
}

#[test]
fn a_window_that_has_reset_is_stale_and_one_with_no_reset_time_still_counts() {
    assert!(exhausted_of(vec![window(QuotaWindowKind::Session, 100.0, Some(NOW))]).is_none());
    assert!(exhausted_of(vec![window(QuotaWindowKind::Session, 100.0, Some(NOW - 1))]).is_none());
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
    assert!(asked(AgentId::CODEX, Some("a"), Some("/accounts/a")).is_some());
    // No reading for the account, and one taken under another directory.
    assert!(asked(AgentId::CODEX, Some("b"), Some("/accounts/b")).is_none());
    assert!(asked(AgentId::CODEX, Some("a"), Some("/accounts/other")).is_none());
    // A reading without windows.
    assert!(exhausted_of(Vec::new()).is_none());
}

#[test]
fn windows_kept_over_a_failed_re_read_still_count() {
    let mut held = readings(vec![window(QuotaWindowKind::Session, 100.0, Some(NOW + 1))]);
    for entry in held.values_mut() {
        entry.quota.unavailable = Some(QuotaUnavailable::new(code::TIMED_OUT, "no", &[]));
    }
    let asked = exhausted_in(&held, AgentId::CODEX, Some("a"), Some("/accounts/a"), NOW);
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

const HOUR: i64 = 60 * 60 * 1000;

fn signal(weekly_in: i64, session_in: Option<i64>) -> AccountSignal {
    let mut windows = vec![window(QuotaWindowKind::Weekly, 50.0, Some(NOW + weekly_in))];
    windows.extend(session_in.map(|at| window(QuotaWindowKind::Session, 10.0, Some(NOW + at))));
    AccountSignal::of(&windows, NOW).unwrap()
}

/// Weekly resets less than a session window apart are tied, and a running session window then
/// goes first; a weekly reset a session window or more later is not in the tie.
#[test]
fn a_running_session_window_decides_between_weekly_resets_within_five_hours() {
    // A later weekly reset with a session window running beats a sooner one without.
    assert_eq!(
        preferred_signal(&[signal(24 * HOUR, None), signal(26 * HOUR, Some(3 * HOUR))]),
        Some(1)
    );
    // Far apart weekly resets are not tied: the sooner one wins whatever the session says.
    assert_eq!(
        preferred_signal(&[
            signal(4 * HOUR, None),
            signal(6 * 24 * HOUR, Some(3 * HOUR))
        ]),
        Some(0)
    );
    // Exactly one session window after the soonest is outside the tie; a minute less is in.
    assert_eq!(
        preferred_signal(&[signal(24 * HOUR, None), signal(29 * HOUR, Some(HOUR))]),
        Some(0)
    );
    assert_eq!(
        preferred_signal(&[
            signal(24 * HOUR, None),
            signal(29 * HOUR - 60_000, Some(HOUR))
        ]),
        Some(1)
    );
    // The tie is measured against the soonest, not chained: the third is 7 hours out.
    assert_eq!(
        preferred_signal(&[
            signal(24 * HOUR, None),
            signal(27 * HOUR, None),
            signal(31 * HOUR, Some(HOUR))
        ]),
        Some(0)
    );
}

/// Among the tied, the sooner session reset, then the sooner weekly reset, then the larger
/// remainder decide, and an exact tie keeps the earlier candidate.
#[test]
fn tied_accounts_are_ranked_by_session_then_weekly_reset_then_remainder() {
    assert_eq!(
        preferred_signal(&[
            signal(24 * HOUR, Some(3 * HOUR)),
            signal(25 * HOUR, Some(2 * HOUR))
        ]),
        Some(1)
    );
    assert_eq!(
        preferred_signal(&[signal(25 * HOUR, None), signal(24 * HOUR, None)]),
        Some(1)
    );
    assert_eq!(
        preferred_signal(&[signal(24 * HOUR, None), signal(24 * HOUR, None)]),
        Some(0)
    );
    let fuller = AccountSignal {
        weekly: ExpiringWeekly {
            remaining: 90.0,
            ..signal(24 * HOUR, None).weekly
        },
        ..signal(24 * HOUR, None)
    };
    assert_eq!(
        preferred_signal(&[signal(24 * HOUR, None), fuller]),
        Some(1)
    );
    assert_eq!(preferred_signal(&[]), None);
}

/// A session window counts as running only with quota left and a reset still ahead.
#[test]
fn a_session_window_that_is_used_up_stale_or_without_a_reset_is_not_running() {
    for session in [
        window(QuotaWindowKind::Session, 100.0, Some(NOW + HOUR)),
        window(QuotaWindowKind::Session, 10.0, Some(NOW - 1)),
        window(QuotaWindowKind::Session, 10.0, None),
    ] {
        let windows = [
            window(QuotaWindowKind::Weekly, 50.0, Some(NOW + HOUR)),
            session,
        ];
        assert_eq!(
            AccountSignal::of(&windows, NOW).unwrap().session_resets_at,
            None
        );
    }
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
    let exhausted =
        |kind| exhausted_of(vec![window(kind, 100.0, Some(NOW + 1))]).expect("a used-up window");
    let session = exhausted(QuotaWindowKind::Session);
    assert_eq!(session.window, "5-hour");
    assert_eq!(session.named_for(AgentId::CLAUDE), "session");
    assert_eq!(session.named_for(AgentId::CODEX), "5h");
    assert_eq!(session.named_for(AgentId::GROK), "5h");
    assert_eq!(
        exhausted(QuotaWindowKind::Weekly).named_for(AgentId::CLAUDE),
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
    assert_eq!(blocked.named_for(AgentId::CLAUDE), "Weekly · all models");
    assert_eq!(blocked.named_for(AgentId::CODEX), "Weekly");
}
