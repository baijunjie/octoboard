//! Throwaway Tauri 2 shell for octoboard milestone 00 (technical validation). Not a real
//! product crate — see prototype/README.md.
//!
//! The one job here: launch `obd-proto` as a sidecar (so it exits with the app via its own
//! `--parent-pid` watchdog, already implemented on the daemon side) and hand the window the
//! port it printed. Everything after the window loads talks to the daemon over WebSocket only,
//! per this prototype's one architectural rule (see prototype/README.md): this process reads
//! the sidecar's stdout itself and bakes the port into the window's URL as a `?port=` query
//! parameter *before* creating the window, so there is no Tauri `invoke` call at all.

use std::sync::mpsc;
use std::time::Duration;

use tauri::{WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

/// How long to wait for `obd-proto` to print its port line before giving up. Generous: this is
/// a cold process start, not a steady-state operation.
const SIDECAR_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let port = spawn_daemon_and_wait_for_port(app.handle())?;
            open_main_window(app.handle(), port)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Spawns the `obd-proto` sidecar with `--parent-pid` set to this process, so the daemon's own
/// watchdog (already implemented, see prototype/daemon/src/watchdog.rs) exits it when this app
/// does — no explicit kill-on-exit needed on this side. Blocks (via a channel, off the async
/// runtime) until the daemon's startup line appears on stdout or the timeout elapses.
fn spawn_daemon_and_wait_for_port(app: &tauri::AppHandle) -> tauri::Result<u16> {
    let pid = std::process::id();
    let (mut receiver, _child) = app
        .shell()
        .sidecar("obd-proto")
        .map_err(|err| tauri::Error::Anyhow(anyhow::anyhow!(err)))?
        .args(["--parent-pid", &pid.to_string()])
        .spawn()
        .map_err(|err| tauri::Error::Anyhow(anyhow::anyhow!(err)))?;

    // T4 is specifically about sidecar lifecycle, so a sidecar that fails to start needs to say
    // why, not just that it never printed its port — collect stderr alongside the port search so
    // a startup failure surfaces the real error instead of a bare timeout.
    let (tx, rx) = mpsc::channel();
    tauri::async_runtime::spawn(async move {
        let mut stderr = String::new();
        while let Some(event) = receiver.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    let text = String::from_utf8_lossy(&line);
                    if let Some(port) = parse_port(&text) {
                        let _ = tx.send(Ok(port));
                        return;
                    }
                }
                CommandEvent::Stderr(line) => {
                    stderr.push_str(&String::from_utf8_lossy(&line));
                }
                CommandEvent::Terminated(payload) => {
                    let _ = tx.send(Err(format!(
                        "obd-proto exited ({:?}) before printing its port; stderr: {stderr}",
                        payload.code
                    )));
                    return;
                }
                _ => {}
            }
        }
    });

    match rx.recv_timeout(SIDECAR_STARTUP_TIMEOUT) {
        Ok(Ok(port)) => Ok(port),
        Ok(Err(err)) => Err(tauri::Error::Anyhow(anyhow::anyhow!(err))),
        Err(err) => Err(tauri::Error::Anyhow(anyhow::anyhow!(
            "obd-proto never printed its port: {err}"
        ))),
    }
}

/// Parses `obd-proto listening on 127.0.0.1:<port>`, exactly as printed by main.rs — only that
/// one line shape is accepted, unlike a bare "whatever comes after the last colon" parse, which
/// would also match a colon-digit sequence that coincidentally ends some unrelated sidecar line.
/// common.mjs's own port search (a regex anchored on the same prefix) makes the same choice.
const PORT_LINE_PREFIX: &str = "obd-proto listening on 127.0.0.1:";

fn parse_port(line: &str) -> Option<u16> {
    line.trim().strip_prefix(PORT_LINE_PREFIX)?.parse().ok()
}

fn open_main_window(app: &tauri::AppHandle, port: u16) -> tauri::Result<()> {
    let url = if cfg!(debug_assertions) {
        format!("http://localhost:5173/?port={port}")
    } else {
        format!("index.html?port={port}")
    };
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title("octoboard prototype")
        .inner_size(1100.0, 700.0)
        .build()?;
    Ok(())
}
