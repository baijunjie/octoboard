//! One daemon at a time per data directory.
//!
//! Two daemons would own two sets of agent processes while sharing one database, and the second
//! one's startup would mark the first one's live sessions interrupted — the session list of a
//! perfectly healthy application would go stale in front of the user. Refusing to start is the
//! legible failure; the application surfaces the message below.

use std::fs::File;
use std::os::unix::io::AsRawFd;

use anyhow::{bail, Context, Result};

/// Holds the lock for as long as it is alive. The lock is released by the OS when the process
/// exits, so a crashed daemon leaves no stale lock behind.
pub struct InstanceLock {
    _file: File,
}

pub fn acquire() -> Result<InstanceLock> {
    let dir = crate::paths::data_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join("daemon.lock");
    let file = File::create(&path).with_context(|| format!("opening {}", path.display()))?;

    // SAFETY: the descriptor belongs to `file`, which outlives this call.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
            bail!(
                "another octoboardd is already running against {}",
                dir.display()
            );
        }
        return Err(err).with_context(|| format!("locking {}", path.display()));
    }
    Ok(InstanceLock { _file: file })
}
