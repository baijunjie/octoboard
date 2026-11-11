//! Following a planned restart through the commands: which daemon a command finds running, the
//! handover a daemon notes before it releases the lock to its successor, and how a stop or a
//! restart tells from the lock and the owner records that a daemon has handed over to the next one,
//! has ended with none, or has been followed by an unrelated one.

use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;

use crate::instance_lock;
use crate::owner_record::{HandoverNote, Owner};
use crate::platform::proc::ProcessIdentity;

pub(super) const POLL: Duration = Duration::from_millis(100);

/// How long `stop` goes on watching, once a daemon a service manager restarts has ended with the
/// lock free, for that manager to start the daemon that takes over: systemd starts it 100 ms after
/// the exit by default (`RestartSec=`), and a daemon takes the lock first thing. An ordinary stop
/// of such a daemon, which no successor follows, takes this much longer to report.
pub(super) const SERVICE_RESTART_GRACE: Duration = Duration::from_secs(3);

/// What a command finds of the daemon running against the data directory ([`find_daemon`]).
#[derive(Debug)]
pub(super) enum Found {
    /// This daemon holds the lock and has recorded itself; `after` is the daemon whose handover
    /// was under way while the command waited, if any (see [`handed_over_to`]).
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
/// may still see that one take it ([`handover_pending`]), and while a daemon holds the lock
/// without having recorded itself, as the restarting daemon does once it has removed its record
/// and its successor does until it writes its own. That second wait is made when
/// `through_unrecorded`, and otherwise only while the note of a handover seen pending is still on
/// disk: the successor removes it only after it has recorded itself, so a holder that has recorded
/// nothing, with no such note, will not record. A command that never waits on a daemon that cannot
/// record itself thus still waits out the window between the successor's lock and its record.
///
/// The daemon that then holds the lock and has recorded itself is the one found, along with the
/// daemon whose handover the command waited through, if any, which it has asked nothing: whether
/// the one found is that daemon's successor is the caller's to judge.
pub(super) fn find_daemon(wait: Duration, through_unrecorded: bool) -> Result<Found> {
    let deadline = Instant::now() + wait;
    let mut waited = false;
    let mut watch = HandoverWatch::default();
    let mut after = None;
    loop {
        let lock_held = instance_lock::is_running()?;
        let noted = Owner::handover_noted();
        let pending = watch.pending(noted.as_ref(), Instant::now(), SystemTime::now());
        let noted_from = noted.map(|noted| noted.from);
        if pending {
            after = noted_from;
        }
        if lock_held {
            if let Some(owner) = Owner::recorded() {
                let after = after.filter(|from| from.identity != owner.identity);
                return Ok(Found::Running { owner, after });
            }
            // The note is removed after the record is written, so no note and no record means
            // that this holder will not record.
            let note_stands = after
                .zip(noted_from)
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

/// What a command has seen of the daemon a handover note names, which tells since when it has
/// ended for [`handover_pending`].
#[derive(Default)]
pub(super) struct HandoverWatch {
    /// The daemon last seen running.
    seen_running: Option<ProcessIdentity>,
    /// Since when that daemon has been seen ended.
    gone_since: Option<Instant>,
}

impl HandoverWatch {
    pub(super) fn pending(
        &mut self,
        noted: Option<&HandoverNote>,
        now: Instant,
        wall_now: SystemTime,
    ) -> bool {
        let Some(noted) = noted else {
            return false;
        };
        let identity = noted.from.identity;
        let running = identity.is_running();
        if running {
            self.seen_running = Some(identity);
            self.gone_since = None;
        } else if self.seen_running == Some(identity) {
            self.gone_since.get_or_insert(now);
        }
        let gone_since = self
            .gone_since
            .filter(|_| self.seen_running == Some(identity));
        handover_pending(noted, running, gone_since, now, wall_now)
    }
}

/// Whether the daemon `noted` names, which released the lock to its successor, may still see that
/// one take it, with the lock free: while its process still runs, since a daemon that starts its
/// successor itself waits for it to report listening, which comes after it records itself; and,
/// for a daemon whose successor a service manager starts, for [`SERVICE_RESTART_GRACE`] after it
/// ended. That end counts from when this command first saw it ended (`gone_since`), for a daemon
/// it saw running, and in any case from when the note was written, shortly before the daemon
/// exited, so that a command that comes late is not held up by an old note no successor followed.
fn handover_pending(
    noted: &HandoverNote,
    running: bool,
    gone_since: Option<Instant>,
    now: Instant,
    wall_now: SystemTime,
) -> bool {
    if running {
        return true;
    }
    if noted.from.service_managed != Some(true) {
        return false;
    }
    // A note dated after now, by a clock set back since, is an old one.
    let since_noted = wall_now
        .duration_since(noted.noted)
        .unwrap_or(Duration::MAX);
    since_noted < SERVICE_RESTART_GRACE
        || gone_since
            .is_some_and(|since| now.saturating_duration_since(since) < SERVICE_RESTART_GRACE)
}

/// Where a daemon asked to stop has got to.
pub(super) enum Handover {
    /// It has ended and no daemon holds the lock.
    Gone,
    /// It handed over to `next`, which holds the lock and shows itself its successor, by the key
    /// or, `by_service`, as the successor a service manager starts (see [`same_key`] and
    /// [`service_successor`]); `key` is the key `next` recorded, where it can be read.
    Successor {
        next: Owner,
        key: Option<String>,
        by_service: bool,
    },
    /// It has ended, and this daemon, which holds the lock, cannot be shown to be its successor.
    Other(Owner),
}

/// What a stop knows besides the lock and the records, for [`handover`].
#[derive(Clone, Copy)]
pub(super) struct Standing {
    /// Whether a successor a service manager starts may still be followed.
    pub(super) follow_service: bool,
    /// That a service manager has had its time to start one (see [`SERVICE_RESTART_GRACE`]).
    pub(super) grace_over: bool,
    /// That a note has named the stopped daemon as handing over.
    pub(super) handing_over: bool,
}

/// Tells from the lock, the records and the process where the daemon `owner` names, whose key was
/// `key`, has got to: handed over to `recorded` (the other daemon the owner record names, with its
/// key where it can be read), ended, or ended with another daemon running in its place. `None`
/// until one of these. `ended` says whether the process the owner names has ended; `standing` has
/// the rest.
///
/// Only a daemon that shows itself the successor is stopped: one the desktop application or a user
/// started once the stopped daemon had gone is left running, and so is any daemon when the stopped
/// one's record shows no succession — no key could be read before it was asked (an earlier build,
/// a failed record, or a record removed just then) and the record does not say a service manager
/// starts its successor — except where a note names the stopped daemon as handing over (below).
/// Such a daemon is reported only if one already holds the lock when this looks. A daemon that
/// does not show itself the successor is judged only once the stopped one has ended: until then it
/// may still be ending, and a free lock alone does not say that none follows either, since the
/// stopped daemon releases it before it starts its successor. For the same reason a stopped daemon
/// whose successor a service manager starts, which takes the lock only after the stopped daemon
/// has exited, is gone only once `grace_over`.
///
/// Where no key could be read for the stopped daemon but `handing_over` says a note names it as
/// handing over, its key went with its records, as a daemon that starts its successor itself
/// removes them. The daemon that holds the lock next is then judged as [`handed_over_to`] judges
/// it, as it is for a command that waits through a handover. That applies to a daemon that starts
/// its successor itself: a stopped daemon whose successor a service manager starts leaves its
/// records for its successor to show itself by, and is judged only as [`service_successor`] has it.
///
/// A stop follows one successor a service manager starts, not a second: a unit that restarts the
/// daemon after every exit starts one after each stop too, which would have the stop follow it for
/// as long as the unit restarts it; the second is reported and left running instead.
pub(super) fn handover(
    owner: &Owner,
    key: Option<&str>,
    lock_held: bool,
    ended: bool,
    recorded: Option<(Owner, Option<String>)>,
    standing: Standing,
) -> Option<Handover> {
    let Standing {
        follow_service,
        grace_over,
        handing_over,
    } = standing;
    if let Some((next, next_key)) = recorded.filter(|_| lock_held) {
        if same_key(key, next_key.as_deref()) {
            return Some(Handover::Successor {
                next,
                key: next_key,
                by_service: false,
            });
        }
        if follow_service && service_successor(owner, &next, ended) {
            return Some(Handover::Successor {
                next,
                key: next_key,
                by_service: true,
            });
        }
        if key.is_none()
            && handing_over
            && owner.service_managed != Some(true)
            && handed_over_to(owner, &next)
        {
            return Some(Handover::Successor {
                next,
                key: next_key,
                by_service: false,
            });
        }
        return ended.then_some(Handover::Other(next));
    }
    let awaits_service = owner.service_managed == Some(true) && !grace_over;
    (!lock_held && ended && !awaits_service).then_some(Handover::Gone)
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

/// Since when a daemon asked to stop has been gone — the lock free and its process ended — given
/// when it was before and whether it is `now`. The lock is released before the daemon's teardown
/// and its exit, and a service manager starts the successor only after the exit, so a lock free
/// while the process still runs does not count.
pub(super) fn gone_since_after(
    since: Option<Instant>,
    now: Instant,
    gone: bool,
) -> Option<Instant> {
    gone.then(|| since.unwrap_or(now))
}

/// Whether a service manager has had `grace` to start a successor, given since when the daemon has
/// been gone.
pub(super) fn grace_elapsed(gone_since: Option<Instant>, now: Instant, grace: Duration) -> bool {
    gone_since.is_some_and(|since| now.saturating_duration_since(since) >= grace)
}

/// Who holds the lock after a planned restart was asked of a daemon.
pub(super) enum TookOver {
    /// The daemon that took over from it, as far as its record shows.
    Successor(Owner),
    /// A daemon that cannot be shown to have taken over from it, which has ended.
    Other(Owner),
}

/// Tells from the lock, the records and the process whether `recorded` (the other daemon the owner
/// record names, with its key where it can be read) took over from the daemon `owner` names, whose
/// key was `key`, after a planned restart was asked of it; `None` while that cannot yet be told.
///
/// A daemon whose record says how its successor is started is taken over by the daemon with its
/// key, or by one a service manager starts after it ([`same_key`], [`service_successor`]); a daemon
/// that shows neither is judged only once the restarted one has ended, which until then may still
/// be starting its successor. Where nothing can be shown, any daemon that holds the lock after it
/// is taken for its successor, as nothing better is known: the record of an earlier build does not
/// say how the successor is started, and for a daemon that starts it itself the key proves nothing
/// when either key could not be read.
pub(super) fn took_over(
    owner: &Owner,
    key: Option<&str>,
    lock_held: bool,
    recorded: Option<(Owner, Option<String>)>,
) -> Option<TookOver> {
    let ended = !owner.identity.is_running();
    let (next, next_key) = recorded.filter(|_| lock_held)?;
    let unproven = match owner.service_managed {
        None => true,
        Some(false) => key.is_none() || next_key.is_none(),
        Some(true) => false,
    };
    if unproven || same_key(key, next_key.as_deref()) || service_successor(owner, &next, ended) {
        return Some(TookOver::Successor(next));
    }
    ended.then_some(TookOver::Other(next))
}

/// Whether `next`, found recorded once the handover of the daemon `from` names was over, shows
/// itself that daemon's successor. Never one that follows the application: only a daemon that does
/// not writes a note, and starts its successor without `--parent-pid`, so that one is a daemon the
/// application started in the free lock. A daemon whose successor a service manager starts is
/// succeeded by one recorded as service-managed, as [`service_successor`] has it. A daemon that
/// starts its successor itself removes its key record with its owner record, so its key cannot be
/// compared: nothing better than holding the lock next shows its successor, which it starts as
/// soon as it has released the lock, so that only a start racing it could come first.
pub(super) fn handed_over_to(from: &Owner, next: &Owner) -> bool {
    next.follows_application != Some(true)
        && (from.service_managed != Some(true) || next.service_managed == Some(true))
}

/// Whether the daemon `from`, whose handover a command waited through, still has to end before the
/// daemon holding the lock can be judged its successor (`ended` says whether it has). Where its
/// record says a service manager starts that successor, the manager does so only after `from` has
/// exited, so a daemon holding the lock before then cannot be it.
pub(super) fn awaits_end(from: &Owner, ended: bool) -> bool {
    from.service_managed == Some(true) && !ended
}

/// What `stop` makes of the daemon found holding the lock after the handover of the daemon `from`
/// it waited through ([`judge_after`]).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Judged {
    /// `from` has not ended yet, where it has to before the daemon found can be judged.
    Waiting,
    /// The daemon found is `from`'s successor.
    Successor,
    /// The daemon found is not.
    Other,
}

/// Judges the daemon found holding the lock after the handover of `from`, which `stop` waited
/// through. `found_before_end` says that [`awaits_end`] was true when it was found: a daemon that
/// held the lock and had recorded itself while a service-managed `from` still ran is not the one
/// the service manager starts after `from` has exited, whatever its record shows, so it is
/// [`Judged::Other`] once `from` has ended, which `stop` still waits for. `ended` says whether
/// `from` has now, and `handed_over` is [`handed_over_to`] for the daemon found, which judges any
/// other.
pub(super) fn judge_after(found_before_end: bool, ended: bool, handed_over: bool) -> Judged {
    match (ended, found_before_end || !handed_over) {
        (false, _) => Judged::Waiting,
        (true, false) => Judged::Successor,
        (true, true) => Judged::Other,
    }
}

/// Whether the daemon `owner` names, asked for a planned restart, has ended with no successor on
/// its way: the lock is free and no handover of its own is pending ([`handover_pending`] of the
/// note it names it in, which it writes before it releases the lock and its successor removes once
/// it has recorded itself). So a stop that cut the restart short, a successor that did not start,
/// and one that has been stopped again since all show the same, as does a daemon that exited for a
/// service manager that starts none, once the grace is over. Told only of a daemon whose record
/// says how its successor is started, as a build that notes a handover writes it: an earlier build
/// notes none.
pub(super) fn abandoned(owner: &Owner, lock_held: bool, ended: bool, handing_over: bool) -> bool {
    owner.service_managed.is_some() && !lock_held && ended && !handing_over
}

/// Whether `next_key`, the key another daemon recorded, shows it the successor of the daemon whose
/// key was `key`: a daemon that starts its successor itself hands it its own key, which the
/// successor records, and every other start makes a key of its own.
fn same_key(key: Option<&str>, next_key: Option<&str>) -> bool {
    key.is_some() && key == next_key
}

/// Whether `next` shows itself the successor a service manager started for the daemon `owner`
/// names, which has `ended` or not. That successor makes a key of its own, so what shows it is
/// the daemon before it recorded as restarted by a service manager and having ended, which the
/// service manager waits for, and the successor's record saying it is service-managed too, as
/// for [`handed_over_to`]. Nothing stronger is read: systemd's `INVOCATION_ID` changes with every
/// start of a unit, so a new one does not tell a successor from any other start, its `MainPID`
/// takes the unit's name and a question to systemd, and `--service-managed` stands for any service
/// manager, not systemd alone.
fn service_successor(owner: &Owner, next: &Owner, ended: bool) -> bool {
    ended && owner.service_managed == Some(true) && next.service_managed == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::platform::proc::ProcessId;

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn owner_on(port: u16) -> Owner {
        Owner {
            identity: ProcessIdentity {
                pid: ProcessId(std::process::id()),
                start_micros: 1,
            },
            port,
            follows_application: Some(false),
            service_managed: Some(false),
        }
    }

    /// A key no daemon a planned restart starts would have: one a fresh start made of its own.
    const OTHER_KEY: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    /// A daemon still running: this process, checked by its own start time.
    fn running_owner() -> Owner {
        Owner::current(1, false, true).expect("this process's start time")
    }

    /// [`handover`] for a stop that has followed no successor a service manager started yet.
    fn first_handover(
        owner: &Owner,
        key: Option<&str>,
        lock_held: bool,
        recorded: Option<(Owner, Option<String>)>,
        grace_over: bool,
    ) -> Option<Handover> {
        let ended = !owner.identity.is_running();
        let standing = Standing {
            follow_service: true,
            grace_over,
            handing_over: false,
        };
        handover(owner, key, lock_held, ended, recorded, standing)
    }

    /// The daemon that holds the lock and has recorded the key the stopped daemon had is its
    /// successor, and the one to ask next, whether the stopped one has ended or still waits for it
    /// to report listening.
    #[test]
    fn a_successor_with_the_stopped_daemons_key_is_followed() {
        for owner in [owner_on(1), running_owner()] {
            let next = Some((owner_on(2), Some(KEY.to_owned())));
            assert!(matches!(
                first_handover(&owner, Some(KEY), true, next, false),
                Some(Handover::Successor { next, key, by_service: false })
                    if next.port == 2 && key.as_deref() == Some(KEY)
            ));
        }
    }

    /// A daemon that took the lock after the stopped one ended, with a key of its own, as one the
    /// desktop application or a user starts, is not its successor: it is reported and left running.
    #[test]
    fn a_daemon_with_another_key_is_not_followed() {
        let owner = owner_on(1);
        let next = Some((owner_on(2), Some(OTHER_KEY.to_owned())));
        assert!(matches!(
            first_handover(&owner, Some(KEY), true, next, false),
            Some(Handover::Other(other)) if other.port == 2
        ));
    }

    /// Without a key to compare, nothing shows succession: neither when the other daemon's key
    /// cannot be read, nor when the stopped daemon's could not be.
    #[test]
    fn without_both_keys_no_successor_is_followed() {
        let owner = owner_on(1);
        assert!(matches!(
            first_handover(&owner, Some(KEY), true, Some((owner_on(2), None)), false),
            Some(Handover::Other(other)) if other.port == 2
        ));
        assert!(matches!(
            first_handover(&owner, None, true, Some((owner_on(2), Some(KEY.to_owned()))), false),
            Some(Handover::Other(other)) if other.port == 2
        ));
        assert!(matches!(
            first_handover(&owner, None, true, Some((owner_on(2), None)), false),
            Some(Handover::Other(_))
        ));
    }

    /// A daemon with another key is not judged while the stopped one still runs, which may still be
    /// ending: the wait goes on.
    #[test]
    fn a_daemon_with_another_key_is_waited_on_while_the_stopped_one_runs() {
        let owner = running_owner();
        assert!(first_handover(
            &owner,
            Some(KEY),
            true,
            Some((owner_on(2), Some(OTHER_KEY.to_owned()))),
            false
        )
        .is_none());
        assert!(
            first_handover(&owner, Some(KEY), true, Some((owner_on(2), None)), false).is_none()
        );
    }

    /// A daemon whose process still runs is still ending, even with the lock free: it releases the
    /// lock before it starts its successor, so with none recorded yet, or one recorded but not yet
    /// holding the lock, the wait goes on.
    #[test]
    fn a_daemon_that_still_runs_with_the_lock_free_is_waited_for() {
        let owner = running_owner();
        assert!(first_handover(&owner, Some(KEY), false, None, true).is_none());
        assert!(first_handover(
            &owner,
            Some(KEY),
            false,
            Some((owner_on(2), Some(KEY.to_owned()))),
            true
        )
        .is_none());
    }

    /// A daemon that has ended, with the lock free and nobody recorded after it, is gone at once;
    /// one whose lock is held by no recorded daemon is waited for.
    #[test]
    fn a_daemon_that_has_ended_with_no_successor_is_gone() {
        let owner = owner_on(1);
        for key in [Some(KEY), None] {
            assert!(matches!(
                first_handover(&owner, key, false, None, false),
                Some(Handover::Gone)
            ));
            assert!(first_handover(&owner, key, true, None, true).is_none());
        }
    }

    /// A daemon recorded as one a service manager restarts, as the given daemon on `port`.
    fn service_owner_on(port: u16) -> Owner {
        owner_on(port).with_service_manager(true)
    }

    /// A daemon recorded before the record said how its successor is started.
    fn unknown_owner_on(port: u16) -> Owner {
        Owner {
            service_managed: None,
            ..owner_on(port)
        }
    }

    /// The successor a service manager starts makes a key of its own: once the stopped daemon,
    /// recorded as restarted by a service manager, has ended, a daemon recorded the same way that
    /// holds the lock is its successor, and the one to ask next, with its own key.
    #[test]
    fn a_service_managed_successor_with_a_new_key_is_followed() {
        for next_key in [Some(OTHER_KEY.to_owned()), None] {
            assert!(matches!(
                first_handover(
                    &service_owner_on(1),
                    Some(KEY),
                    true,
                    Some((service_owner_on(2), next_key.clone())),
                    false
                ),
                Some(Handover::Successor { next, key, by_service: true })
                    if next.port == 2 && key == next_key
            ));
        }
        // Whether or not the stopped daemon's key could be read.
        assert!(matches!(
            first_handover(
                &service_owner_on(1),
                None,
                true,
                Some((service_owner_on(2), None)),
                false
            ),
            Some(Handover::Successor { next, key: None, by_service: true }) if next.port == 2
        ));
    }

    /// A service manager starts the successor only once the stopped daemon has exited, so a daemon
    /// recorded as restarted by one is not taken for it while the stopped one still runs.
    #[test]
    fn a_service_managed_daemon_is_waited_on_while_the_stopped_one_runs() {
        let owner = running_owner().with_service_manager(true);
        assert!(first_handover(
            &owner,
            Some(KEY),
            true,
            Some((service_owner_on(2), Some(OTHER_KEY.to_owned()))),
            false
        )
        .is_none());
    }

    /// A daemon not recorded as service-managed is not the successor of one that a service manager
    /// restarts, and neither is any daemon with another key the successor of one that starts its
    /// successor itself, or of one whose record does not say: each is reported and left running.
    #[test]
    fn a_successor_with_another_key_is_followed_only_where_the_records_show_it() {
        let cases = [
            (service_owner_on(1), owner_on(2)),
            (service_owner_on(1), unknown_owner_on(2)),
            (owner_on(1), service_owner_on(2)),
            (unknown_owner_on(1), service_owner_on(2)),
        ];
        for (owner, next) in cases {
            let next = Some((next, Some(OTHER_KEY.to_owned())));
            assert!(matches!(
                first_handover(&owner, Some(KEY), true, next, false),
                Some(Handover::Other(other)) if other.port == 2
            ));
        }
    }

    /// The key still shows succession whatever the records say of a service manager, an earlier
    /// build's record included.
    #[test]
    fn the_same_key_shows_succession_whatever_the_records_say() {
        let owners = [owner_on(1), service_owner_on(1), unknown_owner_on(1)];
        let nexts = [owner_on(2), service_owner_on(2), unknown_owner_on(2)];
        for owner in owners {
            for next in nexts {
                let next = Some((next, Some(KEY.to_owned())));
                assert!(matches!(
                    first_handover(&owner, Some(KEY), true, next, false),
                    Some(Handover::Successor { next, by_service: false, .. }) if next.port == 2
                ));
            }
        }
    }

    /// A daemon a service manager restarts that has ended with the lock free may still have a
    /// successor coming: it is gone only once the manager has had its grace. Any other daemon is
    /// gone at once.
    #[test]
    fn a_service_managed_daemon_that_has_ended_is_gone_only_after_the_grace() {
        assert!(first_handover(&service_owner_on(1), Some(KEY), false, None, false).is_none());
        assert!(matches!(
            first_handover(&service_owner_on(1), Some(KEY), false, None, true),
            Some(Handover::Gone)
        ));
        for owner in [owner_on(1), unknown_owner_on(1)] {
            assert!(matches!(
                first_handover(&owner, Some(KEY), false, None, false),
                Some(Handover::Gone)
            ));
        }
        // A lock held by a daemon not yet recorded is waited on, grace or not.
        assert!(first_handover(&service_owner_on(1), Some(KEY), true, None, true).is_none());
    }

    /// A note adds nothing for a service-managed daemon: that daemon that has released the lock but
    /// not yet exited is not shown to have handed over to the daemon holding the lock, since a
    /// service manager starts its successor only after the exit, with or without the note, and
    /// the wait goes on until it has ended.
    #[test]
    fn a_service_managed_daemon_that_still_runs_is_waited_on_whatever_the_note_says() {
        let running = running_owner().with_service_manager(true);
        for handing_over in [true, false] {
            let standing = Standing {
                follow_service: true,
                grace_over: false,
                handing_over,
            };
            let ended = false;
            assert!(handover(
                &running,
                None,
                true,
                ended,
                Some((service_owner_on(2), None)),
                standing
            )
            .is_none());
        }
    }

    /// Once a service-managed daemon with no key has ended, its successor is followed by the
    /// records alone: the note is not read for it.
    #[test]
    fn a_service_managed_daemon_with_no_key_that_has_ended_is_followed_without_a_note() {
        let ended = true;
        for handing_over in [true, false] {
            let standing = Standing {
                follow_service: true,
                grace_over: false,
                handing_over,
            };
            let next = Some((service_owner_on(2), None));
            assert!(matches!(
                handover(&service_owner_on(1), None, true, ended, next, standing),
                Some(Handover::Successor { next, by_service: true, .. }) if next.port == 2
            ));
        }
    }

    /// A stop that has followed one successor a service manager started does not follow a second,
    /// as a unit that restarts the daemon after every exit would start: it is reported and left
    /// running. A successor with the key is followed all the same.
    #[test]
    fn a_second_service_managed_successor_is_not_followed() {
        let owner = service_owner_on(1);
        let ended = true;
        let standing = Standing {
            follow_service: false,
            grace_over: false,
            handing_over: false,
        };
        let next = Some((service_owner_on(2), Some(OTHER_KEY.to_owned())));
        assert!(matches!(
            handover(&owner, Some(KEY), true, ended, next, standing),
            Some(Handover::Other(other)) if other.port == 2
        ));
        let next = Some((service_owner_on(2), Some(KEY.to_owned())));
        assert!(matches!(
            handover(&owner, Some(KEY), true, ended, next, standing),
            Some(Handover::Successor { next, by_service: false, .. }) if next.port == 2
        ));
        // The grace still applies, for that second daemon to be reported rather than missed.
        let lock_held = false;
        assert!(handover(&owner, Some(KEY), lock_held, ended, None, standing).is_none());
    }

    /// A daemon whose key was read after its handover removed it, a note naming it, has its next
    /// daemon judged as a command that waited through the handover would: by the records alone.
    /// Without the note, nothing shows a succession and that daemon is reported.
    #[test]
    fn a_handover_noted_for_a_daemon_with_no_key_follows_the_daemon_recorded_next() {
        let handed = |owner: Owner, next: Owner, follow_service: bool, handing_over: bool| {
            let ended = true;
            let next = Some((next, None));
            let standing = Standing {
                follow_service,
                grace_over: false,
                handing_over,
            };
            handover(&owner, None, true, ended, next, standing)
        };
        for (owner, next) in [
            (owner_on(1), owner_on(2)),
            (owner_on(1), service_owner_on(2)),
            (unknown_owner_on(1), owner_on(2)),
        ] {
            assert!(matches!(
                handed(owner, next, true, true),
                Some(Handover::Successor { next, key: None, by_service: false })
                    if next.port == 2
            ));
            assert!(matches!(
                handed(owner, next, true, false),
                Some(Handover::Other(_))
            ));
        }
        // Not one that follows the application, nor, after a service-managed daemon, one not
        // recorded as service-managed.
        let applications = Owner {
            follows_application: Some(true),
            ..owner_on(2)
        };
        assert!(matches!(
            handed(owner_on(1), applications, true, true),
            Some(Handover::Other(_))
        ));
        for next in [owner_on(2), unknown_owner_on(2)] {
            assert!(matches!(
                handed(service_owner_on(1), next, true, true),
                Some(Handover::Other(_))
            ));
        }
        // A second successor a service manager starts is not followed on the note's account.
        assert!(matches!(
            handed(service_owner_on(1), service_owner_on(2), false, true),
            Some(Handover::Other(_))
        ));
    }

    /// A restarted daemon that starts its successor itself is taken over only by the daemon with
    /// its key, even while it still runs, waiting for that daemon to report listening.
    #[test]
    fn a_restart_is_taken_over_by_the_daemon_with_the_key() {
        for owner in [owner_on(1), running_owner().with_service_manager(false)] {
            assert!(matches!(
                took_over(&owner, Some(KEY), true, Some((owner_on(2), Some(KEY.to_owned())))),
                Some(TookOver::Successor(next)) if next.port == 2
            ));
        }
    }

    /// A daemon with another key did not take over from one that starts its successor itself, when
    /// both keys are known: it is reported once the restarted daemon has ended, and waited on until
    /// then.
    #[test]
    fn a_restart_is_not_taken_over_by_a_daemon_with_another_key() {
        let cases = [
            (Some(KEY), Some(OTHER_KEY.to_owned()), owner_on(2)),
            (Some(KEY), Some(OTHER_KEY.to_owned()), service_owner_on(2)),
        ];
        for (key, next_key, next) in cases {
            assert!(matches!(
                took_over(&owner_on(1), key, true, Some((next, next_key.clone()))),
                Some(TookOver::Other(other)) if other.port == 2
            ));
            let running = running_owner().with_service_manager(false);
            assert!(took_over(&running, key, true, Some((next, next_key))).is_none());
        }
    }

    /// The key proves nothing when either could not be read: any daemon that holds the lock after
    /// a restarted daemon that starts its successor itself is taken for its successor, as for a
    /// record that does not say.
    #[test]
    fn a_restart_is_taken_over_by_any_daemon_when_a_key_could_not_be_read() {
        for (key, next_key) in [
            (None, Some(KEY.to_owned())),
            (Some(KEY), None),
            (None, None),
        ] {
            assert!(matches!(
                took_over(&owner_on(1), key, true, Some((owner_on(2), next_key))),
                Some(TookOver::Successor(next)) if next.port == 2
            ));
        }
    }

    /// The same holds while the restarted daemon still runs, waiting for the daemon with no key to
    /// report listening.
    #[test]
    fn a_running_restart_is_taken_over_by_a_daemon_when_a_key_could_not_be_read() {
        let running = running_owner().with_service_manager(false);
        assert!(matches!(
            took_over(&running, None, true, Some((owner_on(2), None))),
            Some(TookOver::Successor(next)) if next.port == 2
        ));
    }

    /// A restarted daemon a service manager restarts is taken over by a daemon recorded as
    /// service-managed, with a key of its own, once it has ended; not by one that is not.
    #[test]
    fn a_service_managed_restart_is_taken_over_by_a_service_managed_daemon() {
        assert!(matches!(
            took_over(
                &service_owner_on(1),
                Some(KEY),
                true,
                Some((service_owner_on(2), Some(OTHER_KEY.to_owned())))
            ),
            Some(TookOver::Successor(next)) if next.port == 2
        ));
        let running = running_owner().with_service_manager(true);
        assert!(took_over(
            &running,
            Some(KEY),
            true,
            Some((service_owner_on(2), Some(OTHER_KEY.to_owned())))
        )
        .is_none());
        assert!(matches!(
            took_over(
                &service_owner_on(1),
                Some(KEY),
                true,
                Some((owner_on(2), Some(OTHER_KEY.to_owned())))
            ),
            Some(TookOver::Other(other)) if other.port == 2
        ));
        assert!(matches!(
            took_over(
                &service_owner_on(1),
                Some(KEY),
                true,
                Some((unknown_owner_on(2), Some(OTHER_KEY.to_owned())))
            ),
            Some(TookOver::Other(other)) if other.port == 2
        ));
    }

    /// An unreadable key does not make a service-managed restart unproven: a daemon not recorded
    /// as service-managed is not taken for its successor.
    #[test]
    fn a_service_managed_restart_is_not_unproven_by_a_key_that_could_not_be_read() {
        assert!(matches!(
            took_over(&service_owner_on(1), None, true, Some((owner_on(2), None))),
            Some(TookOver::Other(other)) if other.port == 2
        ));
    }

    /// A record that does not say how the successor is started: any daemon that
    /// holds the lock after it is taken for its successor, as before the record said.
    #[test]
    fn a_restart_of_a_daemon_whose_record_does_not_say_is_taken_over_by_any_daemon() {
        for owner in [unknown_owner_on(1), running_owner()] {
            for (key, next_key) in [
                (Some(KEY), Some(OTHER_KEY.to_owned())),
                (None, None),
                (Some(KEY), Some(KEY.to_owned())),
            ] {
                assert!(matches!(
                    took_over(&owner, key, true, Some((owner_on(2), next_key))),
                    Some(TookOver::Successor(next)) if next.port == 2
                ));
            }
        }
    }

    /// Nothing has taken over while no daemon holds the lock, or none has recorded itself yet.
    #[test]
    fn a_restart_is_not_taken_over_without_a_recorded_daemon_holding_the_lock() {
        for owner in [owner_on(1), service_owner_on(1), unknown_owner_on(1)] {
            assert!(took_over(&owner, Some(KEY), true, None).is_none());
            assert!(took_over(&owner, Some(KEY), false, None).is_none());
            for next in [owner_on(2), service_owner_on(2)] {
                assert!(
                    took_over(&owner, Some(KEY), false, Some((next, Some(KEY.to_owned()))))
                        .is_none()
                );
            }
        }
    }

    /// The grace a service manager has runs from when the daemon was gone, the lock free and its
    /// process ended, not from when the lock was first seen free: the lock is released before the
    /// daemon's teardown, which has no bound.
    #[test]
    fn the_grace_runs_from_when_the_daemon_is_gone() {
        let start = Instant::now();
        let at = |secs| start + Duration::from_secs(secs);
        let grace = Duration::from_secs(3);
        let mut since = None;
        // The lock is free from the start, but the process runs for 10 seconds more.
        for secs in [0, 5, 9] {
            since = gone_since_after(since, at(secs), false);
            assert!(!grace_elapsed(since, at(secs), grace), "{secs}");
        }
        since = gone_since_after(since, at(10), true);
        assert_eq!(since, Some(at(10)));
        since = gone_since_after(since, at(12), true);
        assert_eq!(since, Some(at(10)));
        assert!(!grace_elapsed(since, at(12), grace));
        assert!(grace_elapsed(since, at(13), grace));
    }

    /// A lock held again, or a daemon not ended, drops the time since which the daemon was gone.
    #[test]
    fn the_time_gone_is_dropped_when_the_daemon_is_not_gone_again() {
        let start = Instant::now();
        let later = start + Duration::from_secs(2);
        let since = gone_since_after(None, start, true);
        assert_eq!(gone_since_after(since, later, false), None);
    }

    /// A note of a handover written `ago` by the daemon `from` names.
    fn noted(from: Owner, ago: Duration) -> HandoverNote {
        HandoverNote {
            from,
            noted: SystemTime::now() - ago,
        }
    }

    /// A daemon that noted it hands over and still runs may still see its successor take the
    /// lock, however long ago the note was written: it waits for that one to report listening.
    #[test]
    fn a_handover_is_pending_while_the_daemon_that_noted_it_runs() {
        let now = Instant::now();
        for from in [owner_on(1), service_owner_on(1), unknown_owner_on(1)] {
            let note = noted(from, Duration::from_secs(600));
            assert!(handover_pending(&note, true, None, now, SystemTime::now()));
        }
    }

    /// A daemon that starts its successor itself exits only once that one has reported listening,
    /// which comes after it recorded itself: once it has ended, no successor is on its way.
    #[test]
    fn a_handover_by_a_daemon_that_starts_its_successor_itself_is_over_once_it_has_ended() {
        let now = Instant::now();
        for from in [owner_on(1), unknown_owner_on(1)] {
            let note = noted(from, Duration::ZERO);
            assert!(!handover_pending(
                &note,
                false,
                Some(now),
                now,
                SystemTime::now()
            ));
        }
    }

    /// The successor a service manager starts comes after the daemon has exited: the handover is
    /// pending for the grace after the note was written, and for the grace after the daemon was
    /// seen to end; an old note, or one dated after now, holds nothing up.
    #[test]
    fn a_handover_to_a_service_managed_successor_is_pending_for_the_grace() {
        let now = Instant::now();
        let wall_now = SystemTime::now();
        let from = service_owner_on(1);
        let fresh = noted(from, Duration::from_secs(1));
        assert!(handover_pending(&fresh, false, None, now, wall_now));
        let old = noted(from, Duration::from_secs(60));
        assert!(!handover_pending(&old, false, None, now, wall_now));
        let later = now + Duration::from_secs(2);
        assert!(handover_pending(&old, false, Some(now), later, wall_now));
        let after_grace = now + SERVICE_RESTART_GRACE;
        assert!(!handover_pending(
            &old,
            false,
            Some(now),
            after_grace,
            wall_now
        ));
        let ahead = HandoverNote {
            from,
            noted: wall_now + Duration::from_secs(60),
        };
        assert!(!handover_pending(&ahead, false, None, now, wall_now));
    }

    /// The grace after the daemon ended runs from when this command saw it end, only for a daemon
    /// it saw running: one first seen ended counts from its note.
    #[test]
    fn the_watch_counts_the_grace_from_when_it_saw_the_daemon_end() {
        let now = Instant::now();
        let wall_now = SystemTime::now();
        let old = noted(service_owner_on(1), Duration::from_secs(60));
        let mut watch = HandoverWatch::default();
        assert!(
            !watch.pending(Some(&old), now, wall_now),
            "never seen running"
        );
        assert!(!watch.pending(None, now, wall_now), "no note");

        let running = noted(
            running_owner().with_service_manager(true),
            Duration::from_secs(60),
        );
        assert!(watch.pending(Some(&running), now, wall_now));
        // The same daemon, as its note reads once it has ended, is pending for the grace after
        // it was first seen ended.
        let mut watch = HandoverWatch {
            seen_running: Some(old.from.identity),
            gone_since: None,
        };
        assert!(watch.pending(Some(&old), now, wall_now));
        let later = now + Duration::from_secs(2);
        assert!(watch.pending(Some(&old), later, wall_now));
        assert!(!watch.pending(Some(&old), now + SERVICE_RESTART_GRACE, wall_now));
    }

    /// After the handover of a daemon whose successor a service manager starts, a daemon recorded
    /// as service-managed is its successor; after one that starts its successor itself, the daemon
    /// that holds the lock next is, as is any after a record that does not say. None is one that
    /// follows the application, which only a start of the application's makes.
    #[test]
    fn the_successor_after_a_handover_is_judged_by_the_records() {
        for next in [owner_on(2), service_owner_on(2), unknown_owner_on(2)] {
            assert!(handed_over_to(&owner_on(1), &next));
            assert!(handed_over_to(&unknown_owner_on(1), &next));
        }
        assert!(handed_over_to(&service_owner_on(1), &service_owner_on(2)));
        assert!(!handed_over_to(&service_owner_on(1), &unknown_owner_on(2)));
        assert!(!handed_over_to(&service_owner_on(1), &owner_on(2)));
        for from in [owner_on(1), service_owner_on(1), unknown_owner_on(1)] {
            let applications = Owner {
                follows_application: Some(true),
                ..service_owner_on(2)
            };
            assert!(!handed_over_to(&from, &applications));
        }
    }

    /// Only a daemon whose successor a service manager starts, which does so after its exit, has to
    /// end before the daemon holding the lock can be judged its successor.
    #[test]
    fn only_a_service_managed_daemon_has_to_end_before_its_successor_is_judged() {
        assert!(awaits_end(&service_owner_on(1), false));
        assert!(!awaits_end(&service_owner_on(1), true));
        for from in [owner_on(1), unknown_owner_on(1)] {
            assert!(!awaits_end(&from, false));
        }
    }

    /// A daemon found before a service-managed `from` ended is never its successor, whatever its
    /// record shows; any other is judged by its record, and nothing is judged while `from` runs.
    #[test]
    fn a_daemon_found_before_the_handing_over_one_ended_is_not_its_successor() {
        for handed_over in [false, true] {
            assert_eq!(judge_after(true, false, handed_over), Judged::Waiting);
            assert_eq!(judge_after(false, false, handed_over), Judged::Waiting);
            assert_eq!(judge_after(true, true, handed_over), Judged::Other);
        }
        assert_eq!(judge_after(false, true, true), Judged::Successor);
        assert_eq!(judge_after(false, true, false), Judged::Other);
    }

    /// A restarted daemon that has ended with the lock free and no handover of its own pending
    /// has no successor on its way; a pending one, or a lock held, says one may be.
    #[test]
    fn a_restart_is_abandoned_once_the_daemon_ended_with_the_lock_free_and_no_handover_pending() {
        for owner in [owner_on(1), service_owner_on(1)] {
            assert!(abandoned(&owner, false, true, false));
            assert!(!abandoned(&owner, false, true, true));
            assert!(!abandoned(&owner, true, true, false));
            assert!(!abandoned(&owner, false, false, false));
        }
    }

    /// A daemon that exited for a service manager that starts none (its note is its own) is
    /// abandoned once the grace is over, not held to the note for as long as the file lasts.
    #[test]
    fn a_service_managed_restart_with_no_manager_is_abandoned_once_the_grace_is_over() {
        let now = Instant::now();
        let wall_now = SystemTime::now();
        let own = noted(service_owner_on(1), Duration::from_secs(60));
        let mut watch = HandoverWatch {
            seen_running: Some(own.from.identity),
            gone_since: None,
        };
        assert!(watch.pending(Some(&own), now, wall_now));
        let later = now + SERVICE_RESTART_GRACE;
        let handing_over = watch.pending(Some(&own), later, wall_now);
        assert!(abandoned(&own.from, false, true, handing_over));
    }

    /// A daemon whose record does not say how its successor is started is of an earlier build,
    /// which notes no handover: a free lock does not show that none follows it.
    #[test]
    fn a_restart_of_an_earlier_build_is_never_taken_for_abandoned() {
        assert!(!abandoned(&unknown_owner_on(1), false, true, false));
    }
}
