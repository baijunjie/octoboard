//! `start`, `stop`, `restart` and `status`: running the daemon in the background from the command
//! line. A running daemon is found through the instance lock ([`instance_lock::is_running`]), which
//! is authoritative; what it records beside the lock ([`Owner`]) says which process to stop and
//! where it listens. Each returns the exit status. The daemon itself starts its successor after a
//! planned restart the way `start` starts it ([`start_successor`]).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::instance_lock::{self, Owner};
use crate::platform::proc::{self, Polite, Sent, Signal};

/// Exit status of `status` when no daemon runs.
const NOT_RUNNING: i32 = 3;

/// How long `start` waits for the daemon's handshake line: opening the store and sweeping what the
/// previous run left come first.
const START_WAIT: Duration = Duration::from_secs(30);

/// How long `stop` waits for the lock to be free: ending every session takes time.
const STOP_WAIT: Duration = Duration::from_secs(60);

/// How long `restart` waits for the next daemon to hold the lock: the planned stop, then the next
/// daemon's start, each within what `stop` and `start` wait for.
const RESTART_WAIT: Duration = Duration::from_secs(STOP_WAIT.as_secs() + START_WAIT.as_secs());

const POLL: Duration = Duration::from_millis(100);

/// What the starter keeps of what a starting daemon wrote on its stderr, for a failure to show.
const STDERR_KEPT: usize = 16 * 1024;

fn address(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn not_running() -> String {
    format!("{} is not running", crate::APP_NAME)
}

/// What is known of the running daemon, to follow "is" or "is already": who it is and where it
/// listens, when the record beside the lock says.
fn running_state() -> String {
    match Owner::recorded() {
        Some(owner) => format!(
            "running: pid {}, {}",
            owner.identity.pid.0,
            address(owner.port)
        ),
        None => "running; its address is not known".to_owned(),
    }
}

pub(super) fn status() -> Result<i32> {
    if !instance_lock::is_running()? {
        println!("{}", not_running());
        return Ok(NOT_RUNNING);
    }
    println!("{} is {}", crate::APP_NAME, running_state());
    Ok(0)
}

pub(super) fn stop() -> Result<i32> {
    if !instance_lock::is_running()? {
        println!("{}", not_running());
        return Ok(0);
    }
    let Some(owner) = Owner::recorded() else {
        eprintln!(
            "{} is running, but no record says which process it is (it may still be starting or \
             stopping); not stopping it",
            crate::APP_NAME
        );
        return Ok(1);
    };
    match stop_and_wait(&owner)? {
        StopOutcome::Stopped => {
            println!("{} stopped", crate::APP_NAME);
            Ok(0)
        }
        StopOutcome::Unsupported => {
            eprintln!("stopping the daemon is not supported on this host");
            Ok(1)
        }
        StopOutcome::StillStopping => {
            eprintln!(
                "{} is still stopping after {} seconds (pid {})",
                crate::APP_NAME,
                STOP_WAIT.as_secs(),
                owner.identity.pid.0
            );
            Ok(1)
        }
    }
}

pub(super) fn restart() -> Result<i32> {
    if !instance_lock::is_running()? {
        eprintln!("{}", not_running());
        return Ok(1);
    }
    let Some(owner) = Owner::recorded() else {
        eprintln!(
            "{} is running, but no record says which process it is (it may still be starting or \
             stopping); not restarting it",
            crate::APP_NAME
        );
        return Ok(1);
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
        // A record without the token (an older build, or a daemon whose handler for the request
        // could not be installed) would be ended by the request, not restarted.
        None => {
            eprintln!(
                "{} (pid {}) cannot take a restart request (an older build, or its handler could \
                 not be installed); stop and start it instead",
                crate::APP_NAME,
                owner.identity.pid.0
            );
            return Ok(1);
        }
    }
    match proc::request_restart(&owner.identity) {
        Sent::Delivered => {}
        Sent::Gone => {
            eprintln!("{} stopped before it could be restarted", crate::APP_NAME);
            return Ok(1);
        }
        Sent::Failed(err) if err.kind() == std::io::ErrorKind::Unsupported => {
            eprintln!("restarting the daemon is not supported on this host");
            return Ok(1);
        }
        Sent::Failed(err) => {
            return Err(err).context(format!("signalling pid {}", owner.identity.pid.0))
        }
    }
    let deadline = Instant::now() + RESTART_WAIT;
    loop {
        if let Some(next) = Owner::recorded().filter(|next| next.identity != owner.identity) {
            if instance_lock::is_running()? {
                println!(
                    "{} restarted: pid {}, {}",
                    crate::APP_NAME,
                    next.identity.pid.0,
                    address(next.port)
                );
                return Ok(0);
            }
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
            RESTART_WAIT.as_secs(),
            owner.identity.pid.0
        );
    } else {
        eprintln!(
            "{} stopped, but no daemon took over within {} seconds; its log says why, and the \
             sessions are kept for the next start",
            crate::APP_NAME,
            RESTART_WAIT.as_secs()
        );
    }
    Ok(1)
}

pub(super) fn start(port: u16) -> Result<i32> {
    if instance_lock::is_running()? {
        eprintln!("{} is already {}", crate::APP_NAME, running_state());
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
            Ok(0)
        }
        Err(StartFailure::Unsupported) => {
            eprintln!("running in the background is not supported on this host");
            Ok(1)
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
}

/// Why no daemon was started.
enum StartFailure {
    /// The host cannot run the daemon in the background.
    Unsupported,
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
            true
        }
        Err(StartFailure::Unsupported) => {
            tracing::error!("the next daemon cannot be started in the background on this host; the sessions are kept for the next start");
            false
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
    // The pipes carry the handshake and what a failed start wrote. They close when the starter
    // exits, so what the daemon writes to stderr from then on fails; its log tolerates that (see
    // `init_logging`), but a process the daemon starts must set its own stdio, or one that
    // inherited the dead stderr would be killed by SIGPIPE.
    command
        .args(["--port", &port.to_string()])
        .current_dir(&data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if key.is_some() {
        command.arg("--key-stdin").stdin(Stdio::piped());
    }
    match proc::run_in_background(&mut command) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::Unsupported => {
            return Err(StartFailure::Unsupported)
        }
        Err(err) => {
            return Err(anyhow::Error::new(err)
                .context("preparing the background start")
                .into())
        }
    }
    let mut child = command.spawn().context("starting the daemon")?;
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
    let stderr_reader = {
        let kept = stderr_text.clone();
        std::thread::spawn(move || drain_kept(stderr, &kept))
    };

    let outcome = line_rx.recv_timeout(START_WAIT);
    let timed_out = outcome.is_err();
    if let Ok(Some(line)) = outcome {
        return Ok(Started { pid, line });
    }
    if timed_out {
        child.kill().ok();
    }
    let status = child.wait().context("waiting for the daemon")?;
    // The pipe closed with the child; what it wrote is all there.
    stderr_reader.join().ok();
    let said = String::from_utf8_lossy(&stderr_text.lock().unwrap()).into_owned();
    Err(if timed_out {
        StartFailure::TimedOut { said }
    } else {
        StartFailure::Ended { status, said }
    })
}

/// How [`stop_and_wait`] ended.
enum StopOutcome {
    /// The daemon is gone: the lock is free.
    Stopped,
    /// The host cannot signal a daemon to stop.
    Unsupported,
    /// The daemon still holds the lock after [`STOP_WAIT`].
    StillStopping,
}

/// Asks the daemon `owner` names to end, and waits for it to release the lock.
fn stop_and_wait(owner: &Owner) -> Result<StopOutcome> {
    match proc::signal_identified(&owner.identity, Signal::Polite(Polite::Terminate)) {
        Sent::Delivered | Sent::Gone => {}
        Sent::Failed(err) if err.kind() == std::io::ErrorKind::Unsupported => {
            return Ok(StopOutcome::Unsupported)
        }
        Sent::Failed(err) => {
            return Err(err).context(format!("signalling pid {}", owner.identity.pid.0))
        }
    }
    let deadline = Instant::now() + STOP_WAIT;
    while instance_lock::is_running()? {
        if Instant::now() >= deadline {
            return Ok(StopOutcome::StillStopping);
        }
        std::thread::sleep(POLL);
    }
    Ok(StopOutcome::Stopped)
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
