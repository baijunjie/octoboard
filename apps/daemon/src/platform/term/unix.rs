//! The Unix terminal backend: a PTY from `portable-pty`, read and written through its master
//! descriptor directly.
//!
//! Why raw reads and writes rather than `portable_pty`'s `Read`/`Write` handles: a macOS PTY
//! master accepts only about 1022 bytes before returning `EAGAIN` while the child is not draining,
//! so a message written with one blocking `write_all` parks whatever thread issues it for as long
//! as the child takes to catch up — potentially forever, if a modal dialog is up. The write path
//! here is a non-blocking partial-write-and-retry loop in slices with a deadline instead (see
//! "Writing into a running session" in `docs/agent-cli-reference.md`).
//!
//! The descriptor is put into non-blocking mode once, which is a property of the open file
//! description and therefore applies to reads as well — hence [`PtyReader::read_chunk`], which
//! waits for readability with `poll()` instead of blocking in `read()`.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use super::{encode, hung_up, Command, Foreground, Input, PartialWrite, Size, Spawned, TermFacts};
use crate::platform::proc::{ChildHandle, Containment, Polite, ProcessId, Reach};

pub(super) fn host_facts() -> TermFacts {
    TermFacts {
        host_renders: false,
        host_answers_queries: false,
        host_waits_on_queries: false,
        paste_as_keys: false,
        foreground_job: true,
        settle_window: Duration::from_millis(100),
    }
}

/// A PTY's output is the process's own from its first byte.
#[cfg(test)]
pub(super) fn host_output_preamble() -> &'static [u8] {
    b""
}

/// Slice size for one `write()` attempt. Deliberately below the ~1022 bytes a macOS PTY master
/// accepts in one go, so a slice either goes in whole or the descriptor is genuinely full.
const WRITE_SLICE: usize = 512;

/// How long a write may keep retrying before giving up. A child that has not drained a single
/// slice in this long is not going to: either it is sitting at a modal dialog (where nothing may
/// be written anyway) or it is wedged, and the caller is better off being told.
const WRITE_DEADLINE: Duration = Duration::from_secs(5);

/// Opens a PTY of `size` and starts `command` on it. `portable-pty` makes the process the leader
/// of a session of its own (`setsid()`) with the PTY as its controlling terminal, so it is outside
/// the daemon's process group and its pgid is its own pid. With a `containment`, the process is
/// adopted into it as it starts.
pub fn spawn(command: Command, size: Size, containment: Option<&Containment>) -> Result<Spawned> {
    let pty = native_pty_system()
        .openpty(pty_size(size))
        .context("opening a PTY")?;
    // Everything that can fail without a process to clean up comes first.
    let fd = pty
        .master
        .as_raw_fd()
        .ok_or_else(|| anyhow!("the PTY master has no file descriptor"))?;
    set_nonblocking(fd).context("putting the PTY master into non-blocking mode")?;
    let wake = WakePipe::new().context("opening the reader's wake pipe")?;
    let child = pty.slave.spawn_command(builder(command))?;
    started(child, pty.master, fd, wake, containment)
}

fn builder(command: Command) -> CommandBuilder {
    let mut builder = CommandBuilder::new(&command.program);
    builder.args(&command.args);
    if !command.inherits_env {
        builder.env_clear();
    }
    for (key, value) in &command.env {
        builder.env(key, value);
    }
    if let Some(cwd) = &command.cwd {
        builder.cwd(cwd);
    }
    builder
}

fn pty_size(size: Size) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Completes a [`Spawned`] from a process just started. `portable-pty` neither kills nor reaps a
/// child when its handle is dropped, so one that cannot be taken on is killed and reaped here
/// instead of being left running with nothing to account for it. One that is taken on is reaped
/// through its [`ChildHandle`] from then on.
fn started(
    mut child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    fd: RawFd,
    wake: WakePipe,
    containment: Option<&Containment>,
) -> Result<Spawned> {
    let taken_on = child
        .process_id()
        .ok_or_else(|| anyhow!("the spawned process has no pid"))
        .and_then(|pid| {
            ChildHandle::started(ProcessId(pid), Reach::Session, containment)
                .context("putting the process into its containment")
        });
    let handle = match taken_on {
        Ok(handle) => handle,
        Err(err) => {
            if let Err(err) = child.kill() {
                tracing::warn!(%err, "killing a process that could not be taken on failed");
            }
            if let Err(err) = child.wait() {
                tracing::warn!(%err, "reaping a process that could not be taken on failed");
            }
            return Err(err);
        }
    };
    let pid = handle.pid();
    Ok(Spawned {
        child: handle,
        pid,
        pty: Pty {
            master: Mutex::new(Some(Arc::new(Master {
                fd,
                pty: Mutex::new(master),
            }))),
            wake: Arc::new(wake),
            leader: pid,
            input_lock: Mutex::new(()),
        },
    })
}

/// The PTY master, whose descriptor is open for as long as this lives.
struct Master {
    fd: RawFd,
    /// Kept so the window can be resized, and so that dropping this closes the descriptor.
    pty: Mutex<Box<dyn MasterPty + Send>>,
}

/// What a reader waits on beside the PTY, so that it can be told to stop without the PTY being
/// closed under it: a descriptor another thread is polling must never be closed, since its number
/// can be reused at once for something else.
struct WakePipe {
    read: OwnedFd,
    write: OwnedFd,
}

impl WakePipe {
    fn new() -> io::Result<Self> {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: a two-element array is exactly what `pipe` writes into.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `pipe` succeeded, so both are open descriptors owned by nothing else.
        let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        for fd in [read.as_raw_fd(), write.as_raw_fd()] {
            // Not inherited by what the daemon starts. `portable-pty` closes stray descriptors in
            // its children anyway; a child started any other way would hold the pipe open.
            // SAFETY: `fd` is open for as long as this call.
            if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        set_nonblocking(write.as_raw_fd())?;
        Ok(Self { read, write })
    }

    /// Makes the read end readable for good. Never blocks: once the pipe holds a byte, a full
    /// pipe is as good as another byte.
    fn wake(&self) {
        // SAFETY: one byte from a valid buffer into a descriptor this struct owns.
        unsafe { libc::write(self.write.as_raw_fd(), [1u8].as_ptr().cast(), 1) };
    }
}

/// The terminal a process runs on, from the daemon's side.
pub struct Pty {
    /// `None` once [`Self::hang_up`] has let it go. A reader or a write in progress holds its
    /// own reference, so the descriptor is closed only once neither uses it any more.
    master: Mutex<Option<Arc<Master>>>,
    wake: Arc<WakePipe>,
    /// The process started on the terminal, which leads its session.
    leader: ProcessId,
    /// See [`Self::input_lock`].
    input_lock: Mutex<()>,
}

/// Reads what the process on a [`Pty`] prints, from one thread at a time.
pub struct PtyReader {
    master: Option<Arc<Master>>,
    wake: Arc<WakePipe>,
}

impl PtyReader {
    /// Waits for output, then reads once into `buf`. Returns `Ok(Some(0))` for end of stream,
    /// which on a PTY master is either a zero-length read or `EIO` once the child is gone, and
    /// `Ok(None)` as soon as the reader has been stopped ([`Pty::stop_reader`], [`Pty::hang_up`])
    /// instead: it reads nothing more.
    pub fn read_chunk(&self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        match &self.master {
            Some(master) => read_chunk(master.fd, self.wake.read.as_raw_fd(), buf),
            None => Ok(None),
        }
    }
}

impl Pty {
    /// The lock the owner makes every write into the input under, so that writes that belong
    /// together (a bracketed paste) are never split by another.
    pub fn input_lock(&self) -> &Mutex<()> {
        &self.input_lock
    }

    fn master(&self) -> Option<Arc<Master>> {
        self.master.lock().expect("master mutex poisoned").clone()
    }

    /// A reader of the terminal's output. One that is never dropped keeps the PTY open after a
    /// hang-up.
    pub fn reader(&self) -> PtyReader {
        PtyReader {
            master: self.master(),
            wake: self.wake.clone(),
        }
    }

    /// Makes every reader of this terminal return `Ok(None)`, now and from then on, without
    /// closing anything.
    pub fn stop_reader(&self) {
        self.wake.wake();
    }

    /// Writes `bytes` as they are: a client's keystrokes and its replies to the program's queries.
    /// Never blocks for long: a full input is waited on and the write resumed, and it fails with
    /// `TimedOut` once the child has accepted nothing for a few seconds.
    pub fn write_raw(&self, bytes: &[u8]) -> Result<(), PartialWrite> {
        let Some(master) = self.master() else {
            return Err(hung_up());
        };
        write_all(master.fd, bytes)
    }

    /// Writes the daemon's own `input`, encoded as the terminal sends it, as [`Self::write_raw`]
    /// writes; the error counts encoded bytes.
    pub fn write_input(&self, input: &[Input]) -> Result<(), PartialWrite> {
        self.write_raw(&encode(input))
    }

    /// The window's size now, as the terminal reports it.
    pub fn size(&self) -> Result<Size> {
        let Some(master) = self.master() else {
            anyhow::bail!("the terminal has been hung up");
        };
        let size = master.pty.lock().expect("pty mutex poisoned").get_size()?;
        Ok(Size {
            rows: size.rows,
            cols: size.cols,
        })
    }

    pub fn resize(&self, size: Size) -> Result<()> {
        let Some(master) = self.master() else {
            anyhow::bail!("the terminal has been hung up");
        };
        let master = master.pty.lock().expect("pty mutex poisoned");
        master.resize(pty_size(size))?;
        Ok(())
    }

    /// Hangs the terminal up, as closing a terminal window does: stops every reader and lets the
    /// PTY master go. Once nothing reads or writes it any more, it is closed and the kernel hangs
    /// up the other side — a process still on it gets `SIGHUP` where it is in the terminal's
    /// foreground, and an error on every read and write from then on. Every write after this one
    /// fails.
    pub fn hang_up(&self) {
        self.stop_reader();
        self.master.lock().expect("master mutex poisoned").take();
    }

    /// How long a polite request to end the process takes on this host before the request itself
    /// is made: none, since the request is a signal sent at once.
    pub fn polite_stop_lead(&self, _how: Polite) -> Duration {
        Duration::ZERO
    }

    /// Whether [`Self::hang_up`] has run.
    #[cfg(test)]
    pub fn is_hung_up(&self) -> bool {
        self.master.lock().expect("master mutex poisoned").is_none()
    }

    /// Whose job is in the foreground of the terminal, the one its input and its signals go to:
    /// the process's own, or another it started. Only meaningful while the process runs.
    pub fn foreground_job(&self) -> Foreground {
        let Some(master) = self.master() else {
            return Foreground::Unsupported;
        };
        // SAFETY: `tcgetpgrp` only reads the terminal's state, through a descriptor that holding
        // `master` keeps open.
        let group = unsafe { libc::tcgetpgrp(master.fd) };
        match group {
            group if group <= 0 => Foreground::Unsupported,
            group if group as u32 == self.leader.0 => Foreground::Own,
            _ => Foreground::Other,
        }
    }

    /// The device path of the terminal's slave side, what a process on the terminal has open as
    /// its stdin, stdout or stderr, or `None` when the system cannot say or the terminal is hung
    /// up.
    pub fn slave_path(&self) -> Option<PathBuf> {
        let master = self.master()?;
        let mut name = [0 as libc::c_char; 256];
        #[cfg(target_os = "linux")]
        // SAFETY: the call writes at most the buffer's size, through a descriptor that holding
        // `master` keeps open.
        let named = unsafe { libc::ptsname_r(master.fd, name.as_mut_ptr(), name.len()) } == 0;
        #[cfg(target_os = "macos")]
        let named = {
            // SAFETY: the request fills in a buffer of 128 bytes, which `name` exceeds, through a
            // descriptor that holding `master` keeps open.
            let status =
                unsafe { libc::ioctl(master.fd, libc::TIOCPTYGNAME as _, name.as_mut_ptr()) };
            status == 0
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let named = false;
        if !named {
            return None;
        }
        // SAFETY: the system wrote a NUL-terminated name into the zeroed buffer.
        let name = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) };
        Some(PathBuf::from(name.to_string_lossy().into_owned()))
    }
}

fn set_nonblocking(fd: RawFd) -> io::Result<()> {
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

/// Waits for the descriptor to become readable, then reads once. Returns `Ok(Some(0))` for end of
/// stream, which on a PTY master is either a zero-length read or `EIO` once the child is gone, and
/// `Ok(None)` as soon as `wake` becomes readable instead: the reader is being asked to stop, and
/// reads nothing more.
fn read_chunk(fd: RawFd, wake: RawFd, buf: &mut [u8]) -> io::Result<Option<usize>> {
    loop {
        let mut pollfds = [
            libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wake,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: two valid `pollfd`s are passed with a matching count.
        if unsafe { libc::poll(pollfds.as_mut_ptr(), 2, -1) } < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        if pollfds[1].revents != 0 {
            return Ok(None);
        }
        // SAFETY: `buf` is a valid mutable slice and the length passed matches it.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n >= 0 {
            return Ok(Some(n as usize));
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EAGAIN) | Some(libc::EINTR) => continue,
            // The child is gone: macOS reports the closed slave side as EIO rather than EOF.
            Some(libc::EIO) => return Ok(Some(0)),
            _ => return Err(err),
        }
    }
}

/// Writes every byte of `data`, in slices, never blocking: a full descriptor is waited on with
/// `poll()` and the partial write resumed. Fails with `TimedOut` if the child has not accepted
/// anything for `WRITE_DEADLINE`.
fn write_all(fd: RawFd, data: &[u8]) -> Result<(), PartialWrite> {
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
    use std::io::{self, Read};
    use std::os::unix::io::{FromRawFd, RawFd};

    use portable_pty::native_pty_system;

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

        // 256 KiB against a pipe buffer of a few tens of KiB, with the reader held off until the
        // pipe is full, so the write is guaranteed to hit `EAGAIN` and have to resume.
        let payload = vec![b'x'; 256 * 1024];
        let expected = payload.len();
        let reader = std::thread::spawn(move || {
            // Full is not writable: less room than `PIPE_BUF` is left, with nothing read yet.
            let full = || {
                let mut pollfd = libc::pollfd {
                    fd: write_fd,
                    events: libc::POLLOUT,
                    revents: 0,
                };
                // SAFETY: one valid `pollfd`, for a descriptor the test keeps open until this
                // thread has ended.
                unsafe { libc::poll(&mut pollfd, 1, 0) == 0 }
            };
            let filled = crate::session::wait::wait_for(crate::test_support::PATIENCE, full);
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
            (filled, received.len())
        });

        write_all(write_fd, &payload).expect("every byte goes through");
        let (filled, received) = reader.join().expect("the reader thread");
        assert!(
            filled,
            "the pipe never filled up, so the write never had to wait"
        );
        assert_eq!(received, expected);
        // SAFETY: the write end is still ours and is not used after this.
        unsafe { libc::close(write_fd) };
    }

    /// A child whose pid cannot be read, the one way taking on a started process can fail.
    #[derive(Debug)]
    struct Unidentified(Box<dyn portable_pty::Child + Send + Sync>);

    impl portable_pty::ChildKiller for Unidentified {
        fn kill(&mut self) -> io::Result<()> {
            self.0.kill()
        }

        fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
            self.0.clone_killer()
        }
    }

    impl portable_pty::Child for Unidentified {
        fn try_wait(&mut self) -> io::Result<Option<portable_pty::ExitStatus>> {
            self.0.try_wait()
        }

        fn wait(&mut self) -> io::Result<portable_pty::ExitStatus> {
            self.0.wait()
        }

        fn process_id(&self) -> Option<u32> {
            None
        }
    }

    #[test]
    fn a_process_that_cannot_be_taken_on_is_killed_and_reaped() {
        let pty = native_pty_system()
            .openpty(super::pty_size(super::Size { rows: 24, cols: 80 }))
            .expect("a PTY");
        let mut cmd = portable_pty::CommandBuilder::new("/bin/sleep");
        cmd.arg("300");
        let child = pty.slave.spawn_command(cmd).expect("the process");
        let pid = child.process_id().expect("a pid") as libc::pid_t;
        let fd = pty.master.as_raw_fd().expect("a descriptor");
        let wake = super::WakePipe::new().expect("the wake pipe");
        let outcome = super::started(Box::new(Unidentified(child)), pty.master, fd, wake, None);
        assert!(outcome.is_err());
        // SAFETY: signal 0 only checks that the pid names a process. A reaped child is gone
        // entirely, where a killed one left unreaped would still be a zombie and answer.
        assert_ne!(
            unsafe { libc::kill(pid, 0) },
            0,
            "the process was left behind"
        );
    }
}
