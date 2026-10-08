//! Spawning the `octoboardd` sidecar and parsing its startup output.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

/// How long to wait for `octoboardd` to print its port line before giving up. Generous: this is a
/// cold process start (including, on first launch, SQLite schema setup), not a steady-state
/// operation.
const SIDECAR_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether the daemon is gone: it failed to start or has exited. With no session left to keep
/// running, the window's close button quits and a quit needs no confirmation.
#[derive(Default)]
pub struct DaemonState {
    stopped: AtomicBool,
}

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
    let (mut receiver, _child) = app
        .shell()
        .sidecar("octoboardd")
        .map_err(|err| anyhow::anyhow!(err))?
        .args(["--parent-pid", &pid.to_string()])
        .spawn()
        .map_err(|err| anyhow::anyhow!(err))?;

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
