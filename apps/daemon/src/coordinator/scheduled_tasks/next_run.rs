//! When a scheduled task next runs: a pure function of its schedule, a reference instant and the
//! time zone the schedule is read in (the daemon machine's own, [`local_zone`]).

use jiff::civil::{Date, Weekday as CivilWeekday};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};

use crate::protocol::{Schedule, TimeOfDay, Weekday};

const MINUTE_MILLIS: i64 = 60_000;

/// The time zone schedules are read in: the local one of the daemon's machine, UTC when it cannot
/// be determined.
pub(crate) fn local_zone() -> TimeZone {
    TimeZone::system()
}

/// The IANA name of `zone` (`Europe/Berlin`), as `scheduled_tasks_listed` tells clients the zone
/// schedules are read in; `None` for a zone that has none, such as one given as a POSIX rule.
pub(crate) fn zone_name(zone: &TimeZone) -> Option<String> {
    zone.iana_name().map(str::to_owned)
}

/// The first instant strictly after `after` (milliseconds since the epoch) at which `schedule` is
/// due when read in `zone`, or `None` when it never is again: a manual-only schedule, or a one-off
/// whose time is not after `after`.
///
/// A wall-clock time that a change to daylight saving time skips is run at the same distance past
/// the change (02:30 on a night that jumps from 02:00 to 03:00 runs at 03:30); one that it repeats
/// is run at its first occurrence. A monthly schedule in a month without its day runs on that month's last
/// day. An hourly schedule follows the clock's minute through the
/// change, so it runs in every real hour, the repeated one included.
pub(crate) fn next_run(schedule: &Schedule, after: i64, zone: &TimeZone) -> Option<i64> {
    match schedule {
        Schedule::Once { run_at } => (*run_at > after).then_some(*run_at),
        Schedule::Manual => None,
        Schedule::Hourly { minute } => next_hourly(*minute, after, zone),
        Schedule::Daily { time } => next_at_time(*time, after, zone, |_| true),
        Schedule::Weekdays { time } => next_at_time(*time, after, zone, |day| {
            !matches!(day, CivilWeekday::Saturday | CivilWeekday::Sunday)
        }),
        Schedule::Weekly { time, days } => next_at_time(*time, after, zone, |day| {
            days.iter().any(|wanted| civil_weekday(*wanted) == day)
        }),
        Schedule::Monthly { day, time } => next_monthly(*day, *time, after, zone),
    }
}

/// The first whole minute after `after` whose wall-clock minute in `zone` is `minute`. Looked for
/// minute by minute over two hours, which holds an hour of each offset a zone may move to.
fn next_hourly(minute: u8, after: i64, zone: &TimeZone) -> Option<i64> {
    let first = (after.div_euclid(MINUTE_MILLIS) + 1) * MINUTE_MILLIS;
    (0..120)
        .map(|step| first + step * MINUTE_MILLIS)
        .find(|candidate| {
            Timestamp::from_millisecond(*candidate)
                .is_ok_and(|instant| zone.to_datetime(instant).minute() == minute as i8)
        })
}

/// The first instant after `after` that is `time` on a day `wanted` takes, reading days and times
/// in `zone`. Eight days on from the day `after` falls on cover every weekday whatever the time.
fn next_at_time(
    time: TimeOfDay,
    after: i64,
    zone: &TimeZone,
    wanted: impl Fn(CivilWeekday) -> bool,
) -> Option<i64> {
    let start: Date = zone
        .to_datetime(Timestamp::from_millisecond(after).ok()?)
        .date();
    (0..=8).find_map(|offset| {
        let day = start.checked_add(offset.days()).ok()?;
        if !wanted(day.weekday()) {
            return None;
        }
        let at = day.at(time.hour as i8, time.minute as i8, 0, 0);
        let instant = zone
            .to_ambiguous_zoned(at)
            .compatible()
            .ok()?
            .timestamp()
            .as_millisecond();
        (instant > after).then_some(instant)
    })
}

/// The first instant after `after` that is `time` on day `day` of a month in `zone`, or on that month's last day
/// where it is shorter. This month and the next two cover it: the first may be past, the second always is ahead.
fn next_monthly(day: u8, time: TimeOfDay, after: i64, zone: &TimeZone) -> Option<i64> {
    let start: Date = zone
        .to_datetime(Timestamp::from_millisecond(after).ok()?)
        .date();
    let first_of_month = start.first_of_month();
    (0..=2).find_map(|offset| {
        let month = first_of_month.checked_add(offset.months()).ok()?;
        let date = month
            .with()
            .day(day.min(month.days_in_month() as u8) as i8)
            .build()
            .ok()?;
        let at = date.at(time.hour as i8, time.minute as i8, 0, 0);
        let instant = zone
            .to_ambiguous_zoned(at)
            .compatible()
            .ok()?
            .timestamp()
            .as_millisecond();
        (instant > after).then_some(instant)
    })
}

fn civil_weekday(day: Weekday) -> CivilWeekday {
    match day {
        Weekday::Monday => CivilWeekday::Monday,
        Weekday::Tuesday => CivilWeekday::Tuesday,
        Weekday::Wednesday => CivilWeekday::Wednesday,
        Weekday::Thursday => CivilWeekday::Thursday,
        Weekday::Friday => CivilWeekday::Friday,
        Weekday::Saturday => CivilWeekday::Saturday,
        Weekday::Sunday => CivilWeekday::Sunday,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zone with daylight saving time, given by its rule so the test needs no time zone database:
    /// UTC-5, and UTC-4 from 02:00 on the second Sunday of March to 02:00 on the first Sunday of
    /// November.
    fn new_york() -> TimeZone {
        TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").expect("rule")
    }

    /// Milliseconds of a wall-clock time in `zone` (its first occurrence).
    fn at(zone: &TimeZone, text: &str) -> i64 {
        let civil: jiff::civil::DateTime = text.parse().expect("civil time");
        zone.to_ambiguous_zoned(civil)
            .earlier()
            .expect("in range")
            .timestamp()
            .as_millisecond()
    }

    fn time(hour: u8, minute: u8) -> TimeOfDay {
        TimeOfDay { hour, minute }
    }

    /// One case per schedule kind, read in a fixed zone: 2026-11-18 is a Wednesday.
    #[test]
    fn each_schedule_kind_is_next_due_where_its_rule_says() {
        let zone = TimeZone::fixed(jiff::tz::offset(9));
        let now = at(&zone, "2026-11-18T10:20:00");
        let cases = [
            (
                Schedule::Once {
                    run_at: at(&zone, "2026-11-20T08:00"),
                },
                Some("2026-11-20T08:00"),
            ),
            (
                Schedule::Once {
                    run_at: at(&zone, "2026-11-18T10:20"),
                },
                None,
            ),
            (Schedule::Manual, None),
            (
                Schedule::Monthly {
                    day: 18,
                    time: time(18, 0),
                },
                Some("2026-11-18T18:00"),
            ),
            (
                Schedule::Monthly {
                    day: 18,
                    time: time(10, 0),
                },
                Some("2026-12-18T10:00"),
            ),
            (Schedule::Hourly { minute: 45 }, Some("2026-11-18T10:45")),
            (Schedule::Hourly { minute: 5 }, Some("2026-11-18T11:05")),
            (Schedule::Hourly { minute: 20 }, Some("2026-11-18T11:20")),
            (
                Schedule::Daily { time: time(18, 0) },
                Some("2026-11-18T18:00"),
            ),
            (
                Schedule::Daily { time: time(9, 0) },
                Some("2026-11-19T09:00"),
            ),
            (
                Schedule::Weekdays { time: time(9, 0) },
                Some("2026-11-19T09:00"),
            ),
            (
                Schedule::Weekly {
                    time: time(10, 20),
                    days: vec![Weekday::Wednesday],
                },
                Some("2026-11-25T10:20"),
            ),
            (
                Schedule::Weekly {
                    time: time(7, 0),
                    days: vec![Weekday::Sunday, Weekday::Monday],
                },
                Some("2026-11-22T07:00"),
            ),
            (
                Schedule::Weekly {
                    time: time(7, 0),
                    days: vec![],
                },
                None,
            ),
        ];
        for (schedule, expected) in cases {
            assert_eq!(
                next_run(&schedule, now, &zone),
                expected.map(|text| at(&zone, text)),
                "{schedule:?}"
            );
        }
    }

    /// Weekdays skip the weekend: after Friday's run comes Monday's.
    #[test]
    fn weekdays_skip_saturday_and_sunday() {
        let zone = TimeZone::UTC;
        let friday_run = at(&zone, "2026-11-20T09:00");
        assert_eq!(
            next_run(&Schedule::Weekdays { time: time(9, 0) }, friday_run, &zone),
            Some(at(&zone, "2026-11-23T09:00"))
        );
    }

    /// A daily time the spring change skips runs as far past the change; one the autumn change
    /// repeats runs once, at its first occurrence.
    #[test]
    fn a_daily_time_follows_the_daylight_saving_changes() {
        let zone = new_york();
        let daily = Schedule::Daily { time: time(2, 30) };
        // 2026-03-08 02:00 EST jumps to 03:00 EDT, so 02:30 does not exist: 03:30 EDT is 07:30 UTC.
        assert_eq!(
            next_run(&daily, at(&zone, "2026-03-08T00:00"), &zone),
            Some(at(&TimeZone::UTC, "2026-03-08T07:30"))
        );
        // 2026-11-01 02:00 EDT falls back to 01:00 EST: 01:30 happens twice, run at the first.
        let early = Schedule::Daily { time: time(1, 30) };
        let first = at(&TimeZone::UTC, "2026-11-01T05:30");
        assert_eq!(
            next_run(&early, at(&zone, "2026-11-01T00:00"), &zone),
            Some(first)
        );
        assert_eq!(
            next_run(&early, first, &zone),
            Some(at(&zone, "2026-11-02T01:30"))
        );
    }

    /// An hourly schedule runs in every real hour across the changes: the repeated hour of the
    /// autumn change runs twice, and the spring change's skipped hour is not waited out.
    #[test]
    fn an_hourly_schedule_runs_every_real_hour_across_the_changes() {
        let zone = new_york();
        let hourly = Schedule::Hourly { minute: 15 };
        let utc = |text| at(&TimeZone::UTC, text);
        // Autumn: 01:15 EDT (05:15 UTC), then 01:15 EST (06:15 UTC), then 02:15 EST.
        assert_eq!(
            next_run(&hourly, utc("2026-11-01T05:00"), &zone),
            Some(utc("2026-11-01T05:15"))
        );
        assert_eq!(
            next_run(&hourly, utc("2026-11-01T05:15"), &zone),
            Some(utc("2026-11-01T06:15"))
        );
        assert_eq!(
            next_run(&hourly, utc("2026-11-01T06:15"), &zone),
            Some(utc("2026-11-01T07:15"))
        );
        // Spring: 01:15 EST (06:15 UTC), then 03:15 EDT (07:15 UTC).
        assert_eq!(
            next_run(&hourly, utc("2026-03-08T06:15"), &zone),
            Some(utc("2026-03-08T07:15"))
        );
    }

    /// An instant inside a minute still finds that hour's minute when it is later.
    #[test]
    fn a_reference_inside_a_minute_is_strictly_before_the_next_run() {
        let zone = TimeZone::UTC;
        let now = at(&zone, "2026-11-18T10:14:59.500");
        assert_eq!(
            next_run(&Schedule::Hourly { minute: 15 }, now, &zone),
            Some(at(&zone, "2026-11-18T10:15"))
        );
        assert_eq!(
            next_run(&Schedule::Daily { time: time(10, 15) }, now, &zone),
            Some(at(&zone, "2026-11-18T10:15"))
        );
    }

    /// The rules that cross a day, a week or a year: a time still ahead today runs today, one past
    /// runs on the next listed day, the weekend is passed over, and the new year is crossed.
    #[test]
    fn schedules_roll_over_days_weeks_and_years_as_their_rules_say() {
        let zone = TimeZone::UTC;
        // 2026-11-20 is a Friday, 2026-11-21 a Saturday, 2026-12-31 a Thursday.
        let friday_morning = at(&zone, "2026-11-20T10:00");
        let saturday_noon = at(&zone, "2026-11-21T12:00");
        let new_years_eve = at(&zone, "2026-12-31T23:30");
        let weekly = |hour, days: Vec<Weekday>| Schedule::Weekly {
            time: time(hour, 0),
            days,
        };
        let cases = [
            (
                "weekdays, later the same Friday",
                Schedule::Weekdays { time: time(18, 0) },
                friday_morning,
                "2026-11-20T18:00",
            ),
            (
                "weekdays, from a Saturday",
                Schedule::Weekdays { time: time(9, 0) },
                saturday_noon,
                "2026-11-23T09:00",
            ),
            (
                "weekly, later the same day",
                weekly(13, vec![Weekday::Monday, Weekday::Saturday]),
                saturday_noon,
                "2026-11-21T13:00",
            ),
            (
                "weekly, the time passed so the next listed day",
                weekly(11, vec![Weekday::Monday, Weekday::Saturday]),
                saturday_noon,
                "2026-11-23T11:00",
            ),
            (
                "weekly, a single day a week ahead",
                weekly(9, vec![Weekday::Friday]),
                friday_morning,
                "2026-11-27T09:00",
            ),
            (
                "daily across the new year",
                Schedule::Daily { time: time(0, 10) },
                new_years_eve,
                "2027-01-01T00:10",
            ),
            (
                "hourly across the new year",
                Schedule::Hourly { minute: 0 },
                new_years_eve,
                "2027-01-01T00:00",
            ),
        ];
        for (name, schedule, after, expected) in cases {
            assert_eq!(
                next_run(&schedule, after, &zone),
                Some(at(&zone, expected)),
                "{name}"
            );
        }
    }

    /// A monthly schedule in a month without its day runs on that month's last day: day 31 is the
    /// last day of every month, 30 and 29 give way in February, and 29 holds in a leap year.
    #[test]
    fn a_monthly_schedule_falls_back_to_the_last_day_of_a_short_month() {
        let zone = TimeZone::UTC;
        let monthly = |day| Schedule::Monthly {
            day,
            time: time(9, 0),
        };
        let cases = [
            (
                "31 from Jan 31 after its time",
                31,
                "2027-01-31T09:00",
                "2027-02-28T09:00",
            ),
            ("31 from Feb 28", 31, "2027-02-28T09:00", "2027-03-31T09:00"),
            ("31 from Mar 31", 31, "2027-03-31T09:00", "2027-04-30T09:00"),
            (
                "31 from before Jan 31's time",
                31,
                "2027-01-31T08:00",
                "2027-01-31T09:00",
            ),
            ("30 in February", 30, "2027-01-30T09:00", "2027-02-28T09:00"),
            (
                "29 in a common February",
                29,
                "2027-01-29T09:00",
                "2027-02-28T09:00",
            ),
            (
                "29 in a leap February",
                29,
                "2028-01-29T09:00",
                "2028-02-29T09:00",
            ),
            (
                "31 in a leap February",
                31,
                "2028-01-31T09:00",
                "2028-02-29T09:00",
            ),
            (
                "1 from the 1st after its time",
                1,
                "2027-03-01T09:00",
                "2027-04-01T09:00",
            ),
            (
                "15 from mid-month before its time",
                15,
                "2027-03-10T12:00",
                "2027-03-15T09:00",
            ),
            (
                "15 from after its time",
                15,
                "2027-03-20T12:00",
                "2027-04-15T09:00",
            ),
            (
                "15 across the new year",
                15,
                "2026-12-20T12:00",
                "2027-01-15T09:00",
            ),
            (
                "31 across the new year",
                31,
                "2026-12-31T09:00",
                "2027-01-31T09:00",
            ),
            (
                "30 across the new year",
                30,
                "2026-12-31T12:00",
                "2027-01-30T09:00",
            ),
        ];
        for (name, day, after, expected) in cases {
            assert_eq!(
                next_run(&monthly(day), at(&zone, after), &zone),
                Some(at(&zone, expected)),
                "{name}"
            );
        }
    }

    /// A one-off in the future is next due at its own time, and one at or before the reference is
    /// not due again.
    #[test]
    fn a_one_off_is_due_only_at_its_own_time() {
        let zone = TimeZone::UTC;
        let run_at = at(&zone, "2026-11-20T08:00");
        let once = Schedule::Once { run_at };
        assert_eq!(next_run(&once, run_at - 1, &zone), Some(run_at));
        assert_eq!(next_run(&once, run_at, &zone), None);
        assert_eq!(next_run(&once, run_at + 1, &zone), None);
    }

    /// A wall time the spring change skips runs on the day of the change, after the change and
    /// within the hour that follows it, for every schedule that names a time of day.
    #[test]
    fn a_skipped_wall_time_runs_just_after_the_change_on_every_schedule_kind() {
        let zone = new_york();
        // 2026-03-08 is a Sunday; 02:00 EST (07:00 UTC) jumps to 03:00 EDT.
        let change = at(&TimeZone::UTC, "2026-03-08T07:00");
        let before = at(&zone, "2026-03-07T12:00");
        for schedule in [
            Schedule::Daily { time: time(2, 30) },
            Schedule::Weekly {
                time: time(2, 30),
                days: vec![Weekday::Sunday],
            },
            Schedule::Monthly {
                day: 8,
                time: time(2, 30),
            },
        ] {
            let run = next_run(&schedule, before, &zone).expect("a next run");
            assert!(
                run >= change && run < change + 60 * 60 * 1000,
                "{schedule:?} ran at {run}, change at {change}"
            );
        }
    }

    /// A monthly time the autumn change repeats runs once, at its first occurrence, and the last-day
    /// fallback reads the day in the zone, not in UTC.
    #[test]
    fn a_monthly_time_follows_the_zone() {
        let zone = new_york();
        // 2026-11-01 01:30 happens twice; the first is 05:30 UTC.
        let monthly = Schedule::Monthly {
            day: 1,
            time: time(1, 30),
        };
        assert_eq!(
            next_run(&monthly, at(&zone, "2026-10-15T00:00"), &zone),
            Some(at(&TimeZone::UTC, "2026-11-01T05:30"))
        );
        // 2027-01-31 23:30 in New York is already February in UTC; the zone's own month decides.
        let last = Schedule::Monthly {
            day: 31,
            time: time(23, 30),
        };
        assert_eq!(
            next_run(&last, at(&zone, "2027-01-31T23:30"), &zone),
            Some(at(&zone, "2027-02-28T23:30"))
        );
    }

    /// Clients are told a zone by its IANA name, and that it has none when it is only a rule.
    #[test]
    fn a_zone_is_named_by_its_iana_name() {
        assert_eq!(zone_name(&TimeZone::UTC).as_deref(), Some("UTC"));
        assert_eq!(zone_name(&new_york()), None);
    }
}
