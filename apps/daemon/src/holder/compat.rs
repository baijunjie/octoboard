//! Which holders a daemon takes over: those whose version lies between a floor and the daemon's
//! own, by semantic-version precedence.

use std::cmp::Ordering;

/// The oldest version of a holder this build adopts. It must hold the previous stable release
/// before this build's version, whose holders and protocol this build still understands. No stable
/// release exists yet, so it is the lowest version there is: every holder up to this build's own
/// is accepted. `apps/desktop/scripts/release-desktop.mjs` (`release:version`) refuses a release
/// while this is not the greatest stable `desktop-v*` tag below the new version (`0.0.0` while
/// there is none), so the first release after a stable release moves it to that stable release,
/// in a commit of its own, first.
pub const FLOOR: &str = "0.0.0";

/// A version as semantic versioning orders it: the numbers first, then a prerelease before the
/// release it leads up to, its dot-separated identifiers one by one (numbers as numbers and below
/// any text). Build metadata does not count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    core: [u64; 3],
    prerelease: Vec<Identifier>,
}

/// One dot-separated identifier of a prerelease; the order of the variants is the order of the
/// kinds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    Number(u64),
    Text(String),
}

impl Version {
    /// `major.minor.patch`, with `-prerelease` and `+build` optional; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let without_build = text.split('+').next()?;
        let (core, prerelease) = match without_build.split_once('-') {
            Some((core, prerelease)) => (core, Some(prerelease)),
            None => (without_build, None),
        };
        let mut numbers = core.split('.');
        let mut number = || numbers.next()?.parse::<u64>().ok();
        let core_numbers = [number()?, number()?, number()?];
        if numbers.next().is_some() {
            return None;
        }
        let prerelease = match prerelease {
            None => Vec::new(),
            Some(prerelease) => prerelease
                .split('.')
                .map(|identifier| {
                    let valid = !identifier.is_empty()
                        && identifier
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-');
                    if !valid {
                        return None;
                    }
                    Some(match identifier.parse::<u64>() {
                        Ok(number) if identifier.chars().all(|c| c.is_ascii_digit()) => {
                            Identifier::Number(number)
                        }
                        _ => Identifier::Text(identifier.to_string()),
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        };
        Some(Self {
            core: core_numbers,
            prerelease,
        })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core.cmp(&other.core).then_with(|| {
            match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                // A prefix comes before what extends it, which is how a vector orders.
                (false, false) => self.prerelease.cmp(&other.prerelease),
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// What the range makes of a holder's version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Adoptable,
    /// Older than the floor.
    TooOld,
    /// Newer than the daemon.
    TooNew,
    /// Not a version this code can read, so nothing is known of the holder.
    Unreadable,
}

/// The versions a daemon adopts holders of.
#[derive(Debug, Clone)]
pub struct Range {
    /// The oldest, inclusive.
    pub floor: String,
    /// The daemon's own, the newest, inclusive.
    pub own: String,
}

impl Range {
    /// This build's: [`FLOOR`] up to its own version.
    pub fn current() -> Self {
        Self {
            floor: FLOOR.to_string(),
            own: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn judge(&self, holder_version: &str) -> Verdict {
        let (Some(floor), Some(own)) = (Version::parse(&self.floor), Version::parse(&self.own))
        else {
            // A range that does not read adopts nothing it cannot read the version of, and
            // nothing else is refused on its account.
            tracing::error!(floor = %self.floor, own = %self.own, "the holder range does not read");
            return Verdict::Unreadable;
        };
        let Some(holder) = Version::parse(holder_version) else {
            return Verdict::Unreadable;
        };
        if holder < floor {
            Verdict::TooOld
        } else if holder > own {
            Verdict::TooNew
        } else {
            Verdict::Adoptable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} reads"))
    }

    #[test]
    fn versions_order_by_semantic_versioning_precedence() {
        // The order the specification gives as its example.
        let ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.1.0",
            "2.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
        assert!(v("0.10.0") > v("0.9.0"));
        assert!(v("0.1.0-beta.3") > v("0.1.0-beta.2"));
        assert!(v("0.1.0-beta.3") < v("0.1.0"));
    }

    #[test]
    fn build_metadata_does_not_count() {
        assert_eq!(v("1.2.3+abc").cmp(&v("1.2.3+def")), Ordering::Equal);
        assert_eq!(v("1.2.3-rc.1+x"), v("1.2.3-rc.1"));
    }

    #[test]
    fn what_is_not_a_version_does_not_read() {
        for text in [
            "",
            "test",
            "1",
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "1.2.3-",
            "1.2.3-a..b",
            "v1.2.3",
        ] {
            assert!(Version::parse(text).is_none(), "{text}");
        }
    }

    fn range(floor: &str, own: &str) -> Range {
        Range {
            floor: floor.to_string(),
            own: own.to_string(),
        }
    }

    #[test]
    fn a_holder_from_the_floor_up_to_the_daemon_is_adopted() {
        let range = range("0.2.0", "0.4.0-beta.2");
        assert_eq!(range.judge("0.2.0"), Verdict::Adoptable);
        assert_eq!(range.judge("0.3.5"), Verdict::Adoptable);
        assert_eq!(range.judge("0.4.0-beta.1"), Verdict::Adoptable);
        assert_eq!(range.judge("0.4.0-beta.2"), Verdict::Adoptable);
    }

    #[test]
    fn a_holder_below_the_floor_or_above_the_daemon_is_out_of_range() {
        let range = range("0.2.0", "0.4.0-beta.2");
        assert_eq!(range.judge("0.1.9"), Verdict::TooOld);
        assert_eq!(range.judge("0.2.0-rc.1"), Verdict::TooOld);
        assert_eq!(range.judge("0.4.0-beta.3"), Verdict::TooNew);
        assert_eq!(range.judge("0.4.0"), Verdict::TooNew);
        assert_eq!(range.judge("1.0.0"), Verdict::TooNew);
    }

    #[test]
    fn a_version_that_does_not_read_is_unreadable() {
        assert_eq!(range("0.0.0", "1.0.0").judge("test"), Verdict::Unreadable);
    }

    #[test]
    fn the_floor_and_this_builds_version_read() {
        assert!(Version::parse(FLOOR).is_some());
        let current = Range::current();
        assert!(Version::parse(&current.own).is_some());
        assert_eq!(current.judge(&current.own), Verdict::Adoptable);
    }
}
