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

use crate::processes::leader;

/// Starts `command` in a new session and reaps it on another thread. The new session is what
/// keeps the application alive after the daemon exits; the thread is what keeps it from staying a
/// zombie until then. A failure to exec is reported by `spawn` itself.
pub fn spawn_detached(mut command: Command) -> Result<(), String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    leader::lead_session(&mut command);
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
const PIPE_CHUNK_SIZE: usize = 8192;

/// How long the stop of an agent CLI run leaves the CLI to exit after `SIGTERM` before it is
/// killed. Just above the five seconds Claude Code's own shutdown allows itself before it forces
/// an exit, so a CLI that is cleaning up is not cut short.
const AGENT_TERM_GRACE: Duration = Duration::from_secs(6);

/// The most a stop waits for its CLI after `SIGTERM` once the run is cancelled, which is how a
/// daemon shutdown reaches the runs it can cancel. A restart of the daemon by the application
/// gives it a bounded time to exit (see the "Restarting" section of
/// `docs/product/application-lifecycle.md`) and then kills it, which would leave a CLI that
/// ignores `SIGTERM` behind; this keeps the runs' waits within that. Longer than the two seconds
/// Claude Code's own cleanup is allowed to take.
const CANCEL_TERM_GRACE: Duration = Duration::from_secs(3);

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
    /// How long a stop leaves the child to exit after `SIGTERM` before the kill; see
    /// [`Bounds::term_grace`].
    term_grace: Duration,
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
            match leader::exited_unreaped(self.pid()) {
                Some(false) => return Ok(None),
                Some(true) => leader::kill_members(self.pid()).log(self.pid(), &self.label),
                None => {}
            }
        }
        self.child.try_wait()
    }

    /// Ends what the run reaches and waits for the child to be reaped: first, when the run has a
    /// `term_grace`, `SIGTERM` to the group and up to that long for the child to exit, so that an
    /// agent CLI can finish what it is writing (Claude Code writes its `.claude.json` to a
    /// temporary file and renames it into place, so a kill in between leaves the temporary file
    /// behind).
    /// Then the kill; for a session, up to one `REAP_GRACE` for the child to exit before the
    /// sweep, then, for either reach, up to another for the reap. The steps every stop of
    /// [`run_bounded`] and [`run_exchange`] needs, together, so none of them can apply only the
    /// first and leave the child running unreaped. Only for a child not yet reaped by this run,
    /// whose pid and process group are still its own.
    ///
    /// Once `cancel` is set, before the wait or during it, the wait for the child after `SIGTERM`
    /// ends at most [`CANCEL_TERM_GRACE`] from then: a shutdown is not to wait out a grace that a
    /// timeout began just before it.
    fn stop(&mut self, cancel: &std::sync::atomic::AtomicBool) {
        let pid = self.pid();
        if !self.term_grace.is_zero() {
            leader::signal_group(pid, libc::SIGTERM);
            let mut deadline = Instant::now() + self.term_grace;
            let mut cancelled = false;
            // A group member the leader outlives is killed below; `None`, where `waitid` cannot
            // say, ends the wait for the kill to decide.
            while leader::exited_unreaped(pid) == Some(false) && Instant::now() < deadline {
                if !cancelled && cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    cancelled = true;
                    deadline = deadline.min(Instant::now() + CANCEL_TERM_GRACE);
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        leader::kill_group(pid);
        if self.reach == Reach::Session {
            // Swept once the child has exited, so that it can start nothing after the sweep; one
            // that outlives the grace is swept as it is, still unreaped. When `waitid` cannot
            // tell, the child may have been reaped and its pid passed on, so nothing is swept.
            let exit_deadline = Instant::now() + REAP_GRACE;
            let unreaped = loop {
                match leader::exited_unreaped(pid) {
                    Some(false) if Instant::now() < exit_deadline => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Some(_) => break true,
                    None => break false,
                }
            };
            if unreaped {
                leader::kill_members(pid).log(pid, &self.label);
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
    run_unbounded(command, timeout, None)
}

/// [`run_with_timeout`] with `input` written to the command's stdin, which is closed after it.
/// For a command that is handed a list on stdin and answers on stdout (`git cat-file --batch`).
pub fn run_with_input(command: &mut Command, input: Vec<u8>, timeout: Duration) -> Result<Output> {
    run_unbounded(command, timeout, Some(input))
}

fn run_unbounded(
    command: &mut Command,
    timeout: Duration,
    input: Option<Vec<u8>>,
) -> Result<Output> {
    static NEVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let bounds = Bounds {
        timeout,
        stdout_limit: usize::MAX,
        stderr_limit: usize::MAX,
        reach: Reach::ProcessGroup,
        term_grace: Duration::ZERO,
    };
    run_bounded_with(command, &bounds, &NEVER, input).map_err(BoundedFailure::into_error)
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
    /// How long a stop leaves the program to exit after `SIGTERM` to its group before the kill.
    /// Zero kills at once, which is right for a program that has nothing of its own to finish.
    pub term_grace: Duration,
}

/// How much of an agent CLI query's stderr [`Bounds::agent_query`] keeps for the failure that
/// reports it.
const AGENT_STDERR_LIMIT: usize = 4096;

impl Bounds {
    /// The bounds of a short query of an agent CLI: `timeout`, at most `stdout_limit` of output,
    /// the first `AGENT_STDERR_LIMIT` of its stderr, and a stop that reaches the CLI's whole
    /// session ([`Reach::Session`]) and gives the CLI `AGENT_TERM_GRACE` to exit on `SIGTERM`
    /// first.
    pub fn agent_query(timeout: Duration, stdout_limit: usize) -> Self {
        Self {
            timeout,
            stdout_limit,
            stderr_limit: AGENT_STDERR_LIMIT,
            reach: Reach::Session,
            term_grace: AGENT_TERM_GRACE,
        }
    }
}

/// Why [`run_bounded`] did not produce an output. Each case has stopped the run and killed what it
/// reaches first.
#[derive(Debug)]
pub enum BoundedFailure {
    TimedOut(Duration),
    StdoutExceeded(usize),
    Cancelled,
    Failed(anyhow::Error),
}

impl BoundedFailure {
    /// The failure as the error [`run_with_timeout`] returns for it: a failure to run carries its
    /// own error, with whatever context it had, and a stop becomes its message.
    pub fn into_error(self) -> anyhow::Error {
        match self {
            Self::Failed(err) => err,
            other => anyhow::anyhow!("{other}"),
        }
    }
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
pub(crate) enum BoundedChunk {
    Data(Vec<u8>),
    Exceeded,
    Failed(io::Error),
}

/// Reads `pipe` until EOF, forwarding at most `limit` bytes over `tx`. Past the limit it either
/// stops reading altogether and reports [`BoundedChunk::Exceeded`] (`stop_at_limit`), which drops
/// the read end so the writer gets `EPIPE` instead of a pipe that never drains, or keeps reading
/// and discards the rest, so a chatty writer is never blocked on a full pipe. Either way at most
/// `limit` bytes are ever held, here or in the channel.
pub(crate) fn bounded_pipe_reader(
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
        Reach::Session => leader::lead_session(command),
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
            term_grace: bounds.term_grace,
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
                        leader.stop(cancel);
                    }
                    return Err(BoundedFailure::StdoutExceeded(bounds.stdout_limit));
                }
                Ok(BoundedChunk::Failed(err)) => {
                    if exited.is_none() {
                        leader.stop(cancel);
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
                    leader.stop(cancel);
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
            // that still has a member keeps its id, so it is signalled; one whose holder left it
            // (a daemonizing helper) may be empty with its id free to pass to another process, and
            // is not. A run that reaches the session does not signal it either, as once the
            // session was swept the group may be empty. What the sweep missed, if it ran at
            // all, is left.
            Some((_, at)) if at.elapsed() >= REAP_GRACE => {
                if leader.reach == Reach::ProcessGroup
                    && leader::group_has_members(leader.pid()) == Some(true)
                {
                    leader::kill_group(leader.pid());
                }
                return Err(BoundedFailure::Failed(anyhow::anyhow!(
                    "a process it started kept its output open after it exited"
                )));
            }
            Some(_) => continue,
        }
        if cancel.load(Ordering::Relaxed) {
            leader.stop(cancel);
            return Err(BoundedFailure::Cancelled);
        }
        if Instant::now() >= deadline {
            leader.stop(cancel);
            return Err(BoundedFailure::TimedOut(bounds.timeout));
        }
    }
}

/// Why [`run_exchange`] did not return a reply. Each case has stopped the run and killed what it
/// reaches first.
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

/// How long an agent CLI query's program is given to exit by itself after its reply, once its stdin
/// is closed (see [`run_exchange_traced`]). The agents exit within a second or two of the end of
/// their input; this leaves room for a login being written back. A program that has not exited
/// by then is stopped, with `SIGTERM` before the kill as [`Bounds::term_grace`] says.
pub const AGENT_REPLY_GRACE: Duration = Duration::from_secs(6);

/// Runs an already-configured `command` as a conversation over lines: writes every line of
/// `input` to its stdin, keeps stdin open, and reads stdout line by line until `is_reply` accepts
/// one, which is returned. For a program that answers a request on stdout and exits when its
/// input ends, possibly before it has answered, so [`run_bounded`], which gives a closed stdin,
/// cannot be used. Whatever else it prints is passed over, a line `is_reply` rejects included.
///
/// Once the reply is in, the program is given [`AGENT_REPLY_GRACE`] to exit by itself, as
/// [`run_exchange_traced`] says. Every other end stops the program as [`Bounds::term_grace`] says
/// and kills what `bounds.reach` names. The same bounds
/// apply as to [`run_bounded`]: `bounds.timeout` from the start, `bounds.stdout_limit` over
/// everything printed (one byte more fails the run), and `bounds.stderr_limit` bytes of stderr
/// kept, for the failure that reports it. Setting `cancel` ends the run within one
/// [`BOUNDED_POLL`].
pub fn run_exchange(
    command: &mut Command,
    input: &[String],
    bounds: &Bounds,
    cancel: &std::sync::atomic::AtomicBool,
    is_reply: impl FnMut(&str) -> bool,
) -> std::result::Result<String, ExchangeFailure> {
    run_exchange_traced(
        command,
        input,
        bounds,
        cancel,
        AGENT_REPLY_GRACE,
        &mut ExchangeTrace::default(),
        is_reply,
    )
}

/// How a run ended after its reply came, when the program was given time to exit by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterReply {
    /// The program exited by itself, `after` the reply.
    Exited { after: Duration },
    /// It had not exited, and was stopped `after` the reply for `why`.
    Killed { after: Duration, why: &'static str },
}

/// What [`run_exchange_traced`] observed of the child, for a caller that logs how a run went.
#[derive(Debug, Default)]
pub struct ExchangeTrace {
    /// The child's pid; `None` when it could not be started.
    pub pid: Option<u32>,
    /// How long the run took, up to the moment it was ended (not counting the stop itself).
    pub elapsed: Duration,
    /// The stderr kept (up to `bounds.stderr_limit`). Collected for a run that did not return a
    /// reply after waiting a moment for it to settle, and for one that did as far as had
    /// arrived by the end.
    pub stderr: Vec<u8>,
    /// How the program ended after the reply; `None` when no reply came or no grace was given.
    pub after_reply: Option<AfterReply>,
}

/// Waits until the child `pid` has exited by itself, `until` has come, `cancel` is set or its
/// output has gone past the limit, passing over whatever it prints meanwhile. The child is not
/// reaped here.
fn await_exit(
    pid: libc::pid_t,
    stdout_rx: &mpsc::Receiver<BoundedChunk>,
    until: Instant,
    cancel: &std::sync::atomic::AtomicBool,
) -> AfterReply {
    let waited = Instant::now();
    let killed = |why| AfterReply::Killed {
        after: waited.elapsed(),
        why,
    };
    loop {
        // `None`, where `waitid` cannot say, leaves the matter to the stop that follows.
        if leader::exited_unreaped(pid) != Some(false) {
            return AfterReply::Exited {
                after: waited.elapsed(),
            };
        }
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return killed("cancelled");
        }
        if Instant::now() >= until {
            return killed("it did not exit in time");
        }
        match stdout_rx.recv_timeout(BOUNDED_POLL) {
            Ok(BoundedChunk::Exceeded) => return killed("its output went past the limit"),
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            // Nothing more will come on stdout; the exit is polled.
            Err(mpsc::RecvTimeoutError::Disconnected) => thread::sleep(BOUNDED_POLL),
        }
    }
}

/// [`run_exchange`] with the time the program is given to exit after its reply set by
/// `reply_grace`, that also records the child's pid, the time the run took, its stderr and how it
/// ended after the reply in `trace`, whether or not a reply came.
///
/// After a reply its stdin is closed and the program is waited for, for `reply_grace` or until
/// the run's own deadline if that is nearer, with its output drained and still bounded, and
/// `cancel` still ending the wait. The program may still be finishing work begun for the request
/// (Claude Code can be writing a rotated OAuth token to the keychain through a `security` child of
/// its own session), which a kill at the reply could lose. Only a program that has not exited by
/// then is stopped. What it leaves in its group or session is swept as it is for any other end, and
/// the reply is returned whichever way the program ended. A `reply_grace` of zero stops at the
/// reply.
pub fn run_exchange_traced(
    command: &mut Command,
    input: &[String],
    bounds: &Bounds,
    cancel: &std::sync::atomic::AtomicBool,
    reply_grace: Duration,
    trace: &mut ExchangeTrace,
    mut is_reply: impl FnMut(&str) -> bool,
) -> std::result::Result<String, ExchangeFailure> {
    let Spawned {
        mut leader,
        stdin,
        stdout: stdout_rx,
        stderr: stderr_rx,
    } = spawn_bounded(command, bounds, true).map_err(ExchangeFailure::Failed)?;
    trace.pid = Some(leader.child.id());
    let started = Instant::now();
    // Held until the reply is in, or the run is stopped below: closing it early is what makes such
    // a program exit.
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
    if outcome.is_ok() && !reply_grace.is_zero() {
        // Closing stdin is what tells the program its input has ended.
        drop(stdin);
        let until = (Instant::now() + reply_grace).min(deadline);
        trace.after_reply = Some(await_exit(leader.pid(), &stdout_rx, until, cancel));
    }
    trace.elapsed = started.elapsed();
    leader.stop(cancel);
    match outcome {
        Err(ExchangeFailure::ClosedWithoutReply { .. }) => {
            let mut stderr = Vec::new();
            settle_stderr(&stderr_rx, &mut stderr);
            trace.stderr = stderr.clone();
            Err(ExchangeFailure::ClosedWithoutReply {
                status: leader.child.try_wait().ok().flatten(),
                stderr,
            })
        }
        Ok(reply) => {
            if matches!(trace.after_reply, Some(AfterReply::Exited { .. })) {
                // The program is gone and the stop swept what it left, so the pipe ends soon and
                // what it wrote is complete.
                settle_stderr(&stderr_rx, &mut trace.stderr);
            } else {
                // Otherwise the reply is not waited on for its stderr; what has arrived is enough.
                while let Ok(chunk) = stderr_rx.try_recv() {
                    if let BoundedChunk::Data(bytes) = chunk {
                        trace.stderr.extend_from_slice(&bytes);
                    }
                }
            }
            Ok(reply)
        }
        other => {
            settle_stderr(&stderr_rx, &mut trace.stderr);
            other
        }
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
            term_grace: Duration::ZERO,
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
            term_grace: Duration::ZERO,
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
            term_grace: Duration::ZERO,
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

    /// Runs `script` under `/bin/sh` through [`run_exchange_traced`], with no time to exit after
    /// the reply and `input` on its stdin, the script recording its pid first; `None` when the run
    /// ended before it did (see [`until_the_shell_ran`]).
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
        exchange_graced_by(
            script,
            input,
            bounds,
            cancel,
            Duration::ZERO,
            &mut ExchangeTrace::default(),
        )
    }

    /// [`exchange_cancelled_by`] with the `grace` the program is given to exit after its reply,
    /// and its `trace`.
    fn exchange_graced_by(
        script: &str,
        input: &[&str],
        bounds: Bounds,
        cancel: &AtomicBool,
        grace: Duration,
        trace: &mut ExchangeTrace,
    ) -> Option<(std::result::Result<String, ExchangeFailure>, i32)> {
        let pid_file = ScratchFile::new("env-shell-exchange", "pid");
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            &format!("echo $$ > '{}'; {script}", pid_file.display()),
        ]);
        let input: Vec<String> = input.iter().map(|line| line.to_string()).collect();
        let outcome = run_exchange_traced(
            &mut command,
            &input,
            &bounds,
            cancel,
            grace,
            trace,
            |line| line.starts_with("reply"),
        );
        Some((outcome, read_pid_file(&pid_file)?))
    }

    #[test]
    fn a_program_that_exits_when_its_stdin_closes_is_left_to_exit_by_itself() {
        // Replies, then idles until its input ends, as the agents do.
        let script = "read a; echo \"reply $a\"; cat >/dev/null; echo done >&2";
        let mut trace = ExchangeTrace::default();
        let (outcome, pid) = until_the_shell_ran(|| {
            exchange_graced_by(
                script,
                &["x"],
                exchange_bounds(PATIENCE),
                &AtomicBool::new(false),
                PATIENCE,
                &mut trace,
            )
        });
        assert_eq!(outcome.expect("a reply"), "reply x");
        assert!(
            matches!(trace.after_reply, Some(AfterReply::Exited { .. })),
            "{:?}",
            trace.after_reply
        );
        // It ran on to the end, which a kill at the reply would have cut short.
        assert_eq!(String::from_utf8_lossy(&trace.stderr).trim(), "done");
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn a_program_that_ignores_its_stdin_closing_is_killed_after_the_grace_with_its_reply_kept() {
        let grace = Duration::from_millis(300);
        let mut trace = ExchangeTrace::default();
        let (outcome, pid) = until_the_shell_ran(|| {
            exchange_graced_by(
                "read a; echo \"reply $a\"; exec /bin/sleep 300",
                &["x"],
                exchange_bounds(PATIENCE),
                &AtomicBool::new(false),
                grace,
                &mut trace,
            )
        });
        assert_eq!(outcome.expect("the reply survives the kill"), "reply x");
        match trace.after_reply {
            Some(AfterReply::Killed { after, .. }) => assert!(after >= grace, "{after:?}"),
            other => panic!("{other:?}"),
        }
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn the_grace_ends_with_the_run_deadline_when_that_is_nearer() {
        let mut trace = ExchangeTrace::default();
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            exchange_graced_by(
                "echo reply; exec /bin/sleep 300",
                &[],
                exchange_bounds(Duration::from_millis(400)),
                &AtomicBool::new(false),
                Duration::from_secs(60),
                &mut trace,
            )
        });
        assert_eq!(outcome.expect("the reply survives the kill"), "reply");
        assert!(matches!(trace.after_reply, Some(AfterReply::Killed { .. })));
        assert!(started.elapsed() < Duration::from_secs(30));
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn cancelling_during_the_grace_kills_the_program_and_keeps_the_reply() {
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::Relaxed);
        });
        let mut trace = ExchangeTrace::default();
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            exchange_graced_by(
                "echo reply; exec /bin/sleep 300",
                &[],
                exchange_bounds(PATIENCE),
                &cancel,
                Duration::from_secs(60),
                &mut trace,
            )
        });
        assert_eq!(outcome.expect("the reply survives the kill"), "reply");
        assert!(
            matches!(
                trace.after_reply,
                Some(AfterReply::Killed {
                    why: "cancelled",
                    ..
                })
            ),
            "{:?}",
            trace.after_reply
        );
        assert!(started.elapsed() < Duration::from_secs(30));
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    fn exchange_bounds(timeout: Duration) -> Bounds {
        Bounds {
            timeout,
            stdout_limit: 64 * 1024,
            stderr_limit: 1024,
            reach: Reach::ProcessGroup,
            term_grace: Duration::ZERO,
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
        /// Times out with time to exit on `SIGTERM`, which the stand-in does.
        TimesOutGraced,
        Cancelled,
        /// Replies and keeps running until it is killed.
        Replies,
        /// Replies and exits by itself when its input ends.
        RepliesThenExits,
    }

    /// Runs a stand-in agent CLI that leaves a helper in a process group of its own, through
    /// [`run_bounded`] or (`Replies`) [`run_exchange`] with [`Reach::Session`], ended as `end`
    /// says. Returns the helper's pid once the run is over, or `None` when the run ended before
    /// the stand-in recorded it (see [`until_the_shell_ran`]).
    fn session_run_ending(end: &SessionEnd) -> Option<libc::pid_t> {
        let pid_file = ScratchFile::new("session-run", "pid");
        let then = match end {
            SessionEnd::Exits => "exit 0",
            SessionEnd::TimesOut | SessionEnd::TimesOutGraced | SessionEnd::Cancelled => {
                "exec /bin/sleep 300"
            }
            SessionEnd::Replies => "read line; echo \"reply $line\"; exec /bin/sleep 300",
            SessionEnd::RepliesThenExits => "read line; echo \"reply $line\"; cat >/dev/null",
        };
        let mut command = Command::new("/bin/bash");
        command.args([
            "-c",
            &format!("{}; {then}", helper_in_its_own_group(&pid_file)),
        ]);
        let bounds = Bounds {
            timeout: match end {
                SessionEnd::TimesOut | SessionEnd::TimesOutGraced => Duration::from_millis(300),
                _ => PATIENCE,
            },
            stdout_limit: 64 * 1024,
            stderr_limit: 1024,
            reach: Reach::Session,
            term_grace: match end {
                SessionEnd::TimesOutGraced => PATIENCE,
                _ => Duration::ZERO,
            },
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
            SessionEnd::Replies | SessionEnd::RepliesThenExits => {
                let grace = match end {
                    SessionEnd::RepliesThenExits => PATIENCE,
                    _ => Duration::ZERO,
                };
                let reply = run_exchange_traced(
                    &mut command,
                    &["x".to_string()],
                    &bounds,
                    &AtomicBool::new(false),
                    grace,
                    &mut ExchangeTrace::default(),
                    |line| line.starts_with("reply"),
                );
                assert_eq!(reply.expect("a reply"), "reply x");
            }
            _ => {
                let outcome = run_bounded(&mut command, &bounds, &cancel);
                match (end, outcome) {
                    (SessionEnd::Exits, Ok(output)) => assert!(output.status.success()),
                    (
                        SessionEnd::TimesOut | SessionEnd::TimesOutGraced,
                        Err(BoundedFailure::TimedOut(_)),
                    )
                    | (SessionEnd::Cancelled, Err(BoundedFailure::Cancelled)) => {}
                    (_, other) => panic!("{end:?} ended as {other:?}"),
                }
            }
        }
        read_pid_file(&pid_file)
    }

    /// The bounds of a stand-in agent CLI that is stopped after 300 ms and given `term_grace` to
    /// exit on `SIGTERM`.
    fn terminable_bounds(term_grace: Duration) -> Bounds {
        Bounds {
            timeout: Duration::from_millis(300),
            stdout_limit: 1024,
            stderr_limit: 1024,
            reach: Reach::Session,
            term_grace,
        }
    }

    /// A stand-in that, on `SIGTERM`, writes `marker` and exits, and otherwise runs `then`: the
    /// marker is there only if the program was asked to exit, which a kill never does.
    fn term_marking_script(marker: &Path, then: &str) -> String {
        format!(
            "trap \"echo termed > '{}'; exit 0\" TERM; {then}",
            marker.display()
        )
    }

    #[test]
    fn a_timed_out_run_is_asked_to_exit_before_it_is_killed() {
        let marker = ScratchFile::new("term-handled", "marker");
        let script = term_marking_script(&marker, "while :; do /bin/sleep 0.05; done");
        let started = Instant::now();
        let (outcome, pid) =
            until_the_shell_ran(|| bounded_run_of(&script, terminable_bounds(PATIENCE), None));
        assert!(matches!(outcome, Err(BoundedFailure::TimedOut(_))));
        assert!(
            marker.exists(),
            "the program was killed without being asked to exit"
        );
        assert!(started.elapsed() < PATIENCE, "the grace was waited out");
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn a_run_that_ignores_sigterm_is_killed_once_the_grace_has_passed() {
        let grace = Duration::from_millis(600);
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            bounded_run_of(
                "trap '' TERM; while :; do /bin/sleep 0.05; done",
                terminable_bounds(grace),
                None,
            )
        });
        assert!(matches!(outcome, Err(BoundedFailure::TimedOut(_))));
        assert!(
            started.elapsed() >= Duration::from_millis(300) + grace,
            "killed before the grace passed: {:?}",
            started.elapsed()
        );
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn a_cancelled_run_is_asked_to_exit_before_it_is_killed() {
        let marker = ScratchFile::new("term-handled-cancel", "marker");
        let script = term_marking_script(&marker, "while :; do /bin/sleep 0.05; done");
        let bounds = Bounds {
            timeout: PATIENCE,
            ..terminable_bounds(PATIENCE)
        };
        let (outcome, pid) = until_the_shell_ran(|| {
            bounded_run_of(&script, bounds, Some(Duration::from_millis(300)))
        });
        assert!(matches!(outcome, Err(BoundedFailure::Cancelled)));
        assert!(
            marker.exists(),
            "the program was killed without being asked to exit"
        );
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn a_stop_in_progress_waits_no_longer_than_the_cancel_cap_once_cancelled() {
        // Times out at 300 ms with a grace far longer than `PATIENCE`, and is cancelled while it
        // waits out that grace: well after the shell started, so the timeout has fired first.
        let bounds = terminable_bounds(Duration::from_secs(120));
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = cancel.clone();
            let pid_file = ScratchFile::new("cancel-cap", "pid");
            let pid_path = pid_file.to_path_buf();
            thread::spawn(move || {
                let deadline = Instant::now() + PATIENCE;
                while !pid_path.exists() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(20));
                }
                thread::sleep(Duration::from_millis(1500));
                flag.store(true, Ordering::Relaxed);
            });
            let mut command = Command::new("/bin/sh");
            command.args([
                "-c",
                &format!(
                    "trap '' TERM; echo $$ > '{}'; while :; do /bin/sleep 0.05; done",
                    pid_file.display()
                ),
            ]);
            let outcome = run_bounded(&mut command, &bounds, &cancel);
            Some((outcome, read_pid_file(&pid_file)?))
        });
        assert!(matches!(outcome, Err(BoundedFailure::TimedOut(_))));
        let waited = started.elapsed();
        assert!(
            waited >= CANCEL_TERM_GRACE && waited < PATIENCE,
            "{waited:?}"
        );
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn a_program_still_running_after_the_reply_grace_is_asked_to_exit_before_it_is_killed() {
        let marker = ScratchFile::new("term-handled-reply", "marker");
        let script = term_marking_script(
            &marker,
            "read a; echo \"reply $a\"; while :; do /bin/sleep 0.05; done",
        );
        let bounds = Bounds {
            timeout: PATIENCE,
            ..terminable_bounds(PATIENCE)
        };
        let mut trace = ExchangeTrace::default();
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            exchange_graced_by(
                &script,
                &["x"],
                bounds,
                &AtomicBool::new(false),
                Duration::from_millis(200),
                &mut trace,
            )
        });
        assert_eq!(outcome.expect("the reply survives the stop"), "reply x");
        assert!(matches!(trace.after_reply, Some(AfterReply::Killed { .. })));
        assert!(started.elapsed() < PATIENCE, "the grace was waited out");
        assert!(
            marker.exists(),
            "the program was killed without being asked to exit"
        );
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn a_run_that_reaches_the_session_leaves_no_helper_behind_however_it_ends() {
        for end in [
            SessionEnd::Exits,
            SessionEnd::TimesOut,
            SessionEnd::TimesOutGraced,
            SessionEnd::Cancelled,
            SessionEnd::Replies,
            SessionEnd::RepliesThenExits,
        ] {
            let helper = until_the_shell_ran(|| session_run_ending(&end));
            let mut guard = KillOnDrop(vec![helper]);
            assert_stops_running(helper);
            guard.0.clear();
        }
    }
}
