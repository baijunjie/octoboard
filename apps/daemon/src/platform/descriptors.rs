//! Closing, or marking close-on-exec, the descriptors a forked child inherited and must not keep,
//! for the Unix hosts. The bound is worked out before the fork, since a child may not look it up,
//! and the closing is made of system calls alone.

use std::io;
use std::os::fd::RawFd;

/// The most descriptors a child walks one by one where nothing better bounds them: a soft limit
/// can be set to a billion.
const MOST: RawFd = 1 << 16;

/// The descriptor number above the highest one in the listing of this process's open descriptors,
/// or `None` where the listing cannot be read. Allocates and reads the file system.
fn listed_bound() -> Option<RawFd> {
    #[cfg(target_os = "linux")]
    let descriptors = "/proc/self/fd";
    #[cfg(not(target_os = "linux"))]
    let descriptors = "/dev/fd";
    let entries = std::fs::read_dir(descriptors).ok()?;
    Some(
        entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse::<RawFd>().ok())
            .map(|fd| fd.saturating_add(1))
            .max()
            .unwrap_or(0),
    )
}

/// The descriptor number above every descriptor this process can open now: the current
/// allocation limit, raised to cover the open descriptors where the snapshot of them can be read,
/// which also takes in those that were already open when that limit was lowered. Fails only where
/// the limit is not known, as nothing can then be said to be closed. Allocates and reads the file
/// system, so it is for the parent, ahead of a fork; [`close_from`] takes the result.
pub(crate) fn bound() -> io::Result<RawFd> {
    // SAFETY: sysconf only reads the process's descriptor allocation limit.
    let limit = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
    let bound = RawFd::try_from(limit)
        .ok()
        .filter(|limit| *limit > 0)
        .ok_or_else(|| io::Error::other("the descriptor limit is not known"))?;
    Ok(bound.max(listed_bound().unwrap_or(0)))
}

/// The descriptor number above the descriptors this process has open or may open meanwhile, for
/// [`mark_close_on_exec_from`] to walk below where it has no `close_range`: the higher of the
/// highest one in the listing of them, which covers those above the soft limit too, and the soft
/// limit ([`bound`]) capped at [`MOST`], and [`MOST`] where the limit is not known, so that a start
/// does not fail for it. The limit is in the bound because the listing is a snapshot: a descriptor
/// another thread opens after it (a pipe or a terminal, say) is below the limit but not in it, and
/// is still open without close-on-exec until its opener sets the flag. Allocates and reads the
/// file system, so it is for the parent, ahead of a fork.
pub(crate) fn open_bound() -> RawFd {
    let limit = bound().map_or(MOST, |bound| bound.min(MOST));
    limit.max(listed_bound().unwrap_or(0))
}

/// Closes every descriptor from `first` up, in the child of a fork: system calls alone, so that
/// nothing allocates. Linux closes the whole range at once; older kernels and other hosts close
/// the descriptors below `bound` ([`bound`], taken before the fork) one by one.
///
/// # Safety
///
/// The caller owns the descriptor table, as a forked child does, and nothing in it still uses a
/// descriptor from `first` up.
pub(crate) unsafe fn close_from(first: RawFd, bound: RawFd) {
    // SAFETY: closes descriptors of this process only; no memory is involved.
    unsafe {
        #[cfg(target_os = "linux")]
        if libc::syscall(
            libc::SYS_close_range,
            first as libc::c_uint,
            libc::c_uint::MAX,
            0 as libc::c_uint,
        ) == 0
        {
            return;
        }
        for fd in first..bound {
            libc::close(fd);
        }
    }
}

/// Makes every descriptor from `first` up close on `exec`, in the child of a fork: system calls
/// alone, so that nothing allocates. Linux marks the whole range at once (`close_range` with
/// `CLOSE_RANGE_CLOEXEC`, from 5.11); older kernels and other hosts mark the descriptors below
/// `bound` ([`open_bound`], taken before the fork) one by one.
///
/// # Safety
///
/// The caller owns the descriptor table, as a forked child does, and nothing in it relies on a
/// descriptor from `first` up staying open across the `exec` that follows.
pub(crate) unsafe fn mark_close_on_exec_from(first: RawFd, bound: RawFd) {
    // SAFETY: changes the flags of descriptors of this process only; no memory is involved, and a
    // descriptor that is not open only makes the call fail.
    unsafe {
        #[cfg(target_os = "linux")]
        if libc::syscall(
            libc::SYS_close_range,
            first as libc::c_uint,
            libc::c_uint::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        ) == 0
        {
            return;
        }
        for fd in first..bound {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    use crate::platform::test_helper::{self, ROLE};

    const MARKER: &str = "descriptor_marker";

    /// The process that marks descriptors close-on-exec from a descriptor up, run as `ROLE` says
    /// and a no-op in an ordinary test run. It is a process of its own so that the descriptors it
    /// marks are not the test process's, whose other tests open descriptors that must stay
    /// inheritable.
    #[test]
    fn a_descriptor_marker() {
        if std::env::var(ROLE).as_deref() != Ok(MARKER) {
            return;
        }
        let (reader, writer) = std::io::pipe().expect("a pipe");
        // SAFETY: duplicates a descriptor this process owns; the copies are closed on exit.
        let copy = unsafe { libc::fcntl(writer.as_raw_fd(), libc::F_DUPFD, 0) };
        assert!(copy >= 0);
        // SAFETY: the copy is open and owned here.
        let copy = unsafe { OwnedFd::from_raw_fd(copy) };
        let bound = open_bound();
        assert!(bound > copy.as_raw_fd() && bound > reader.as_raw_fd());
        // SAFETY: reads the flags of a descriptor that is open for as long as this call.
        let flag = |fd: RawFd| unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC;
        assert_eq!(flag(copy.as_raw_fd()), 0);
        // SAFETY: only flags change, and nothing here needs the descriptor across an exec.
        unsafe { mark_close_on_exec_from(copy.as_raw_fd(), bound) };
        assert_ne!(flag(copy.as_raw_fd()), 0);
    }

    /// The bound is above the descriptors open by now, and marking from a descriptor up makes it
    /// close on `exec`, in a process started for the purpose ([`a_descriptor_marker`]).
    #[test]
    fn marking_from_a_descriptor_up_makes_it_close_on_exec() {
        let status = std::process::Command::new(test_helper::program())
            .args(test_helper::args(
                "platform::descriptors::tests::a_descriptor_marker",
            ))
            .env(ROLE, MARKER)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("the marker runs");
        assert!(status.success(), "the marker failed: {status}");
    }
}
