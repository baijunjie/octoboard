//! Running one subprocess the daemon spawned directly, to completion, within bounds: a deadline,
//! a ceiling on what it may print, and a cancel. Every stop kills the whole process group rather
//! than the direct child, so anything it forked (`ssh`, an askpass helper) goes with it. Capturing
//! the login shell's environment, which can leave a background process of its own holding a pipe
//! open and so needs a completion marker rather than end-of-file, is `crate::env_shell`'s.

use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// Upper bound on the bounded reap attempted after a timeout's `SIGKILL`. A process wedged in an
/// uninterruptible kernel wait (a dead network mount under `/Volumes` is the realistic trigger
/// here) can leave the signal pending indefinitely, and nothing short of the kernel itself can
/// force that reap to finish — so this is a best effort, not a guarantee, and giving up after it
/// elapses risks at most one leaked zombie.
pub(crate) const REAP_GRACE: Duration = Duration::from_millis(500);

/// Size of each chunk a pipe reader thread forwards over its channel. Arbitrary beyond "comfortably
/// smaller than the pipe's own OS buffer", since the channel is unbounded and nothing here needs
/// the chunking to line up with any particular boundary in the data.
pub(crate) const PIPE_CHUNK_SIZE: usize = 8192;

/// Kills the whole process group behind `child` and waits, bounded by `REAP_GRACE`, for it to be
/// reaped — the two steps every stop of [`run_bounded`] needs, pulled out so none of them can
/// apply only the first and leave the child running unreaped.
fn kill_process_group(child: &mut Child) {
    let pid = child.id();
    // SAFETY: a plain signal to a process group number. Every call site reaches here only while
    // the group is known to have a member — `child` has not been reaped, or something it left
    // behind still holds its stdout open — the same condition `kill_group_after_timeout` relies
    // on for `-pid` to stay valid.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    let reap_deadline = Instant::now() + REAP_GRACE;
    while Instant::now() < reap_deadline {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Runs an already-configured `command` to completion, killing its whole process group and
/// returning a timeout error if it is still running after `timeout`: [`run_bounded`] with no
/// ceiling on the output and nothing to cancel it, for a caller whose subprocess prints little.
/// Like it, a command whose output is still held open by something it started after it has
/// exited fails, its group killed, rather than returning output that may stop short.
pub fn run_with_timeout(command: &mut Command, timeout: Duration) -> Result<Output> {
    static NEVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let bounds = Bounds {
        timeout,
        stdout_limit: usize::MAX,
        stderr_limit: usize::MAX,
    };
    run_bounded(command, &bounds, &NEVER).map_err(|failure| match failure {
        BoundedFailure::Failed(err) => err,
        other => anyhow::anyhow!("{other}"),
    })
}

/// The ceilings [`run_bounded`] holds a subprocess to.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub timeout: Duration,
    /// The most stdout a run may produce. One byte more ends the run as
    /// [`BoundedFailure::StdoutExceeded`]: the output is the thing the caller asked for, so a cut
    /// copy of it is never handed back as if it were whole.
    pub stdout_limit: usize,
    /// The most stderr kept. Stderr is only ever a message for the user, so what lies past this
    /// is read and thrown away rather than failing the run.
    pub stderr_limit: usize,
}

/// Why [`run_bounded`] did not produce an output. Each case has killed the process group first.
#[derive(Debug)]
pub enum BoundedFailure {
    TimedOut(Duration),
    StdoutExceeded(usize),
    Cancelled,
    Failed(anyhow::Error),
}

impl std::fmt::Display for BoundedFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut(timeout) => write!(formatter, "did not finish within {timeout:?}"),
            Self::StdoutExceeded(limit) => {
                write!(formatter, "produced more than {limit} bytes of output")
            }
            Self::Cancelled => formatter.write_str("was cancelled"),
            Self::Failed(err) => write!(formatter, "{err:#}"),
        }
    }
}

/// What a bounded pipe reader forwards: a chunk, the news that the pipe went past its limit (after
/// which the reader has stopped reading), or a read error.
enum BoundedChunk {
    Data(Vec<u8>),
    Exceeded,
    Failed(io::Error),
}

/// Reads `pipe` until EOF, forwarding at most `limit` bytes over `tx`. Past the limit it either
/// stops reading altogether and reports [`BoundedChunk::Exceeded`] (`stop_at_limit`), which drops
/// the read end so the writer gets `EPIPE` instead of a pipe that never drains, or keeps reading
/// and discards the rest, so a chatty writer is never blocked on a full pipe. Either way at most
/// `limit` bytes are ever held, here or in the channel.
fn bounded_pipe_reader(
    mut pipe: impl Read,
    tx: &mpsc::Sender<BoundedChunk>,
    limit: usize,
    stop_at_limit: bool,
) {
    let mut chunk = vec![0u8; PIPE_CHUNK_SIZE];
    let mut forwarded = 0usize;
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let room = limit - forwarded;
                if n > room {
                    if room > 0 && tx.send(BoundedChunk::Data(chunk[..room].to_vec())).is_err() {
                        break;
                    }
                    forwarded = limit;
                    if stop_at_limit {
                        let _ = tx.send(BoundedChunk::Exceeded);
                        break;
                    }
                    continue;
                }
                forwarded += n;
                if tx.send(BoundedChunk::Data(chunk[..n].to_vec())).is_err() {
                    break;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => {
                let _ = tx.send(BoundedChunk::Failed(err));
                break;
            }
        }
    }
}

/// How often [`run_bounded`] looks at the deadline, the cancel flag and the child between pipe
/// reads. Short, because its callers answer a person who is waiting on a file to open.
const BOUNDED_POLL: Duration = Duration::from_millis(5);

/// Runs an already-configured `command` to completion under `bounds`, polling `try_wait` rather
/// than blocking on it, so the deadline can act without a second thread, and killing the whole
/// process group rather than only the direct child on every stop, so anything it forked (`ssh`, an
/// askpass helper) goes with it. The output itself is bounded while it is read rather than
/// after: stdout is never held past `bounds.stdout_limit` (one byte more kills the process group
/// and fails the run), stderr is kept up to `bounds.stderr_limit`, and setting `cancel` kills the
/// group within one [`BOUNDED_POLL`]. A deadline alone would not do — a command can produce
/// gigabytes well inside any deadline worth having. A child that exits while something it started
/// still holds stdout open fails once `REAP_GRACE` has passed, its group killed, rather than
/// handing back output that may stop short.
pub fn run_bounded(
    command: &mut Command,
    bounds: &Bounds,
    cancel: &std::sync::atomic::AtomicBool,
) -> std::result::Result<Output, BoundedFailure> {
    use std::sync::atomic::Ordering;

    command.process_group(0);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .context("spawning the bounded command")
        .map_err(BoundedFailure::Failed)?;

    let stdout_pipe = child.stdout.take().expect("stdout was piped");
    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    let stdout_limit = bounds.stdout_limit;
    let stderr_limit = bounds.stderr_limit;
    thread::spawn(move || bounded_pipe_reader(stdout_pipe, &stdout_tx, stdout_limit, true));
    thread::spawn(move || bounded_pipe_reader(stderr_pipe, &stderr_tx, stderr_limit, false));

    let deadline = Instant::now() + bounds.timeout;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdout_open = true;
    // Set once `try_wait` has reaped the child. Its pid may then be reused, so from there on the
    // group is signalled only while something it left behind is known to still hold stdout open.
    let mut exited: Option<(ExitStatus, Instant)> = None;
    loop {
        if stdout_open {
            match stdout_rx.recv_timeout(BOUNDED_POLL) {
                Ok(BoundedChunk::Data(bytes)) => stdout.extend_from_slice(&bytes),
                // The reader has stopped and dropped its end of the pipe, so a writer the reaped
                // child left behind gets `EPIPE` and needs no signal.
                Ok(BoundedChunk::Exceeded) => {
                    if exited.is_none() {
                        kill_process_group(&mut child);
                    }
                    return Err(BoundedFailure::StdoutExceeded(bounds.stdout_limit));
                }
                Ok(BoundedChunk::Failed(err)) => {
                    if exited.is_none() {
                        kill_process_group(&mut child);
                    }
                    return Err(BoundedFailure::Failed(
                        anyhow::Error::new(err).context("reading stdout from the subprocess"),
                    ));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => stdout_open = false,
            }
        } else {
            thread::sleep(BOUNDED_POLL);
        }
        while let Ok(chunk) = stderr_rx.try_recv() {
            if let BoundedChunk::Data(bytes) = chunk {
                stderr.extend_from_slice(&bytes);
            }
        }
        match exited {
            None => match child.try_wait() {
                Ok(Some(status)) => exited = Some((status, Instant::now())),
                Ok(None) => {}
                Err(err) => {
                    kill_process_group(&mut child);
                    return Err(BoundedFailure::Failed(err.into()));
                }
            },
            Some((status, _)) if !stdout_open => {
                // Whatever stderr is still in flight is a message, worth a moment's settle so it
                // does not reach the user cut at a chunk boundary.
                let settle = Instant::now() + REAP_GRACE;
                while Instant::now() < settle {
                    match stderr_rx.recv_timeout(BOUNDED_POLL) {
                        Ok(BoundedChunk::Data(bytes)) => stderr.extend_from_slice(&bytes),
                        Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            // The child is gone but something it started still holds stdout open, so what has
            // arrived may not be all of it: that is not an output to hand back as whole.
            Some((_, at)) if at.elapsed() >= REAP_GRACE => {
                kill_process_group(&mut child);
                return Err(BoundedFailure::Failed(anyhow::anyhow!(
                    "a process it started kept its output open after it exited"
                )));
            }
            Some(_) => continue,
        }
        if cancel.load(Ordering::Relaxed) {
            kill_process_group(&mut child);
            return Err(BoundedFailure::Cancelled);
        }
        if Instant::now() >= deadline {
            kill_process_group(&mut child);
            return Err(BoundedFailure::TimedOut(bounds.timeout));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        assert_gone_by, read_pid_file, until_the_shell_ran, ScratchFile, PATIENCE,
    };

    /// Runs `script` under `/bin/sh` through [`run_bounded`], the script recording its pid in a
    /// file first, and returns the outcome with that pid once the script has written it — or
    /// `None` when the run ended before it did, which a stalled exec can cause (see
    /// [`until_the_shell_ran`]).
    fn bounded_run_of(
        script: &str,
        bounds: Bounds,
        cancel_after: Option<Duration>,
    ) -> Option<(std::result::Result<Output, BoundedFailure>, i32)> {
        let pid_file = ScratchFile::new("env-shell-bounded", "pid");
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            &format!("echo $$ > '{}'; {script}", pid_file.display()),
        ]);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Some(delay) = cancel_after {
            let (cancel, pid_file) = (cancel.clone(), pid_file.to_path_buf());
            thread::spawn(move || {
                let deadline = Instant::now() + PATIENCE;
                while read_pid_file(&pid_file).is_none() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(5));
                }
                thread::sleep(delay);
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        }
        let outcome = run_bounded(&mut command, &bounds, &cancel);
        Some((outcome, read_pid_file(&pid_file)?))
    }

    #[test]
    fn a_bounded_run_stopped_for_any_reason_leaves_no_process_behind() {
        let bounds = Bounds {
            timeout: PATIENCE,
            stdout_limit: 64 * 1024,
            stderr_limit: 1024,
        };
        let short = Bounds {
            timeout: Duration::from_millis(300),
            ..bounds
        };
        // `exec`, so the pid recorded is the process doing the work, not a shell waiting on it.
        let cases: [(&str, Bounds, Option<Duration>); 3] = [
            ("exec /usr/bin/yes", bounds, None),
            (
                "exec /bin/sleep 300",
                bounds,
                Some(Duration::from_millis(50)),
            ),
            ("exec /bin/sleep 300", short, None),
        ];
        for (script, bounds, cancel_after) in cases {
            let (outcome, pid) =
                until_the_shell_ran(|| bounded_run_of(script, bounds, cancel_after));
            let failure = outcome.expect_err("the run was stopped");
            match (script, cancel_after) {
                ("exec /usr/bin/yes", _) => {
                    assert!(
                        matches!(failure, BoundedFailure::StdoutExceeded(_)),
                        "{failure}"
                    )
                }
                (_, Some(_)) => assert!(matches!(failure, BoundedFailure::Cancelled), "{failure}"),
                _ => assert!(matches!(failure, BoundedFailure::TimedOut(_)), "{failure}"),
            }
            assert_gone_by(pid, Instant::now() + PATIENCE);
        }
    }

    #[test]
    fn a_bounded_run_whose_output_outlives_the_child_fails_and_leaves_nothing_behind() {
        let bounds = Bounds {
            timeout: PATIENCE,
            stdout_limit: 1024,
            stderr_limit: 1024,
        };
        let (outcome, pid) = until_the_shell_ran(|| {
            let pid_file = ScratchFile::new("env-shell-leftover", "pid");
            let mut command = Command::new("/bin/sh");
            let script = format!(
                "/bin/sleep 300 & echo $! > '{}'; echo partial",
                pid_file.display()
            );
            command.args(["-c", &script]);
            let never = std::sync::atomic::AtomicBool::new(false);
            let outcome = run_bounded(&mut command, &bounds, &never);
            Some((outcome, read_pid_file(&pid_file)?))
        });
        let failure = outcome.expect_err("output still held open is not whole");
        assert!(matches!(failure, BoundedFailure::Failed(_)), "{failure}");
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn a_bounded_run_keeps_its_stderr_to_the_limit_without_failing() {
        let bounds = Bounds {
            timeout: PATIENCE,
            stdout_limit: 1024,
            stderr_limit: 10,
        };
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf 'out'; printf '0123456789abcdef' >&2"]);
        let output = run_bounded(
            &mut command,
            &bounds,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .expect("a run within its stdout limit");
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"0123456789");
    }
}
