//! Ends a Unix server's process group when the process that owns it disappears. The watcher is
//! forked after the server has led its session but before its program runs, so it pins that
//! group's id and covers an owner killed while `spawn` is still in progress. A second fork and
//! the intermediate child's reap leave the watcher outside the server's children.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::platform::descriptors;

/// The only write end of the watcher's pipe. Closing it also covers an orderly owner exit.
pub(super) struct Guard {
    _write: OwnedFd,
}

impl Guard {
    /// Registers after the command's `setsid` hook. The watcher holds no inherited descriptors
    /// except its read end, and runs only async-signal-safe calls after the fork.
    pub(super) fn prepare(command: &mut Command) -> io::Result<Self> {
        let (read, write) = pipe()?;
        let write_fd = write.as_raw_fd();
        let close_until = descriptors::bound()?;
        // SAFETY: the hooks run in order, so the child already leads its group. The child and
        // its watcher only call plain libc functions before exec or _exit; no Rust locks or
        // destructors are used on the watcher path.
        unsafe {
            command.pre_exec(move || {
                libc::close(write_fd);
                let mut signals: libc::sigset_t = std::mem::zeroed();
                let mut previous: libc::sigset_t = std::mem::zeroed();
                libc::sigfillset(&mut signals);
                if libc::sigprocmask(libc::SIG_SETMASK, &signals, &mut previous) != 0 {
                    return Err(io::Error::last_os_error());
                }
                let child = libc::fork();
                if child == 0 {
                    match libc::fork() {
                        -1 => libc::_exit(1),
                        0 => watch(read.as_raw_fd(), close_until),
                        _ => libc::_exit(0),
                    }
                }
                let fork_error = if child == -1 {
                    Some(io::Error::last_os_error())
                } else {
                    let mut status = 0;
                    loop {
                        if libc::waitpid(child, &mut status, 0) >= 0 {
                            break;
                        }
                        let error = io::Error::last_os_error();
                        if error.raw_os_error() != Some(libc::EINTR) {
                            return Err(error);
                        }
                    }
                    if libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0 {
                        None
                    } else {
                        Some(io::Error::from_raw_os_error(libc::EIO))
                    }
                };
                if libc::sigprocmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut()) != 0 {
                    return Err(io::Error::last_os_error());
                }
                match fork_error {
                    Some(error) => Err(error),
                    None => {
                        libc::close(read.as_raw_fd());
                        Ok(())
                    }
                }
            });
        }
        Ok(Self { _write: write })
    }
}

fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let (read, write) = io::pipe()?;
    let ends: [OwnedFd; 2] = [read.into(), write.into()];
    let duplicate = |fd: &OwnedFd| {
        // Keep both ends above stdio, which Command replaces in the child. CLOEXEC makes the
        // owner's write end unavailable to any program it starts concurrently.
        // SAFETY: `fd` is open, and F_DUPFD_CLOEXEC returns a new owned descriptor.
        let raw = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if raw < 0 {
            Err(io::Error::last_os_error())
        } else {
            // SAFETY: only this value owns the successful duplication.
            Ok(unsafe { OwnedFd::from_raw_fd(raw) })
        }
    };
    Ok((duplicate(&ends[0])?, duplicate(&ends[1])?))
}

/// Never returns to Rust after the fork. Its membership reserves the group id until the kill,
/// even when the server already exited and the owner has not observed that exit yet.
unsafe fn watch(read: RawFd, close_until: RawFd) -> ! {
    // SAFETY: all operations below use local buffers and descriptors inherited by this process.
    // Block catchable signals so a polite stop of an ignoring server cannot end its watcher.
    unsafe {
        if libc::dup2(read, libc::STDIN_FILENO) < 0 {
            libc::kill(-libc::getpgrp(), libc::SIGKILL);
            libc::_exit(1);
        }
        descriptors::close_from(1, close_until);
        let mut byte = 0u8;
        loop {
            let count = libc::read(libc::STDIN_FILENO, (&mut byte as *mut u8).cast(), 1);
            if count <= 0 {
                break;
            }
        }
        libc::kill(-libc::getpgrp(), libc::SIGKILL);
        libc::_exit(0);
    }
}
