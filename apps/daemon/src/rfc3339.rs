//! Reading the timestamps the agents write, and writing the compact UTC stamp the daemon names
//! backups with. The daemon has no date library and needs nothing else of one.

/// Milliseconds since the epoch of an RFC 3339 timestamp (`2026-10-24T11:59:59.899528+00:00`),
/// the form Claude Code and Grok Build write.
pub fn parse_rfc3339_millis(text: &str) -> Option<i64> {
    let (date, time) = text.split_once(['T', 't'])?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;
    if date.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let zone_at = time.find(['Z', 'z', '+', '-'])?;
    let (clock, zone) = time.split_at(zone_at);
    let (clock, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut clock = clock.split(':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let second: i64 = clock.next()?.parse().ok()?;
    if clock.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    if !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let millis: i64 = format!("{fraction:0<3}")[..3].parse().ok()?;
    let offset_seconds = match zone.strip_prefix(['+', '-']) {
        None if zone.eq_ignore_ascii_case("z") => 0,
        None => return None,
        Some(offset) => {
            let (hours, minutes) = offset.split_once(':')?;
            let seconds = hours.parse::<i64>().ok()? * 3600 + minutes.parse::<i64>().ok()? * 60;
            if zone.starts_with('-') {
                -seconds
            } else {
                seconds
            }
        }
    };
    let days = days_from_civil(year, month, day);
    let seconds = ((days * 24 + hour) * 60 + minute) * 60 + second - offset_seconds;
    Some(seconds * 1000 + millis)
}

/// Days from 1970-01-01 to the given proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// `seconds` since the epoch as a compact UTC stamp, `20261020T153012Z`: sortable, and free of
/// the characters a file name cannot hold.
pub fn compact_utc_stamp(seconds: i64) -> String {
    let (days, time) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        time / 3_600,
        time % 3_600 / 60,
        time % 60
    )
}

/// The proleptic Gregorian date `days` after 1970-01-01, the inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compact_stamp_reads_back_as_the_same_moment() {
        assert_eq!(compact_utc_stamp(1_792_843_199), "20261024T115959Z");
        assert_eq!(compact_utc_stamp(951_782_400), "20000229T000000Z");
        assert_eq!(compact_utc_stamp(0), "19700101T000000Z");
    }

    #[test]
    fn timestamps_parse_to_milliseconds_with_their_offset_applied() {
        assert_eq!(
            parse_rfc3339_millis("2026-10-24T11:59:59.899528+00:00"),
            Some(1_792_843_199_899)
        );
        assert_eq!(
            parse_rfc3339_millis("2026-10-24T12:00:00Z"),
            Some(1_792_843_200_000)
        );
        assert_eq!(
            parse_rfc3339_millis("2026-10-24T21:00:00.5+09:00"),
            Some(1_792_843_200_500)
        );
        assert_eq!(
            parse_rfc3339_millis("1970-01-01T00:00:00-01:30"),
            Some(5_400_000)
        );
        assert_eq!(
            parse_rfc3339_millis("2024-02-29T00:00:00Z"),
            Some(1_709_164_800_000)
        );
        for bad in [
            "",
            "2026-10-24",
            "2026-13-01T00:00:00Z",
            "2026-10-24T25:00:00Z",
            "x",
        ] {
            assert_eq!(parse_rfc3339_millis(bad), None, "{bad}");
        }
    }
}
