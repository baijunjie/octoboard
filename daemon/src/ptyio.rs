//! Raw reads and writes on a PTY master file descriptor.
//!
//! Why this exists rather than `portable_pty`'s `Read`/`Write` handles: a macOS PTY master accepts
//! only about 1022 bytes before returning `EAGAIN` while the child is not draining, so a message
//! written with one blocking `write_all` parks whatever thread issues it for as long as the child
//! takes to catch up — potentially forever, if a modal dialog is up. The write path here is a
//! non-blocking partial-write-and-retry loop in slices with a deadline instead (see "Writing into
//! a running session" in `docs/mvp.md` section 6).
//!
//! The descriptor is put into non-blocking mode once, which is a property of the open file
//! description and therefore applies to reads as well — hence `read_chunk`, which waits for
//! readability with `poll()` instead of blocking in `read()`.

use std::io;
use std::os::unix::io::RawFd;
use std::time::{Duration, Instant};

/// Slice size for one `write()` attempt. Deliberately below the ~1022 bytes a macOS PTY master
/// accepts in one go, so a slice either goes in whole or the descriptor is genuinely full.
const WRITE_SLICE: usize = 512;

/// How long a write may keep retrying before giving up. A child that has not drained a single
/// slice in this long is not going to: either it is sitting at a modal dialog (where nothing may
/// be written anyway) or it is wedged, and the caller is better off being told.
const WRITE_DEADLINE: Duration = Duration::from_secs(5);

pub fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: `fd` is a descriptor the caller owns for the lifetime of this call.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Waits for the descriptor to become readable, then reads once. Returns `Ok(0)` for end of
/// stream, which on a PTY master is either a zero-length read or `EIO` once the child is gone.
pub fn read_chunk(fd: RawFd, buf: &mut [u8]) -> io::Result<usize> {
    loop {
        match poll_once(fd, libc::POLLIN, -1) {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
        // SAFETY: `buf` is a valid mutable slice and the length passed matches it.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n >= 0 {
            return Ok(n as usize);
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EAGAIN) | Some(libc::EINTR) => continue,
            // The child is gone: macOS reports the closed slave side as EIO rather than EOF.
            Some(libc::EIO) => return Ok(0),
            _ => return Err(err),
        }
    }
}

/// A write that did not finish, and how far it got. The count matters to a caller deciding whether
/// to retry: the bytes already accepted are in the child's input buffer, so writing the whole
/// message again would deliver that prefix twice.
#[derive(Debug)]
pub struct PartialWrite {
    pub written: usize,
    pub error: io::Error,
}

impl std::fmt::Display for PartialWrite {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} ({} bytes written)", self.error, self.written)
    }
}

impl std::error::Error for PartialWrite {}

/// Writes every byte of `data`, in slices, never blocking: a full descriptor is waited on with
/// `poll()` and the partial write resumed. Fails with `TimedOut` if the child has not accepted
/// anything for `WRITE_DEADLINE`.
pub fn write_all(fd: RawFd, data: &[u8]) -> Result<(), PartialWrite> {
    let mut offset = 0;
    let mut deadline = Instant::now() + WRITE_DEADLINE;
    while offset < data.len() {
        let end = (offset + WRITE_SLICE).min(data.len());
        let slice = &data[offset..end];
        // SAFETY: `slice` is a valid slice and the length passed matches it.
        let n = unsafe { libc::write(fd, slice.as_ptr() as *const libc::c_void, slice.len()) };
        if n > 0 {
            offset += n as usize;
            // Progress resets the deadline: a large message written to a child that drains slowly
            // but steadily is fine, it is a child draining nothing at all that has to be given up
            // on.
            deadline = Instant::now() + WRITE_DEADLINE;
            continue;
        }
        if n == 0 {
            continue;
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EAGAIN) | Some(libc::EINTR) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(PartialWrite {
                        written: offset,
                        error: io::Error::new(
                            io::ErrorKind::TimedOut,
                            "the session did not accept input within the write deadline",
                        ),
                    });
                }
                let timeout_ms = remaining.as_millis().min(i32::MAX as u128) as i32;
                match poll_once(fd, libc::POLLOUT, timeout_ms) {
                    Ok(_) => {}
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                    Err(err) => {
                        return Err(PartialWrite {
                            written: offset,
                            error: err,
                        })
                    }
                }
            }
            _ => {
                return Err(PartialWrite {
                    written: offset,
                    error: err,
                })
            }
        }
    }
    Ok(())
}

/// One `poll()` on a single descriptor. `timeout_ms` of -1 waits indefinitely.
fn poll_once(fd: RawFd, events: libc::c_short, timeout_ms: i32) -> io::Result<()> {
    let mut pollfd = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    // SAFETY: a single valid `pollfd` is passed with a matching count of 1.
    let rc = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::os::unix::io::{FromRawFd, RawFd};
    use std::time::{Duration, Instant};

    use super::{set_nonblocking, write_all};

    /// The write path's whole reason for existing: the destination stops accepting bytes partway
    /// through and the write has to wait it out and resume rather than fail or park a thread
    /// forever. A pipe reproduces that deterministically — a PTY master's own threshold depends on
    /// the child's terminal settings — and `write_all` is not PTY-specific.
    #[test]
    fn a_large_message_resumes_after_the_destination_fills_up() {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: a two-element array is exactly what `pipe` writes into.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "pipe()");
        let (read_fd, write_fd) = (fds[0], fds[1]);
        set_nonblocking(write_fd).expect("non-blocking mode");

        // 256 KiB against a pipe buffer of a few tens of KiB, with the reader held off at first, so
        // the write is guaranteed to hit `EAGAIN` and have to resume.
        let payload = vec![b'x'; 256 * 1024];
        let expected = payload.len();
        let reader = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            // SAFETY: this thread takes sole ownership of the read end.
            let mut file = unsafe { std::fs::File::from_raw_fd(read_fd) };
            let mut received = Vec::with_capacity(expected);
            let mut buf = [0u8; 8192];
            while received.len() < expected {
                match file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => received.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
            received.len()
        });

        let started = Instant::now();
        write_all(write_fd, &payload).expect("every byte goes through");
        assert!(
            started.elapsed() >= Duration::from_millis(200),
            "the reader was held off, so this must have waited rather than fail"
        );
        assert_eq!(reader.join().expect("the reader thread"), expected);
        // SAFETY: the write end is still ours and is not used after this.
        unsafe { libc::close(write_fd) };
    }
}
