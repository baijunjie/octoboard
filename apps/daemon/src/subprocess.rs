//! Running a subprocess to completion within bounds, or detaching an external application.
//! Bounded runs and an exchange of lines with a program that answers on stdout while its stdin
//! stays open both have a deadline, an output ceiling, and a cancel. Every stop kills
//! more than the direct child, as far as the run's [`Reach`] says: the whole process group, so
//! anything it forked (`ssh`, an askpass helper) goes with it, or, for an agent CLI, its whole
//! session. Capturing the login shell's environment, which can leave a background process of its
//! own holding a pipe open and so needs a completion marker rather than end-of-file, is
//! `crate::env_shell`'s.

use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::process_session;

/// Starts `command` in a new session and reaps it on another thread. The new session is what
/// keeps the application alive after the daemon exits; the thread is what keeps it from staying a
/// zombie until then. A failure to exec is reported by `spawn` itself.
pub fn spawn_detached(mut command: Command) -> Result<(), String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    process_session::lead_session(&mut command);
    let mut child = command.spawn().map_err(|err| err.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

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

/// What a stop of a bounded run kills besides the command itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The process group the command leads: what it forked without moving it elsewhere, such as
    /// the `ssh` or askpass helper of a `git`. Left alone when the command ends by itself.
    ProcessGroup,
    /// The session the command leads, for an agent CLI: it moves some helpers into process groups
    /// of their own, which a signal to its group misses. Killed when the command ends by itself
    /// too, as nothing is left to stop a helper of a CLI that has gone.
    Session,
}

/// A spawned child and what stopping it reaches.
struct Leader {
    child: Child,
    reach: Reach,
    /// Names the child in the log of a session sweep.
    label: String,
}

impl Leader {
    fn pid(&self) -> libc::pid_t {
        self.child.id() as libc::pid_t
    }

    /// Polls the child without blocking, reaping it once it has exited. For a session, what the
    /// child left there is killed first, while its zombie keeps the session id from passing to
    /// another process; when `waitid` cannot tell whether it has exited, `try_wait` decides and
    /// nothing is swept.
    fn try_reap(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.reach == Reach::Session {
            match process_session::exited_unreaped(self.pid()) {
                Some(false) => return Ok(None),
                Some(true) => {
                    process_session::kill_members(self.pid()).log(self.pid(), &self.label)
                }
                None => {}
            }
        }
        self.child.try_wait()
    }

    /// Kills what the run reaches and waits for the child to be reaped: for a session, up to one
    /// `REAP_GRACE` for the child to exit before the sweep, then, for either reach, up to another
    /// for the reap. The steps every stop of [`run_bounded`] and [`run_exchange`] needs, together,
    /// so none of them can apply only the first and leave the child running unreaped. Only for a
    /// child not yet reaped by this run, whose pid and process group are still its own.
    fn stop(&mut self) {
        let pid = self.pid();
        process_session::kill_group(pid);
        if self.reach == Reach::Session {
            // Swept once the child has exited, so that it can start nothing after the sweep; one
            // that outlives the grace is swept as it is, still unreaped. When `waitid` cannot
            // tell, the child may have been reaped and its pid passed on, so nothing is swept.
            let exit_deadline = Instant::now() + REAP_GRACE;
            let unreaped = loop {
                match process_session::exited_unreaped(pid) {
                    Some(false) if Instant::now() < exit_deadline => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Some(_) => break true,
                    None => break false,
                }
            };
            if unreaped {
                process_session::kill_members(pid).log(pid, &self.label);
            }
        }
        // A grace of its own, and at least one attempt, so that a child that exits late in the
        // wait above is still reaped.
        let reap_deadline = Instant::now() + REAP_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if Instant::now() < reap_deadline => {
                    thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => break,
            }
        }
    }
}

/// Runs an already-configured `command` to completion, killing its whole process group and
/// returning a timeout error if it is still running after `timeout`: [`run_bounded`] with no
/// ceiling on the output and nothing to cancel it, for a caller whose subprocess prints little.
/// Like it, a command whose output is still held open by something it started after it has
/// exited fails, its group killed, rather than returning output that may stop short.
pub fn run_with_timeout(command: &mut Command, timeout: Duration) -> Result<Output> {
    run_unbounded(command, timeout, Reach::ProcessGroup, None)
}

/// [`run_with_timeout`] with `input` written to the command's stdin, which is closed after it.
/// For a command that is handed a list on stdin and answers on stdout (`git cat-file --batch`).
pub fn run_with_input(command: &mut Command, input: Vec<u8>, timeout: Duration) -> Result<Output> {
    run_unbounded(command, timeout, Reach::ProcessGroup, Some(input))
}

/// [`run_with_timeout`] for an agent CLI: it reaches the command's whole session
/// ([`Reach::Session`]) rather than its process group.
pub fn run_agent_with_timeout(command: &mut Command, timeout: Duration) -> Result<Output> {
    run_unbounded(command, timeout, Reach::Session, None)
}

fn run_unbounded(
    command: &mut Command,
    timeout: Duration,
    reach: Reach,
    input: Option<Vec<u8>>,
) -> Result<Output> {
    static NEVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let bounds = Bounds {
        timeout,
        stdout_limit: usize::MAX,
        stderr_limit: usize::MAX,
        reach,
    };
    run_bounded_with(command, &bounds, &NEVER, input).map_err(|failure| match failure {
        BoundedFailure::Failed(err) => err,
        other => anyhow::anyhow!("{other}"),
    })
}

/// The ceilings [`run_bounded`] and [`run_exchange`] hold a subprocess to, and what a stop of it
/// reaches.
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
    pub reach: Reach,
}

/// Why [`run_bounded`] did not produce an output. Each case has killed what the run reaches first.
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

/// A spawned child with its pipes read on threads of their own, bounded as `bounds` says.
struct Spawned {
    leader: Leader,
    /// Present only when the caller asked for stdin to be piped.
    stdin: Option<std::process::ChildStdin>,
    stdout: mpsc::Receiver<BoundedChunk>,
    stderr: mpsc::Receiver<BoundedChunk>,
}

/// Starts `command` leading the process group or the session `bounds.reach` names, with stdout
/// and stderr piped (and stdin piped or null) and a reader thread on each pipe: stdout stops at
/// `bounds.stdout_limit`, stderr is kept up to `bounds.stderr_limit` and the rest discarded.
fn spawn_bounded(
    command: &mut Command,
    bounds: &Bounds,
    pipe_stdin: bool,
) -> std::result::Result<Spawned, anyhow::Error> {
    match bounds.reach {
        Reach::ProcessGroup => {
            command.process_group(0);
        }
        Reach::Session => process_session::lead_session(command),
    }
    let label = Path::new(command.get_program())
        .file_name()
        .unwrap_or(command.get_program())
        .to_string_lossy()
        .into_owned();
    command.stdin(if pipe_stdin {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child = command.spawn().context("spawning the command")?;

    let stdin = child.stdin.take();
    let stdout_pipe = child.stdout.take().expect("stdout was piped");
    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let (stdout_tx, stdout) = mpsc::channel();
    let (stderr_tx, stderr) = mpsc::channel();
    let stdout_limit = bounds.stdout_limit;
    let stderr_limit = bounds.stderr_limit;
    thread::spawn(move || bounded_pipe_reader(stdout_pipe, &stdout_tx, stdout_limit, true));
    thread::spawn(move || bounded_pipe_reader(stderr_pipe, &stderr_tx, stderr_limit, false));
    Ok(Spawned {
        leader: Leader {
            child,
            reach: bounds.reach,
            label,
        },
        stdin,
        stdout,
        stderr,
    })
}

/// Appends to `stderr` whatever is still in flight on the pipe, for up to a moment: a message
/// for the user, worth a settle so it does not reach them cut at a chunk boundary.
fn settle_stderr(rx: &mpsc::Receiver<BoundedChunk>, stderr: &mut Vec<u8>) {
    let settle = Instant::now() + REAP_GRACE;
    while Instant::now() < settle {
        match rx.recv_timeout(BOUNDED_POLL) {
            Ok(BoundedChunk::Data(bytes)) => stderr.extend_from_slice(&bytes),
            Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

/// How often [`run_bounded`] and [`run_exchange`] look at the deadline, the cancel flag and the
/// child between pipe reads. Short, because neither a person waiting on the result nor a cancel
/// should wait on the poll.
const BOUNDED_POLL: Duration = Duration::from_millis(5);

/// Runs an already-configured `command` to completion under `bounds`, polling the child rather
/// than blocking on it, so the deadline can act without a second thread, and killing what
/// `bounds.reach` names rather than only the direct child on every stop, so anything it forked
/// (`ssh`, an askpass helper, an agent's helper) goes with it. The output itself is bounded while
/// it is read rather than after: stdout is never held past `bounds.stdout_limit` (one byte more
/// stops the run and fails it), stderr is kept up to `bounds.stderr_limit`, and setting `cancel`
/// stops the run within one [`BOUNDED_POLL`]. A deadline alone would not do — a command can
/// produce gigabytes well inside any deadline worth having. A child that exits while something it
/// started still holds stdout open fails once `REAP_GRACE` has passed, rather than handing back
/// output that may stop short.
pub fn run_bounded(
    command: &mut Command,
    bounds: &Bounds,
    cancel: &std::sync::atomic::AtomicBool,
) -> std::result::Result<Output, BoundedFailure> {
    run_bounded_with(command, bounds, cancel, None)
}

/// [`run_bounded`] with `input`, when there is any, written to the command's stdin on a thread of
/// its own, so that a command which answers before it has read it all cannot block the deadline.
fn run_bounded_with(
    command: &mut Command,
    bounds: &Bounds,
    cancel: &std::sync::atomic::AtomicBool,
    input: Option<Vec<u8>>,
) -> std::result::Result<Output, BoundedFailure> {
    use std::sync::atomic::Ordering;

    let Spawned {
        mut leader,
        stdin,
        stdout: stdout_rx,
        stderr: stderr_rx,
    } = spawn_bounded(command, bounds, input.is_some()).map_err(BoundedFailure::Failed)?;
    if let (Some(mut stdin), Some(input)) = (stdin, input) {
        // A broken pipe means the command stopped reading; its exit status tells the rest.
        thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
    }

    let deadline = Instant::now() + bounds.timeout;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdout_open = true;
    // Set once the child has been reaped. Its pid may then be reused, so from there on the group
    // is signalled only while something it left behind is known to still hold stdout open, and a
    // session not at all: it was swept as the child exited.
    let mut exited: Option<(ExitStatus, Instant)> = None;
    loop {
        if stdout_open {
            match stdout_rx.recv_timeout(BOUNDED_POLL) {
                Ok(BoundedChunk::Data(bytes)) => stdout.extend_from_slice(&bytes),
                // The reader has stopped and dropped its end of the pipe, so a writer the reaped
                // child left behind gets `EPIPE` and needs no signal.
                Ok(BoundedChunk::Exceeded) => {
                    if exited.is_none() {
                        leader.stop();
                    }
                    return Err(BoundedFailure::StdoutExceeded(bounds.stdout_limit));
                }
                Ok(BoundedChunk::Failed(err)) => {
                    if exited.is_none() {
                        leader.stop();
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
            None => match leader.try_reap() {
                Ok(Some(status)) => exited = Some((status, Instant::now())),
                Ok(None) => {}
                Err(err) => {
                    leader.stop();
                    return Err(BoundedFailure::Failed(err.into()));
                }
            },
            Some((status, _)) if !stdout_open => {
                settle_stderr(&stderr_rx, &mut stderr);
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            // The child is gone but something it started still holds stdout open, so what has
            // arrived may not be all of it: that is not an output to hand back as whole. A group
            // that still has a member keeps its id, so it is signalled; a run that reaches the
            // session does not signal it, as once the session was swept the group may be empty
            // and its id free to pass to another process. What the sweep missed, if it ran at
            // all, is left.
            Some((_, at)) if at.elapsed() >= REAP_GRACE => {
                if leader.reach == Reach::ProcessGroup {
                    process_session::kill_group(leader.pid());
                }
                return Err(BoundedFailure::Failed(anyhow::anyhow!(
                    "a process it started kept its output open after it exited"
                )));
            }
            Some(_) => continue,
        }
        if cancel.load(Ordering::Relaxed) {
            leader.stop();
            return Err(BoundedFailure::Cancelled);
        }
        if Instant::now() >= deadline {
            leader.stop();
            return Err(BoundedFailure::TimedOut(bounds.timeout));
        }
    }
}

/// Why [`run_exchange`] did not return a reply. Each case has killed what the run reaches first.
#[derive(Debug)]
pub enum ExchangeFailure {
    TimedOut(Duration),
    StdoutExceeded(usize),
    /// The child closed its stdout without having sent a reply, normally by exiting. `status` is
    /// what it exited with, `None` when that could not be read, and `stderr` is what it wrote, up
    /// to the bound.
    ClosedWithoutReply {
        status: Option<ExitStatus>,
        stderr: Vec<u8>,
    },
    /// The cancel flag was set.
    Cancelled,
    Failed(anyhow::Error),
}

impl std::fmt::Display for ExchangeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut(timeout) => write!(formatter, "did not reply within {timeout:?}"),
            Self::StdoutExceeded(limit) => {
                write!(formatter, "produced more than {limit} bytes of output")
            }
            Self::ClosedWithoutReply { status, stderr } => {
                formatter.write_str("ended without replying")?;
                if let Some(status) = status {
                    write!(formatter, " ({status})")?;
                }
                let stderr = String::from_utf8_lossy(stderr);
                match stderr.trim().lines().next_back() {
                    Some(last) => write!(formatter, ": {last}"),
                    None => Ok(()),
                }
            }
            Self::Cancelled => formatter.write_str("was cancelled"),
            Self::Failed(err) => write!(formatter, "{err:#}"),
        }
    }
}

/// Runs an already-configured `command` as a conversation over lines: writes every line of
/// `input` to its stdin, keeps stdin open, and reads stdout line by line until `is_reply` accepts
/// one, which is returned. For a program that answers a request on stdout and exits when its
/// input ends, possibly before it has answered, so [`run_bounded`], which gives a closed stdin,
/// cannot be used. Whatever else it prints is passed over, a line `is_reply` rejects included.
///
/// Every end, a reply too, kills what `bounds.reach` names, since the program is by then running
/// on nothing the caller wants. The same bounds apply as to [`run_bounded`]: `bounds.timeout`
/// from the start, `bounds.stdout_limit` over everything printed (one byte more fails the run),
/// and `bounds.stderr_limit` bytes of stderr kept, for the failure that reports it. Setting
/// `cancel` ends the run within one [`BOUNDED_POLL`].
pub fn run_exchange(
    command: &mut Command,
    input: &[String],
    bounds: &Bounds,
    cancel: &std::sync::atomic::AtomicBool,
    mut is_reply: impl FnMut(&str) -> bool,
) -> std::result::Result<String, ExchangeFailure> {
    let Spawned {
        mut leader,
        stdin,
        stdout: stdout_rx,
        stderr: stderr_rx,
    } = spawn_bounded(command, bounds, true).map_err(ExchangeFailure::Failed)?;
    // Held until the run is stopped below: closing it early is what makes such a program exit.
    let mut stdin = stdin.expect("stdin was piped");
    let stdout_limit = bounds.stdout_limit;

    let deadline = Instant::now() + bounds.timeout;
    // A child that is already gone fails the write; the read below then reports how it ended.
    for line in input {
        if writeln!(stdin, "{line}").is_err() {
            break;
        }
    }
    let _ = stdin.flush();

    let mut pending: Vec<u8> = Vec::new();
    let outcome = loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break Err(ExchangeFailure::Cancelled);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match stdout_rx.recv_timeout(left.min(BOUNDED_POLL)) {
            Ok(BoundedChunk::Data(bytes)) => {
                pending.extend_from_slice(&bytes);
                if let Some(reply) = take_reply(&mut pending, &mut is_reply) {
                    break Ok(reply);
                }
            }
            Ok(BoundedChunk::Exceeded) => break Err(ExchangeFailure::StdoutExceeded(stdout_limit)),
            Ok(BoundedChunk::Failed(err)) => {
                break Err(ExchangeFailure::Failed(
                    anyhow::Error::new(err).context("reading stdout from the subprocess"),
                ))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline {
                    break Err(ExchangeFailure::TimedOut(bounds.timeout));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // The last line may come without its newline.
                pending.push(b'\n');
                break match take_reply(&mut pending, &mut is_reply) {
                    Some(reply) => Ok(reply),
                    None => Err(ExchangeFailure::ClosedWithoutReply {
                        status: None,
                        stderr: Vec::new(),
                    }),
                };
            }
        }
    };
    leader.stop();
    match outcome {
        Err(ExchangeFailure::ClosedWithoutReply { .. }) => {
            let mut stderr = Vec::new();
            settle_stderr(&stderr_rx, &mut stderr);
            Err(ExchangeFailure::ClosedWithoutReply {
                status: leader.child.try_wait().ok().flatten(),
                stderr,
            })
        }
        other => other,
    }
}

/// Removes the complete lines from the front of `pending` one at a time and returns the first
/// that `is_reply` accepts; what follows it, and any partial line, stays.
fn take_reply(pending: &mut Vec<u8>, is_reply: &mut impl FnMut(&str) -> bool) -> Option<String> {
    while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = pending.drain(..=end).collect();
        let line = String::from_utf8_lossy(&line);
        let line = line.trim();
        if !line.is_empty() && is_reply(line) {
            return Some(line.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        assert_gone_by, assert_stops_running, helper_in_its_own_group, read_pid_file,
        until_the_shell_ran, KillOnDrop, ScratchFile, PATIENCE,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

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
    fn input_larger_than_the_pipe_reaches_the_command_whole() {
        let input = vec![b'x'; 1 << 20];
        let output =
            run_with_input(&mut Command::new("/bin/cat"), input.clone(), PATIENCE).unwrap();
        assert_eq!(output.stdout, input);
    }

    #[test]
    fn a_bounded_run_stopped_for_any_reason_leaves_no_process_behind() {
        let bounds = Bounds {
            timeout: PATIENCE,
            stdout_limit: 64 * 1024,
            stderr_limit: 1024,
            reach: Reach::ProcessGroup,
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
            reach: Reach::ProcessGroup,
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
            reach: Reach::ProcessGroup,
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

    /// Runs `script` under `/bin/sh` through [`run_exchange`] with `input` on its stdin, the
    /// script recording its pid first; `None` when the run ended before it did (see
    /// [`until_the_shell_ran`]).
    fn exchange_of(
        script: &str,
        input: &[&str],
        bounds: Bounds,
    ) -> Option<(std::result::Result<String, ExchangeFailure>, i32)> {
        exchange_cancelled_by(script, input, bounds, &AtomicBool::new(false))
    }

    /// [`exchange_of`] with a `cancel` flag for the run.
    fn exchange_cancelled_by(
        script: &str,
        input: &[&str],
        bounds: Bounds,
        cancel: &AtomicBool,
    ) -> Option<(std::result::Result<String, ExchangeFailure>, i32)> {
        let pid_file = ScratchFile::new("env-shell-exchange", "pid");
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            &format!("echo $$ > '{}'; {script}", pid_file.display()),
        ]);
        let input: Vec<String> = input.iter().map(|line| line.to_string()).collect();
        let outcome = run_exchange(&mut command, &input, &bounds, cancel, |line| {
            line.starts_with("reply")
        });
        Some((outcome, read_pid_file(&pid_file)?))
    }

    fn exchange_bounds(timeout: Duration) -> Bounds {
        Bounds {
            timeout,
            stdout_limit: 64 * 1024,
            stderr_limit: 1024,
            reach: Reach::ProcessGroup,
        }
    }

    #[test]
    fn an_exchange_returns_the_reply_to_what_was_written_and_ends_the_child() {
        // Waits for two lines, so stdin must have stayed open for the second; a reply is not
        // the end of the program, which then idles.
        let script = "read a; echo noise; read b; echo \"reply $a $b\"; exec /bin/sleep 300";
        let (outcome, pid) =
            until_the_shell_ran(|| exchange_of(script, &["one", "two"], exchange_bounds(PATIENCE)));
        assert_eq!(outcome.expect("a reply"), "reply one two");
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn an_exchange_reads_a_reply_that_ends_the_stream_without_a_newline() {
        let (outcome, _) =
            until_the_shell_ran(|| exchange_of("printf 'reply x'", &[], exchange_bounds(PATIENCE)));
        assert_eq!(outcome.expect("a reply"), "reply x");
    }

    #[test]
    fn an_exchange_that_gets_no_reply_in_time_fails_and_leaves_nothing_behind() {
        let (outcome, pid) = until_the_shell_ran(|| {
            exchange_of(
                "echo noise; exec /bin/sleep 300",
                &[],
                exchange_bounds(Duration::from_millis(500)),
            )
        });
        let failure = outcome.expect_err("no reply");
        assert!(matches!(failure, ExchangeFailure::TimedOut(_)), "{failure}");
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn an_exchange_that_is_cancelled_ends_at_once_and_leaves_nothing_behind() {
        let (outcome, pid) = until_the_shell_ran(|| {
            let cancel = Arc::new(AtomicBool::new(false));
            let setter = cancel.clone();
            // A child that never replies, so only the flag can end the run before its timeout.
            let started = Instant::now();
            let canceller = thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                setter.store(true, Ordering::Relaxed);
            });
            let ran = exchange_cancelled_by(
                "echo noise; exec /bin/sleep 300",
                &[],
                exchange_bounds(PATIENCE),
                &cancel,
            );
            canceller.join().unwrap();
            assert!(
                started.elapsed() < PATIENCE / 2,
                "the cancel was not noticed"
            );
            ran
        });
        let failure = outcome.expect_err("no reply");
        assert!(matches!(failure, ExchangeFailure::Cancelled), "{failure}");
        assert_eq!(failure.to_string(), "was cancelled");
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn an_exchange_whose_child_exits_without_replying_reports_how_it_ended() {
        let (outcome, _) = until_the_shell_ran(|| {
            exchange_of(
                "echo noise; echo 'bad login' >&2; exit 3",
                &["ignored"],
                exchange_bounds(PATIENCE),
            )
        });
        let failure = outcome.expect_err("no reply");
        match &failure {
            ExchangeFailure::ClosedWithoutReply { status, stderr } => {
                assert_eq!(status.and_then(|status| status.code()), Some(3));
                assert_eq!(stderr, b"bad login\n");
            }
            other => panic!("{other}"),
        }
        assert!(failure.to_string().ends_with("bad login"), "{failure}");
    }

    #[test]
    fn an_exchange_that_prints_past_its_limit_fails_and_leaves_nothing_behind() {
        let bounds = Bounds {
            stdout_limit: 1024,
            ..exchange_bounds(PATIENCE)
        };
        let (outcome, pid) = until_the_shell_ran(|| exchange_of("exec /usr/bin/yes", &[], bounds));
        let failure = outcome.expect_err("too much output");
        assert!(
            matches!(failure, ExchangeFailure::StdoutExceeded(1024)),
            "{failure}"
        );
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    /// How a run that reaches the session ends in one case of the test below.
    #[derive(Debug)]
    enum SessionEnd {
        Exits,
        TimesOut,
        Cancelled,
        Replies,
    }

    /// Runs a stand-in agent CLI that leaves a helper in a process group of its own, through
    /// [`run_bounded`] or (`Replies`) [`run_exchange`] with [`Reach::Session`], ended as `end`
    /// says. Returns the helper's pid once the run is over, or `None` when the run ended before
    /// the stand-in recorded it (see [`until_the_shell_ran`]).
    fn session_run_ending(end: &SessionEnd) -> Option<libc::pid_t> {
        let pid_file = ScratchFile::new("session-run", "pid");
        let then = match end {
            SessionEnd::Exits => "exit 0",
            SessionEnd::TimesOut | SessionEnd::Cancelled => "exec /bin/sleep 300",
            SessionEnd::Replies => "read line; echo \"reply $line\"; exec /bin/sleep 300",
        };
        let mut command = Command::new("/bin/bash");
        command.args([
            "-c",
            &format!("{}; {then}", helper_in_its_own_group(&pid_file)),
        ]);
        let bounds = Bounds {
            timeout: match end {
                SessionEnd::TimesOut => Duration::from_millis(300),
                _ => PATIENCE,
            },
            stdout_limit: 64 * 1024,
            stderr_limit: 1024,
            reach: Reach::Session,
        };
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        if matches!(end, SessionEnd::Cancelled) {
            let (cancel, pid_file) = (cancel.clone(), pid_file.to_path_buf());
            thread::spawn(move || {
                let deadline = Instant::now() + PATIENCE;
                while read_pid_file(&pid_file).is_none() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(5));
                }
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        }
        match end {
            SessionEnd::Replies => {
                let reply = run_exchange(
                    &mut command,
                    &["x".to_string()],
                    &bounds,
                    &AtomicBool::new(false),
                    |line| line.starts_with("reply"),
                );
                assert_eq!(reply.expect("a reply"), "reply x");
            }
            _ => {
                let outcome = run_bounded(&mut command, &bounds, &cancel);
                match (end, outcome) {
                    (SessionEnd::Exits, Ok(output)) => assert!(output.status.success()),
                    (SessionEnd::TimesOut, Err(BoundedFailure::TimedOut(_)))
                    | (SessionEnd::Cancelled, Err(BoundedFailure::Cancelled)) => {}
                    (_, other) => panic!("{end:?} ended as {other:?}"),
                }
            }
        }
        read_pid_file(&pid_file)
    }

    #[test]
    fn a_run_that_reaches_the_session_leaves_no_helper_behind_however_it_ends() {
        for end in [
            SessionEnd::Exits,
            SessionEnd::TimesOut,
            SessionEnd::Cancelled,
            SessionEnd::Replies,
        ] {
            let helper = until_the_shell_ran(|| session_run_ending(&end));
            let mut guard = KillOnDrop(vec![helper]);
            assert_stops_running(helper);
            guard.0.clear();
        }
    }
}
