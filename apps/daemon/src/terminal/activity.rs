//! Which of the clients attached to a terminal was active most recently.
//!
//! A client is active when it attaches, whenever it sends input, and whenever it says the person
//! turned to it (an `activate` frame: they selected it, clicked it or came back to its window).
//! Whatever follows "the most recently active client" (the terminal's size, and the display facts
//! the daemon answers colour and size queries with) asks this, so one client's activity is what
//! moves it to another.
//!
//! Attaching is passive (a reconnect in the background attaches too) and takes precedence only for
//! [`ATTACH_CLAIM`]: past that, a client that has done nothing since ranks below every client the
//! person used, and the latest attach is only the fallback while nobody has used any. Input and
//! activation never lapse, so the client used last stays the most recently active one until
//! another client attaches or is used. Nothing else counts: the daemon cannot tell whether anyone
//! is looking at a client, and a focus or pointer report is the terminal's own traffic.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long attaching takes precedence for a client that has not been used since. A chosen default,
/// not a measured idle time: long enough to read what a glance at the terminal attached for, short
/// enough that a client left open and untouched does not hold the size another client is working
/// at.
pub const ATTACH_CLAIM: Duration = Duration::from_secs(30);

#[cfg(test)]
thread_local! {
    /// [`ATTACH_CLAIM`] for a test that cannot wait that long, on the test's own thread.
    pub(crate) static ATTACH_CLAIM_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// [`ATTACH_CLAIM`], or a test's override of it. Read on the thread that attaches.
fn attach_claim() -> Duration {
    #[cfg(test)]
    if let Some(claim) = ATTACH_CLAIM_OVERRIDE.get() {
        return claim;
    }
    ATTACH_CLAIM
}

/// One attached client's latest activity.
#[derive(Debug, Clone, Copy)]
struct Latest {
    /// When it was, among every activity: a later one is a larger number.
    tick: u64,
    /// Until when it takes precedence, for an attach with no activity since; `None` once the
    /// client has been used, which never lapses.
    claims_until: Option<Instant>,
}

impl Latest {
    /// How the client ranks at `now`: the larger the more recently active. An attach that has
    /// lapsed ranks below every use, and among lapsed attaches the later one ranks higher.
    fn rank(&self, now: Instant) -> (u64, u64) {
        let claims = self.claims_until.is_none_or(|until| now < until);
        (if claims { self.tick } else { 0 }, self.tick)
    }
}

/// The order of the attached clients' latest activity.
#[derive(Debug, Default)]
pub struct Activity {
    /// Counts every activity, so that a later one is a larger number.
    ticks: u64,
    /// Each client's latest activity.
    latest: HashMap<u64, Latest>,
}

impl Activity {
    /// Notes that `client` attached at `now`. Returns when its precedence lapses, unless it is
    /// used before.
    pub fn attach(&mut self, client: u64, now: Instant) -> Instant {
        let until = now + attach_claim();
        self.note(client, Some(until));
        until
    }

    /// Notes that `client` was used: it sent input, or the person turned to it.
    pub fn activate(&mut self, client: u64) {
        self.note(client, None);
    }

    fn note(&mut self, client: u64, claims_until: Option<Instant>) {
        self.ticks += 1;
        self.latest.insert(
            client,
            Latest {
                tick: self.ticks,
                claims_until,
            },
        );
    }

    /// Forgets `client`, which has left.
    pub fn forget(&mut self, client: u64) {
        self.latest.remove(&client);
    }

    /// The most recently active client at `now`, if any is attached.
    #[cfg(test)]
    fn most_recent(&self, now: Instant) -> Option<u64> {
        self.most_recent_of(|_| true, now)
    }

    /// The most recently active client at `now` of those `among` takes.
    pub fn most_recent_of(&self, among: impl Fn(u64) -> bool, now: Instant) -> Option<u64> {
        self.latest
            .iter()
            .filter(|&(&client, _)| among(client))
            .max_by_key(|&(_, latest)| latest.rank(now))
            .map(|(&client, _)| client)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{Activity, ATTACH_CLAIM};

    #[test]
    fn the_latest_activity_decides_and_a_client_that_left_no_longer_counts() {
        let now = Instant::now();
        let mut activity = Activity::default();
        assert_eq!(activity.most_recent(now), None);
        activity.attach(0, now);
        activity.attach(1, now);
        assert_eq!(activity.most_recent(now), Some(1));
        activity.activate(1);
        assert_eq!(activity.most_recent(now), Some(1));
        activity.activate(0);
        assert_eq!(activity.most_recent(now), Some(0));
        assert_eq!(activity.most_recent_of(|client| client != 0, now), Some(1));
        activity.forget(0);
        assert_eq!(activity.most_recent(now), Some(1));
    }

    #[test]
    fn an_attach_without_input_stops_claiming_once_its_claim_lapses() {
        let now = Instant::now();
        let later = now + ATTACH_CLAIM;
        let mut activity = Activity::default();
        activity.attach(0, now);
        activity.activate(0);
        assert_eq!(activity.attach(1, now), later, "when the precedence lapses");
        assert_eq!(activity.most_recent(now), Some(1), "attaching is activity");
        assert_eq!(
            activity.most_recent(later),
            Some(0),
            "the quiet client no longer takes precedence, and use never lapses"
        );

        activity.attach(2, now);
        assert_eq!(
            activity.most_recent_of(|client| client != 0, later),
            Some(2),
            "while nobody has been used, the latest attach is the fallback"
        );

        activity.activate(1);
        assert_eq!(
            activity.most_recent(later + ATTACH_CLAIM),
            Some(1),
            "being used takes it back"
        );
        activity.attach(3, later + ATTACH_CLAIM);
        assert_eq!(
            activity.most_recent(later + ATTACH_CLAIM),
            Some(3),
            "a new attach claims at once"
        );
    }
}
