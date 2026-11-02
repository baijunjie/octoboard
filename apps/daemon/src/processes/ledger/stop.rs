//! Stopping processes by identity: never by name, port or look, and never a pid that has passed to
//! another process; and the ledger's rounds of signals, which go on to what started from a target
//! while it was being stopped.

use std::borrow::Cow;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::attribute::Book;
use super::table::{Identity, Table};
use super::ProcessLedger;

/// How long a process asked to stop has before it is killed.
pub(super) const STOP_GRACE: Duration = Duration::from_secs(3);

#[cfg(test)]
thread_local! {
    /// Pids a `SIGKILL` from this thread is taken to have been sent to but is not, so that a
    /// test can have a process outlive one, as a process in an uninterruptible sleep does.
    pub(super) static SPARED: std::cell::RefCell<std::collections::HashSet<i32>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Sends `SIGTERM` to each of `targets` still running, waits up to [`STOP_GRACE`] for them to go,
/// then sends `SIGKILL` to what is left that `kept` does not name by then, and waits a moment
/// for that to land. Answers what was still running after its `SIGKILL`. **Blocks.**
pub(super) fn stop(targets: &[Identity], kept: &dyn Fn(&Identity) -> bool) -> Vec<Identity> {
    let mut left: Vec<Identity> = targets
        .iter()
        .copied()
        .filter(|id| signal(id, libc::SIGTERM))
        .collect();
    let deadline = Instant::now() + STOP_GRACE;
    while !left.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        left.retain(Identity::is_running);
    }
    // Kept during the grace: the user's now, whatever it was asked to do before.
    left.retain(|id| !kept(id) && signal(id, libc::SIGKILL));
    let deadline = Instant::now() + Duration::from_secs(1);
    while !left.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        left.retain(Identity::is_running);
    }
    left
}

/// Sends `signal` to `id` if it is still running, and says whether it was sent. The start time is
/// read just before the signal, so a pid that has passed to another process is not signalled; the
/// moment between the read and the signal is the only window left, and a pid is not handed out
/// again that fast. Pid 1 and the daemon itself are never signalled, whatever asks.
pub(super) fn signal(id: &Identity, signal: libc::c_int) -> bool {
    if id.pid <= 1 || id.pid == std::process::id() as i32 || !id.is_running() {
        return false;
    }
    #[cfg(test)]
    if signal == libc::SIGKILL && SPARED.with_borrow(|spared| spared.contains(&id.pid)) {
        return true;
    }
    // SAFETY: a plain signal to a pid that had the identity's start time a moment ago, as above.
    unsafe { libc::kill(id.pid, signal) == 0 }
}

impl ProcessLedger {
    /// One round of signals to `targets` (see [`stop`]): a process kept meanwhile is not
    /// killed, and one still running after its `SIGKILL` is left for
    /// [`STUBBORN_RETRY`](super::STUBBORN_RETRY), with one warning. **Blocks.**
    pub(super) fn signal_round(&self, targets: &[Identity]) {
        let survivors = stop(targets, &|id| self.book().kept.contains(id));
        let mut stubborn = self
            .stubborn
            .lock()
            .expect("stubborn processes lock poisoned");
        let now = Instant::now();
        for id in survivors {
            if stubborn.insert(id, Some(now)).is_none() {
                tracing::warn!(
                    pid = id.pid,
                    "a process was still running after SIGKILL; it is tried again only after a while"
                );
            }
        }
    }

    /// Stops `targets`, read in `before`, then, a few times, what started meanwhile that is
    /// attributed to the same launch as a target and descends from one: through its chain of
    /// parents, or by sitting in a terminal session a target led. A kept process is left out
    /// just before each round of signals. **Blocks.**
    pub(super) fn stop_set(&self, mut targets: Vec<Identity>, before: &Table) {
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
        let mut led: HashSet<i32> = HashSet::new();
        let mut known: HashSet<Identity> = before.procs().map(|proc| proc.id).collect();
        let mut current = Cow::Borrowed(before);
        for _ in 0..super::STOP_PASSES {
            {
                let book = self.book();
                targets.retain(|id| !book.kept.contains(id));
            }
            if targets.is_empty() {
                return;
            }
            led.extend(
                targets
                    .iter()
                    .filter(|id| {
                        current
                            .get(id.pid)
                            .is_some_and(|proc| proc.id == **id && proc.sid == id.pid)
                    })
                    .map(|id| id.pid),
            );
            self.signal_round(&targets);
            stopped.extend(targets.iter().copied());
            let Some(table) = self.scan(false) else {
                return;
            };
            targets = follow_ups(&self.book(), &table, &known, &stopped, &led, &launches);
            known.extend(table.procs().map(|proc| proc.id));
            current = Cow::Owned(table);
        }
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
    led: &HashSet<i32>,
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
            led.contains(&proc.sid)
                || table
                    .ancestors(proc)
                    .any(|ancestor| stopped.contains(&ancestor.id))
        })
        .map(|proc| proc.id)
        .collect()
}
