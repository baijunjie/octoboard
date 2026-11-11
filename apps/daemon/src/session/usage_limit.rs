//! Where a running session stands with the hold Octoboard puts on its own writes once a turn has
//! stopped on its account's usage limit: each message it wrote would open another turn that fails
//! the same way. The hold belongs to the process, so a new process starts free of it. What arms
//! and releases it is `crate::messaging::usage_limit`'s; for the rules see
//! docs/product/messages-held-after-a-usage-limit.md.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a failed turn holds the session's writes while its conversation record is read for
/// whether the usage limit stopped it. The read itself gives the record four seconds to say so,
/// and the default account's record needs the shell's environment first, which a cold cache takes
/// a few seconds more for; past this the writes go ahead, and a verdict that comes later still
/// holds what is written from then on.
const JUDGING: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Free,
    /// A turn failed and its record is being read, for at most [`JUDGING`] from `started`.
    Judging {
        started: Instant,
    },
    /// The record said the turn stopped on the usage limit.
    Held,
    /// Held, and a turn that Octoboard's held writes did not open is running: the person's own
    /// prompt, a message of theirs that went past the hold, or a turn the agent opened by itself.
    /// How that turn ends decides whether the hold goes on.
    Probing,
}

#[derive(Debug)]
struct Inner {
    phase: Phase,
    /// When the failed turn that armed the hold was seen to end (milliseconds since the epoch).
    since: i64,
    /// Counts the judgements begun, so that the verdict of one overtaken by a later turn is
    /// dropped.
    judgement: u64,
}

/// The hold of one process; see the module's documentation.
#[derive(Debug)]
pub struct UsageLimitHold(Mutex<Inner>);

impl Default for UsageLimitHold {
    fn default() -> Self {
        Self(Mutex::new(Inner {
            phase: Phase::Free,
            since: 0,
            judgement: 0,
        }))
    }
}

impl UsageLimitHold {
    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().expect("usage limit hold lock poisoned")
    }

    /// A turn has failed at `since`: writes are held until [`Self::judged`] is given the verdict
    /// on it, whatever held them before. Returns the judgement to hand that verdict back with.
    pub fn begin_judging(&self, since: i64) -> u64 {
        let mut inner = self.inner();
        inner.judgement += 1;
        inner.phase = Phase::Judging {
            started: Instant::now(),
        };
        inner.since = since;
        inner.judgement
    }

    /// The verdict on `judgement`: whether the turn stopped on the usage limit. Nothing when a
    /// later turn or a release has overtaken it.
    pub fn judged(&self, judgement: u64, stopped_on_limit: bool) {
        let mut inner = self.inner();
        if inner.judgement != judgement || !matches!(inner.phase, Phase::Judging { .. }) {
            return;
        }
        inner.phase = if stopped_on_limit {
            Phase::Held
        } else {
            Phase::Free
        };
    }

    /// A turn has started. While held, or while a failed turn is being judged, the hold waits for
    /// that turn's end instead ([`Self::turn_ended`]); a pending verdict no longer counts.
    pub fn turn_started(&self) {
        let mut inner = self.inner();
        if inner.phase != Phase::Free {
            inner.judgement += 1;
            inner.phase = Phase::Probing;
        }
    }

    /// A turn has ended other than in a failure: one started while held proves the account can
    /// run turns again. Whether that released the hold.
    pub fn turn_ended(&self) -> bool {
        let mut inner = self.inner();
        if inner.phase == Phase::Probing {
            inner.phase = Phase::Free;
            return true;
        }
        false
    }

    /// When the turn that armed the hold ended, while it holds anything or is being judged.
    pub fn armed_since(&self) -> Option<i64> {
        let inner = self.inner();
        (inner.phase != Phase::Free).then_some(inner.since)
    }

    /// Releases the hold armed at `since`, unless a later failed turn has armed it again since.
    /// Whether it released it.
    pub fn release_armed_at(&self, since: i64) -> bool {
        let mut inner = self.inner();
        if inner.phase == Phase::Free || inner.since != since {
            return false;
        }
        inner.judgement += 1;
        inner.phase = Phase::Free;
        true
    }

    /// Whether Octoboard's own writes are held now.
    pub fn holds(&self) -> bool {
        match self.inner().phase {
            Phase::Free => false,
            Phase::Judging { started } => started.elapsed() < JUDGING,
            Phase::Held | Phase::Probing => true,
        }
    }

    /// Whether the hold is one the turn's record has confirmed, the one reported to owners: a
    /// failed turn still being judged is not yet known to be one.
    pub fn confirmed(&self) -> bool {
        matches!(self.inner().phase, Phase::Held | Phase::Probing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_turn_holds_while_judged_and_after_a_verdict_that_it_hit_the_limit() {
        let hold = UsageLimitHold::default();
        assert!(!hold.holds());
        let judgement = hold.begin_judging(10);
        assert!(hold.holds());
        assert!(!hold.confirmed());
        hold.judged(judgement, true);
        assert!(hold.holds() && hold.confirmed());
        assert_eq!(hold.armed_since(), Some(10));
    }

    #[test]
    fn a_verdict_that_the_limit_was_not_hit_lets_go() {
        let hold = UsageLimitHold::default();
        let judgement = hold.begin_judging(10);
        hold.judged(judgement, false);
        assert!(!hold.holds());
        assert_eq!(hold.armed_since(), None);
    }

    #[test]
    fn a_verdict_overtaken_by_a_turn_or_a_later_failure_is_dropped() {
        let hold = UsageLimitHold::default();
        let first = hold.begin_judging(10);
        let second = hold.begin_judging(20);
        hold.judged(first, false);
        assert!(hold.holds());
        hold.turn_started();
        hold.judged(second, false);
        assert!(hold.confirmed(), "the turn's end decides now");
    }

    #[test]
    fn a_turn_started_while_held_decides_by_how_it_ends() {
        let hold = UsageLimitHold::default();
        let judgement = hold.begin_judging(10);
        hold.judged(judgement, true);
        assert!(!hold.turn_ended(), "no turn of its own had started");
        hold.turn_started();
        assert!(hold.holds());
        assert!(hold.turn_ended());
        assert!(!hold.holds());
        hold.turn_started();
        assert!(!hold.holds(), "a turn of a free session holds nothing");
    }

    #[test]
    fn a_release_is_for_the_failure_it_was_judged_against() {
        let hold = UsageLimitHold::default();
        let judgement = hold.begin_judging(10);
        hold.judged(judgement, true);
        let judgement = hold.begin_judging(20);
        hold.judged(judgement, true);
        assert!(!hold.release_armed_at(10));
        assert!(hold.holds());
        assert!(hold.release_armed_at(20));
        assert!(!hold.holds());
        assert!(!hold.release_armed_at(20));
    }

    /// A failed turn whose record is not read within the judging bound lets the writes go, and a
    /// verdict that comes later still holds what is written from then on.
    #[test]
    fn a_failed_turn_not_judged_in_time_lets_the_writes_go_until_a_late_verdict_holds_them() {
        let hold = UsageLimitHold::default();
        let judgement = hold.begin_judging(10);
        assert!(hold.holds());
        let overdue = Instant::now()
            .checked_sub(JUDGING + Duration::from_secs(1))
            .expect("the clock is far enough from its start");
        hold.0.lock().unwrap().phase = Phase::Judging { started: overdue };
        assert!(!hold.holds(), "past the bound the writes go ahead");
        assert!(!hold.confirmed(), "and nothing is reported");
        hold.judged(judgement, true);
        assert!(
            hold.holds() && hold.confirmed(),
            "held again by the verdict"
        );
    }

    /// A turn that starts while a failed turn is still being judged decides the hold by how it
    /// ends, like one that starts while held.
    #[test]
    fn a_turn_started_while_a_failure_is_judged_decides_by_how_it_ends() {
        let hold = UsageLimitHold::default();
        let judgement = hold.begin_judging(10);
        hold.turn_started();
        hold.judged(judgement, true);
        assert!(hold.holds());
        assert!(hold.turn_ended());
        assert!(!hold.holds());
        assert_eq!(hold.armed_since(), None);
    }

    /// A free hold has nothing to release, and a failed turn that ends a probing turn arms it
    /// afresh for the new failure.
    #[test]
    fn a_free_hold_releases_nothing_and_a_failed_probe_arms_it_again() {
        let hold = UsageLimitHold::default();
        assert!(!hold.turn_ended());
        assert!(!hold.release_armed_at(0));
        let judgement = hold.begin_judging(10);
        hold.judged(judgement, true);
        hold.turn_started();
        let judgement = hold.begin_judging(30);
        hold.judged(judgement, true);
        assert_eq!(hold.armed_since(), Some(30));
        assert!(hold.confirmed());
        assert!(!hold.turn_ended(), "no turn of its own has started since");
    }
}
