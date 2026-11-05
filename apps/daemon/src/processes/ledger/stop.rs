//! Stopping processes by identity: never by name, port or look, and never a pid that has passed to
//! another process; and the ledger's rounds of signals, which go on to what started from a target
//! while it was being stopped.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::attribute::Book;
use super::table::{Identity, Table};
use super::ProcessLedger;
use crate::platform::proc::{self, ContainerId, Polite, Sent, Signal};
use crate::protocol::UnstoppedReason;

/// How long a process asked to stop has before it is killed.
pub(super) const STOP_GRACE: Duration = Duration::from_secs(3);

#[cfg(test)]
thread_local! {
    /// Pids a kill from this thread is taken to have been sent to but is not, so that a
    /// test can have a process outlive one, as a process in an uninterruptible sleep does.
    pub(super) static SPARED: std::cell::RefCell<std::collections::HashSet<i32>> =
        std::cell::RefCell::new(std::collections::HashSet::new());

    /// Pids a signal from this thread is refused to for lack of permission, as one to a process
    /// running with more rights than the daemon is, so that a test can have a stop refused.
    pub(super) static DENIED: std::cell::RefCell<std::collections::HashSet<i32>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// A process still running once a stop was through with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Unstopped {
    pub id: Identity,
    pub reason: UnstoppedReason,
    /// The target of the stop's first round it was stopped with: itself, or the one it descends
    /// from when it started while that was being stopped.
    pub origin: Identity,
    /// Its name, as the table the round read it in gives it.
    pub name: String,
}

/// Asks each of `targets` still running to terminate, waits up to [`STOP_GRACE`] for those that
/// were asked to go, then kills what is left that `kept` does not name by then, and waits a moment
/// for that to land. A target the host has no polite request for is killed after the grace with
/// the rest. Answers what was still running after its kill, and what the system refused to kill
/// that is still running, each with why. **Blocks.**
pub(super) fn stop(
    targets: &[Identity],
    kept: &dyn Fn(&Identity) -> bool,
) -> Vec<(Identity, UnstoppedReason)> {
    let mut asked: Vec<Identity> = targets
        .iter()
        .copied()
        .filter(|id| signal(id, Signal::Polite(Polite::Terminate)))
        .collect();
    let deadline = Instant::now() + STOP_GRACE;
    while !asked.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        asked.retain(Identity::is_running);
    }
    // Kept during the grace: the user's now, whatever it was asked to do before. A target that
    // has gone meanwhile is not signalled, as `send` reads its start time first.
    let mut left: Vec<Identity> = Vec::new();
    let mut refused: Vec<(Identity, UnstoppedReason)> = Vec::new();
    for id in targets.iter().copied().filter(|id| !kept(id)) {
        match send(&id, Signal::Kill) {
            Some(Sent::Delivered) => left.push(id),
            Some(Sent::Failed(err)) => {
                let reason = if err.kind() == std::io::ErrorKind::PermissionDenied {
                    UnstoppedReason::AccessDenied
                } else {
                    UnstoppedReason::Other
                };
                tracing::debug!(pid = id.pid.0, %err, "the system refused to kill a process");
                refused.push((id, reason));
            }
            Some(Sent::Gone) | None => {}
        }
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    while !left.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        left.retain(Identity::is_running);
    }
    left.into_iter()
        .map(|id| (id, UnstoppedReason::Other))
        .chain(refused.into_iter().filter(|(id, _)| id.is_running()))
        .collect()
}

/// Sends `signal` to `id` if it is still running, and says whether it was sent. The start time is
/// read just before the signal, so a pid that has passed to another process is not signalled;
/// where the host signals by pid alone, the moment between the read and the signal is the only
/// window left, and a pid is not handed out again that fast. Pid 1 and the daemon itself are never
/// signalled, whatever asks.
pub(super) fn signal(id: &Identity, signal: Signal) -> bool {
    send(id, signal).is_some_and(|sent| sent.delivered())
}

/// What came of sending `signal` to `id` (see [`signal`]); `None` when it was not sent, the
/// process being gone already or one never signalled.
fn send(id: &Identity, signal: Signal) -> Option<Sent> {
    if id.pid.0 <= 1 || id.pid.0 == std::process::id() || !id.is_running() {
        return None;
    }
    #[cfg(test)]
    if DENIED.with_borrow(|denied| denied.contains(&(id.pid.0 as i32))) {
        return Some(Sent::Failed(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        )));
    }
    #[cfg(test)]
    if signal == Signal::Kill && SPARED.with_borrow(|spared| spared.contains(&(id.pid.0 as i32))) {
        return Some(Sent::Delivered);
    }
    Some(proc::signal_identified(id, signal))
}

impl ProcessLedger {
    /// One round of signals to `targets` (see [`stop`]): a process kept meanwhile is not
    /// killed, and one still running after its kill, or that the system refused to kill, is left
    /// for [`STUBBORN_RETRY`](super::STUBBORN_RETRY), with one warning. Answers those, with why.
    /// **Blocks.**
    pub(super) fn signal_round(&self, targets: &[Identity]) -> Vec<(Identity, UnstoppedReason)> {
        let survivors = stop(targets, &|id| self.book().kept.contains(id));
        let mut stubborn = self
            .stubborn
            .lock()
            .expect("stubborn processes lock poisoned");
        let now = Instant::now();
        for (id, reason) in &survivors {
            if stubborn.insert(*id, Some(now)).is_none() {
                match reason {
                    UnstoppedReason::AccessDenied => tracing::warn!(
                        pid = id.pid.0,
                        "the system refused to kill a process for lack of permission; it is tried again only after a while"
                    ),
                    UnstoppedReason::Other => tracing::warn!(
                        pid = id.pid.0,
                        "a process was still running after it was killed; it is tried again only after a while"
                    ),
                }
            }
        }
        survivors
    }

    /// Stops `targets`, read in `before`, then, a few times, what started meanwhile that is
    /// attributed to the same launch as a target and descends from one: through its chain of
    /// parents, or by sitting in a terminal session a target led. A kept process is left out
    /// just before each round of signals. Answers what was still running after its round (see
    /// [`Self::signal_round`]) and still is at the end. **Blocks.**
    pub(super) fn stop_set(&self, mut targets: Vec<Identity>, before: &Table) -> Vec<Unstopped> {
        let launches: HashSet<String> = {
            let book = self.book();
            targets
                .iter()
                .filter_map(|id| {
                    book.attributed
                        .get(id)
                        .map(|attributed| attributed.launch.clone())
                })
                .collect()
        };
        let mut stopped: HashSet<Identity> = HashSet::new();
        let mut led: HashSet<ContainerId> = HashSet::new();
        let mut known: HashSet<Identity> = before.procs().map(|proc| proc.id).collect();
        // The target of the first round each process signalled descends from, and the one each
        // terminal session in `led` was led by.
        let mut origins: HashMap<Identity, Identity> =
            targets.iter().map(|id| (*id, *id)).collect();
        let mut led_origins: HashMap<ContainerId, Identity> = HashMap::new();
        let mut unstopped: Vec<Unstopped> = Vec::new();
        let mut current = Cow::Borrowed(before);
        for _ in 0..super::STOP_PASSES {
            {
                let book = self.book();
                targets.retain(|id| !book.kept.contains(id));
            }
            if targets.is_empty() {
                break;
            }
            for id in &targets {
                let Some(sid) = current
                    .get(id.pid)
                    .filter(|proc| proc.id == *id)
                    .and_then(|proc| proc.sid)
                    .filter(|sid| sid.leader() == id.pid)
                else {
                    continue;
                };
                led.insert(sid);
                led_origins.entry(sid).or_insert(origins[id]);
            }
            for (id, reason) in self.signal_round(&targets) {
                unstopped.push(Unstopped {
                    id,
                    reason,
                    origin: origins[&id],
                    name: current
                        .get(id.pid)
                        .filter(|proc| proc.id == id)
                        .map(|proc| proc.comm.clone())
                        .unwrap_or_default(),
                });
            }
            stopped.extend(targets.iter().copied());
            let Some(table) = self.scan(false) else {
                break;
            };
            targets = follow_ups(&self.book(), &table, &known, &stopped, &led, &launches);
            for id in &targets {
                let origin = table.get(id.pid).and_then(|proc| {
                    table
                        .ancestors(proc)
                        .find_map(|ancestor| origins.get(&ancestor.id).copied())
                        .or_else(|| proc.sid.and_then(|sid| led_origins.get(&sid).copied()))
                });
                // There is one, as `follow_ups` found it through one or the other.
                origins.insert(*id, origin.unwrap_or(*id));
            }
            known.extend(table.procs().map(|proc| proc.id));
            current = Cow::Owned(table);
        }
        unstopped.retain(|unstopped| unstopped.id.is_running());
        unstopped
    }
}

/// What a stop goes on to after a round of signals: the processes of `table` not `known` before
/// that are attributed to one of `launches` and descend from what was `stopped`, through their
/// chain of parents or by sitting in a terminal session one of those `led`.
pub(super) fn follow_ups(
    book: &Book,
    table: &Table,
    known: &HashSet<Identity>,
    stopped: &HashSet<Identity>,
    led: &HashSet<ContainerId>,
    launches: &HashSet<String>,
) -> Vec<Identity> {
    table
        .procs()
        .filter(|proc| {
            !known.contains(&proc.id)
                && book
                    .attributed
                    .get(&proc.id)
                    .is_some_and(|attributed| launches.contains(&attributed.launch))
        })
        .filter(|proc| {
            proc.sid.is_some_and(|sid| led.contains(&sid))
                || table
                    .ancestors(proc)
                    .any(|ancestor| stopped.contains(&ancestor.id))
        })
        .map(|proc| proc.id)
        .collect()
}
