//! `start`, `stop`, `restart`, `status` and `address`: running the daemon in the background from
//! the command line. A running daemon is found through the instance lock
//! ([`instance_lock::is_running`]), which is authoritative; what it records beside the lock
//! ([`Owner`]) says which process to stop, where it listens and what key it requires, and each
//! command waits for a planned restart caught half-way to settle before it judges
//! ([`find_daemon`]). Each returns the exit status. The daemon itself starts its successor after a
//! planned restart the way `start` starts it ([`start_successor`]). `stop` and `restart` signal the
//! daemon where the host has a signal for it; where it has none (Windows), they ask over loopback,
//! with the key the daemon recorded ([`request_over_loopback`]), which `address --rotate` uses on
//! every host to ask the daemon to replace its owner key.

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

use super::succession::{
    abandoned, awaits_end, find_daemon, gone_since_after, grace_elapsed, handed_over_to, handover,
    judge_after, recorded_after, took_over, Found, Handover, HandoverWatch, Judged, Standing,
    TookOver, POLL, SERVICE_RESTART_GRACE,
};

/// Exit status of `status` and `address` when no daemon runs.
const NOT_RUNNING: i32 = 3;

/// How long `start` waits for the daemon's handshake line: opening the store and sweeping what the
/// previous run left come first.
const START_WAIT: Duration = Duration::from_secs(30);

/// How long `stop` waits for the lock to be free: ending every session takes time.
const STOP_WAIT: Duration = Duration::from_secs(60);

/// How long `restart` waits for the next daemon to hold the lock: the planned stop, then the next
/// daemon's start, each within what `stop` and `start` wait for.
const RESTART_WAIT: Duration = Duration::from_secs(STOP_WAIT.as_secs() + START_WAIT.as_secs());

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

fn not_running() -> String {
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
/// waiting `waited` for it to.
fn no_record(what: &str, waited: Duration) -> String {
    format!(
        "{} is running, but after {} seconds still no record says which process it is (it may be \
         starting or stopping, or could not write the record); not {what} it",
        crate::APP_NAME,
        waited.as_secs()
    )
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

/// Prints the address to open the running daemon at in a browser, its key included, as one line on
/// stdout: `http://localhost:<port>/?key=<key>`, `localhost` being what a browser treats as a
/// secure context and what an SSH port forward gives on the owner's machine. Exits 3 when no
/// daemon runs. The key is the owner key when the daemon's record says it accepts one and
/// `owner.key` holds one, which outlives the daemon, else the launch key from the daemon's
/// owner-only record. Only the daemon's own user obtains either.
///
/// With `rotate`, the owner key is replaced first (see [`rotate_owner_key`]) and the address
/// printed is the new one's, except where the daemon does not take an owner key: it is the launch
/// key's, and the new owner key is for the next daemon started on its own.
pub(super) fn print_address(rotate: bool) -> Result<i32> {
    print_address_of(rotate, false)
}

/// [`print_address`], waiting through a daemon that holds the lock but has not recorded itself when
/// `through_unrecorded`: it is a daemon this command saw start, whose record is yet to come.
fn print_address_of(rotate: bool, through_unrecorded: bool) -> Result<i32> {
    let owner = match find_daemon(RESTART_WAIT, through_unrecorded)? {
        Found::NotRunning { .. } => {
            if rotate {
                return match rotate_without_daemon(
                    &crate::paths::data_dir(),
                    instance_lock::is_running,
                )? {
                    Some(status) => Ok(status),
                    // A daemon started while the file was being replaced, and may have read the
                    // earlier key before it was: rotate through the daemon, which keeps the file
                    // and its memory alike. It has only just started, so its record may be still
                    // to come: wait through a daemon that has not recorded itself yet.
                    None => print_address_of(true, true),
                };
            }
            eprintln!("{}", not_running());
            return Ok(NOT_RUNNING);
        }
        Found::Running { owner, .. } => owner,
        Found::Unrecorded => {
            eprintln!(
                "{} is running, but its address is not known (it may still be starting or \
                 stopping)",
                crate::APP_NAME
            );
            return Ok(1);
        }
    };
    let data = crate::paths::data_dir();
    let Some(key) = owner.recorded_key() else {
        eprintln!("{}", address_key_not_recorded(&owner));
        return Ok(1);
    };
    if rotate && owner.takes_owner_key {
        return rotate_owner_key(&owner, &key, &data);
    }
    if rotate {
        return rotate_for_daemon_without_owner_key(&owner, &key, &data);
    }
    let (address, lasting) = address_to_print(&owner, &key, &data);
    if !lasting {
        eprintln!(
            "the address printed lasts only until {} restarts, as owner.key is missing or holds no \
             usable key; `{} address --rotate` makes a lasting one",
            crate::APP_NAME,
            crate::APP_NAME
        );
    }
    println!("{address}");
    Ok(0)
}

/// The address `address` prints for `owner`, and whether it is one that outlives the daemon: with
/// the owner key when the daemon accepts one and `data` holds one, else with the launch key `key`.
/// A daemon that takes no owner key has no lasting address to lose, so it counts as one.
fn address_to_print(owner: &Owner, key: &str, data: &Path) -> (String, bool) {
    let owner_key = owner
        .takes_owner_key
        .then(|| crate::owner_key::read_in(data))
        .flatten();
    let lasting = owner_key.is_some() || !owner.takes_owner_key;
    (
        browser_address(owner.port, owner_key.as_deref().unwrap_or(key)),
        lasting,
    )
}

/// `address --rotate` for a running daemon that takes no owner key: replaces the owner key in
/// `data` for the next daemon started on its own, says so on stderr, and prints the launch-key
/// address of the running one.
fn rotate_for_daemon_without_owner_key(owner: &Owner, key: &str, data: &Path) -> Result<i32> {
    crate::owner_key::replace_in(data).context("replacing the owner key")?;
    eprintln!(
        "the running {} does not take the owner key (it follows the desktop application or is an \
         earlier build, or could not read or write owner.key at its start); the next daemon \
         started on its own takes the new one",
        crate::APP_NAME
    );
    println!("{}", browser_address(owner.port, key));
    Ok(0)
}

/// Why `address` cannot print the address of a daemon whose launch key is not recorded.
fn address_key_not_recorded(owner: &Owner) -> String {
    format!(
        "{} is running (pid {}, port {}), but its key is not recorded: it may be an earlier \
         build, which a restart with this build replaces, or one that could not record it, \
         as its log says",
        crate::APP_NAME,
        owner.identity.pid.0,
        owner.port
    )
}

/// Asks the running daemon `owner` names to replace its owner key, presenting its launch key
/// `key`, and prints the address with the new one. A refusal or a failure is said on stderr, with
/// status 1 and nothing on stdout.
fn rotate_owner_key(owner: &Owner, key: &str, data: &Path) -> Result<i32> {
    let failed = |what: String| {
        eprintln!("the owner key was not rotated: {what}");
        Ok(1)
    };
    match request_over_loopback(owner, key, "/owner-key/rotate") {
        Ok(Asked::Accepted) => {}
        Ok(Asked::Refused(message)) => return failed(format!("the daemon refused: {message}")),
        Ok(Asked::Gone) => return failed("the daemon no longer accepts connections".to_owned()),
        Err(err) => return failed(format!("{err:#}")),
    }
    match crate::owner_key::read_in(data) {
        Some(new) => {
            println!("{}", browser_address(owner.port, &new));
            Ok(0)
        }
        None => failed("the daemon took the request, but owner.key holds no key".to_owned()),
    }
}

/// `address --rotate` with no daemon running: replaces the owner key in the data directory `data`
/// for the next daemon started on its own, unless there is no data directory, which it does not
/// create. Prints nothing on stdout, and exits 3 as `address` does.
///
/// The instance lock cannot be held while the file is written (it is the daemon's for its
/// lifetime, and a probe holds it for a moment), so `running` is asked again afterwards: `None`
/// when a daemon holds the lock by then, which the caller rotates through instead.
fn rotate_without_daemon(data: &Path, running: impl Fn() -> Result<bool>) -> Result<Option<i32>> {
    if !data.is_dir() {
        eprintln!("{}", not_running());
        return Ok(Some(NOT_RUNNING));
    }
    crate::owner_key::replace_in(data).context("replacing the owner key")?;
    if running()? {
        return Ok(None);
    }
    eprintln!(
        "{} is not running; the next daemon started on its own takes the new owner key",
        crate::APP_NAME
    );
    Ok(Some(NOT_RUNNING))
}

fn browser_address(port: u16, key: &str) -> String {
    format!("http://localhost:{port}/?key={key}")
}

pub(super) fn stop() -> Result<i32> {
    let began = Instant::now();
    let outcome = match find_daemon(RESTART_WAIT, true)? {
        // Judged once the daemon whose handover the stop waited through has ended, where its
        // successor starts only after that. One that is not its successor is left running, as a
        // daemon that took over from none the stop followed would be.
        Found::Running {
            owner,
            after: Some(from),
        } => {
            // A daemon found holding the lock while a service-managed `from` still ran is not its
            // successor, which the service manager starts only after `from` has exited.
            let found_before_end = awaits_end(&from, !from.identity.is_running());
            let ended = ended_by(&from, began + RESTART_WAIT);
            match judge_after(found_before_end, ended, handed_over_to(&from, &owner)) {
                Judged::Waiting => StopOutcome::StillStopping {
                    pid: from.identity.pid.0,
                    waited: began.elapsed(),
                    other: found_before_end.then_some(owner),
                },
                Judged::Successor => stop_and_wait(&owner, began)?,
                Judged::Other => StopOutcome::Superseded {
                    ended: from.identity.pid.0,
                    other: owner,
                },
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
            eprintln!("{}", no_record("stopping", RESTART_WAIT));
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
/// ended where [`awaits_end`] says it must before the daemon found can be judged its successor.
/// Whether it has.
fn ended_by(from: &Owner, deadline: Instant) -> bool {
    while awaits_end(from, !from.identity.is_running()) {
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
            eprintln!("{}", no_record("restarting", RESTART_WAIT));
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
    // Read before the request, since a daemon removes its record as it ends: the successor it
    // starts itself shows itself by this key.
    let key = owner.recorded_key();
    let asked = match proc::request_restart(&owner.identity) {
        Sent::Delivered => Asked::Accepted,
        Sent::Gone => Asked::Gone,
        Sent::Failed(err) if err.kind() == std::io::ErrorKind::Unsupported => {
            let Some(key) = key.as_deref() else {
                eprintln!("{}", key_not_recorded(&owner, "restart"));
                return Ok(1);
            };
            request_over_loopback(&owner, key, "/restart")?
        }
        Sent::Failed(err) => {
            return Err(err).context(format!("signalling pid {}", owner.identity.pid.0))
        }
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
    let mut watch = HandoverWatch::default();
    // Since when the lock has been seen free with the daemon's process ended.
    let mut gone_since: Option<Instant> = None;
    loop {
        let lock_held = instance_lock::is_running()?;
        match took_over(&owner, key.as_deref(), lock_held, recorded_after(&owner)) {
            Some(TookOver::Successor(next)) => {
                println!(
                    "{} restarted: pid {}, {}",
                    crate::APP_NAME,
                    next.identity.pid.0,
                    address(next.port)
                );
                return Ok(0);
            }
            Some(TookOver::Other(other)) => {
                eprintln!(
                    "{}, but another daemon has started since and is left running: pid {}, {}; \
                     nothing shows that the restarted one handed over to it",
                    stopped_phrase(owner.identity.pid.0, not_accepting),
                    other.identity.pid.0,
                    address(other.port)
                );
                return Ok(1);
            }
            None => {}
        }
        let ended = !owner.identity.is_running();
        // The lock looked at again after the note: a successor that took it and recorded itself
        // since it was seen free has removed the note.
        let noted = Owner::handover_noted().filter(|noted| noted.from.identity == owner.identity);
        let now = Instant::now();
        let handing_over = watch.pending(noted.as_ref(), now, SystemTime::now());
        gone_since = gone_since_after(gone_since, now, !lock_held && ended);
        let grace_over = grace_elapsed(gone_since, now, SERVICE_RESTART_GRACE);
        if restart_abandoned(
            &owner,
            lock_held,
            ended,
            handing_over,
            grace_over,
            not_accepting,
        ) && !instance_lock::is_running()?
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

/// Whether `restart` has seen the daemon `owner` names end with no successor on its way
/// ([`abandoned`]), with `grace_over` saying that a service manager has had its time to start one
/// ([`SERVICE_RESTART_GRACE`]): a daemon the manager restarts has no note of a handover when it
/// ended before it was asked, so the grace is waited out for it before judging. A daemon that did
/// not accept the request (`not_accepting`), whose record may not say how its successor starts, is
/// judged ended once the lock is free and no handover is noted.
fn restart_abandoned(
    owner: &Owner,
    lock_held: bool,
    ended: bool,
    handing_over: bool,
    grace_over: bool,
    not_accepting: bool,
) -> bool {
    let judged = abandoned(owner, lock_held, ended, handing_over)
        || (not_accepting && !lock_held && ended && !handing_over);
    judged && (owner.service_managed != Some(true) || grace_over)
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
    match spawn_and_await_handshake(&exe, port, None) {
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
/// it at the same address with the same key. `exe` is [`launch_path`]'s answer, taken at this
/// daemon's start (see `installation.rs` for what it follows). Called once this daemon has
/// released the instance lock and the port. Logs how it went; whether it came up. **Blocks** until
/// it reports listening, or for at most [`START_WAIT`].
pub(crate) fn start_successor(port: u16, key: &str, exe: &Path) -> bool {
    match spawn_and_await_handshake(exe, port, Some(key)) {
        Ok(started) => {
            tracing::info!(pid = started.pid, port, "the next daemon is listening");
            if started.in_starter_job {
                tracing::warn!("the job this daemon runs in does not let a process break away: the next daemon ends when that job does");
            }
            true
        }
        Err(StartFailure::Error(err)) => {
            tracing::error!(err = %format!("{err:#}"), "starting the next daemon failed; the sessions are kept for the next start");
            false
        }
        Err(StartFailure::TimedOut { said }) => {
            tracing::error!(
                said = %said.trim_end(),
                "the next daemon did not report listening within {} seconds and was killed; the sessions are kept for the next start",
                START_WAIT.as_secs()
            );
            false
        }
        Err(StartFailure::Ended { status, said }) => {
            tracing::error!(%status, said = %said.trim_end(), "the next daemon ended while starting; the sessions are kept for the next start");
            false
        }
    }
}

/// Starts the daemon `exe` on `port` in the background and waits for its handshake line, killing it
/// when none comes in time. With `key`, the daemon is told to read its key from its stdin, where it
/// is written; otherwise it makes one.
fn spawn_and_await_handshake(
    exe: &Path,
    port: u16,
    key: Option<&str>,
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
    if key.is_some() {
        command.arg("--key-stdin");
    }
    // The pipes on stdout and stderr carry the handshake and what a failed start wrote, and the one
    // on stdin, with a key, the key. They close when the starter exits, so what the daemon writes
    // to stderr from then on fails; its log tolerates that (see `init_logging`), but a process the
    // daemon starts must set its own stdio, or one that inherited the dead stderr would be killed
    // by SIGPIPE.
    let (mut child, in_starter_job): (proc::BackgroundChild, bool) =
        proc::spawn_in_background(&mut command, key.is_some()).context("starting the daemon")?;
    let pid = child.id();
    if let Some(key) = key {
        // A line, well within what a pipe holds, so the write does not wait for the daemon to read
        // it; the pipe is closed once it is written.
        let mut stdin = child.stdin.take().expect("stdin was piped");
        if let Err(err) = writeln!(stdin, "{key}") {
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
    stop_following(owner, owner.recorded_key(), true, began, None)
}

/// Asks the daemon `owner` names to end — by the termination signal where the host has one, else
/// over loopback with `key` — and waits for it to be gone, following it through a planned restart
/// so that the stop leaves no daemon of its line running, as an ordinary stop would. A daemon in a
/// planned restart no longer accepts connections, and one that has already settled on its
/// successor ignores the signal; either way its successor takes the lock, and is then asked in its
/// place (see [`handover`] for which daemon counts as one). `key` is the key recorded for `owner`,
/// read before it is asked, since a daemon removes its record as it ends. `follow_service` says
/// whether a successor a service manager starts may still be followed. Until [`STOP_WAIT`] after a
/// daemon takes the request, and in any case [`RESTART_WAIT`] after `began`, when the command
/// started, which the wait for the daemon to be found counts in too; a daemon that has ended with
/// the lock free by then is gone, whatever grace a service manager had left. `taken_at` says
/// when a daemon this stop asked before `owner`, on its way to it, took the request over
/// loopback, which is when the bound of [`STOP_WAIT`] counts from for `owner` too.
fn stop_following(
    owner: &Owner,
    key: Option<String>,
    follow_service: bool,
    began: Instant,
    taken_at: Option<Instant>,
) -> Result<StopOutcome> {
    let mut over_loopback = false;
    let asked = match proc::signal_identified(&owner.identity, Signal::Polite(Polite::Terminate)) {
        Sent::Delivered => Asked::Accepted,
        Sent::Gone => Asked::Gone,
        Sent::Failed(err) if err.kind() == std::io::ErrorKind::Unsupported => {
            let Some(key) = key.as_deref() else {
                return Ok(StopOutcome::NoKey(*owner));
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
    // Since when the lock has been seen free with the daemon's process ended, for the grace a
    // service manager has to start the successor.
    let mut gone_since: Option<Instant> = None;
    // Whether a note has named `owner` as handing over. It is read before the record below, since
    // the next daemon removes it only once it has recorded itself.
    let mut handing_over = false;
    loop {
        let lock_held = instance_lock::is_running()?;
        let ended = !owner.identity.is_running();
        handing_over |=
            Owner::handover_noted().is_some_and(|noted| noted.from.identity == owner.identity);
        let now = Instant::now();
        gone_since = gone_since_after(gone_since, now, !lock_held && ended);
        let grace_over = now >= deadline || grace_elapsed(gone_since, now, SERVICE_RESTART_GRACE);
        match handover(
            owner,
            key.as_deref(),
            lock_held,
            ended,
            recorded_after(owner),
            Standing {
                follow_service,
                grace_over,
                handing_over,
            },
        ) {
            Some(Handover::Gone) => return Ok(outcome_once_gone(taken_at.is_some(), took)),
            Some(Handover::Successor {
                next,
                key,
                by_service,
            }) => {
                let follow_service = follow_service && !by_service;
                let taken_at = carried_on(taken_at, took && over_loopback, asked_at);
                return stop_following(&next, key, follow_service, began, taken_at);
            }
            Some(Handover::Other(other)) => {
                return Ok(StopOutcome::Superseded {
                    ended: owner.identity.pid.0,
                    other,
                })
            }
            None => {}
        }
        if Instant::now() >= deadline {
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
/// For which requests count, see docs/product/server-form.md's "Running in the background from the
/// command line" section.
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
enum Asked {
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
/// failure is an error.
fn request_over_loopback(owner: &Owner, key: &str, path: &str) -> Result<Asked> {
    match loopback::post(owner.port, key, path, b"{}", REQUEST_WAIT) {
        Ok(_) => Ok(Asked::Accepted),
        Err(err) if loopback::error_status(&err) == Some(409) => Ok(Asked::Refused(
            loopback::error_body(&err)
                .and_then(|body| serde_json::from_slice::<serde_json::Value>(body).ok())
                .and_then(|body| body["message"].as_str().map(str::to_owned))
                .unwrap_or_else(|| "no reason given".to_owned()),
        )),
        Err(err) if no_longer_accepting(&err) => Ok(Asked::Gone),
        Err(err) => Err(err).context(format!(
            "asking pid {} on port {} ({path})",
            owner.identity.pid.0, owner.port
        )),
    }
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
mod tests {
    use super::*;

    use std::net::TcpListener;

    use crate::platform::proc::{ProcessId, ProcessIdentity};
    use crate::test_support::PATIENCE;

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn owner_on(port: u16) -> Owner {
        Owner {
            identity: ProcessIdentity {
                pid: ProcessId(std::process::id()),
                start_micros: 1,
            },
            port,
            follows_application: Some(false),
            service_managed: Some(false),
            takes_owner_key: false,
        }
    }

    /// Answers one request with `response`, and gives its request line (through the first line
    /// of the head) back through the handle.
    fn answer_once(response: impl Into<String>) -> (u16, std::thread::JoinHandle<String>) {
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

    #[test]
    fn the_browser_address_is_localhost_at_the_port_with_the_key() {
        assert_eq!(
            browser_address(7531, KEY),
            format!("http://localhost:7531/?key={KEY}")
        );
    }

    const OWNER_KEY: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    fn owner_taking_key_on(port: u16) -> Owner {
        owner_on(port).with_owner_key(true)
    }

    /// The owner key is printed only for a daemon that accepts one and only when the file holds a
    /// key; every other case gets the launch key.
    #[test]
    fn the_address_has_the_owner_key_only_where_the_daemon_takes_it() {
        let dir = crate::test_support::ScratchDir::new("address-owner-key");
        let launch = format!("http://localhost:7531/?key={KEY}");
        let owner = format!("http://localhost:7531/?key={OWNER_KEY}");
        // No file.
        assert_eq!(
            address_to_print(&owner_taking_key_on(7531), KEY, &dir),
            (launch.clone(), false)
        );
        std::fs::write(crate::owner_key::path_in(&dir), format!("{OWNER_KEY}\n")).unwrap();
        assert_eq!(
            address_to_print(&owner_taking_key_on(7531), KEY, &dir),
            (owner, true)
        );
        // A daemon without the token (desktop application, earlier build).
        assert_eq!(
            address_to_print(&owner_on(7531), KEY, &dir),
            (launch.clone(), true)
        );
        // A file that holds no key.
        std::fs::write(crate::owner_key::path_in(&dir), "nonsense\n").unwrap();
        assert_eq!(
            address_to_print(&owner_taking_key_on(7531), KEY, &dir),
            (launch, false)
        );
    }

    /// The rotation goes to `/owner-key/rotate` with the launch key, and on a `204` the address
    /// printed is of the key the daemon wrote.
    #[test]
    fn a_rotation_asks_with_the_launch_key_and_reads_the_new_key() {
        let dir = crate::test_support::ScratchDir::new("rotate-asked");
        std::fs::write(crate::owner_key::path_in(&dir), format!("{OWNER_KEY}\n")).unwrap();
        let (port, served) = answer_once("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
        let status = rotate_owner_key(&owner_taking_key_on(port), KEY, &dir).unwrap();
        assert_eq!(status, 0);
        assert_eq!(
            served.join().unwrap(),
            format!("POST /owner-key/rotate?key={KEY} HTTP/1.1\r\n")
        );
    }

    /// A refusal or a failed request is status 1, and the owner key is not read.
    #[test]
    fn a_refused_or_failed_rotation_is_status_1() {
        let dir = crate::test_support::ScratchDir::new("rotate-refused");
        std::fs::write(crate::owner_key::path_in(&dir), format!("{OWNER_KEY}\n")).unwrap();
        let refusal = r#"{"message":"this daemon holds no owner key"}"#;
        for response in [
            "HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n".to_owned(),
            "HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\n".to_owned(),
            format!(
                "HTTP/1.1 409 Conflict\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{refusal}",
                refusal.len()
            ),
        ] {
            let (port, served) = answer_once(response.clone());
            assert_eq!(
                rotate_owner_key(&owner_taking_key_on(port), KEY, &dir).unwrap(),
                1,
                "{response}"
            );
            served.join().unwrap();
        }
        // A daemon that took the request but left no key behind.
        std::fs::remove_file(crate::owner_key::path_in(&dir)).unwrap();
        let (port, served) = answer_once("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
        assert_eq!(
            rotate_owner_key(&owner_taking_key_on(port), KEY, &dir).unwrap(),
            1
        );
        served.join().unwrap();
    }

    /// With no daemon, a rotation replaces the file when the data directory exists and creates
    /// nothing when it does not; both are status 3.
    #[test]
    fn a_rotation_with_no_daemon_replaces_the_file_only_in_an_existing_data_directory() {
        let dir = crate::test_support::ScratchDir::new("rotate-no-daemon");
        std::fs::write(crate::owner_key::path_in(&dir), format!("{OWNER_KEY}\n")).unwrap();
        assert_eq!(
            rotate_without_daemon(&dir, || Ok(false)).unwrap(),
            Some(NOT_RUNNING)
        );
        let new = crate::owner_key::read_in(&dir).expect("a key");
        assert_ne!(new, OWNER_KEY);

        let missing = dir.join("no-such-data-directory");
        assert_eq!(
            rotate_without_daemon(&missing, || Ok(false)).unwrap(),
            Some(NOT_RUNNING)
        );
        assert!(!missing.exists());
    }

    /// A daemon that takes no owner key is told so and the file is replaced for the next one; the
    /// address is its launch key's.
    #[test]
    fn a_rotation_for_a_daemon_without_an_owner_key_replaces_the_file_and_exits_0() {
        let dir = crate::test_support::ScratchDir::new("rotate-daemon-without");
        std::fs::write(crate::owner_key::path_in(&dir), format!("{OWNER_KEY}\n")).unwrap();
        assert_eq!(
            rotate_for_daemon_without_owner_key(&owner_on(7531), KEY, &dir).unwrap(),
            0
        );
        let new = crate::owner_key::read_in(&dir).expect("a key");
        assert_ne!(new, OWNER_KEY);
    }

    /// A daemon that holds the lock once the file is written is not told the next one takes the
    /// key: the caller rotates through it.
    #[test]
    fn a_daemon_that_started_during_a_rotation_without_one_is_rotated_through() {
        let dir = crate::test_support::ScratchDir::new("rotate-raced");
        assert_eq!(rotate_without_daemon(&dir, || Ok(true)).unwrap(), None);
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

    /// A daemon that ended without being asked, which a service manager restarts, is not judged
    /// abandoned before the manager has had its grace; one without a record of how its successor
    /// starts, which cannot be waited for, is judged at once; a handover noted or a held lock
    /// holds the judgement back.
    #[test]
    fn a_restart_gives_a_service_manager_its_grace_before_judging_a_gone_daemon() {
        let mut owner = owner_on(1);
        owner.service_managed = Some(true);
        assert!(!restart_abandoned(&owner, false, true, false, false, true));
        assert!(restart_abandoned(&owner, false, true, false, true, true));
        assert!(!restart_abandoned(&owner, false, true, true, true, true));
        assert!(!restart_abandoned(&owner, true, true, false, true, true));
        // One that accepted the request and ended without a note waits out the grace as well.
        assert!(!restart_abandoned(&owner, false, true, false, false, false));
        assert!(restart_abandoned(&owner, false, true, false, true, false));
        owner.service_managed = None;
        assert!(restart_abandoned(&owner, false, true, false, false, true));
        assert!(!restart_abandoned(&owner, false, true, false, false, false));
        owner.service_managed = Some(false);
        assert!(restart_abandoned(&owner, false, true, false, false, false));
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
}
