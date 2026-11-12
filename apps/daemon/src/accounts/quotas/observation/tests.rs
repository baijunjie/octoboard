use super::*;

use crate::accounts::quotas::tests::window;
use crate::protocol::QuotaWindowKind;

const T0: i64 = 1_800_000_000_000;
/// A 5-hour window's length, as Codex's placeholder reset follows it.
const SESSION_MS: i64 = 5 * 60 * 60 * 1000;
const RESET: i64 = T0 + SESSION_MS;

type Tracked = Vec<(WindowId, Observation)>;

/// What a session window reporting `resets_at` at `now` is left with after being judged against
/// `tracked`.
fn judged(tracked: &mut Tracked, resets_at: i64, now: i64) -> Option<i64> {
    let mut windows = [window(QuotaWindowKind::Session, 0.0, Some(resets_at))];
    judge(tracked, &mut windows, now);
    windows[0].resets_at
}

/// `tracked` after one session window reporting `RESET` was seen at `T0`.
fn seen_first() -> Tracked {
    let mut tracked = Tracked::new();
    assert_eq!(judged(&mut tracked, RESET, T0), Some(RESET));
    tracked
}

#[test]
fn a_first_observation_is_counting_and_keeps_its_reset() {
    let tracked = seen_first();
    assert_eq!(tracked.len(), 1);
    assert_eq!(tracked[0].1.resets_at, RESET);
}

#[test]
fn a_window_with_no_reset_time_is_left_alone_and_not_tracked() {
    let mut tracked = seen_first();
    let mut windows = [window(QuotaWindowKind::Session, 0.0, None)];
    judge(&mut tracked, &mut windows, T0 + MIN_OBSERVATION);
    assert_eq!(windows[0].resets_at, None);
    assert!(tracked.is_empty());
}

/// Codex's placeholder reset moves with each reading, a started window's stands still: the first
/// is held with no reset time while the raw reset stays tracked, and once it stands still, as after
/// a kick has started the window, the window counts and shows its reset.
#[test]
fn a_reset_that_follows_the_clock_is_not_counting_until_it_stands_still() {
    let mut tracked = seen_first();
    let slid = RESET + MIN_OBSERVATION;
    assert_eq!(judged(&mut tracked, slid, T0 + MIN_OBSERVATION), None);
    assert_eq!(tracked[0].1.resets_at, slid);

    // The reading after a kick: the reset no longer moves.
    assert_eq!(
        judged(&mut tracked, slid, T0 + 2 * MIN_OBSERVATION),
        Some(slid)
    );
}

#[test]
fn a_fixed_reset_stays_counting_and_keeps_its_reset() {
    let mut tracked = seen_first();
    assert_eq!(
        judged(&mut tracked, RESET, T0 + MIN_OBSERVATION),
        Some(RESET)
    );
    assert_eq!(
        judged(&mut tracked, RESET, T0 + 2 * MIN_OBSERVATION),
        Some(RESET)
    );
}

/// Before [`MIN_OBSERVATION`] a counting window stays counting though its reset moved as the
/// clock does, and the baseline stays the first reading, so a reading [`MIN_OBSERVATION`] after it
/// is judged against that one.
#[test]
fn a_reading_sooner_than_the_minimum_keeps_a_counting_verdict_and_the_baseline() {
    let mut tracked = seen_first();
    let early = MIN_OBSERVATION - 1;
    assert_eq!(
        judged(&mut tracked, RESET + early, T0 + early),
        Some(RESET + early)
    );
    assert_eq!(
        judged(&mut tracked, RESET + MIN_OBSERVATION, T0 + MIN_OBSERVATION),
        None
    );
}

/// The same for a window that is not counting: a reset that stands still sooner than
/// [`MIN_OBSERVATION`] after the baseline does not make it count.
#[test]
fn a_reading_sooner_than_the_minimum_keeps_a_not_counting_verdict_and_the_baseline() {
    let mut tracked = seen_first();
    let slid = RESET + MIN_OBSERVATION;
    assert_eq!(judged(&mut tracked, slid, T0 + MIN_OBSERVATION), None);

    let early = T0 + 2 * MIN_OBSERVATION - 1;
    assert_eq!(judged(&mut tracked, slid, early), None);
    assert_eq!(
        judged(&mut tracked, slid, T0 + 2 * MIN_OBSERVATION),
        Some(slid)
    );
}

/// A window that is not counting but shows some use is counting at once, without waiting for
/// [`MIN_OBSERVATION`]: it keeps its reset time. The baseline's reset and time stay as they were, so
/// the next comparison after [`MIN_OBSERVATION`] is made against them and not against the early
/// reading.
#[test]
fn a_used_reading_sooner_than_the_minimum_counts_at_once_and_keeps_the_baseline() {
    let mut tracked = seen_first();
    let slid = RESET + MIN_OBSERVATION;
    assert_eq!(judged(&mut tracked, slid, T0 + MIN_OBSERVATION), None);

    let early = T0 + MIN_OBSERVATION + MIN_OBSERVATION - 1;
    let mut windows = [window(QuotaWindowKind::Session, 3.0, Some(slid))];
    judge(&mut tracked, &mut windows, early);
    assert_eq!(windows[0].resets_at, Some(slid));
    assert_eq!(tracked[0].1.resets_at, slid);
    assert_eq!(tracked[0].1.at, T0 + MIN_OBSERVATION);

    // Against the baseline at `T0 + MIN_OBSERVATION` the reset moved by the whole time passed, so
    // nothing is counting; against the early reading the time passed would be too short to judge
    // and the verdict from the use would stand.
    assert_eq!(
        judged(
            &mut tracked,
            slid + MIN_OBSERVATION,
            T0 + 2 * MIN_OBSERVATION
        ),
        None
    );
}

/// A reset that moved by the time passed less the slack is still taken for one following the
/// clock; one more millisecond of difference and it is a counting window's.
#[test]
fn the_slack_decides_at_its_boundary_whether_a_reset_moved_with_the_clock() {
    let cases = [
        ("exactly elapsed less slack", 0, None),
        (
            "1 ms short of that",
            -1,
            Some(RESET + MIN_OBSERVATION - OBSERVATION_SLACK - 1),
        ),
    ];
    for (name, extra, expected) in cases {
        let mut tracked = seen_first();
        let moved = MIN_OBSERVATION - OBSERVATION_SLACK + extra;
        assert_eq!(
            judged(&mut tracked, RESET + moved, T0 + MIN_OBSERVATION),
            expected,
            "{name}"
        );
    }
}

/// A window no request has started has nothing used, so a reset that follows the clock holds it
/// back only while nothing of it is used: a used window is counting whatever its reset does, and
/// keeps showing it.
#[test]
fn a_reset_that_follows_the_clock_is_not_counting_only_while_nothing_is_used() {
    let cases = [
        ("nothing used", 0.0, None),
        (
            "a tenth of a percent used",
            0.1,
            Some(RESET + MIN_OBSERVATION),
        ),
        ("a few percent used", 17.0, Some(RESET + MIN_OBSERVATION)),
        (
            "no reading of the usage",
            f64::NAN,
            Some(RESET + MIN_OBSERVATION),
        ),
    ];
    for (name, used_percent, expected) in cases {
        let mut tracked = seen_first();
        let mut windows = [window(
            QuotaWindowKind::Session,
            used_percent,
            Some(RESET + MIN_OBSERVATION),
        )];
        judge(&mut tracked, &mut windows, T0 + MIN_OBSERVATION);
        assert_eq!(windows[0].resets_at, expected, "{name}");
        assert_eq!(tracked.len(), 1, "{name}");
        assert_eq!(tracked[0].1.resets_at, RESET + MIN_OBSERVATION, "{name}");
    }
}

/// A baseline whose reset time has passed belongs to a window that ended: the next reading starts
/// afresh as counting, though its reset moved by exactly the time passed.
#[test]
fn a_baseline_whose_reset_has_passed_starts_afresh() {
    let mut tracked = Tracked::new();
    let base_reset = T0 + MIN_OBSERVATION;
    judged(&mut tracked, base_reset, T0);
    let next = base_reset + MIN_OBSERVATION;
    // The baseline's reset is exactly now.
    assert_eq!(judged(&mut tracked, next, base_reset), Some(next));
    assert_eq!(tracked[0].1.at, base_reset);
}

/// A reset further ahead than the time passed accounts for is a new window's: a real reset or a
/// used reset credit. Exactly the time passed and the slack is still a reset that follows the
/// clock.
#[test]
fn a_reset_that_jumped_ahead_of_the_time_passed_starts_afresh() {
    let cases = [
        ("exactly elapsed and slack", 0, None),
        (
            "1 ms more",
            1,
            Some(RESET + MIN_OBSERVATION + OBSERVATION_SLACK + 1),
        ),
    ];
    for (name, extra, expected) in cases {
        let mut tracked = seen_first();
        let moved = MIN_OBSERVATION + OBSERVATION_SLACK + extra;
        assert_eq!(
            judged(&mut tracked, RESET + moved, T0 + MIN_OBSERVATION),
            expected,
            "{name}"
        );
    }
}

/// Windows are told apart by their kind: one following the clock does not make another that
/// stands still not counting, and a window that is gone from a reading is no longer tracked.
#[test]
fn each_window_is_judged_against_its_own_baseline() {
    let week_reset = T0 + 7 * 24 * 60 * 60 * 1000;
    let reading = |session: i64, week: i64| {
        [
            window(QuotaWindowKind::Session, 0.0, Some(session)),
            window(QuotaWindowKind::Weekly, 30.0, Some(week)),
        ]
    };
    let mut tracked = Tracked::new();
    judge(&mut tracked, &mut reading(RESET, week_reset), T0);
    assert_eq!(tracked.len(), 2);

    let mut windows = reading(RESET + MIN_OBSERVATION, week_reset);
    judge(&mut tracked, &mut windows, T0 + MIN_OBSERVATION);
    assert_eq!(windows[0].resets_at, None);
    assert_eq!(windows[1].resets_at, Some(week_reset));

    let mut only_session = [window(QuotaWindowKind::Session, 0.0, Some(RESET))];
    judge(&mut tracked, &mut only_session, T0 + 2 * MIN_OBSERVATION);
    assert_eq!(tracked.len(), 1);
}
