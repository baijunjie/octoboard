//! Stopping processes by identity: never by name, port or look, and never a pid that has passed to
//! another process.

use std::time::{Duration, Instant};

use super::table::Identity;

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
