//! Spawning the `octoboardd` sidecar, parsing its startup output, and stopping it for a restart.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

/// How long to wait for `octoboardd` to print its port line before giving up. Generous: this is a
/// cold process start (including, on first launch, SQLite schema setup), not a steady-state
/// operation.
const SIDECAR_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// The sidecar's state: whether the daemon is gone, and the handle `stop_daemon` ends it through.
#[derive(Default)]
pub struct DaemonState {
    /// Whether the daemon failed to start or has exited. With no session left to keep running, the
    /// window's close button quits and a quit needs no confirmation.
    stopped: AtomicBool,
    /// The running sidecar, for `stop_daemon` to end when it does not exit by itself.
    child: Mutex<Option<CommandChild>>,
}

/// How long a restart waits for the daemon to exit on its own after the UI asked it to shut down,
/// before it sends SIGTERM.
const DAEMON_EXIT_GRACE: Duration = Duration::from_secs(2);

/// How long a restart waits for the daemon to exit after SIGTERM, which has it stop its sessions'
/// processes first, before it kills it.
const DAEMON_TERM_GRACE: Duration = Duration::from_secs(5);

/// How long a restart waits for the daemon to be gone after killing it.
const DAEMON_KILL_WAIT: Duration = Duration::from_secs(2);

/// Polls `condition` every few milliseconds until it holds or `timeout` has passed; says whether it held.
fn wait_for(timeout: Duration, condition: impl Fn() -> bool) -> bool {
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

pub fn mark_daemon_stopped(app: &AppHandle) {
    app.state::<DaemonState>()
        .stopped
        .store(true, Ordering::SeqCst);
}

/// Spawns the `octoboardd` sidecar with `--parent-pid` set to this process, so the daemon's own
/// watchdog exits it when this app does — a backstop for a crash, not the normal quit path (that
/// is the explicit `shutdown` request the exit flow sends before confirming the quit). Blocks (via
/// a channel, off the async runtime) until the daemon's startup line appears on stdout or the
/// timeout elapses; the spawned task itself keeps running for the life of the process so a daemon
/// that dies later is noticed too (see the loop below).
pub fn spawn_daemon_and_wait_for_port(app: &AppHandle) -> anyhow::Result<u16> {
    let pid = std::process::id();
    let (mut receiver, child) = app
        .shell()
        .sidecar("octoboardd")
        .map_err(|err| anyhow::anyhow!(err))?
        .args(["--parent-pid", &pid.to_string()])
        .spawn()
        .map_err(|err| anyhow::anyhow!(err))?;

    *app.state::<DaemonState>()
        .child
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(child);

    let (tx, rx) = mpsc::channel();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut stderr = String::new();
        let mut port_sent = false;
        // Keeps reading for as long as the sidecar lives, rather than returning the moment the
        // port line is found — a sidecar that keeps running for the application's whole lifetime
        // also keeps producing stdout/stderr for that whole lifetime, and an early return here
        // would silently discard every line after the first.
        while let Some(event) = receiver.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    if !port_sent {
                        if let Some(port) = parse_port(&String::from_utf8_lossy(&line)) {
                            port_sent = true;
                            let _ = tx.send(Ok(port));
                        }
                    }
                }
                CommandEvent::Stderr(line) => {
                    let text = String::from_utf8_lossy(&line);
                    stderr.push_str(&text);
                    // Forwarded to this process' own stderr (captured by Console.app on a packaged
                    // build) rather than discarded — there is no other log sink here.
                    eprintln!("octoboardd: {}", text.trim_end());
                }
                CommandEvent::Terminated(payload) => {
                    mark_daemon_stopped(&app_handle);
                    if !port_sent {
                        let _ = tx.send(Err(format!(
                            "the daemon stopped with {} before reporting its port. {stderr}",
                            describe_termination(payload.code)
                        )));
                    } else {
                        // The daemon was up and serving, then died later (crash, or another
                        // instance's lock kicking in) — tell the window so the user is not left
                        // staring at a UI that silently stopped working.
                        let _ =
                            app_handle.emit("daemon-exited", describe_termination(payload.code));
                    }
                    return; // Terminated is final; no more events will follow.
                }
                _ => {}
            }
        }
    });

    match rx.recv_timeout(SIDECAR_STARTUP_TIMEOUT) {
        Ok(Ok(port)) => Ok(port),
        Ok(Err(err)) => Err(anyhow::anyhow!(err)),
        Err(err) => Err(anyhow::anyhow!("octoboardd never printed its port: {err}")),
    }
}

/// Parses `octoboardd listening on 127.0.0.1:<port>`, exactly as the daemon prints it — only that
/// one line shape is accepted, unlike a bare "whatever comes after the last colon" parse, which
/// would also match a colon-digit sequence that coincidentally ends some unrelated sidecar log
/// line.
const PORT_LINE_PREFIX: &str = "octoboardd listening on 127.0.0.1:";

fn parse_port(line: &str) -> Option<u16> {
    line.trim().strip_prefix(PORT_LINE_PREFIX)?.parse().ok()
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
