//! Following a planned restart through the commands: which daemon a command finds running, the
//! handover a daemon notes before it releases the lock to its successor, which daemon shows itself
//! the successor of another (by the identity it records as its predecessor), and whether a daemon
//! that has ended with the lock free may still be followed by one.

use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;

use crate::instance_lock;
use crate::owner_record::{Owner, Predecessor};
use crate::platform::proc::ProcessIdentity;

use super::service_unit::{self, UnitState};

pub(super) const POLL: Duration = Duration::from_millis(100);

/// How long a command waits, once a service-managed daemon has ended with the lock free, for the
/// service manager to start the daemon that takes over, where systemd cannot be asked whether it
/// is restarting the unit (no unit is recorded, or `systemctl --user` does not answer): systemd
/// starts it 100 ms after the exit by default (`RestartSec=`), and a daemon takes the lock first
/// thing. Only that fallback waits a fixed time.
pub(super) const SERVICE_RESTART_GRACE: Duration = Duration::from_secs(3);

/// What a command finds of the daemon running against the data directory ([`find_daemon`]).
#[derive(Debug)]
pub(super) enum Found {
    /// This daemon holds the lock and has recorded itself; `after` is the daemon whose handover
    /// was under way while the command waited, if any.
    Running { owner: Owner, after: Option<Owner> },
    /// A daemon holds the lock without having recorded itself: it is starting or stopping, or
    /// could not write the record.
    Unrecorded,
    /// No daemon runs, and none is on its way. `waited` says whether one was when the command
    /// looked: a daemon held the lock without a record, or was handing over to its successor.
    NotRunning { waited: bool },
}

/// Finds the daemon running against the data directory, waiting up to `wait` for a planned restart
/// half-way to settle: while the daemon that released the lock to its successor, as its note says,
/// may still see that one take it ([`next_may_come`]), and while a daemon holds the lock without
/// having recorded itself, as the restarting daemon does once it has removed its record and its
/// successor does until it writes its own. That second wait is made when `through_unrecorded`, and
/// otherwise only while the note of a handover seen pending is still on disk: the successor removes
/// it only after it has recorded itself, so a holder that has recorded nothing, with no such note,
/// will not record. A command that never waits on a daemon that cannot record itself thus still
/// waits out the window between the successor's lock and its record.
///
/// The daemon that then holds the lock and has recorded itself is the one found, along with the
/// daemon whose handover the command waited through, if any, which it has asked nothing: whether
/// the one found is that daemon's successor is the caller's to judge ([`succession`]).
pub(super) fn find_daemon(wait: Duration, through_unrecorded: bool) -> Result<Found> {
    let deadline = Instant::now() + wait;
    let mut waited = false;
    let mut watch = EndWatch::default();
    let mut after = None;
    loop {
        let lock_held = instance_lock::is_running()?;
        let noted = Owner::handover_noted();
        let pending = noted.as_ref().is_some_and(|noted| {
            let running = noted.from.identity.is_running();
            let gone_since = watch.observe(noted.from.identity, running, Instant::now());
            next_may_come(
                &Awaited {
                    from: &noted.from,
                    ended: !running,
                    noted_at: Some(noted.noted),
                    crashed: false,
                    gone_since,
                },
                Instant::now(),
                SystemTime::now(),
                service_unit::state,
            )
        });
        let noted_from = noted.map(|noted| noted.from);
        if pending {
            after.clone_from(&noted_from);
        }
        if lock_held {
            if let Some(owner) = Owner::recorded() {
                let after = after.filter(|from| from.identity != owner.identity);
                return Ok(Found::Running { owner, after });
            }
            // The note is removed after the record is written, so no note and no record means
            // that this holder will not record.
            let note_stands = after
                .as_ref()
                .zip(noted_from.as_ref())
                .is_some_and(|(after, noted)| after.identity == noted.identity);
            if !(through_unrecorded || note_stands) || Instant::now() >= deadline {
                return Ok(Found::Unrecorded);
            }
        } else {
            if !pending {
                // Looked at again: a successor that took the lock since it was seen free, and
                // recorded itself, has removed the note.
                if instance_lock::is_running()? {
                    continue;
                }
                return Ok(Found::NotRunning { waited });
            }
            if Instant::now() >= deadline {
                return Ok(Found::NotRunning { waited: true });
            }
        }
        waited = true;
        std::thread::sleep(POLL);
    }
}

/// Whether a daemon shows itself the successor of another ([`succession`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Succession {
    /// It took over from that daemon: its record names that daemon as its predecessor, or, where
    /// its record does not say, it recorded the very key that daemon had.
    Successor,
    /// It did not: its record names another predecessor or none, or, where it does not say, it
    /// recorded another key.
    Other,
    /// Nothing shows either way: its record does not say (see [`Predecessor::Unsaid`] for when),
    /// and a key to compare could not be read for one of the two.
    Unproven,
}

/// Whether `next`, with the key `next_key` where it could be read, took over from `from`, whose key
/// was `from_key`. A daemon of this build records its predecessor: the daemon that started it
/// itself hands it its identity with its key, and one a service manager started copies it from
/// the handover note (see [`Owner::service_predecessor`]), so the identities are compared. Where
/// the record does not say (see [`Predecessor::Unsaid`] for when), the keys are compared, since a daemon that starts its successor itself hands it
/// its key and every other start makes one of its own.
pub(super) fn succession(
    from: &Owner,
    from_key: Option<&str>,
    next: &Owner,
    next_key: Option<&str>,
) -> Succession {
    match next.predecessor {
        Predecessor::Daemon(identity) if identity == from.identity => Succession::Successor,
        Predecessor::Daemon(_) | Predecessor::None => Succession::Other,
        Predecessor::Unsaid => match (from_key, next_key) {
            (Some(from_key), Some(next_key)) if from_key == next_key => Succession::Successor,
            (Some(_), Some(_)) => Succession::Other,
            _ => Succession::Unproven,
        },
    }
}

/// The other daemon the owner record names, if any, with its key where it can be read.
pub(super) fn recorded_after(owner: &Owner) -> Option<(Owner, Option<String>)> {
    Owner::recorded()
        .filter(|next| next.identity != owner.identity)
        .map(|next| {
            let key = next.recorded_key();
            (next, key)
        })
}

/// What a command knows of a daemon whose successor it may still be waiting for, with the lock
/// free, for [`next_may_come`].
pub(super) struct Awaited<'a> {
    /// The daemon, as its record named it.
    pub(super) from: &'a Owner,
    /// Whether its process has ended.
    pub(super) ended: bool,
    /// When its note of a handover was written, where one names it.
    pub(super) noted_at: Option<SystemTime>,
    /// That it ended leaving its owner record behind, as a daemon that crashed or was killed does:
    /// one that stops removes it, and one that hands over notes that first.
    pub(super) crashed: bool,
    /// Since when this command has seen it ended, having seen it running ([`EndWatch`]).
    pub(super) gone_since: Option<Instant>,
}

/// Whether a daemon may still take over from `awaited.from` while the lock is free.
///
/// While that daemon still runs, it does if it noted a handover: it released the lock before it
/// starts its successor, or exits for its service manager to start one. Once it has ended, a
/// daemon that started its successor itself has seen that one report listening, which comes after
/// it recorded itself, so none is on its way. For a service-managed one, systemd is asked through
/// `unit_state` about the unit its record names: a unit activating or deactivating (restarting
/// passes through both), or running a main process, may yet bring up a daemon, and one that is
/// inactive or failed will not. That is asked whether or not a note names the daemon, so that a
/// stop is not reported done while a unit that restarts after every exit brings up the next one.
/// Where systemd cannot be asked, a daemon that noted a handover or crashed may still be followed
/// for [`SERVICE_RESTART_GRACE`] after the note was written or after this command saw it end.
pub(super) fn next_may_come(
    awaited: &Awaited<'_>,
    now: Instant,
    wall_now: SystemTime,
    unit_state: impl FnOnce(&str) -> Option<UnitState>,
) -> bool {
    let from = awaited.from;
    let marked = awaited.noted_at.is_some() || awaited.crashed;
    if !awaited.ended {
        return awaited.noted_at.is_some();
    }
    if from.service_managed != Some(true) {
        return false;
    }
    if let Some(state) = from.unit.as_deref().and_then(unit_state) {
        return match state {
            UnitState::Changing => true,
            UnitState::Active { main_pid } => main_pid != 0,
            UnitState::Down => false,
        };
    }
    if !marked {
        return false;
    }
    // A note dated after now, by a clock set back since, is an old one.
    let since_noted = awaited
        .noted_at
        .map(|noted| wall_now.duration_since(noted).unwrap_or(Duration::MAX));
    since_noted.is_some_and(|since| since < SERVICE_RESTART_GRACE)
        || awaited
            .gone_since
            .is_some_and(|since| now.saturating_duration_since(since) < SERVICE_RESTART_GRACE)
}

/// What a command has seen of a daemon it may be waiting on, which tells since when it has ended
/// for the fallback of [`next_may_come`]: counted only for a daemon this command saw running, so
/// that a command that comes late is not held up by an old note no successor followed.
#[derive(Default)]
pub(super) struct EndWatch {
    /// The daemon last seen running.
    seen_running: Option<ProcessIdentity>,
    /// Since when that daemon has been seen ended.
    gone_since: Option<Instant>,
}

impl EndWatch {
    /// A watch of a daemon this command found running.
    pub(super) fn found_running(identity: ProcessIdentity) -> Self {
        Self {
            seen_running: Some(identity),
            gone_since: None,
        }
    }

    /// Takes in whether the daemon `identity` names is `running` at `now`, and returns since when
    /// it has been seen ended after it was seen running.
    pub(super) fn observe(
        &mut self,
        identity: ProcessIdentity,
        running: bool,
        now: Instant,
    ) -> Option<Instant> {
        if running {
            self.seen_running = Some(identity);
            self.gone_since = None;
        } else if self.seen_running == Some(identity) {
            self.gone_since.get_or_insert(now);
        }
        self.gone_since
            .filter(|_| self.seen_running == Some(identity))
    }
}

/// What a daemon that starts its successor itself writes on that one's stdin: the key, and on a
/// second line its own identity, which the successor records as its predecessor. A build before
/// that line reads the first line alone.
pub(super) fn handover_input(key: &str, from: Option<ProcessIdentity>) -> String {
    match from {
        Some(from) => format!("{key}\nafter {}\n", from.key()),
        None => format!("{key}\n"),
    }
}

/// The predecessor named by the line after the key on a successor's stdin ([`handover_input`]):
/// a daemon of a build before that line names none, which reads as not said.
pub(crate) fn handed_over_from(second_line: &str) -> Predecessor {
    second_line
        .trim()
        .strip_prefix("after ")
        .and_then(|identity| ProcessIdentity::parse(identity.trim()))
        .map_or(Predecessor::Unsaid, Predecessor::Daemon)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::platform::proc::ProcessId;

    fn identity(pid: u32, start_micros: u64) -> ProcessIdentity {
        ProcessIdentity {
            pid: ProcessId(pid),
            start_micros,
        }
    }

    fn owner(pid: u32, predecessor: Predecessor) -> Owner {
        Owner {
            identity: identity(pid, 1_000 + u64::from(pid)),
            port: 7531,
            follows_application: Some(false),
            service_managed: Some(false),
            takes_owner_key: false,
            unit: None,
            predecessor,
        }
    }

    #[test]
    fn a_daemon_that_names_the_other_as_its_predecessor_is_its_successor_whatever_the_keys_say() {
        let from = owner(10, Predecessor::Unsaid);
        let next = owner(11, Predecessor::Daemon(from.identity));
        for (from_key, next_key) in [
            (Some("a"), Some("a")),
            (Some("a"), Some("b")),
            (Some("a"), None),
            (None, Some("b")),
            (None, None),
        ] {
            assert_eq!(
                succession(&from, from_key, &next, next_key),
                Succession::Successor,
                "{from_key:?} {next_key:?}"
            );
        }
    }

    #[test]
    fn a_daemon_that_names_another_predecessor_or_none_is_another_whatever_the_keys_say() {
        let from = owner(10, Predecessor::Unsaid);
        let others = [
            owner(11, Predecessor::Daemon(identity(12, 1_012))),
            // The same pid, started at another time, is another process.
            owner(
                11,
                Predecessor::Daemon(identity(
                    from.identity.pid.0,
                    from.identity.start_micros + 1,
                )),
            ),
            owner(11, Predecessor::None),
        ];
        for next in &others {
            for (from_key, next_key) in [
                (Some("a"), Some("a")),
                (Some("a"), Some("b")),
                (Some("a"), None),
                (None, None),
            ] {
                assert_eq!(
                    succession(&from, from_key, next, next_key),
                    Succession::Other,
                    "{:?} {from_key:?} {next_key:?}",
                    next.predecessor
                );
            }
        }
    }

    /// A record that does not say is judged by the keys: the same key is a handover, another key
    /// is another daemon, and with a key unreadable nothing shows either way.
    #[test]
    fn a_daemon_that_does_not_say_is_judged_by_its_key() {
        let from = owner(10, Predecessor::Unsaid);
        let next = owner(11, Predecessor::Unsaid);
        assert_eq!(
            succession(&from, Some("a"), &next, Some("a")),
            Succession::Successor
        );
        assert_eq!(
            succession(&from, Some("a"), &next, Some("b")),
            Succession::Other
        );
        assert_eq!(
            succession(&from, Some("a"), &next, None),
            Succession::Unproven
        );
        assert_eq!(
            succession(&from, None, &next, Some("a")),
            Succession::Unproven
        );
        assert_eq!(succession(&from, None, &next, None), Succession::Unproven);
    }

    const BASE_WALL: Duration = Duration::from_secs(1_000_000);

    /// A daemon that has ended, or not, for [`next_may_come`].
    fn awaited(
        from: &Owner,
        ended: bool,
        noted_at: Option<SystemTime>,
        crashed: bool,
        gone_since: Option<Instant>,
    ) -> Awaited<'_> {
        Awaited {
            from,
            ended,
            noted_at,
            crashed,
            gone_since,
        }
    }

    fn wall(offset: Duration) -> SystemTime {
        SystemTime::UNIX_EPOCH + BASE_WALL + offset
    }

    fn managed(unit: Option<&str>) -> Owner {
        Owner {
            service_managed: Some(true),
            unit: unit.map(str::to_owned),
            ..owner(10, Predecessor::Unsaid)
        }
    }

    /// A daemon that still runs may be followed exactly when it noted a handover, whatever systemd
    /// says of its unit.
    #[test]
    fn a_daemon_that_still_runs_may_be_followed_only_if_it_noted_a_handover() {
        let now = Instant::now();
        for from in [
            managed(None),
            managed(Some("a.service")),
            owner(10, Predecessor::Unsaid),
        ] {
            for state in [None, Some(UnitState::Down), Some(UnitState::Changing)] {
                assert!(
                    next_may_come(
                        &awaited(&from, false, Some(wall(Duration::ZERO)), false, None),
                        now,
                        wall(Duration::ZERO),
                        |_| state
                    ),
                    "{from:?} {state:?}"
                );
                for crashed in [false, true] {
                    assert!(
                        !next_may_come(
                            &awaited(&from, false, None, crashed, None),
                            now,
                            wall(Duration::ZERO),
                            |_| state
                        ),
                        "{from:?} {state:?} {crashed}"
                    );
                }
            }
        }
    }

    /// Once it has ended, a daemon that was not service-managed, or does not say, is followed by
    /// none, noted or crashed, and systemd is not asked.
    #[test]
    fn an_ended_daemon_that_is_not_service_managed_is_followed_by_none() {
        let base = Instant::now();
        for service_managed in [Some(false), None] {
            let from = Owner {
                service_managed,
                unit: Some("a.service".to_owned()),
                ..owner(10, Predecessor::Unsaid)
            };
            assert!(
                !next_may_come(
                    &awaited(&from, true, Some(wall(Duration::ZERO)), true, Some(base)),
                    base,
                    wall(Duration::ZERO),
                    |_| Some(UnitState::Changing)
                ),
                "{service_managed:?}"
            );
        }
    }

    /// Where systemd answers, a unit that is changing, or runs a main process, may yet bring up a
    /// daemon, and one that is down will not, whether or not the daemon noted a handover or
    /// crashed.
    #[test]
    fn systemd_says_whether_the_unit_of_an_ended_service_managed_daemon_brings_up_another() {
        let base = Instant::now();
        let from = managed(Some("a.service"));
        let cases = [
            (UnitState::Changing, true),
            (UnitState::Active { main_pid: 4242 }, true),
            (UnitState::Active { main_pid: 0 }, false),
            (UnitState::Down, false),
        ];
        for (state, expected) in cases {
            // (noted_at, crashed, gone_since): none of it, or all of it fresh.
            for (noted_at, crashed, gone_since) in [
                (None, false, None),
                (Some(wall(Duration::ZERO)), true, Some(base)),
            ] {
                let mut asked = Vec::new();
                let may = next_may_come(
                    &awaited(&from, true, noted_at, crashed, gone_since),
                    base,
                    wall(Duration::ZERO),
                    |unit| {
                        asked.push(unit.to_owned());
                        Some(state)
                    },
                );
                assert_eq!(may, expected, "{state:?} {noted_at:?} {crashed}");
                assert_eq!(asked, ["a.service"]);
            }
        }
    }

    /// Without a unit recorded, systemd is not asked; the fallback decides.
    #[test]
    fn systemd_is_not_asked_without_a_recorded_unit() {
        let base = Instant::now();
        let from = managed(None);
        assert!(!next_may_come(
            &awaited(&from, true, None, false, None),
            base,
            wall(Duration::ZERO),
            |unit| panic!("asked about {unit:?}")
        ));
    }

    /// With no answer from systemd, an ended daemon is waited for only if it noted a handover or
    /// crashed, and then for the grace after the note was written or after this command saw it
    /// end.
    #[test]
    fn without_an_answer_a_noted_or_crashed_daemon_is_waited_for_the_grace() {
        let base = Instant::now();
        let grace = SERVICE_RESTART_GRACE;
        let second = Duration::from_secs(1);
        for from in [managed(None), managed(Some("a.service"))] {
            let may = |noted_at, crashed, gone_since, now: Instant, wall_now| {
                next_may_come(
                    &awaited(&from, true, noted_at, crashed, gone_since),
                    now,
                    wall_now,
                    |_| None,
                )
            };
            let noted = Some(wall(Duration::ZERO));

            // Neither noted nor crashed: not waited for, however recently it ended.
            assert!(!may(None, false, None, base, wall(Duration::ZERO)));
            assert!(!may(None, false, Some(base), base, wall(Duration::ZERO)));

            // Noted: for the grace after the note, by the wall clock.
            assert!(may(noted, false, None, base, wall(Duration::ZERO)));
            assert!(may(noted, false, None, base, wall(grace - second)));
            assert!(!may(noted, false, None, base, wall(grace)));
            assert!(!may(noted, false, None, base, wall(grace + second)));
            // A note dated in the future, by a clock set back since, is an old one.
            let future = Some(wall(second));
            assert!(!may(future, false, None, base, wall(Duration::ZERO)));

            // Crashed: for the grace after this command saw it end, by the monotonic clock.
            assert!(!may(None, true, None, base, wall(Duration::ZERO)));
            assert!(may(None, true, Some(base), base, wall(Duration::ZERO)));
            assert!(may(
                None,
                true,
                Some(base),
                base + grace - second,
                wall(Duration::ZERO)
            ));
            assert!(!may(
                None,
                true,
                Some(base),
                base + grace,
                wall(Duration::ZERO)
            ));
            assert!(!may(
                None,
                true,
                Some(base),
                base + grace + second,
                wall(Duration::ZERO)
            ));

            // Either is enough, and a stale one does not cancel a fresh one.
            assert!(may(noted, false, Some(base), base + grace, wall(second)));
            assert!(may(
                noted,
                false,
                Some(base),
                base + second,
                wall(grace + second)
            ));
            assert!(!may(noted, false, Some(base), base + grace, wall(grace)));
            // A sight of its end counts only for a daemon that noted a handover or crashed.
            assert!(!may(
                None,
                false,
                Some(base),
                base + second,
                wall(Duration::ZERO)
            ));
        }
    }

    #[test]
    fn a_watch_that_never_saw_the_daemon_running_has_nothing_to_say_of_its_end() {
        let identity = identity(10, 1_010);
        let base = Instant::now();
        let mut watch = EndWatch::default();
        assert_eq!(watch.observe(identity, false, base), None);
        assert_eq!(
            watch.observe(identity, false, base + Duration::from_secs(1)),
            None
        );
    }

    /// The end is dated from the first time it was seen, and seeing the daemon run again starts
    /// the count over.
    #[test]
    fn a_watch_dates_the_end_of_a_daemon_it_saw_running() {
        let identity = identity(10, 1_010);
        let base = Instant::now();
        let at = |seconds| base + Duration::from_secs(seconds);
        let mut watch = EndWatch::default();
        assert_eq!(watch.observe(identity, true, at(0)), None);
        assert_eq!(watch.observe(identity, false, at(1)), Some(at(1)));
        assert_eq!(watch.observe(identity, false, at(2)), Some(at(1)));
        assert_eq!(watch.observe(identity, true, at(3)), None);
        assert_eq!(watch.observe(identity, false, at(4)), Some(at(4)));
    }

    #[test]
    fn a_watch_of_a_daemon_found_running_dates_its_end() {
        let identity = identity(10, 1_010);
        let base = Instant::now();
        let mut watch = EndWatch::found_running(identity);
        assert_eq!(watch.observe(identity, false, base), Some(base));
    }

    /// The end of a daemon the watch did not see running is not the end of the one it did.
    #[test]
    fn a_watch_does_not_date_the_end_of_another_daemon() {
        let seen = identity(10, 1_010);
        let other = identity(11, 1_011);
        let base = Instant::now();
        let mut watch = EndWatch::found_running(seen);
        assert_eq!(watch.observe(other, false, base), None);
        assert_eq!(
            watch.observe(seen, false, base + Duration::from_secs(1)),
            Some(base + Duration::from_secs(1))
        );
    }

    #[test]
    fn the_handover_input_is_the_key_and_the_identity_of_the_daemon_handing_over() {
        let from = identity(10, 1_010);
        assert_eq!(handover_input("abc", Some(from)), "abc\nafter 10:1010\n");
        assert_eq!(handover_input("abc", None), "abc\n");
    }

    #[test]
    fn the_line_after_the_key_names_the_predecessor() {
        let from = identity(10, 1_010);
        assert_eq!(handed_over_from("after 10:1010"), Predecessor::Daemon(from));
        assert_eq!(
            handed_over_from("  after 10:1010 \r\n"),
            Predecessor::Daemon(from)
        );
        for line in [
            "",
            "\n",
            "garbage",
            "after",
            "after ",
            "after garbage",
            "after 0:5",
            "after 10",
            "after 10:x",
            "10:1010",
            "afterwards 10:1010",
        ] {
            assert_eq!(handed_over_from(line), Predecessor::Unsaid, "{line:?}");
        }
    }

    #[test]
    fn the_predecessor_a_daemon_hands_over_reads_back_from_its_input() {
        let from = identity(10, 1_010);
        let input = handover_input("abc", Some(from));
        let mut lines = input.lines();
        assert_eq!(lines.next(), Some("abc"));
        assert_eq!(
            handed_over_from(lines.next().unwrap()),
            Predecessor::Daemon(from)
        );
        // With none handed over there is no second line, which reads as not said.
        let input = handover_input("abc", None);
        let second = input.lines().nth(1).unwrap_or_default();
        assert_eq!(handed_over_from(second), Predecessor::Unsaid);
    }
}
