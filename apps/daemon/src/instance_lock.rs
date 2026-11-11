//! One daemon at a time per data directory.
//!
//! Two daemons would own two sets of agent processes while sharing one database, and the second
//! one's startup would mark the first one's live sessions interrupted — the session list of a
//! perfectly healthy application would go stale in front of the user. Refusing to start is the
//! legible failure; the application surfaces the message below.
//!
//! A user-run command finds a running daemon without talking to it: [`is_running`] says whether one
//! runs, which only the lock can say; which process it is, where it listens and what key it
//! requires is in [`crate::owner_record`].

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::platform::fs::Busy;

/// How long [`acquire`] keeps trying while the lock is held. [`is_running`] holds the lock for
/// microseconds to find out, while a daemon holds it for its lifetime, so a lock still held after
/// this is a running daemon, not a probe that a start would otherwise be refused by.
const ACQUIRE_WAIT: Duration = Duration::from_secs(1);

/// How long [`acquire`] sleeps between two tries while the lock is held.
const ACQUIRE_STEP: Duration = Duration::from_millis(20);

/// Holds the lock for as long as it is alive. The lock is released by the OS when the process
/// exits, so a crashed daemon leaves no stale lock behind.
pub struct InstanceLock {
    _file: File,
}

pub fn acquire() -> Result<InstanceLock> {
    let dir = crate::paths::data_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    // The files written below hold keys and session data; where the host makes them private
    // through the directory, an unprivate one is still written, so the daemon runs on.
    if let Err(err) = crate::platform::fs::keep_private_below(&dir) {
        tracing::warn!(path = %dir.display(), %err, "making the data directory private failed");
    }
    let path = lock_path();
    match lock_waiting(&path, ACQUIRE_WAIT, ACQUIRE_STEP) {
        Ok(file) => Ok(InstanceLock { _file: file }),
        Err(Busy::Held) => bail!(
            "another octoboardd is already running against {}",
            dir.display()
        ),
        Err(Busy::Open(err)) => Err(err).with_context(|| format!("opening {}", path.display())),
        Err(Busy::Lock(err)) => Err(err).with_context(|| format!("locking {}", path.display())),
    }
}

/// Takes the lock at `path`, trying again every `step` for up to `wait` while it is merely held.
fn lock_waiting(path: &Path, wait: Duration, step: Duration) -> Result<File, Busy> {
    let deadline = Instant::now() + wait;
    loop {
        match crate::platform::fs::try_lock_file(path) {
            Err(Busy::Held) if Instant::now() < deadline => std::thread::sleep(step),
            taken => return taken,
        }
    }
}

fn lock_path() -> PathBuf {
    crate::paths::data_dir().join("daemon.lock")
}

/// Whether a daemon runs against this data directory, which the lock alone says: a crashed daemon
/// leaves no lock behind, and a record of its own could outlive it. The lock is taken shared to
/// find out and released at once, so that commands looking at once do not take each other for a
/// daemon; a daemon starting in that instant waits for it (see [`acquire`]).
pub fn is_running() -> Result<bool> {
    is_running_at(&lock_path())
}

fn is_running_at(path: &Path) -> Result<bool> {
    match crate::platform::fs::try_lock_file_shared(path) {
        Ok(_file) => Ok(false),
        Err(Busy::Held) => Ok(true),
        // No data directory yet: no daemon has ever run here, and a status check creates nothing.
        Err(Busy::Open(err)) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(Busy::Open(err)) => Err(err).with_context(|| format!("opening {}", path.display())),
        Err(Busy::Lock(err)) => Err(err).with_context(|| format!("locking {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    #[test]
    fn a_held_lock_is_a_running_daemon_and_a_free_or_missing_one_is_not() {
        let dir = ScratchDir::new("is-running");
        let path = dir.join("daemon.lock");
        assert!(!is_running_at(&path).unwrap(), "no daemon has run here yet");

        let held = crate::platform::fs::try_lock_file(&path).expect("the lock was taken");
        assert!(is_running_at(&path).unwrap());
        drop(held);
        // A process another test is starting may hold the descriptor between its fork and the
        // exec that closes it.
        let released = crate::session::wait::wait_for(crate::test_support::PATIENCE, || {
            !is_running_at(&path).unwrap()
        });
        assert!(released, "the lock was released");

        // Another command looking at the same moment is not a daemon.
        let looking = crate::platform::fs::try_lock_file_shared(&path).expect("a look");
        assert!(
            !is_running_at(&path).unwrap(),
            "a look was taken for a daemon"
        );
        drop(looking);

        let missing = dir.join("absent").join("daemon.lock");
        assert!(!is_running_at(&missing).unwrap());
        assert!(
            !missing.parent().unwrap().exists(),
            "a check creates nothing"
        );
    }

    /// A probe holds the lock for a moment: a start in that moment gets it once it is released.
    #[test]
    fn a_lock_held_for_a_moment_is_taken_once_released() {
        let dir = ScratchDir::new("acquire-probe");
        let path = dir.join("daemon.lock");
        let held = crate::platform::fs::try_lock_file_shared(&path).expect("the probe's lock");
        let hold = Duration::from_millis(50);
        // Before the holder starts, so its hold is all inside the wait.
        let began = Instant::now();
        let holder = std::thread::spawn(move || {
            std::thread::sleep(hold);
            drop(held);
        });
        let taken = lock_waiting(&path, Duration::from_secs(30), Duration::from_millis(5));
        let waited = began.elapsed();
        holder.join().unwrap();
        assert!(taken.is_ok(), "the lock was free after the moment");
        assert!(waited >= hold, "the lock was taken while it was held");
    }

    /// A daemon holds the lock throughout: a start is still refused, after the wait.
    #[test]
    fn a_lock_held_throughout_is_refused_after_the_wait() {
        let dir = ScratchDir::new("acquire-held");
        let path = dir.join("daemon.lock");
        let _held = crate::platform::fs::try_lock_file(&path).expect("the lock was taken");
        let began = Instant::now();
        let wait = Duration::from_millis(100);
        let refused = lock_waiting(&path, wait, Duration::from_millis(5));
        assert!(matches!(refused, Err(Busy::Held)));
        assert!(began.elapsed() >= wait, "it waited the whole window");
    }
}
