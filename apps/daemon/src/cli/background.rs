//! `start`, `stop`, `restart` and `status`: running the daemon in the background from the command
//! line (`address` is in `address.rs`). A running daemon is found through the instance lock
//! ([`instance_lock::is_running`]), which is authoritative; what it records beside the lock
//! ([`Owner`]) says which process to stop, where it listens and what key it requires, and each
//! command waits for a planned restart caught half-way to settle before it judges
//! ([`find_daemon`]). Each returns the exit status. The daemon itself starts its successor after a
//! planned restart the way `start` starts it ([`start_successor`]). `stop` signals the daemon where
//! the host has a signal for it, and where it has none (Windows) asks over loopback, with the key
//! the daemon recorded ([`request_over_loopback`]); `restart` asks over loopback on every host
//! wherever the key is recorded, so that a refusal comes back with its reason, and signals (outside
//! Windows) only a daemon whose key is not recorded. `address --rotate` asks over loopback too, for
//! the daemon to replace its owner key. Which daemon took over from which is
//! told by the predecessor each records ([`succession`]).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};

use crate::instance_lock;
use crate::loopback;
use crate::owner_record::Owner;
use crate::platform::proc::{self, Polite, Sent, Signal};

use super::lock_holder::holder_of;
use super::service_unit;
use super::succession::{
    find_daemon, handover_input, next_may_come, recorded_after, succession, Awaited, EndWatch,
    Found, Succession, POLL, SERVICE_RESTART_GRACE,
};

/// Exit status of `status` and `address` when no daemon runs.
pub(super) const NOT_RUNNING: i32 = 3;

/// How long `start` waits for the daemon's handshake line: opening the store and sweeping what the
/// previous run left come first.
const START_WAIT: Duration = Duration::from_secs(30);

/// How long `stop` waits for the lock to be free: ending every session takes time.
const STOP_WAIT: Duration = Duration::from_secs(60);

/// How long `restart` waits for the next daemon to hold the lock: the planned stop, then the next
/// daemon's start, each within what `stop` and `start` wait for.
pub(super) const RESTART_WAIT: Duration =
    Duration::from_secs(STOP_WAIT.as_secs() + START_WAIT.as_secs());

/// How long the request to stop or restart waits for the daemon's answer, which it gives before
/// doing either.
const REQUEST_WAIT: Duration = Duration::from_secs(10);

/// What the starter keeps of what a starting daemon wrote on its stderr, for a failure to show.
const STDERR_KEPT: usize = 16 * 1024;

/// How long the starter waits for the end of the daemon's stderr once the daemon has exited. The
/// pipe closes with the daemon unless another process still holds its write end: on Windows, one
/// started concurrently with it can inherit the handle, and on Unix one that inherits a
/// descriptor. What was written by then is all there is to show.
const STDERR_GRACE: Duration = Duration::from_secs(2);

fn address(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

pub(super) fn not_running() -> String {
    format!("{} is not running", crate::APP_NAME)
}

/// What is known of the running daemon, to follow "is" or "is already": who it is and where it
/// listens, when its record says.
fn running_state(owner: Option<&Owner>) -> String {
    match owner {
        Some(owner) => format!(
            "running: pid {}, {}",
            owner.identity.pid.0,
            address(owner.port)
        ),
        None => "running; its address is not known".to_owned(),
    }
}

/// Why a command does not act on a daemon that holds the lock but has recorded nothing, after
/// waiting `waited` for it to, naming the process that holds the lock where the host says
/// (`holder`, see [`lock_holder`]).
fn no_record(what: &str, waited: Duration, holder: Option<u32>) -> String {
    let holder = holder.map_or_else(String::new, |pid| format!(" (pid {pid} holds the lock)"));
    format!(
        "{} is running{holder}, but after {} seconds still no record says which process it is (it \
         may be starting or stopping, or could not write the record); not {what} it",
        crate::APP_NAME,
        waited.as_secs()
    )
}

/// The process holding the instance lock, where the host says.
fn lock_holder() -> Option<u32> {
    // The lock's file, as `instance_lock` names it.
    holder_of(&crate::paths::data_dir().join("daemon.lock"))
}

pub(super) fn status() -> Result<i32> {
    let owner = match find_daemon(RESTART_WAIT, false)? {
        Found::NotRunning { .. } => {
            println!("{}", not_running());
            return Ok(NOT_RUNNING);
        }
        Found::Running { owner, .. } => Some(owner),
        Found::Unrecorded => None,
    };
    println!("{} is {}", crate::APP_NAME, running_state(owner.as_ref()));
    Ok(0)
}

pub(super) fn stop() -> Result<i32> {
    let began = Instant::now();
    let outcome = match find_daemon(RESTART_WAIT, true)? {
        // The daemon found once the handover the stop waited through was over is stopped only if
        // it took over from the daemon that handed over; any other is left running, and reported
        // once that daemon has ended.
        Found::Running {
            owner,
            after: Some(from),
        } => {
            let key = owner.recorded_key();
            match succession(&from, None, &owner, key.as_deref()) {
                Succession::Successor => stop_following(&owner, key, began, None)?,
                Succession::Other | Succession::Unproven => {
                    if ended_by(&from, began + RESTART_WAIT) {
                        StopOutcome::Superseded {
                            ended: from.identity.pid.0,
                            other: owner,
                        }
                    } else {
                        StopOutcome::StillStopping {
                            pid: from.identity.pid.0,
                            waited: began.elapsed(),
                            other: Some(owner),
                        }
                    }
                }
            }
        }
        Found::Running { owner, .. } => stop_and_wait(&owner, began)?,
        Found::NotRunning { waited: false } => {
            println!("{}", not_running());
            return Ok(0);
        }
        // What held the lock, or was handing over, has gone with no daemon after it.
        Found::NotRunning { waited: true } => StopOutcome::EndedUnfollowed,
        Found::Unrecorded => {
            eprintln!("{}", no_record("stopping", RESTART_WAIT, lock_holder()));
            return Ok(1);
        }
    };
    match outcome {
        StopOutcome::Stopped => {
            println!("{} stopped", crate::APP_NAME);
            Ok(0)
        }
        StopOutcome::EndedUnfollowed => {
            println!(
                "{} stopped; it was already stopping, and if that was a planned stop, where \
                 holders run its sessions and shell terminals, they are kept for the next start",
                crate::APP_NAME
            );
            Ok(0)
        }
        StopOutcome::NoKey(daemon) => {
            eprintln!("{}", key_not_recorded(&daemon, "stop"));
            Ok(1)
        }
        StopOutcome::Superseded { ended, other } => {
            eprintln!(
                "{} stopped (pid {ended}), but another daemon has started since and is left \
                 running: pid {}, {}; nothing shows that the stopped one handed over to it",
                crate::APP_NAME,
                other.identity.pid.0,
                address(other.port)
            );
            Ok(1)
        }
        StopOutcome::StillStopping { pid, waited, other } => {
            let others = other.map_or_else(String::new, |other| {
                format!(
                    "; another daemon holds the lock and is left running: pid {}, {}",
                    other.identity.pid.0,
                    address(other.port)
                )
            });
            eprintln!(
                "{} is still stopping after {} seconds (pid {pid}){others}",
                crate::APP_NAME,
                waited.as_secs()
            );
            Ok(1)
        }
    }
}

/// Waits until `deadline` for the daemon `from`, whose handover `stop` waited through, to have
/// ended. Whether it has.
fn ended_by(from: &Owner, deadline: Instant) -> bool {
    while from.identity.is_running() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
    true
}

pub(super) fn restart() -> Result<i32> {
    let began = Instant::now();
    let owner = match find_daemon(RESTART_WAIT, true)? {
        Found::Running { owner, .. } => owner,
        Found::NotRunning { .. } => {
            eprintln!("{}", not_running());
            return Ok(1);
        }
        Found::Unrecorded => {
            eprintln!("{}", no_record("restarting", RESTART_WAIT, lock_holder()));
            return Ok(1);
        }
    };
    match owner.follows_application {
        Some(false) => {}
        Some(true) => {
            eprintln!(
                "{} follows the desktop application (pid {}), which restarts it itself; not \
                 restarting it",
                crate::APP_NAME,
                owner.identity.pid.0
            );
            return Ok(1);
        }
        // A record without the token comes from a daemon whose handler for the request could not
        // be installed, or does not say; the request would end it, not restart it.
        None => {
            eprintln!(
                "{} (pid {}) cannot take a restart request (its handler for it could not be \
                 installed, or its record does not say); stop and start it instead",
                crate::APP_NAME,
                owner.identity.pid.0
            );
            return Ok(1);
        }
    }
    // Read before the request, since a daemon removes its record as it ends: a successor of an
    // earlier build, which records no predecessor, shows itself by this key.
    let key = owner.recorded_key();
    // Asked over loopback wherever the key is recorded, so that a refusal (a holder inside the
    // daemon's own service, say) comes back with its reason at once; the signal, which carries no
    // answer, only for a daemon whose key is not recorded, as an earlier build's is not.
    let asked = match key.as_deref() {
        Some(key) => request_over_loopback(&owner, key, "/restart")?,
        None => match proc::request_restart(&owner.identity) {
            Sent::Delivered => Asked::Accepted,
            Sent::Gone => Asked::Gone,
            Sent::Failed(err) if err.kind() == std::io::ErrorKind::Unsupported => {
                eprintln!("{}", key_not_recorded(&owner, "restart"));
                return Ok(1);
            }
            Sent::Failed(err) => {
                return Err(err).context(format!("signalling pid {}", owner.identity.pid.0))
            }
        },
    };
    // A daemon that no longer accepts connections is not necessarily gone: one in a planned stop
    // of its own resets the request while it ends, and its successor follows. One that has ended
    // is judged by the loop below like any other, so that a service manager restarting it is
    // waited for.
    let not_accepting = match asked {
        Asked::Accepted => false,
        Asked::Gone => true,
        Asked::Refused(message) => {
            eprintln!("{} refused the restart: {message}", crate::APP_NAME);
            return Ok(1);
        }
    };
    let deadline = began + RESTART_WAIT;
    let mut watch = EndWatch::found_running(owner.identity);
    loop {
        let ended = !owner.identity.is_running();
        let awaited = awaited(&owner, ended, &mut watch);
        let lock_held = instance_lock::is_running()?;
        if let Some((next, next_key)) = recorded_after(&owner).filter(|_| lock_held) {
            let verdict = succession(&owner, key.as_deref(), &next, next_key.as_deref());
            match verdict {
                // Where nothing shows either way, the daemon holding the lock once the restarted
                // one has ended is taken for its successor, as nothing better is known: the
                // record of an earlier build does not say, and the key proves nothing when one
                // could not be read. A proven successor is taken at once, even while the
                // restarted daemon waits for it to report listening.
                Succession::Successor | Succession::Unproven
                    if ended || verdict == Succession::Successor =>
                {
                    println!(
                        "{} restarted: pid {}, {}",
                        crate::APP_NAME,
                        next.identity.pid.0,
                        address(next.port)
                    );
                    return Ok(0);
                }
                Succession::Other if ended => {
                    eprintln!(
                        "{}, but another daemon has started since and is left running: pid {}, \
                         {}; nothing shows that the restarted one handed over to it",
                        stopped_phrase(owner.identity.pid.0, not_accepting),
                        next.identity.pid.0,
                        address(next.port)
                    );
                    return Ok(1);
                }
                _ => {}
            }
        } else if !lock_held
            && ended
            && !next_may_come(
                &awaited,
                Instant::now(),
                SystemTime::now(),
                service_unit::state,
            )
            && !earlier_build_may_be_restarted(&owner, &awaited, Instant::now())
            // Looked at again: a successor that took the lock since it was seen free, and
            // recorded itself, has removed the note.
            && !instance_lock::is_running()?
        {
            eprintln!(
                "{}, and no daemon runs in its place: it crashed or was stopped, a stop asked for \
                 meanwhile cut the restart short, or the next daemon did not start; the log says \
                 which",
                stopped_phrase(owner.identity.pid.0, not_accepting)
            );
            return Ok(1);
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(POLL);
    }
    if owner.identity.is_running() {
        eprintln!(
            "{} is still stopping after {} seconds (pid {})",
            crate::APP_NAME,
            began.elapsed().as_secs(),
            owner.identity.pid.0
        );
    } else {
        eprintln!(
            "{} stopped, but no daemon took over within {} seconds; its log says why, and the \
             sessions are kept for the next start",
            crate::APP_NAME,
            began.elapsed().as_secs()
        );
    }
    Ok(1)
}

/// What `stop` and `restart` know of the daemon `owner` names, which they asked, for
/// [`next_may_come`], given whether it has `ended`, which is read first: a daemon writes its note
/// of a handover before it ends, and its successor removes the note only once it holds the lock
/// and has recorded itself, so a command that then finds the lock free with no note has seen all
/// there is.
fn awaited<'a>(owner: &'a Owner, ended: bool, watch: &mut EndWatch) -> Awaited<'a> {
    let noted_at = Owner::handover_noted()
        .filter(|noted| noted.from.identity == owner.identity)
        .map(|noted| noted.noted);
    let crashed =
        ended && Owner::last_recorded().is_some_and(|last| last.identity == owner.identity);
    let gone_since = watch.observe(owner.identity, !ended, Instant::now());
    Awaited {
        from: owner,
        ended,
        noted_at,
        crashed,
        gone_since,
    }
}

/// Whether `restart` still gives a service manager [`SERVICE_RESTART_GRACE`] to start a daemon
/// after `owner`, whose record does not say whether one restarts it: an earlier build's, which
/// may run under systemd with `--service-managed` and exits for it to start the next daemon,
/// leaving neither a note nor its record behind. Counted from when this command saw it end.
fn earlier_build_may_be_restarted(owner: &Owner, awaited: &Awaited<'_>, now: Instant) -> bool {
    owner.service_managed.is_none()
        && awaited
            .gone_since
            .is_some_and(|since| now.saturating_duration_since(since) < SERVICE_RESTART_GRACE)
}

/// What `restart` says of the daemon with pid `pid` that has ended: that it stopped to restart,
/// or, where it no longer accepted the request (`not_accepting`), that it had stopped or was
/// stopping already, which the command cannot tell apart.
fn stopped_phrase(pid: u32, not_accepting: bool) -> String {
    if not_accepting {
        format!(
            "{} (pid {pid}) was no longer accepting the restart and has stopped",
            crate::APP_NAME
        )
    } else {
        format!("{} stopped (pid {pid}) to restart", crate::APP_NAME)
    }
}

pub(super) fn start(port: u16) -> Result<i32> {
    let running = match find_daemon(RESTART_WAIT, true)? {
        Found::NotRunning { .. } => None,
        Found::Running { owner, .. } => Some(Some(owner)),
        Found::Unrecorded => Some(None),
    };
    if let Some(owner) = running {
        eprintln!(
            "{} is already {}",
            crate::APP_NAME,
            running_state(owner.as_ref())
        );
        return Ok(1);
    }
    let exe = super::launch_path()?;
    match spawn_and_await_handshake(&exe, port, None, None) {
        Ok(started) => {
            println!("{}", started.line);
            println!(
                "{} started: pid {}, {}",
                crate::APP_NAME,
                started.pid,
                address(port)
            );
            if started.in_starter_job {
                eprintln!(
                    "note: the job this command runs in does not let a process break away, so {} \
                     ends when that job does (an SSH session ends it at logout)",
                    crate::APP_NAME
                );
            }
            Ok(0)
        }
        Err(StartFailure::Error(err)) => Err(err),
        Err(StartFailure::TimedOut { said }) => {
            eprintln!(
                "{} did not report listening within {} seconds; it was killed",
                crate::APP_NAME,
                START_WAIT.as_secs()
            );
            print_said(&said);
            Ok(1)
        }
        Err(StartFailure::Ended { status, said }) => {
            eprintln!("{} ended while starting ({status})", crate::APP_NAME);
            print_said(&said);
            Ok(1)
        }
    }
}

fn print_said(said: &str) {
    if !said.trim().is_empty() {
        eprintln!("{}", said.trim_end());
    }
}

/// A daemon that reported listening.
struct Started {
    pid: u32,
    /// The handshake line, which carries the address and the key.
    line: String,
    /// Whether the daemon runs in the job of whoever started it, which ends it when the job ends.
    in_starter_job: bool,
}

/// Why no daemon was started.
enum StartFailure {
    /// Preparing or spawning it failed.
    Error(anyhow::Error),
    /// It reported nothing in time and was killed; `said` is what it wrote on stderr.
    TimedOut { said: String },
    /// It ended before reporting listening; `said` is what it wrote on stderr.
    Ended { status: ExitStatus, said: String },
}

impl From<anyhow::Error> for StartFailure {
    fn from(err: anyhow::Error) -> Self {
        Self::Error(err)
    }
}

/// Starts the daemon that takes over from this one after a planned restart: on `port`, in the
/// background as `start` starts it, with `key`, handed over on its stdin, so that its clients find
/// it at the same address with the same key, and this daemon's identity after it, which it records
/// as its predecessor. `exe` is [`launch_path`]'s answer, taken at this
/// daemon's start (see `installation.rs` for what it follows). With `listener`, the duplicate of
/// this daemon's listening socket it kept, the successor takes that socket over rather than bind
/// the port, which is then never unbound between the two. Called once this daemon has released the
/// instance lock and stopped serving the port. Logs how it went; whether it came up. **Blocks**
/// until it reports listening, or for at most [`START_WAIT`].
pub(crate) fn start_successor(
    port: u16,
    key: &str,
    exe: &Path,
    listener: Option<&crate::platform::listener_handover::Kept>,
) -> SuccessorStart {
    let handover = handover_input(key, crate::owner_record::own_identity());
    match spawn_and_await_handshake(exe, port, Some(&handover), listener) {
        Ok(started) => {
            tracing::info!(pid = started.pid, port, "the next daemon is listening");
            if started.in_starter_job {
                tracing::warn!("the job this daemon runs in does not let a process break away: the next daemon ends when that job does");
            }
            SuccessorStart::Started
        }
        Err(StartFailure::Error(err)) => {
            tracing::error!(err = %format!("{err:#}"), "starting the next daemon failed; the sessions are kept for the next start");
            SuccessorStart::Failed
        }
        Err(StartFailure::TimedOut { said }) => {
            tracing::error!(
                said = %said.trim_end(),
                "the next daemon did not report listening within {} seconds and was killed; the sessions are kept for the next start",
                START_WAIT.as_secs()
            );
            SuccessorStart::Failed
        }
        Err(StartFailure::Ended { status, said })
            if listener.is_some() && refused_listener(status.code(), &said) =>
        {
            tracing::warn!(%status, said = %said.trim_end(), "the next daemon refused the listener handed over, as a build from before listeners were handed over does");
            SuccessorStart::RefusedListener
        }
        Err(StartFailure::Ended { status, said }) => {
            tracing::error!(%status, said = %said.trim_end(), "the next daemon ended while starting; the sessions are kept for the next start");
            SuccessorStart::Failed
        }
    }
}

/// How starting the next daemon went ([`start_successor`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SuccessorStart {
    /// It reported listening.
    Started,
    /// It ended at once refusing the argument that hands it the listener: a build from before
    /// listeners were handed over, which would come up binding the port itself.
    RefusedListener,
    /// It did not come up for any other reason.
    Failed,
}

/// Whether a daemon that ended while starting with `status`, having written `said` on stderr,
/// refused `--listen-fd` as an argument it does not know: the command line's usage error, status 2,
/// naming it.
fn refused_listener(status: Option<i32>, said: &str) -> bool {
    status == Some(2) && said.contains("--listen-fd")
}

/// Starts the daemon `exe` on `port` in the background and waits for its handshake line, killing it
/// when none comes in time. With `handover` (the key and the identity of the daemon handing over,
/// [`handover_input`]), the daemon is told to read its key from its stdin, where that is written;
/// otherwise it makes one. With `listener`, it is handed that socket and told to take it over.
fn spawn_and_await_handshake(
    exe: &Path,
    port: u16,
    handover: Option<&str>,
    listener: Option<&crate::platform::listener_handover::Kept>,
) -> Result<Started, StartFailure> {
    // Its working directory is the data directory, not wherever the starter was, so that it pins
    // no mount that is then busy at unmount.
    let data_dir = crate::paths::data_dir();
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating {}", data_dir.display()))?;
    let mut command = Command::new(exe);
    command
        .args(["--port", &port.to_string()])
        .current_dir(&data_dir);
    if handover.is_some() {
        command.arg("--key-stdin");
    }
    if listener.is_some() {
        command.args([
            "--listen-fd",
            &crate::platform::listener_handover::HANDED_FD.to_string(),
        ]);
    }
    // The pipes on stdout and stderr carry the handshake and what a failed start wrote, and the one
    // on stdin, with a key, the key. They close when the starter exits, so what the daemon writes
    // to stderr from then on fails; its log tolerates that (see `init_logging`), but a process the
    // daemon starts must set its own stdio, or one that inherited the dead stderr would be killed
    // by SIGPIPE.
    let (mut child, in_starter_job): (proc::BackgroundChild, bool) =
        proc::spawn_in_background(&mut command, handover.is_some(), listener)
            .context("starting the daemon")?;
    let pid = child.id();
    if let Some(handover) = handover {
        // Two lines, well within what a pipe holds, so the write does not wait for the daemon to
        // read them; the pipe is closed once they are written.
        let mut stdin = child.stdin.take().expect("stdin was piped");
        if let Err(err) = write!(stdin, "{handover}") {
            child.kill().ok();
            child.wait().ok();
            return Err(anyhow::Error::new(err)
                .context("handing the key to the daemon")
                .into());
        }
    }

    // The handshake line is the first line on stdout; the end of stdout without one means the
    // daemon ended first.
    let (line_tx, line_rx) = mpsc::channel();
    let stdout = child.stdout.take().expect("stdout was piped");
    std::thread::spawn(move || {
        let mut line = String::new();
        let first = match BufReader::new(stdout).read_line(&mut line) {
            Ok(n) if n > 0 => Some(line.trim_end().to_owned()),
            _ => None,
        };
        let _ = line_tx.send(first);
    });
    let stderr_text = Arc::new(Mutex::new(Vec::new()));
    let stderr = child.stderr.take().expect("stderr was piped");
    let (stderr_done_tx, stderr_done) = mpsc::channel();
    {
        let kept = stderr_text.clone();
        std::thread::spawn(move || {
            drain_kept(stderr, &kept);
            let _ = stderr_done_tx.send(());
        });
    }

    let timed_out = match wait_for_handshake(
        &line_rx,
        || matches!(child.try_wait(), Ok(Some(_))),
        START_WAIT,
    ) {
        Handshake::Line(line) => {
            return Ok(Started {
                pid,
                line,
                in_starter_job,
            })
        }
        Handshake::Ended => false,
        Handshake::TimedOut => {
            child.kill().ok();
            true
        }
    };
    let status = child.wait().context("waiting for the daemon")?;
    // The pipe closed with the child, so what it wrote is all there, unless something else holds
    // the write end open; the reader is then left to end on its own, not waited on without bound.
    stderr_done.recv_timeout(STDERR_GRACE).ok();
    let said = String::from_utf8_lossy(&stderr_text.lock().unwrap()).into_owned();
    Err(if timed_out {
        StartFailure::TimedOut { said }
    } else {
        StartFailure::Ended { status, said }
    })
}

/// How the wait for the daemon's handshake line ended.
#[derive(Debug, PartialEq, Eq)]
enum Handshake {
    /// The daemon reported listening with this line.
    Line(String),
    /// The daemon's stdout ended without one, or the daemon exited.
    Ended,
    /// Neither happened within the wait.
    TimedOut,
}

/// Waits up to `wait` for the first line of the daemon's stdout to arrive on `lines` (`None`
/// where stdout ended without one). The daemon's exit is polled through `exited` as well, since
/// stdout ends with the daemon only unless another process still holds its write end open, which
/// would otherwise leave this waiting out the whole of `wait`.
fn wait_for_handshake(
    lines: &mpsc::Receiver<Option<String>>,
    mut exited: impl FnMut() -> bool,
    wait: Duration,
) -> Handshake {
    let deadline = Instant::now() + wait;
    loop {
        match lines.recv_timeout(POLL) {
            Ok(Some(line)) => return Handshake::Line(line),
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => return Handshake::Ended,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if exited() {
            // A line written just before the exit may still be on its way to the reader.
            return match lines.recv_timeout(POLL) {
                Ok(Some(line)) => Handshake::Line(line),
                _ => Handshake::Ended,
            };
        }
        if Instant::now() >= deadline {
            return Handshake::TimedOut;
        }
    }
}

/// How [`stop_and_wait`] ended.
enum StopOutcome {
    /// A daemon the stop asked took the request, and the daemon it followed last is gone: the lock
    /// is free and its process has ended.
    Stopped,
    /// The daemon was already ending when asked, or before the stop found it recorded, no daemon
    /// the stop asked took the request, and no daemon took over from it. The stop under way may
    /// have been a planned stop, which leaves the sessions and shell terminals running in their
    /// holders; the command cannot tell.
    EndedUnfollowed,
    /// This daemon has no signal to stop it by on this host, and its key is not recorded to ask it
    /// over loopback.
    NoKey(Owner),
    /// The daemon with pid `ended`, the last one the stop followed, has ended, but `other`, which
    /// nothing shows it handed over to, holds the lock since; it is left running.
    Superseded { ended: u32, other: Owner },
    /// The daemon with this pid, the last one the stop followed, still holds the lock after the
    /// wait, which took this long in all. `other` is a daemon the stop found holding the lock
    /// while that one still ran, which is not its successor and is left running.
    StillStopping {
        pid: u32,
        waited: Duration,
        other: Option<Owner>,
    },
}

/// Asks the daemon `owner` names to end, and waits for it and any successor it hands over to, all
/// within [`RESTART_WAIT`] of `began`, when the command started.
fn stop_and_wait(owner: &Owner, began: Instant) -> Result<StopOutcome> {
    stop_following(owner, owner.recorded_key(), began, None)
}

/// Asks the daemon `owner` names to end — by the termination signal where the host has one, else
/// over loopback with `key` — and waits for it to be gone, following it through a planned restart
/// so that the stop leaves no daemon of its line running, as an ordinary stop would. A daemon in a
/// planned restart no longer accepts connections, and one that has already settled on its
/// successor ignores the signal; either way its successor takes the lock, and is then asked in its
/// place, where it shows itself that daemon's successor ([`succession`]): one that does not, which
/// a user, the desktop application or a service manager restarting after every exit started, is
/// reported and left running once the stopped daemon has ended. `key` is the key recorded for
/// `owner`, read before it is asked, since a daemon removes its record as it ends. A daemon that
/// has ended with the lock free is gone once no daemon may still take over from it
/// ([`next_may_come`]). Until [`STOP_WAIT`] after a daemon takes the request, and in any case
/// [`RESTART_WAIT`] after `began`, when the command started, which the wait for the daemon to be
/// found counts in too; a daemon that has ended with the lock free by then is gone, whatever a
/// service manager may still start. `taken_at` says when a daemon this stop asked before `owner`,
/// on its way to it, took the request over loopback, which is when the bound of [`STOP_WAIT`]
/// counts from for `owner` too.
fn stop_following(
    owner: &Owner,
    key: Option<String>,
    began: Instant,
    taken_at: Option<Instant>,
) -> Result<StopOutcome> {
    let mut over_loopback = false;
    let asked = match proc::signal_identified(&owner.identity, Signal::Polite(Polite::Terminate)) {
        Sent::Delivered => Asked::Accepted,
        Sent::Gone => Asked::Gone,
        Sent::Failed(err) if err.kind() == std::io::ErrorKind::Unsupported => {
            let Some(key) = key.as_deref() else {
                return Ok(StopOutcome::NoKey(owner.clone()));
            };
            over_loopback = true;
            request_over_loopback(owner, key, "/shutdown")?
        }
        Sent::Failed(err) => {
            return Err(err).context(format!("signalling pid {}", owner.identity.pid.0))
        }
    };
    let took = match asked {
        Asked::Accepted => true,
        Asked::Gone => false,
        Asked::Refused(message) => {
            anyhow::bail!("{} refused the stop: {message}", crate::APP_NAME)
        }
    };
    let asked_at = Instant::now();
    let deadline = wait_deadline(taken_at, took.then_some(asked_at), began + RESTART_WAIT);
    let mut watch = EndWatch::found_running(owner.identity);
    loop {
        let ended = !owner.identity.is_running();
        let awaited = awaited(owner, ended, &mut watch);
        let lock_held = instance_lock::is_running()?;
        let now = Instant::now();
        if let Some((next, next_key)) = recorded_after(owner).filter(|_| lock_held) {
            match succession(owner, key.as_deref(), &next, next_key.as_deref()) {
                Succession::Successor => {
                    let taken_at = carried_on(taken_at, took && over_loopback, asked_at);
                    return stop_following(&next, next_key, began, taken_at);
                }
                // Judged only once the stopped daemon has ended: until then it may still be
                // ending, with the lock released before its teardown and its exit.
                _ if ended => {
                    return Ok(StopOutcome::Superseded {
                        ended: owner.identity.pid.0,
                        other: next,
                    })
                }
                _ => {}
            }
        } else if !lock_held
            && ended
            && (now >= deadline
                || !next_may_come(&awaited, now, SystemTime::now(), service_unit::state))
        {
            return Ok(outcome_once_gone(taken_at.is_some(), took));
        }
        if now >= deadline {
            return Ok(StopOutcome::StillStopping {
                pid: owner.identity.pid.0,
                waited: began.elapsed(),
                other: None,
            });
        }
        std::thread::sleep(POLL);
    }
}

/// Until when a stop waits for the daemon it asks to be gone: [`STOP_WAIT`] after the first daemon
/// it asked took the request (`taken_at`, or `took_at` for the one asked now), so that a successor
/// that took the first request is bound by the same 60 seconds, and in any case until `latest`.
/// For which requests count, see docs/product/daemon-command-line.md's "Running in the background
/// from the command line" section.
fn wait_deadline(taken_at: Option<Instant>, took_at: Option<Instant>, latest: Instant) -> Instant {
    taken_at
        .or(took_at)
        .map_or(latest, |at| (at + STOP_WAIT).min(latest))
}

/// When the first request a stop got a daemon to take counts from for the daemon it follows next:
/// `taken_at` if a daemon had taken one, else `asked_at` if the one just asked took it over
/// loopback (`took_over_loopback`). A signal that was delivered is not carried over: it does not
/// show the daemon took the stop, since one that has settled on starting its successor ignores
/// the termination signal, and the successor it then asks is judged on its own.
fn carried_on(
    taken_at: Option<Instant>,
    took_over_loopback: bool,
    asked_at: Instant,
) -> Option<Instant> {
    taken_at.or(took_over_loopback.then_some(asked_at))
}

/// What a stop reports once the daemon it followed last is gone, given whether a daemon it asked
/// on the way there took the request (`taken_before`) and whether that last one did (`took`):
/// stopped if any did, already stopping only if none did. Over loopback, a request sent to a
/// daemon in a planned restart can be taken by its successor, which listens on the same port with
/// the same key once that daemon has let go of them; the successor then no longer accepts the
/// request the stop sends it on following it, yet it stopped for this stop.
fn outcome_once_gone(taken_before: bool, took: bool) -> StopOutcome {
    if taken_before || took {
        StopOutcome::Stopped
    } else {
        StopOutcome::EndedUnfollowed
    }
}

/// Why `stop` or `restart` cannot ask the daemon over loopback.
fn key_not_recorded(owner: &Owner, what: &str) -> String {
    format!(
        "{} is running (pid {}, port {}), but its key is not recorded, so it cannot be asked to \
         {what}: it may be an earlier build, or one that could not record it, as its log says",
        crate::APP_NAME,
        owner.identity.pid.0,
        owner.port
    )
}

/// What the daemon answered a request over loopback with.
#[derive(Debug)]
pub(super) enum Asked {
    /// It took the request.
    Accepted,
    /// It was not listening: it is gone, or going.
    Gone,
    /// It turned the request away, with this message (a `409` and its JSON body).
    Refused(String),
}

/// Asks the daemon `owner` names to do what `path` (`/shutdown`, `/restart` or
/// `/owner-key/rotate`) says, presenting `key`, which goes over the loopback connection only, never
/// in a command line or an environment. What a refusal says is the daemon's own message. Any other
/// failure is an error, which carries the daemon's message where its answer has one, as a failure
/// to write the new owner key does.
pub(super) fn request_over_loopback(owner: &Owner, key: &str, path: &str) -> Result<Asked> {
    match loopback::post(owner.port, key, path, b"{}", REQUEST_WAIT) {
        Ok(_) => Ok(Asked::Accepted),
        Err(err) if loopback::error_status(&err) == Some(409) => Ok(Asked::Refused(
            error_message(&err).unwrap_or_else(|| "no reason given".to_owned()),
        )),
        Err(err) if no_longer_accepting(&err) => Ok(Asked::Gone),
        Err(err) => {
            let err = match error_message(&err) {
                Some(message) => anyhow::anyhow!("{err}: {message}"),
                None => anyhow::Error::new(err),
            };
            Err(err.context(format!(
                "asking pid {} on port {} ({path})",
                owner.identity.pid.0, owner.port
            )))
        }
    }
}

/// The `message` of the JSON body of the daemon's error answer, when `err` is one and has it.
fn error_message(err: &std::io::Error) -> Option<String> {
    loopback::error_body(err)
        .and_then(|body| serde_json::from_slice::<serde_json::Value>(body).ok())
        .and_then(|body| body["message"].as_str().map(str::to_owned))
}

/// Whether `err`, from a request over loopback, says the daemon no longer accepts connections: it
/// refused the connection, or reset or dropped one it had queued but never served, as a daemon
/// that stops listening with connections still queued does on Windows (`WSAECONNRESET`). Such a
/// request counts as not taken; the wait that follows it judges where the daemon has got to all
/// the same.
fn no_longer_accepting(err: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    matches!(
        err.kind(),
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
    )
}

/// Reads `pipe` to its end, keeping the first [`STDERR_KEPT`] bytes in `kept`.
fn drain_kept(mut pipe: impl Read, kept: &Mutex<Vec<u8>>) {
    let mut chunk = [0u8; 4096];
    while let Ok(n) = pipe.read(&mut chunk) {
        if n == 0 {
            break;
        }
        let mut kept = kept.lock().unwrap();
        let room = STDERR_KEPT.saturating_sub(kept.len());
        kept.extend_from_slice(&chunk[..n.min(room)]);
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    use std::net::TcpListener;

    use crate::owner_record::Predecessor;
    use crate::platform::proc::{ProcessId, ProcessIdentity};
    use crate::test_support::PATIENCE;

    pub(in crate::cli) const KEY: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    pub(in crate::cli) fn owner_on(port: u16) -> Owner {
        Owner {
            identity: ProcessIdentity {
                pid: ProcessId(std::process::id()),
                start_micros: 1,
            },
            port,
            follows_application: Some(false),
            service_managed: Some(false),
            takes_owner_key: false,
            unit: None,
            predecessor: Predecessor::None,
        }
    }

    /// Answers one request with `response`, and gives its request line (through the first line
    /// of the head) back through the handle.
    pub(in crate::cli) fn answer_once(
        response: impl Into<String>,
    ) -> (u16, std::thread::JoinHandle<String>) {
        let response = response.into();
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let served = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("a request");
            stream.set_read_timeout(Some(PATIENCE)).unwrap();
            let mut reader = BufReader::new(stream);
            let mut request_line = String::new();
            reader.read_line(&mut request_line).expect("a request line");
            // The rest of the head, up to the blank line, and then the body it announces, so the
            // answer is not sent over unread input (a reset on Windows, which can discard it).
            let mut body_len = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("a header");
                if line.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        body_len = value.trim().parse().expect("a content length");
                    }
                }
            }
            let mut body = vec![0u8; body_len];
            reader.read_exact(&mut body).expect("the body");
            reader
                .get_mut()
                .write_all(response.as_bytes())
                .expect("the answer");
            request_line
        });
        (port, served)
    }

    /// The request goes to the path asked for, with the key as its `key` query, and a `202` is
    /// taking it.
    #[test]
    fn a_request_over_loopback_presents_the_key_and_is_accepted_on_a_202() {
        let (port, served) = answer_once("HTTP/1.1 202 Accepted\r\nConnection: close\r\n\r\n");
        let asked = request_over_loopback(&owner_on(port), KEY, "/shutdown").unwrap();
        assert!(matches!(asked, Asked::Accepted));
        assert_eq!(
            served.join().unwrap(),
            format!("POST /shutdown?key={KEY} HTTP/1.1\r\n")
        );
    }

    /// A `409` is the daemon refusing, and what the refusal says is its JSON message.
    #[test]
    fn a_refusal_over_loopback_carries_the_daemons_message() {
        let body = r#"{"reason":"follows_application","message":"it follows the application"}"#;
        let response = format!(
            "HTTP/1.1 409 Conflict\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (port, served) = answer_once(response);
        let asked = request_over_loopback(&owner_on(port), KEY, "/restart").unwrap();
        served.join().unwrap();
        assert!(
            matches!(&asked, Asked::Refused(message) if message == "it follows the application")
        );
    }

    /// Any other error status is an error naming the call, not a refusal to report as one.
    #[test]
    fn a_wrong_key_over_loopback_is_an_error() {
        let (port, served) = answer_once("HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n");
        let err = request_over_loopback(&owner_on(port), KEY, "/shutdown").unwrap_err();
        served.join().unwrap();
        let said = format!("{err:#}");
        assert!(
            said.contains("/shutdown") && said.contains("HTTP 401"),
            "{said}"
        );
    }

    /// A stop reports the daemon stopped once the last daemon it followed is gone if any daemon it
    /// asked took the request, the first one included: over loopback the successor of a daemon in
    /// a planned restart takes a request sent to that daemon, and then no longer accepts the one
    /// sent to it when the stop follows it. Only a stop no daemon took the request of reports the
    /// daemon as already stopping.
    #[test]
    fn a_stop_any_daemon_took_is_reported_stopped() {
        for (taken_before, took) in [(true, false), (false, true), (true, true)] {
            assert!(
                matches!(outcome_once_gone(taken_before, took), StopOutcome::Stopped),
                "{taken_before} {took}"
            );
        }
        assert!(matches!(
            outcome_once_gone(false, false),
            StopOutcome::EndedUnfollowed
        ));
    }

    /// The bound of [`STOP_WAIT`] counts from the first request a daemon took, so a successor that
    /// took it is not waited on for longer, and never past the overall limit; with none taken it
    /// is the overall limit.
    #[test]
    fn a_stop_is_bound_from_the_first_request_a_daemon_took() {
        let began = Instant::now();
        let latest = began + RESTART_WAIT;
        let first = began + Duration::from_secs(5);
        let later = began + Duration::from_secs(20);
        assert_eq!(wait_deadline(None, None, latest), latest);
        assert_eq!(wait_deadline(None, Some(later), latest), later + STOP_WAIT);
        assert_eq!(
            wait_deadline(Some(first), Some(later), latest),
            first + STOP_WAIT
        );
        assert_eq!(wait_deadline(Some(first), None, latest), first + STOP_WAIT);
        let near_end = latest - Duration::from_secs(1);
        assert_eq!(wait_deadline(None, Some(near_end), latest), latest);
    }

    /// Only a request taken over loopback is carried over to the daemon a stop follows next: a
    /// delivered signal does not show the daemon took the stop, and an earlier one is kept.
    #[test]
    fn only_a_request_taken_over_loopback_is_carried_on() {
        let first = Instant::now();
        let later = first + Duration::from_secs(3);
        assert_eq!(carried_on(None, true, later), Some(later));
        assert_eq!(carried_on(None, false, later), None);
        assert_eq!(carried_on(Some(first), true, later), Some(first));
        assert_eq!(carried_on(Some(first), false, later), Some(first));
    }

    /// The wait for the handshake line ends when the daemon has exited even though its stdout is
    /// still held open (the sender is alive), and does not wait out the whole of it.
    #[test]
    fn an_exit_ends_the_wait_for_the_handshake_though_stdout_stays_open() {
        let (held_open, lines) = mpsc::channel::<Option<String>>();
        let began = Instant::now();
        let outcome = wait_for_handshake(&lines, || true, Duration::from_secs(30));
        assert_eq!(outcome, Handshake::Ended);
        assert!(began.elapsed() < Duration::from_secs(5));
        drop(held_open);
    }

    /// A line, an end of stdout and the lapse of the wait each have their own outcome, and a line
    /// that arrives as the daemon exits is not lost.
    #[test]
    fn the_handshake_is_a_line_an_end_or_a_lapse() {
        let (tx, lines) = mpsc::channel();
        tx.send(Some("listening".to_owned())).unwrap();
        assert_eq!(
            wait_for_handshake(&lines, || false, Duration::from_secs(5)),
            Handshake::Line("listening".to_owned())
        );
        tx.send(None).unwrap();
        assert_eq!(
            wait_for_handshake(&lines, || false, Duration::from_secs(5)),
            Handshake::Ended
        );
        assert_eq!(
            wait_for_handshake(&lines, || false, Duration::ZERO),
            Handshake::TimedOut
        );
        tx.send(Some("late".to_owned())).unwrap();
        assert_eq!(
            wait_for_handshake(&lines, || true, Duration::from_secs(5)),
            Handshake::Line("late".to_owned())
        );
    }

    /// A daemon that listens no more is gone, which the wait for the lock then settles.
    #[test]
    fn a_daemon_that_does_not_listen_is_gone() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let asked = request_over_loopback(&owner_on(port), KEY, "/shutdown").unwrap();
        assert!(matches!(asked, Asked::Gone));
    }

    /// A daemon that resets the connection instead of answering, as one that stops listening with
    /// the request still queued does on Windows, is gone as well, for `stop` and `restart` alike.
    /// The connection is reset by closing it with the request unread.
    #[test]
    fn a_daemon_that_resets_the_connection_is_gone() {
        for path in ["/shutdown", "/restart"] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
            let port = listener.local_addr().unwrap().port();
            let served = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("a request");
                stream.set_read_timeout(Some(PATIENCE)).unwrap();
                let mut first = [0u8; 1];
                stream.read_exact(&mut first).expect("the request begins");
                // The rest of the request arrives meanwhile, and is left unread.
                std::thread::sleep(Duration::from_millis(200));
            });
            let asked = request_over_loopback(&owner_on(port), KEY, path).unwrap();
            served.join().unwrap();
            assert!(matches!(asked, Asked::Gone), "{path}: {asked:?}");
        }
    }

    /// A connection refused, reset or dropped says the daemon no longer accepts connections; a
    /// request that timed out or any other failure does not.
    #[test]
    fn only_a_connection_refused_or_cut_off_is_a_daemon_no_longer_accepting() {
        use std::io::{Error, ErrorKind};

        for kind in [
            ErrorKind::ConnectionRefused,
            ErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted,
            ErrorKind::BrokenPipe,
        ] {
            assert!(no_longer_accepting(&Error::from(kind)), "{kind:?}");
        }
        for kind in [
            ErrorKind::TimedOut,
            ErrorKind::WouldBlock,
            ErrorKind::InvalidData,
            ErrorKind::PermissionDenied,
        ] {
            assert!(!no_longer_accepting(&Error::from(kind)), "{kind:?}");
        }
    }

    fn json_response(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    /// An error answer whose JSON body has a `message` ends the error with it, so that the cause
    /// the daemon gave (a failed write of the new owner key, say) reaches the user.
    #[test]
    fn an_error_answer_over_loopback_ends_with_the_daemons_message() {
        let body = r#"{"reason":"write_failed","message":"writing owner.key failed"}"#;
        for status in [
            "500 Internal Server Error",
            "400 Bad Request",
            "401 Unauthorized",
        ] {
            let (port, served) = answer_once(json_response(status, body));
            let err = request_over_loopback(&owner_on(port), KEY, "/owner-key/rotate").unwrap_err();
            served.join().unwrap();
            let said = format!("{err:#}");
            assert!(
                said.ends_with(": writing owner.key failed"),
                "{status}: {said}"
            );
            let code = status.split(' ').next().unwrap();
            assert!(
                said.contains("/owner-key/rotate") && said.contains(&format!("HTTP {code}")),
                "{status}: {said}"
            );
        }
    }

    /// Without a `message` string to give, the error is what it was: the call and the status.
    #[test]
    fn an_error_answer_over_loopback_without_a_message_is_unchanged() {
        let bodies = [
            "",
            "not json",
            r#"{"reason":"write_failed"}"#,
            r#"{"message":42}"#,
            r#"{"message":null}"#,
            "[]",
        ];
        for body in bodies {
            let (port, served) = answer_once(json_response("500 Internal Server Error", body));
            let err = request_over_loopback(&owner_on(port), KEY, "/shutdown").unwrap_err();
            served.join().unwrap();
            let said = format!("{err:#}");
            assert!(said.ends_with("(HTTP 500)"), "{body:?}: {said}");
            assert!(said.contains("/shutdown"), "{body:?}: {said}");
        }
    }

    /// A `409` stays a refusal, with no reason given where its body has no message.
    #[test]
    fn a_refusal_over_loopback_without_a_message_gives_no_reason() {
        let (port, served) = answer_once(json_response("409 Conflict", r#"{"reason":"x"}"#));
        let asked = request_over_loopback(&owner_on(port), KEY, "/restart").unwrap();
        served.join().unwrap();
        assert!(matches!(&asked, Asked::Refused(message) if message == "no reason given"));
    }

    /// A daemon that holds the lock without a record is named by the process the host says holds
    /// the lock, where it does.
    #[test]
    fn the_account_of_a_daemon_with_no_record_names_the_lock_holder_where_known() {
        let waited = Duration::from_secs(90);
        let named = no_record("stopping", waited, Some(4242));
        assert!(
            named.contains("is running (pid 4242 holds the lock), but"),
            "{named}"
        );
        assert!(
            named.contains("90 seconds") && named.contains("not stopping it"),
            "{named}"
        );
        let unnamed = no_record("stopping", waited, None);
        assert!(!unnamed.contains("holds the lock"), "{unnamed}");
        assert!(unnamed.contains("is running, but"), "{unnamed}");
        assert!(unnamed.contains("90 seconds"), "{unnamed}");
    }

    fn awaited_gone_since<'a>(from: &'a Owner, gone_since: Option<Instant>) -> Awaited<'a> {
        Awaited {
            from,
            ended: true,
            noted_at: None,
            crashed: false,
            gone_since,
        }
    }

    /// A record that does not say whether a service manager restarts its daemon is an earlier
    /// build's, which may run under systemd and leaves neither note nor record: `restart` waits
    /// out the grace period counted from when it saw that daemon end, and no longer.
    #[test]
    fn an_earlier_build_may_be_restarted_until_the_grace_after_it_was_seen_to_end() {
        let mut owner = owner_on(1);
        owner.service_managed = None;
        let seen_ending = Instant::now();
        let awaited = awaited_gone_since(&owner, Some(seen_ending));
        let may_be_restarted =
            |after: Duration| earlier_build_may_be_restarted(&owner, &awaited, seen_ending + after);
        assert!(may_be_restarted(Duration::ZERO));
        assert!(may_be_restarted(
            SERVICE_RESTART_GRACE - Duration::from_millis(1)
        ));
        assert!(!may_be_restarted(SERVICE_RESTART_GRACE));
        assert!(!may_be_restarted(SERVICE_RESTART_GRACE * 10));
    }

    /// A daemon this command never saw running and then ended has no moment to count from.
    #[test]
    fn an_earlier_build_not_seen_to_end_is_not_waited_for() {
        let mut owner = owner_on(1);
        owner.service_managed = None;
        let awaited = awaited_gone_since(&owner, None);
        assert!(!earlier_build_may_be_restarted(
            &owner,
            &awaited,
            Instant::now()
        ));
    }

    /// A record that says whether a service manager restarts its daemon, either way, is a later
    /// build's, which leaves a note or a record where one does: nothing is waited for here.
    #[test]
    fn a_build_that_says_whether_a_service_manager_restarts_it_is_not_waited_for() {
        let seen_ending = Instant::now();
        for managed in [true, false] {
            let mut owner = owner_on(1);
            owner.service_managed = Some(managed);
            let awaited = awaited_gone_since(&owner, Some(seen_ending));
            assert!(
                !earlier_build_may_be_restarted(&owner, &awaited, seen_ending),
                "{managed}"
            );
        }
    }

    /// A clock reading before the moment the daemon was seen to end (as `Instant`s are compared
    /// saturating) counts as no time passed, not as a panic.
    #[test]
    fn an_earlier_build_is_waited_for_when_now_is_before_it_was_seen_to_end() {
        let mut owner = owner_on(1);
        owner.service_managed = None;
        let now = Instant::now();
        let awaited = awaited_gone_since(&owner, Some(now + Duration::from_secs(1)));
        assert!(earlier_build_may_be_restarted(&owner, &awaited, now));
    }

    /// A daemon refused the listener's argument when it ended with the command line's usage-error
    /// status, 2, having named `--listen-fd`: either alone is another failure, and so is an end
    /// with no status (a signal).
    #[test]
    fn only_a_usage_error_naming_the_listener_argument_is_a_refusal_of_it() {
        let clap =
            "error: unexpected argument '--listen-fd' found\n\nUsage: octoboardd [OPTIONS]\n";
        assert!(refused_listener(Some(2), clap));
        assert!(!refused_listener(
            Some(2),
            "error: unexpected argument '--other' found\n"
        ));
        assert!(!refused_listener(Some(2), ""));
        assert!(!refused_listener(Some(1), clap));
        assert!(!refused_listener(Some(0), clap));
        assert!(!refused_listener(None, clap));
    }
}
