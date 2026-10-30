//! Spawning the `octoboardd` sidecar, parsing its startup output, starting it with retries, and
//! stopping it for a restart. What happens when it ends by itself is `recovery.rs`.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::diagnostics::diagnostic;
use crate::recovery::{self, ExitAction, RestartBudget};

/// How long to wait for `octoboardd` to print its startup line before giving up on that attempt.
/// Generous: this is a cold process start (including, on first launch, SQLite schema setup), not a
/// steady-state operation.
const SIDECAR_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// How many times the initial start tries before the window is opened with the failure.
const STARTUP_ATTEMPTS: u32 = 3;

/// How long a start attempt that gave up waits for its daemon to be gone, which releases the
/// instance lock the next attempt needs, after killing it.
const ATTEMPT_KILL_WAIT: Duration = Duration::from_secs(5);

/// How much of a daemon's stderr is kept for the message about a start that failed: the end of it,
/// where the reason is.
const STDERR_TAIL_BYTES: usize = 16 * 1024;

/// The sidecar's state: whether the daemon is gone, where it is now, the handle `stop_daemon` ends
/// it through, and what the recovery in `recovery.rs` keeps between its attempts.
#[derive(Default)]
pub struct DaemonState {
    /// Whether the daemon failed to start or has exited. With no session left to keep running, the
    /// window's close button quits and a quit needs no confirmation.
    stopped: AtomicBool,
    /// Whether a restart or a quit is under way, in which case no daemon is started: set by
    /// `stop_daemon` and on every way out of the application.
    pub(crate) stopping: AtomicBool,
    /// How many recovery threads are running, which a restart waits out before it launches the next
    /// instance.
    pub(crate) recovering: AtomicUsize,
    /// Where the daemon is now, or how it ended, for a window that opens later to be built from.
    status: Mutex<DaemonStatus>,
    /// The number of the latest daemon spawned, or retired by `retire_and_kill`. What a daemon's
    /// events say about it counts only while its own number is this one.
    generation: AtomicU64,
    /// The running sidecar, for `stop_daemon` to end when it does not exit by itself.
    child: Mutex<Option<CommandChild>>,
    pub(crate) restarts: Mutex<RestartBudget>,
}

/// How long a restart waits for the daemon to exit on its own after the UI asked it to shut down,
/// before it sends SIGTERM.
const DAEMON_EXIT_GRACE: Duration = Duration::from_secs(2);

/// How long a restart waits for the daemon to exit after SIGTERM, which has it stop its sessions'
/// processes first, before it kills it.
const DAEMON_TERM_GRACE: Duration = Duration::from_secs(5);

/// How long a restart waits for the daemon to be gone after killing it.
const DAEMON_KILL_WAIT: Duration = Duration::from_secs(2);

/// How long a restart waits for the recoveries under way to notice it and end the daemon they
/// started.
const RECOVERY_END_WAIT: Duration = Duration::from_secs(15);

/// Polls `condition` every few milliseconds until it holds or `timeout` has passed; says whether it held.
pub fn wait_for(timeout: Duration, condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Makes sure the daemon has exited before a restart launches a new instance, whose own daemon refuses to
/// start while this one still holds the instance lock (`instance_lock` in the daemon). The UI has asked it to
/// shut down before confirming the restart, so this normally returns at once or within moments. One that has
/// not exited within `DAEMON_EXIT_GRACE` gets SIGTERM, which the daemon handles like that request, and one
/// still running `DAEMON_TERM_GRACE` later is killed. Blocks the calling thread: call it off the main thread.
pub fn stop_daemon(app: &AppHandle) {
    let state = app.state::<DaemonState>();
    state.stopping.store(true, Ordering::SeqCst);
    // A recovery under way sees the flag and ends the daemon it was starting; the daemon is not stopped
    // until it has.
    wait_for(RECOVERY_END_WAIT, || {
        state.recovering.load(Ordering::SeqCst) == 0
    });
    escalate_stop(
        || daemon_stopped(app),
        || {
            if let Some(child) = app
                .state::<DaemonState>()
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                terminate(child.pid());
            }
        },
        || {
            let child = app
                .state::<DaemonState>()
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(child) = child {
                let _ = child.kill();
            }
        },
        [DAEMON_EXIT_GRACE, DAEMON_TERM_GRACE, DAEMON_KILL_WAIT],
    );
}

/// The escalation of `stop_daemon`: waits for `stopped` for each of the three durations in turn, running
/// `terminate` before the second wait and `kill` before the third, and stops at the first wait that sees it.
fn escalate_stop(
    stopped: impl Fn() -> bool,
    terminate: impl FnOnce(),
    kill: impl FnOnce(),
    [exit_grace, term_grace, kill_wait]: [Duration; 3],
) {
    if wait_for(exit_grace, &stopped) {
        return;
    }
    terminate();
    if wait_for(term_grace, &stopped) {
        return;
    }
    kill();
    wait_for(kill_wait, &stopped);
}

/// Sends SIGTERM to `pid`, the signal the daemon's `shutdown_signal` waits for.
#[cfg(unix)]
fn terminate(pid: u32) {
    // SAFETY: `kill` only sends a signal; a pid that is no longer the daemon's fails with `ESRCH`.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn terminate(_pid: u32) {}

pub fn daemon_stopped(app: &AppHandle) -> bool {
    app.state::<DaemonState>().stopped.load(Ordering::SeqCst)
}

/// Records that the application is going away or restarting, so that no daemon is started from now
/// on.
pub fn mark_daemon_stopping(app: &AppHandle) {
    app.state::<DaemonState>()
        .stopping
        .store(true, Ordering::SeqCst);
}

/// Records that there is no daemon, and `detail` as what a window opened from now on is told.
pub fn mark_daemon_down(app: &AppHandle, detail: String) {
    let state = app.state::<DaemonState>();
    // Both under the status lock, as a start attempt sets them, so the two never disagree.
    let mut status = state.status.lock().unwrap_or_else(|e| e.into_inner());
    state.stopped.store(true, Ordering::SeqCst);
    *status = DaemonStatus::Down(detail);
}

/// The query parameters a window opened now gets: where the daemon is, or why there is none.
pub fn window_params(app: &AppHandle) -> String {
    let status = app
        .state::<DaemonState>()
        .status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    match status {
        DaemonStatus::Up(address) => daemon_params(&address),
        DaemonStatus::Down(detail) => error_param(&detail),
        DaemonStatus::Starting => error_param("the daemon has not started"),
    }
}

/// Where the daemon is now, when it is up.
pub fn current_address(app: &AppHandle) -> Option<DaemonAddress> {
    match &*app
        .state::<DaemonState>()
        .status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
    {
        DaemonStatus::Up(address) => Some(address.clone()),
        _ => None,
    }
}

fn error_param(detail: &str) -> String {
    format!("error={}", crate::percent_encode_query_value(detail))
}

/// Where the daemon is, as far as a window that does not exist yet needs to know.
#[derive(Clone, Default)]
enum DaemonStatus {
    #[default]
    Starting,
    Up(DaemonAddress),
    /// There is none, with the words for why.
    Down(String),
}

/// Starts the daemon for the first time, trying up to `STARTUP_ATTEMPTS` times with a short pause
/// between them. The error says what the last attempt did and how many were made; it is what the
/// window shows. Blocks the calling thread for as long as that takes: call it off the main thread.
pub fn start_daemon(app: &AppHandle) -> Result<DaemonAddress, String> {
    let mut failure = None;
    for attempt in 1..=STARTUP_ATTEMPTS {
        diagnostic!("starting the daemon (attempt {attempt} of {STARTUP_ATTEMPTS})");
        match start_attempt(app) {
            Ok(address) => {
                diagnostic!("the daemon listens on port {}", address.port);
                return Ok(address);
            }
            Err(err) => {
                diagnostic!("start attempt {attempt} failed: {err}");
                let quitting = matches!(err, AttemptFailure::Quitting);
                failure = Some(err);
                if quitting {
                    break;
                }
                if attempt < STARTUP_ATTEMPTS {
                    std::thread::sleep(startup_backoff(attempt));
                }
            }
        }
    }
    let failure = failure.expect("at least one attempt was made");
    let message = startup_failure_message(STARTUP_ATTEMPTS, &failure);
    mark_daemon_down(app, message.clone());
    Err(message)
}

/// How long the initial start waits after the given number of failed attempts: 1 s, then 2 s.
fn startup_backoff(failed_attempts: u32) -> Duration {
    Duration::from_secs(u64::from(failed_attempts))
}

/// What the window shows when no attempt got the daemon started.
fn startup_failure_message(attempts: u32, last: &AttemptFailure) -> String {
    format!("the daemon did not start in {attempts} attempts. The last one: {last}")
}

/// Why one start attempt did not end with an address.
pub(crate) enum AttemptFailure {
    Spawn(String),
    /// The daemon ended before it printed its startup line, with the end of its stderr.
    Exited {
        code: Option<i32>,
        stderr: String,
    },
    Timeout,
    OutputEnded,
    /// A restart or a quit began while the attempt was waiting.
    Quitting,
}

impl fmt::Display for AttemptFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(err) => write!(f, "octoboardd could not be started: {err}"),
            Self::Exited { code, stderr } => write!(
                f,
                "the daemon stopped with {} before reporting its port and key. {stderr}",
                describe_termination(*code)
            ),
            Self::Timeout => write!(
                f,
                "the daemon did not report its port and key within {} seconds and was stopped",
                SIDECAR_STARTUP_TIMEOUT.as_secs()
            ),
            Self::OutputEnded => write!(
                f,
                "the daemon's output ended before it reported its port and key"
            ),
            Self::Quitting => write!(f, "the application is quitting"),
        }
    }
}

/// Spawns the `octoboardd` sidecar with `--parent-pid` set to this process, so the daemon's own
/// watchdog exits it when this app does — a backstop for a crash, not the normal quit path (that
/// is the explicit `shutdown` request the exit flow sends before confirming the quit). Blocks (via
/// a channel, off the async runtime) until the daemon's startup line appears on stdout, it ends,
/// or the timeout elapses; the spawned task itself keeps running for as long as the daemon does,
/// so one that dies later is noticed too (see `watch_daemon`). A daemon that did not report its
/// port in time, or that reports it while a restart or a quit is under way, is killed and waited
/// for, so the next attempt does not find its instance lock held. The daemon counts as running, and
/// is what a window opened from then on is given, only once this returns its address.
pub(crate) fn start_attempt(app: &AppHandle) -> Result<DaemonAddress, AttemptFailure> {
    let state = app.state::<DaemonState>();
    let pid = std::process::id();
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let (receiver, child) = app
        .shell()
        .sidecar("octoboardd")
        .map_err(|err| AttemptFailure::Spawn(err.to_string()))?
        .args(["--parent-pid", &pid.to_string()])
        .spawn()
        .map_err(|err| AttemptFailure::Spawn(err.to_string()))?;

    *state.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);

    let (tx, rx) = mpsc::channel();
    let exited = Arc::new(AtomicBool::new(false));
    tauri::async_runtime::spawn(watch_daemon(
        app.clone(),
        generation,
        receiver,
        tx,
        exited.clone(),
    ));

    let deadline = Instant::now() + SIDECAR_STARTUP_TIMEOUT;
    let failure = loop {
        // In short slices, so a restart or a quit that begins meanwhile is not kept waiting.
        let slice = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(100));
        match rx.recv_timeout(slice) {
            Ok(Ok(address)) => {
                if state.stopping.load(Ordering::SeqCst) {
                    break AttemptFailure::Quitting;
                }
                // Under the status lock, which marking the daemon down takes too: a daemon that
                // has already ended after its port line has started its recovery, whose down
                // state must stand.
                let mut status = state.status.lock().unwrap_or_else(|e| e.into_inner());
                if !exited.load(Ordering::SeqCst) {
                    state.stopped.store(false, Ordering::SeqCst);
                    *status = DaemonStatus::Up(address.clone());
                }
                return Ok(address);
            }
            Ok(Err(failure)) => return Err(failure),
            Err(mpsc::RecvTimeoutError::Disconnected) => break AttemptFailure::OutputEnded,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if state.stopping.load(Ordering::SeqCst) {
                    break AttemptFailure::Quitting;
                }
                if Instant::now() >= deadline {
                    break AttemptFailure::Timeout;
                }
            }
        }
    };
    retire_and_kill(app);
    wait_for(ATTEMPT_KILL_WAIT, || exited.load(Ordering::SeqCst));
    Err(failure)
}

/// Kills the daemon last spawned, and retires it: nothing its events say counts any more, so its
/// end starts no recovery.
fn retire_and_kill(app: &AppHandle) {
    let state = app.state::<DaemonState>();
    state.generation.fetch_add(1, Ordering::SeqCst);
    let child = state.child.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(child) = child {
        let _ = child.kill();
    }
}

/// Reads one daemon's output for as long as it lives, rather than returning the moment the port
/// line is found: the task must see `Terminated` to recover a daemon that dies later, and the
/// daemon keeps producing output that is forwarded for as long as it runs.
async fn watch_daemon(
    app: AppHandle,
    generation: u64,
    mut receiver: tauri::async_runtime::Receiver<CommandEvent>,
    tx: mpsc::Sender<Result<DaemonAddress, AttemptFailure>>,
    exited: Arc<AtomicBool>,
) {
    let mut stderr = StderrTail::new(STDERR_TAIL_BYTES);
    let mut port_sent = false;
    while let Some(event) = receiver.recv().await {
        match event {
            CommandEvent::Stdout(line) => {
                if !port_sent {
                    if let Some(address) = parse_startup_line(&String::from_utf8_lossy(&line)) {
                        port_sent = true;
                        // Only what the daemon says before its port is kept, for a start that fails.
                        stderr = StderrTail::new(0);
                        let _ = tx.send(Ok(address));
                    }
                }
            }
            CommandEvent::Stderr(line) => {
                let text = String::from_utf8_lossy(&line);
                if !port_sent {
                    stderr.push_line(&text);
                }
                // Forwarded to this process' own stderr, which `pnpm tauri dev` shows; a packaged
                // application launched from the Finder has none, and the daemon's own log file
                // (`daemon.log`) is where its output is kept.
                eprintln!("octoboardd: {}", text.trim_end());
            }
            CommandEvent::Terminated(payload) => {
                exited.store(true, Ordering::SeqCst);
                if port_sent {
                    daemon_ended(&app, generation, payload.code);
                } else {
                    let _ = tx.send(Err(AttemptFailure::Exited {
                        code: payload.code,
                        stderr: stderr.as_str().to_string(),
                    }));
                }
                return; // Terminated is final; no more events will follow.
            }
            _ => {}
        }
    }
    exited.store(true, Ordering::SeqCst);
}

/// What the shell does about a daemon that was up and ended: recover it, or leave it and tell the
/// window (`recovery::exit_action` decides). Only the latest daemon counts.
fn daemon_ended(app: &AppHandle, generation: u64, code: Option<i32>) {
    let state = app.state::<DaemonState>();
    let ended = describe_termination(code);
    if state.generation.load(Ordering::SeqCst) != generation {
        diagnostic!("an earlier daemon ended with {ended}; ignored");
        return;
    }
    match recovery::exit_action(state.stopping.load(Ordering::SeqCst), code) {
        ExitAction::Recover => {
            diagnostic!(
                "the daemon ended unexpectedly with {ended}; its own log, daemon.log, says why"
            );
            recovery::recover(app, format!("The daemon exited unexpectedly with {ended}"));
        }
        ExitAction::Stopped => {
            diagnostic!("the daemon ended with {ended}");
            mark_daemon_down(app, ended.clone());
            // The daemon was up and serving, then ended (a requested shutdown, or a restart or quit
            // in progress) — tell the window; the UI ignores it while it is quitting itself.
            let _ = app.emit("daemon-exited", ended);
        }
    }
}

/// The end of a daemon's stderr, at most a fixed number of bytes: appending past the limit drops
/// the oldest text, cutting at a character boundary.
struct StderrTail {
    text: String,
    limit: usize,
}

impl StderrTail {
    fn new(limit: usize) -> Self {
        Self {
            text: String::new(),
            limit,
        }
    }

    fn push_line(&mut self, line: &str) {
        self.text.push_str(line.trim_end());
        self.text.push('\n');
        if self.text.len() > self.limit {
            let mut cut = self.text.len() - self.limit;
            while !self.text.is_char_boundary(cut) {
                cut += 1;
            }
            self.text.drain(..cut);
        }
    }

    fn as_str(&self) -> &str {
        &self.text
    }
}

/// Where the daemon listens and the key it requires of every connection, as its startup line says.
#[derive(Clone, PartialEq)]
pub struct DaemonAddress {
    pub port: u16,
    pub key: String,
}

/// The key stays out of anything that formats an address, so that a log line or a panic message
/// that carries one cannot leak it.
impl fmt::Debug for DaemonAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DaemonAddress")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

/// The parameters of the window's query that say where the daemon is and how to talk to it, which
/// `recovery::rebuild_query` replaces when it points the window at a restarted one. The key is
/// lower-case hex (see `parse_startup_line`), so it needs no percent-encoding.
pub fn daemon_params(address: &DaemonAddress) -> String {
    format!("port={}&key={}", address.port, address.key)
}

/// The names of the parameters `daemon_params` writes, and of `error`, which a window opened while
/// there was no daemon got instead of them.
pub const DAEMON_PARAMS: [&str; 3] = ["port", "key", "error"];

const STARTUP_LINE_PREFIX: &str = "octoboardd listening on 127.0.0.1:";

/// Parses `octoboardd listening on 127.0.0.1:<port> with key <key>`, exactly as the daemon prints
/// it; only that one line shape is accepted. The key has to be lower-case hex, which is what the
/// daemon makes, so that it can go into a URL as it is.
fn parse_startup_line(line: &str) -> Option<DaemonAddress> {
    let (port, key) = line
        .trim()
        .strip_prefix(STARTUP_LINE_PREFIX)?
        .split_once(" with key ")?;
    let valid_key = !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !valid_key {
        return None;
    }
    Some(DaemonAddress {
        port: port.parse().ok()?,
        key: key.to_string(),
    })
}

/// How a process ended, in words. These strings reach the user, so the exit code is spelled out
/// rather than debug-printed as the `Option` it arrives in.
fn describe_termination(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("exit code {code}"),
        None => "a signal".to_string(),
    }
}

#[cfg(test)]
mod wait_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn a_condition_that_holds_ends_the_wait_at_once() {
        assert!(wait_for(Duration::from_secs(5), || true));
    }

    #[test]
    fn a_condition_that_comes_true_is_waited_for() {
        let polls = AtomicUsize::new(0);
        assert!(wait_for(Duration::from_secs(5), || polls
            .fetch_add(1, Ordering::SeqCst)
            >= 3));
    }

    #[test]
    fn a_daemon_that_exits_by_itself_is_not_signalled() {
        let signalled = AtomicUsize::new(0);
        escalate_stop(
            || true,
            || {
                signalled.fetch_add(1, Ordering::SeqCst);
            },
            || {
                signalled.fetch_add(1, Ordering::SeqCst);
            },
            [Duration::from_millis(50); 3],
        );
        assert_eq!(signalled.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_daemon_that_exits_on_sigterm_is_not_killed() {
        let terminated = AtomicBool::new(false);
        let killed = AtomicBool::new(false);
        escalate_stop(
            || terminated.load(Ordering::SeqCst),
            || terminated.store(true, Ordering::SeqCst),
            || killed.store(true, Ordering::SeqCst),
            [Duration::from_millis(50); 3],
        );
        assert!(terminated.load(Ordering::SeqCst));
        assert!(!killed.load(Ordering::SeqCst));
    }

    #[test]
    fn a_daemon_that_ignores_sigterm_is_killed_after_it() {
        let order = Mutex::new(Vec::new());
        escalate_stop(
            || false,
            || order.lock().unwrap().push("terminate"),
            || order.lock().unwrap().push("kill"),
            [Duration::from_millis(20); 3],
        );
        assert_eq!(*order.lock().unwrap(), ["terminate", "kill"]);
    }

    #[test]
    fn a_condition_that_never_holds_gives_up_at_the_timeout() {
        let started = Instant::now();
        assert!(!wait_for(Duration::from_millis(100), || false));
        assert!(started.elapsed() >= Duration::from_millis(100));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[test]
    fn the_stderr_tail_keeps_only_the_end_within_its_limit() {
        let mut tail = StderrTail::new(10);
        tail.push_line("first");
        tail.push_line("second");
        // 13 bytes were written; the oldest 3 are gone.
        assert_eq!(tail.as_str(), "st\nsecond\n");
        assert_eq!(tail.as_str().len(), 10);
    }

    #[test]
    fn the_stderr_tail_is_cut_at_a_character_boundary() {
        let mut tail = StderrTail::new(5);
        // Four 3-byte characters and a newline: the cut at byte 8 falls inside the third.
        tail.push_line("あいうえ");
        assert_eq!(tail.as_str(), "え\n");
    }

    #[test]
    fn the_failure_message_says_what_the_last_attempt_did_and_how_many_were_made() {
        let message = startup_failure_message(
            3,
            &AttemptFailure::Exited {
                code: Some(1),
                stderr: "lock held\n".into(),
            },
        );
        assert_eq!(
            message,
            "the daemon did not start in 3 attempts. The last one: the daemon stopped with exit code 1 before reporting its port and key. lock held\n"
        );
        assert!(startup_failure_message(3, &AttemptFailure::Timeout)
            .ends_with("did not report its port and key within 10 seconds and was stopped"));
    }

    #[test]
    fn the_initial_start_waits_longer_after_each_failure() {
        assert_eq!([1, 2].map(|n| startup_backoff(n).as_secs()), [1, 2]);
    }

    #[test]
    fn only_the_startup_line_is_an_address() {
        assert_eq!(
            parse_startup_line("octoboardd listening on 127.0.0.1:4242 with key 0a1b2c\n"),
            Some(DaemonAddress {
                port: 4242,
                key: "0a1b2c".to_string()
            })
        );
        for line in [
            "something else on 127.0.0.1:4242 with key 0a1b2c",
            "octoboardd listening on 127.0.0.1:4242",
            "octoboardd listening on 127.0.0.1:4242 with key ",
            "octoboardd listening on 127.0.0.1:4242 with key 0A1B",
            "octoboardd listening on 127.0.0.1:4242 with key ab&port=1",
            "octoboardd listening on 127.0.0.1:x with key ab",
            "octoboardd listening on 127.0.0.1:99999 with key ab",
        ] {
            assert_eq!(parse_startup_line(line), None, "{line:?}");
        }
    }

    #[test]
    fn the_window_params_carry_the_port_and_the_key_and_a_debug_print_leaves_the_key_out() {
        let address = DaemonAddress {
            port: 4242,
            key: "0a1b2c".to_string(),
        };
        assert_eq!(daemon_params(&address), "port=4242&key=0a1b2c");
        assert!(!format!("{address:?}").contains("0a1b2c"));
    }
}
