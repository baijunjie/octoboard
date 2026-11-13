//! Running a subprocess to completion within bounds, or detaching an external application.
//! Bounded runs and an exchange of lines with a program that answers on stdout while its stdin
//! stays open both have a deadline, an output ceiling, and a cancel. Every stop kills
//! more than the direct child, as far as the run's [`Reach`] says: the whole process group, so
//! anything it forked (`ssh`, an askpass helper) goes with it, or, for an agent CLI, its whole
//! session. Capturing the login shell's environment, which can leave a background process of its
//! own holding a pipe open and so needs a completion marker rather than end-of-file, is
//! `crate::env_shell`'s.

use std::io::{self, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::platform::fs::create_private;
use crate::platform::proc::{
    self, bounded_pipe_reader, BoundedChunk, ChildHandle, Polite, ProcessId, Reach, REAP_GRACE,
};

/// Starts `command` apart from the daemon and reaps it on another thread. Being apart (a new
/// session) is what keeps the application alive after the daemon exits; the thread is what keeps
/// it from staying a zombie until then. A failure to exec is reported by `spawn` itself.
pub fn spawn_detached(mut command: Command) -> Result<(), String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    proc::detach(&mut command);
    let mut child = command.spawn().map_err(|err| err.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// How long the stop of an agent CLI run leaves the CLI to exit after `SIGTERM` before it is
/// killed. Just above the five seconds Claude Code's own shutdown allows itself before it forces
/// an exit, so a CLI that is cleaning up is not cut short.
const AGENT_TERM_GRACE: Duration = Duration::from_secs(6);

/// The most a stop waits for its CLI after `SIGTERM` once the run is cancelled, which is how a
/// daemon shutdown reaches the runs it can cancel. The application gives a stopping daemon a
/// bounded time to exit (see "Quitting" and "When the system ends the session" in
/// `docs/product/application-lifecycle.md`) and then kills it, which would leave a CLI that
/// ignores `SIGTERM` behind; this keeps the runs' waits within that. Longer than the two seconds
/// Claude Code's own cleanup is allowed to take.
const CANCEL_TERM_GRACE: Duration = Duration::from_secs(3);

/// A spawned child and what stopping it reaches.
struct Leader {
    child: Child,
    /// The child as the platform signals and sweeps it, while it is unreaped.
    handle: ChildHandle,
    reach: Reach,
    /// How long a stop leaves the child to exit after `SIGTERM` before the kill; see
    /// [`Bounds::term_grace`].
    term_grace: Duration,
    /// Names the child in the log of a session sweep.
    label: String,
}

impl Leader {
    fn pid(&self) -> ProcessId {
        self.handle.pid()
    }

    /// Polls the child without blocking, reaping it once it has exited. For a session, what the
    /// child left there is killed first, while its zombie keeps the session id from passing to
    /// another process; when `waitid` cannot tell whether it has exited, `try_wait` decides and
    /// nothing is swept.
    fn try_reap(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.reach == Reach::Session {
            match self.handle.exited_unreaped() {
                Some(false) => return Ok(None),
                Some(true) => self.handle.kill_leftovers(&[]).log(self.pid(), &self.label),
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
        // A host that has no polite request for the child leaves nothing to wait for.
        if !self.term_grace.is_zero() && proc::stop_group(pid, Polite::Terminate).delivered() {
            let mut deadline = Instant::now() + self.term_grace;
            let mut cancelled = false;
            // A group member the leader outlives is killed below; `None`, where `waitid` cannot
            // say, ends the wait for the kill to decide.
            while self.handle.exited_unreaped() == Some(false) && Instant::now() < deadline {
                if !cancelled && cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    cancelled = true;
                    deadline = deadline.min(Instant::now() + CANCEL_TERM_GRACE);
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        self.handle.kill();
        if self.reach == Reach::Session {
            // Swept once the child has exited, so that it can start nothing after the sweep; one
            // that outlives the grace is swept as it is, still unreaped. When `waitid` cannot
            // tell, the child may have been reaped and its pid passed on, so nothing is swept.
            let exit_deadline = Instant::now() + REAP_GRACE;
            let unreaped = loop {
                match self.handle.exited_unreaped() {
                    Some(false) if Instant::now() < exit_deadline => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Some(_) => break true,
                    None => break false,
                }
            };
            if unreaped {
                self.handle.kill_leftovers(&[]).log(pid, &self.label);
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
    let bounds = Bounds {
        timeout,
        stdout_limit: usize::MAX,
        stderr_limit: usize::MAX,
        reach: Reach::Group,
        term_grace: Duration::ZERO,
    };
    run_bounded_with(command, &bounds, &NEVER, input).map_err(BoundedFailure::into_error)
}

/// A cancel flag that is never set, for a run nothing cancels.
static NEVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

/// A spawned child with its pipes read on threads of their own, bounded as `bounds` says.
struct Spawned {
    leader: Leader,
    /// Present only when the caller asked for stdin to be piped.
    stdin: Option<std::process::ChildStdin>,
    stdout: mpsc::Receiver<BoundedChunk>,
    stderr: mpsc::Receiver<BoundedChunk>,
}

/// Starts `command`, whose standard streams the caller has set, leading the process group or the
/// session `reach` names.
fn spawn_leader(
    command: &mut Command,
    reach: Reach,
    term_grace: Duration,
) -> std::result::Result<Leader, anyhow::Error> {
    reach.lead(command);
    let label = Path::new(command.get_program())
        .file_name()
        .unwrap_or(command.get_program())
        .to_string_lossy()
        .into_owned();
    let mut child = command.spawn().context("spawning the command")?;
    let handle = match ChildHandle::started(ProcessId(child.id()), reach, None) {
        Ok(handle) => handle,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err).context("taking on the command");
        }
    };
    Ok(Leader {
        child,
        handle,
        reach,
        term_grace,
        label,
    })
}

/// Starts `command` leading the process group or the session `bounds.reach` names, with stdout
/// and stderr piped (and stdin piped or null) and a reader thread on each pipe: stdout stops at
/// `bounds.stdout_limit`, stderr is kept up to `bounds.stderr_limit` and the rest discarded.
fn spawn_bounded(
    command: &mut Command,
    bounds: &Bounds,
    pipe_stdin: bool,
) -> std::result::Result<Spawned, anyhow::Error> {
    command.stdin(if pipe_stdin {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut leader = spawn_leader(command, bounds.reach, bounds.term_grace)?;

    let stdin = leader.child.stdin.take();
    let stdout_pipe = leader.child.stdout.take().expect("stdout was piped");
    let stderr_pipe = leader.child.stderr.take().expect("stderr was piped");
    let (stdout_tx, stdout) = mpsc::channel();
    let (stderr_tx, stderr) = mpsc::channel();
    let stdout_limit = bounds.stdout_limit;
    let stderr_limit = bounds.stderr_limit;
    thread::spawn(move || bounded_pipe_reader(stdout_pipe, &stdout_tx, stdout_limit, true));
    thread::spawn(move || bounded_pipe_reader(stderr_pipe, &stderr_tx, stderr_limit, false));
    Ok(Spawned {
        leader,
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
                if leader.reach == Reach::Group && leader.handle.group_has_members() == Some(true) {
                    leader.handle.kill();
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

/// Runs an already-configured `command` to completion with its stdout and stderr written to the
/// file at `output` (created, or truncated), killing what `reach` names after `term_grace` of
/// `SIGTERM` if it is still running after `timeout`. For a command that is to outlive the
/// daemon: with no pipe to the daemon, its output does not stop it when the daemon is gone, and
/// nothing here cancels it. The file is not bounded in size: it is the output of a package
/// manager's or an agent's own updater, which prints a few kilobytes, and the next run to the same
/// path truncates it; a stream that filled the disk within `timeout` is not a case worth a cap.
pub fn run_to_file(
    command: &mut Command,
    output: &Path,
    timeout: Duration,
    reach: Reach,
    term_grace: Duration,
) -> std::result::Result<ExitStatus, BoundedFailure> {
    let (stdout, stderr) = create_private(
        output,
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true),
    )
    .and_then(|file| file.try_clone().map(|again| (file, again)))
    .with_context(|| format!("opening {} for the output", output.display()))
    .map_err(BoundedFailure::Failed)?;
    command.stdin(Stdio::null()).stdout(stdout).stderr(stderr);
    let mut leader = spawn_leader(command, reach, term_grace).map_err(BoundedFailure::Failed)?;
    let deadline = Instant::now() + timeout;
    loop {
        match leader.try_reap() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(err) => {
                leader.stop(&NEVER);
                return Err(BoundedFailure::Failed(err.into()));
            }
        }
        if Instant::now() >= deadline {
            leader.stop(&NEVER);
            return Err(BoundedFailure::TimedOut(timeout));
        }
        thread::sleep(BOUNDED_POLL);
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

/// Waits until `child` has exited by itself, `until` has come, `cancel` is set or its
/// output has gone past the limit, passing over whatever it prints meanwhile. The child is not
/// reaped here.
fn await_exit(
    child: &ChildHandle,
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
        if child.exited_unreaped() != Some(false) {
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
    // The leading bytes of `pending` known to hold no newline; see `take_reply`.
    let mut scanned = 0;
    let outcome = loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break Err(ExchangeFailure::Cancelled);
        }
        // Checked on every pass, not only when a poll comes up empty: a child that keeps writing
        // would otherwise never reach the deadline.
        if Instant::now() >= deadline {
            break Err(ExchangeFailure::TimedOut(bounds.timeout));
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match stdout_rx.recv_timeout(left.min(BOUNDED_POLL)) {
            Ok(BoundedChunk::Data(bytes)) => {
                pending.extend_from_slice(&bytes);
                if let Some(reply) = take_reply(&mut pending, &mut scanned, &mut is_reply) {
                    break Ok(reply);
                }
            }
            Ok(BoundedChunk::Exceeded) => break Err(ExchangeFailure::StdoutExceeded(stdout_limit)),
            Ok(BoundedChunk::Failed(err)) => {
                break Err(ExchangeFailure::Failed(
                    anyhow::Error::new(err).context("reading stdout from the subprocess"),
                ))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // The last line may come without its newline.
                pending.push(b'\n');
                break match take_reply(&mut pending, &mut scanned, &mut is_reply) {
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
        trace.after_reply = Some(await_exit(&leader.handle, &stdout_rx, until, cancel));
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
/// that `is_reply` accepts; what follows it, and any partial line, stays. The first `scanned`
/// bytes of `pending` are known to hold no newline and are not searched again; it is left
/// covering what this call searched, so the cost of a line stays linear in its length however
/// many chunks it arrives in.
fn take_reply(
    pending: &mut Vec<u8>,
    scanned: &mut usize,
    is_reply: &mut impl FnMut(&str) -> bool,
) -> Option<String> {
    while let Some(offset) = pending[*scanned..].iter().position(|byte| *byte == b'\n') {
        let end = *scanned + offset;
        *scanned = 0;
        let line: Vec<u8> = pending.drain(..=end).collect();
        let line = String::from_utf8_lossy(&line);
        let line = line.trim();
        if !line.is_empty() && is_reply(line) {
            return Some(line.to_string());
        }
    }
    *scanned = pending.len();
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::test_support::helper_in_its_own_group;
    use crate::test_support::{
        assert_gone_by, assert_stops_running, read_pid_file, until_the_shell_ran, KillOnDrop,
        ScratchFile, PATIENCE,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// Runs `script` under bash through [`run_bounded`], the script recording its pid in a
    /// file first — after `setup`, a trap that has to be in place before anything signals it —
    /// and returns the outcome with that pid once the script has written it, or `None` when the
    /// run ended before it did, which a stalled exec can cause (see [`until_the_shell_ran`]).
    /// When `cancelled`, the run is cancelled once the pid is recorded.
    fn bounded_run_of(
        setup: &str,
        script: &str,
        bounds: Bounds,
        cancelled: bool,
    ) -> Option<(std::result::Result<Output, BoundedFailure>, i32)> {
        let pid_file = ScratchFile::new("env-shell-bounded", "pid");
        let mut command = Command::new(crate::test_support::bash());
        command.args([
            "-c",
            &format!(
                "{setup} echo {} > '{}'; {script}",
                crate::test_support::SHELL_PID,
                pid_file.display()
            ),
        ]);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        if cancelled {
            let (cancel, pid_file) = (cancel.clone(), pid_file.to_path_buf());
            thread::spawn(move || {
                crate::session::wait::wait_for(PATIENCE, || read_pid_file(&pid_file).is_some());
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        }
        let outcome = run_bounded(&mut command, &bounds, &cancel);
        Some((outcome, read_pid_file(&pid_file)?))
    }

    #[test]
    fn input_larger_than_the_pipe_reaches_the_command_whole() {
        let input = vec![b'x'; 1 << 20];
        let output = run_with_input(
            &mut crate::test_support::programs::hold_stdin().command(),
            input.clone(),
            PATIENCE,
        )
        .unwrap();
        assert_eq!(output.stdout, input);
    }

    #[test]
    fn a_bounded_run_stopped_for_any_reason_leaves_no_process_behind() {
        let bounds = Bounds {
            timeout: PATIENCE,
            stdout_limit: 64 * 1024,
            stderr_limit: 1024,
            reach: Reach::Group,
            term_grace: Duration::ZERO,
        };
        let short = Bounds {
            timeout: Duration::from_millis(300),
            ..bounds
        };
        // `exec`, so the pid recorded is the process doing the work, not a shell waiting on it.
        let cases: [(&str, Bounds, bool); 3] = [
            ("exec /usr/bin/yes", bounds, false),
            ("exec /bin/sleep 300", bounds, true),
            ("exec /bin/sleep 300", short, false),
        ];
        for (script, bounds, cancelled) in cases {
            let (outcome, pid) =
                until_the_shell_ran(|| bounded_run_of("", script, bounds, cancelled));
            let failure = outcome.expect_err("the run was stopped");
            match (script, cancelled) {
                ("exec /usr/bin/yes", _) => {
                    assert!(
                        matches!(failure, BoundedFailure::StdoutExceeded(_)),
                        "{failure}"
                    )
                }
                (_, true) => assert!(matches!(failure, BoundedFailure::Cancelled), "{failure}"),
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
            reach: Reach::Group,
            term_grace: Duration::ZERO,
        };
        let (outcome, pid) = until_the_shell_ran(|| {
            let pid_file = ScratchFile::new("env-shell-leftover", "pid");
            let mut command = leaving_its_output_open(&pid_file);
            let never = std::sync::atomic::AtomicBool::new(false);
            let outcome = run_bounded(&mut command, &bounds, &never);
            Some((outcome, read_pid_file(&pid_file)?))
        });
        let failure = outcome.expect_err("output still held open is not whole");
        assert!(matches!(failure, BoundedFailure::Failed(_)), "{failure}");
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    /// A command that starts a process holding its output open, writes that process's pid to
    /// `pid_file`, writes `partial` and exits.
    #[cfg(unix)]
    fn leaving_its_output_open(pid_file: &Path) -> Command {
        let mut command = Command::new(crate::test_support::bash());
        let script = format!(
            "/bin/sleep 300 & echo $! > '{}'; echo partial",
            pid_file.display()
        );
        command.args(["-c", &script]);
        command
    }

    /// A command that starts a process holding its output open, writes that process's pid to
    /// `pid_file`, writes `partial` and exits: a fixture process, whose pids are Windows' own.
    #[cfg(windows)]
    fn leaving_its_output_open(pid_file: &Path) -> Command {
        use crate::test_support::fixture::Steps;
        Steps::new()
            .spawn_with_streams("sleep", Steps::new().sleep(300))
            .write_child_pid(pid_file)
            .say("partial")
            .program("leaver")
            .command()
    }

    #[test]
    fn a_bounded_run_keeps_its_stderr_to_the_limit_without_failing() {
        let bounds = Bounds {
            timeout: PATIENCE,
            stdout_limit: 1024,
            stderr_limit: 10,
            reach: Reach::Group,
            term_grace: Duration::ZERO,
        };
        let mut command = Command::new(crate::test_support::bash());
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

    /// Runs `script` under bash through [`run_exchange_traced`], with no time to exit after
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
        exchange_judged_by(script, input, bounds, cancel, grace, trace, |line| {
            line.starts_with("reply")
        })
    }

    /// [`exchange_graced_by`] with `is_reply` to tell the reply line.
    fn exchange_judged_by(
        script: &str,
        input: &[&str],
        bounds: Bounds,
        cancel: &AtomicBool,
        grace: Duration,
        trace: &mut ExchangeTrace,
        is_reply: impl FnMut(&str) -> bool,
    ) -> Option<(std::result::Result<String, ExchangeFailure>, i32)> {
        let pid_file = ScratchFile::new("env-shell-exchange", "pid");
        let mut command = Command::new(crate::test_support::bash());
        command.args([
            "-c",
            &format!(
                "echo {} > '{}'; {script}",
                crate::test_support::SHELL_PID,
                pid_file.display()
            ),
        ]);
        let input: Vec<String> = input.iter().map(|line| line.to_string()).collect();
        let outcome = run_exchange_traced(
            &mut command,
            &input,
            &bounds,
            cancel,
            grace,
            trace,
            is_reply,
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
        // The program marks the grace once its stdin closes, which happens only after its reply
        // was taken; the cancel follows the mark, so it lands in the grace.
        let in_grace = ScratchFile::new("env-shell-exchange-grace", "mark");
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let (flag, mark) = (cancel.clone(), in_grace.to_path_buf());
        thread::spawn(move || {
            if crate::session::wait::wait_for(PATIENCE, || mark.exists()) {
                flag.store(true, Ordering::Relaxed);
            }
        });
        let script = format!(
            "echo reply; cat > /dev/null; : > '{}'; exec /bin/sleep 300",
            in_grace.display()
        );
        let mut trace = ExchangeTrace::default();
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            exchange_graced_by(
                &script,
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
            reach: Reach::Group,
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
    fn an_exchange_whose_child_keeps_writing_still_ends_at_the_deadline() {
        // The child writes without pause and the reader is slower than it, so output is always
        // waiting and no poll for it ends empty: only the check on each pass reaches the deadline.
        // The limit is far above what the child writes in that time.
        let slow = Duration::from_micros(200);
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            exchange_judged_by(
                "while :; do echo noise; done",
                &[],
                Bounds {
                    stdout_limit: 64 * 1024 * 1024,
                    ..exchange_bounds(Duration::from_millis(500))
                },
                &AtomicBool::new(false),
                Duration::ZERO,
                &mut ExchangeTrace::default(),
                |_| {
                    // A spin, not a wait on time: the cost of judging one line.
                    let judged = Instant::now();
                    while judged.elapsed() < slow {
                        std::hint::spin_loop();
                    }
                    false
                },
            )
        });
        let failure = outcome.expect_err("no reply");
        assert!(matches!(failure, ExchangeFailure::TimedOut(_)), "{failure}");
        assert!(started.elapsed() < Duration::from_secs(20));
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    fn an_exchange_that_is_cancelled_ends_at_once_and_leaves_nothing_behind() {
        let (outcome, pid) = until_the_shell_ran(|| {
            let cancel = Arc::new(AtomicBool::new(false));
            let setter = cancel.clone();
            // A child that never replies, so only the flag can end the run before its timeout. It
            // is cancelled once it runs, which it marks.
            let running = ScratchFile::new("env-shell-exchange-cancel", "mark");
            let mark = running.to_path_buf();
            let script = format!(
                "echo noise; : > '{}'; exec /bin/sleep 300",
                running.display()
            );
            let started = Instant::now();
            let canceller = thread::spawn(move || {
                let marked = crate::session::wait::wait_for(PATIENCE, || mark.exists());
                setter.store(true, Ordering::Relaxed);
                marked
            });
            let ran = exchange_cancelled_by(&script, &[], exchange_bounds(PATIENCE), &cancel);
            let marked = canceller.join().unwrap();
            // A run that ended before the shell recorded its pid is run again.
            let ran = ran?;
            assert!(marked, "the child never marked that it runs");
            assert!(
                started.elapsed() < PATIENCE / 2,
                "the cancel was not noticed"
            );
            Some(ran)
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
    fn session_run_ending(end: &SessionEnd) -> Option<i32> {
        let pid_file = ScratchFile::new("session-run", "pid");
        let mut command = session_stand_in(&pid_file, end);
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

    /// The stand-in of [`session_run_ending`]: it leaves a helper in a process group of its own,
    /// whose pid it writes to `pid_file`, and then goes on as `end` says.
    #[cfg(unix)]
    fn session_stand_in(pid_file: &Path, end: &SessionEnd) -> Command {
        let then = match end {
            SessionEnd::Exits => "exit 0",
            SessionEnd::TimesOut | SessionEnd::TimesOutGraced | SessionEnd::Cancelled => {
                "exec /bin/sleep 300"
            }
            SessionEnd::Replies => "read line; echo \"reply $line\"; exec /bin/sleep 300",
            SessionEnd::RepliesThenExits => "read line; echo \"reply $line\"; cat >/dev/null",
        };
        let mut command = Command::new(crate::test_support::bash());
        command.args([
            "-c",
            &format!("{}; {then}", helper_in_its_own_group(pid_file)),
        ]);
        command
    }

    /// The stand-in of [`session_run_ending`] on Windows, a fixture process: its helper is on its
    /// console, which is its session there. It writes the helper's pid to `pid_file` once the
    /// helper has written it beside it, which the helper does once it runs, and so is on the
    /// console by then.
    #[cfg(windows)]
    fn session_stand_in(pid_file: &Path, end: &SessionEnd) -> Command {
        use crate::test_support::fixture::Steps;
        let running = pid_file.with_extension("helper");
        let steps = Steps::new()
            .spawn_on_console("helper", Steps::new().write_pid(&running).sleep(300))
            .wait_for(&running)
            .write_child_pid(pid_file);
        let steps = match end {
            SessionEnd::Exits => steps,
            SessionEnd::TimesOut | SessionEnd::TimesOutGraced | SessionEnd::Cancelled => {
                steps.sleep(300)
            }
            SessionEnd::Replies => steps.reply().sleep(300),
            SessionEnd::RepliesThenExits => steps.reply().read_input(),
        };
        steps.program("stand-in").command()
    }

    /// The bounds of a stand-in agent CLI that is stopped after 300 ms and given `term_grace` to
    /// exit on `SIGTERM`.
    #[cfg(unix)]
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
    #[cfg(unix)]
    fn term_marking_script(marker: &Path, then: &str) -> String {
        format!("{} {then}", term_marking_trap(marker))
    }

    /// The trap of [`term_marking_script`], for [`bounded_run_of`] to set before the pid it waits
    /// on is recorded: a `SIGTERM` that came before it would end the stand-in unmarked.
    #[cfg(unix)]
    fn term_marking_trap(marker: &Path) -> String {
        format!(
            "trap \"echo termed > '{}'; exit 0\" TERM;",
            marker.display()
        )
    }

    #[test]
    // Unix only: a process without a terminal takes no polite request on Windows.
    #[cfg(unix)]
    fn a_timed_out_run_is_asked_to_exit_before_it_is_killed() {
        let marker = ScratchFile::new("term-handled", "marker");
        let trap = term_marking_trap(&marker);
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            bounded_run_of(
                &trap,
                "while :; do /bin/sleep 0.05; done",
                terminable_bounds(PATIENCE),
                false,
            )
        });
        assert!(matches!(outcome, Err(BoundedFailure::TimedOut(_))));
        assert!(
            marker.exists(),
            "the program was killed without being asked to exit"
        );
        assert!(started.elapsed() < PATIENCE, "the grace was waited out");
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    // Unix only: a process without a terminal takes no polite request on Windows.
    #[cfg(unix)]
    fn a_run_that_ignores_sigterm_is_killed_once_the_grace_has_passed() {
        let grace = Duration::from_millis(600);
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            bounded_run_of(
                "trap '' TERM;",
                "while :; do /bin/sleep 0.05; done",
                terminable_bounds(grace),
                false,
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
    // Unix only: a process without a terminal takes no polite request on Windows.
    #[cfg(unix)]
    fn a_cancelled_run_is_asked_to_exit_before_it_is_killed() {
        let marker = ScratchFile::new("term-handled-cancel", "marker");
        let trap = term_marking_trap(&marker);
        let bounds = Bounds {
            timeout: PATIENCE,
            ..terminable_bounds(PATIENCE)
        };
        let (outcome, pid) = until_the_shell_ran(|| {
            bounded_run_of(&trap, "while :; do /bin/sleep 0.05; done", bounds, true)
        });
        assert!(matches!(outcome, Err(BoundedFailure::Cancelled)));
        assert!(
            marker.exists(),
            "the program was killed without being asked to exit"
        );
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }

    #[test]
    // Unix only: a process without a terminal takes no polite request on Windows.
    #[cfg(unix)]
    fn a_stop_in_progress_waits_no_longer_than_the_cancel_cap_once_cancelled() {
        // Times out at 300 ms with a grace far longer than `PATIENCE`, and is cancelled while it
        // waits out that grace: once the stand-in, which outlives the `SIGTERM`, has marked it, so
        // the timeout has fired first.
        let bounds = terminable_bounds(Duration::from_secs(120));
        let started = Instant::now();
        let (outcome, pid) = until_the_shell_ran(|| {
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = cancel.clone();
            let pid_file = ScratchFile::new("cancel-cap", "pid");
            let termed = ScratchFile::new("cancel-cap-termed", "mark");
            let mark = termed.to_path_buf();
            thread::spawn(move || {
                crate::session::wait::wait_for(PATIENCE, || mark.exists());
                flag.store(true, Ordering::Relaxed);
            });
            let mut command = Command::new(crate::test_support::bash());
            command.args([
                "-c",
                &format!(
                    "trap \": > '{}'\" TERM; echo {} > '{}'; while :; do /bin/sleep 0.05; done",
                    termed.display(),
                    crate::test_support::SHELL_PID,
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
    // Unix only: a process without a terminal takes no polite request on Windows.
    #[cfg(unix)]
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

    /// Feeds `chunks` to [`take_reply`] one at a time as the exchange's read loop does,
    /// accepting lines that start with `reply`, and returns what each call returned, with the
    /// `pending` and `scanned` it was left with.
    fn replies_taken_from(chunks: &[&[u8]]) -> (Vec<Option<String>>, Vec<u8>, usize) {
        let mut pending = Vec::new();
        let mut scanned = 0;
        let mut taken = Vec::new();
        for chunk in chunks {
            pending.extend_from_slice(chunk);
            taken.push(take_reply(&mut pending, &mut scanned, &mut |line| {
                line.starts_with("reply")
            }));
        }
        (taken, pending, scanned)
    }

    #[test]
    fn a_line_arriving_in_many_chunks_is_returned_whole_once_its_newline_comes() {
        let (taken, pending, scanned) =
            replies_taken_from(&[b"re", b"p", b"ly spl", b"it acr", b"oss", b"\n"]);
        assert_eq!(
            taken,
            vec![
                None,
                None,
                None,
                None,
                None,
                Some("reply split across".to_string())
            ]
        );
        assert!(pending.is_empty());
        assert_eq!(scanned, 0);
    }

    #[test]
    fn a_newline_that_begins_the_next_chunk_ends_the_line_scanned_so_far() {
        // The newline is the very first byte after what was already scanned: a scan that
        // starts one byte too late would skip it and wait for a later newline.
        let (taken, pending, _) = replies_taken_from(&[b"reply abc", b"\n"]);
        assert_eq!(taken, vec![None, Some("reply abc".to_string())]);
        assert!(pending.is_empty());
    }

    #[test]
    fn several_lines_in_one_chunk_give_the_first_reply_and_keep_what_follows() {
        let mut pending = b"reply one\nreply two\npartial".to_vec();
        let mut scanned = 0;
        let mut accept = |line: &str| line.starts_with("reply");

        assert_eq!(
            take_reply(&mut pending, &mut scanned, &mut accept),
            Some("reply one".to_string())
        );
        assert_eq!(pending, b"reply two\npartial");
        assert_eq!(scanned, 0);

        assert_eq!(
            take_reply(&mut pending, &mut scanned, &mut accept),
            Some("reply two".to_string())
        );
        assert_eq!(pending, b"partial");
        assert_eq!(scanned, 0);

        assert_eq!(take_reply(&mut pending, &mut scanned, &mut accept), None);
        assert_eq!(pending, b"partial");
        assert_eq!(scanned, pending.len());
    }

    #[test]
    fn lines_the_caller_rejects_are_skipped_up_to_the_first_it_accepts() {
        let mut pending = b"noise\nmore noise\nreply ok\nreply later\n".to_vec();
        let mut scanned = 0;
        let mut seen = Vec::new();
        let reply = take_reply(&mut pending, &mut scanned, &mut |line| {
            seen.push(line.to_string());
            line.starts_with("reply")
        });
        assert_eq!(reply, Some("reply ok".to_string()));
        assert_eq!(seen, ["noise", "more noise", "reply ok"]);
        assert_eq!(
            pending, b"reply later\n",
            "what follows the reply is left alone"
        );
    }

    #[test]
    fn a_rejected_line_ending_in_a_later_chunk_leaves_the_following_reply_findable() {
        // The first chunk leaves `scanned` past the length of what the second chunk's drain
        // leaves behind: a scan that is not restarted at the front of the remainder would
        // start beyond its end and miss, or fail on, the reply there.
        let (taken, pending, scanned) =
            replies_taken_from(&[b"a rejected line that is fairly long", b"\nreply x\n"]);
        assert_eq!(taken, vec![None, Some("reply x".to_string())]);
        assert!(pending.is_empty());
        assert_eq!(scanned, 0);
    }

    #[test]
    fn a_partial_line_after_a_reply_is_completed_by_later_chunks() {
        let (taken, pending, scanned) =
            replies_taken_from(&[b"reply a\nreply b", b" and c", b"\n"]);
        assert_eq!(
            taken,
            vec![
                Some("reply a".to_string()),
                None,
                Some("reply b and c".to_string())
            ]
        );
        assert!(pending.is_empty());
        assert_eq!(scanned, 0);
    }

    #[test]
    fn blank_lines_are_skipped_without_asking_the_caller_and_lines_are_trimmed() {
        let mut pending = b"\n   \r\n  reply padded \r\n".to_vec();
        let mut scanned = 0;
        let mut seen = Vec::new();
        let reply = take_reply(&mut pending, &mut scanned, &mut |line| {
            seen.push(line.to_string());
            true
        });
        assert_eq!(reply, Some("reply padded".to_string()));
        assert_eq!(seen, ["reply padded"]);
    }

    #[test]
    fn a_character_split_across_chunks_is_returned_intact() {
        let line = "reply caf\u{e9} \u{1f600}";
        let bytes = line.as_bytes();
        // Cut inside the two-byte and the four-byte characters.
        let (first, rest) = bytes.split_at(bytes.len() - 5);
        let (second, third) = rest.split_at(2);
        let (taken, _, _) = replies_taken_from(&[first, second, third, b"\n"]);
        assert_eq!(taken.last().cloned().flatten(), Some(line.to_string()));
        assert!(taken[..3].iter().all(Option::is_none));
    }

    #[test]
    fn a_nine_mebibyte_line_in_pipe_sized_chunks_is_returned_whole_and_judged_once() {
        let line = format!("reply {}", "x".repeat(9 * 1024 * 1024));
        let mut input = line.clone().into_bytes();
        input.push(b'\n');

        let mut pending = Vec::new();
        let mut scanned = 0;
        let mut judged = 0;
        let mut taken = None;
        for chunk in input.chunks(8 * 1024) {
            pending.extend_from_slice(chunk);
            taken = take_reply(&mut pending, &mut scanned, &mut |_| {
                judged += 1;
                true
            });
            if taken.is_none() {
                assert_eq!(
                    scanned,
                    pending.len(),
                    "a chunk with no newline is searched once and not again"
                );
            }
        }
        assert_eq!(taken.as_deref(), Some(line.as_str()));
        assert_eq!(judged, 1);
        assert!(pending.is_empty());
    }
}
