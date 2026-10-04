//! The desktop application's only Rust-side job: launch `octoboardd` as a sidecar and hand the
//! window the port it printed. Everything after the window loads talks to the daemon over
//! WebSocket only, per the architectural rule in "Why the daemon is split out in the MVP" in
//! `docs/mvp.md` — no Tauri IPC command carries daemon traffic or session state, so this process
//! reads the sidecar's stdout itself and bakes the port into the window's URL as a `?port=` query
//! parameter *before* creating the window, instead of exposing an `invoke`-able command for it.
//!
//! The two IPC commands this crate does expose, `frontend_handles_exit` and `confirm_quit`, carry
//! no daemon traffic or session data either — they are bare exit-flow signals (see `ExitState`
//! below), not a channel for anything the daemon knows about.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

/// How long to wait for `octoboardd` to print its port line before giving up. Generous: this is a
/// cold process start (including, on first launch, SQLite schema setup), not a steady-state
/// operation.
const SIDECAR_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Tracks the two facts the exit flow needs across `RunEvent::ExitRequested`, which fires for
/// every way the event loop could end — the window being destroyed, `app.exit()`, Cmd+Q, the app
/// menu — with no way to tell those apart from the event alone:
///
/// - `confirmed`: set by `confirm_quit` right before it calls `app.exit(0)`, which raises its own
///   `ExitRequested`. Without this flag that self-raised event would hit `prevent_exit()` again
///   and the application would never actually terminate — just end up running headlessly with no
///   window.
/// - `frontend_registered`: set once the webview has called `frontend_handles_exit`, meaning a
///   confirmation dialog actually exists to show. Before that point (or if the page never finishes
///   loading at all), `prevent_exit()` would trap the user in a window nothing can close — the
///   daemon-failed-to-start screen hits this if it does not register, so the default is to let the
///   exit through rather than trap first and hope the frontend shows up.
#[derive(Default)]
struct ExitState {
    confirmed: AtomicBool,
    frontend_registered: AtomicBool,
}

/// Marks this webview as the one handling the exit flow. Until a window calls this, a quit is let
/// through unchanged: a screen that cannot ask the user about it must not be able to block it.
#[tauri::command]
fn frontend_handles_exit(state: tauri::State<ExitState>) {
    state.frontend_registered.store(true, Ordering::SeqCst);
}

/// Marks the pending quit as confirmed and asks Tauri to exit. The `RunEvent::ExitRequested`
/// handler below sees the flag set, lets this one through, and resets it for next time.
#[tauri::command]
fn confirm_quit(app: tauri::AppHandle, state: tauri::State<ExitState>) {
    state.confirmed.store(true, Ordering::SeqCst);
    app.exit(0);
}

/// How a process ended, in words. These strings reach the user, so the exit code is spelled out
/// rather than debug-printed as the `Option` it arrives in.
fn describe_termination(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("exit code {code}"),
        None => "a signal".to_string(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // Carries no daemon traffic either: the frontend asks for a notification, the plugin
        // shows it.
        .plugin(tauri_plugin_notification::init())
        .manage(ExitState::default())
        .invoke_handler(tauri::generate_handler![
            frontend_handles_exit,
            confirm_quit
        ])
        .on_menu_event(|app, event| {
            if event.id() == "quit" {
                // Same default as the exit-requested path below: until a window says it handles
                // the exit, quitting must still quit. Otherwise the keystroke does nothing at all
                // while the page is still loading.
                if app
                    .state::<ExitState>()
                    .frontend_registered
                    .load(Ordering::SeqCst)
                {
                    let _ = app.emit("exit-requested", ());
                } else {
                    app.exit(0);
                }
            }
        })
        .setup(|app| {
            app.set_menu(build_menu(app.handle())?)?;
            // A daemon that fails to start (another instance already holds its lock, a corrupt
            // database, ...) must not leave the user with no window and no explanation: the window
            // opens either way, carrying whichever of `?port=`/`?error=` applies.
            let startup =
                spawn_daemon_and_wait_for_port(app.handle()).map_err(|err| err.to_string());
            open_main_window(app.handle(), startup)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::ExitRequested { api, .. } = event {
            let state = app_handle.state::<ExitState>();
            if state.confirmed.swap(false, Ordering::SeqCst) {
                // Already confirmed by `confirm_quit` above — let this one through.
                return;
            }
            if !state.frontend_registered.load(Ordering::SeqCst) {
                // No webview has registered as handling the exit flow (it has not finished loading
                // yet, or never will) — nothing could ever confirm this quit, so let it proceed
                // rather than trap the user in a window they cannot close.
                return;
            }
            api.prevent_exit();
            let _ = app_handle.emit("exit-requested", ());
        }
    });
}

/// Builds the app's menu bar. It is close to the framework's own default macOS menu, with one
/// difference that matters: Quit is this crate's own `MenuItem`, not
/// `PredefinedMenuItem::quit`. The predefined one sends the native `terminate:` selector straight
/// to the app, which ends the process through no `RunEvent` any confirmation flow can intercept; a
/// plain menu item instead reaches `on_menu_event` above, which routes it into the same
/// `exit-requested` path as Cmd+Q and the window's close button.
///
/// TODO(milestone 04): the Dock icon's own Quit item is still unhandled — macOS sends it straight
/// to `applicationShouldTerminate:`, which nothing in `tao`/`tauri`/`tauri-runtime-wry` implements,
/// so it bypasses this menu (and the exit-confirmation flow) entirely. Catching it needs an
/// `NSApplicationDelegate` override; 04 owns packaging and the rest of the exit flow.
fn build_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let quit = MenuItemBuilder::with_id("quit", "Quit Octoboard")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "Octoboard")
        .about(None)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .item(&quit)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View").fullscreen().build()?;

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &edit_menu, &view_menu, &window_menu])
        .build()
}

/// Spawns the `octoboardd` sidecar with `--parent-pid` set to this process, so the daemon's own
/// watchdog exits it when this app does — a backstop for a crash, not the normal quit path (that
/// is the explicit `shutdown` request the exit flow sends before confirming the quit). Blocks (via
/// a channel, off the async runtime) until the daemon's startup line appears on stdout or the
/// timeout elapses; the spawned task itself keeps running for the life of the process so a daemon
/// that dies later is noticed too (see the loop below).
fn spawn_daemon_and_wait_for_port(app: &tauri::AppHandle) -> anyhow::Result<u16> {
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

/// Percent-encodes a query-string *value* (not a full URL): letters, digits and `-_.~` pass
/// through verbatim, everything else — including UTF-8 continuation bytes, which this encodes one
/// byte at a time regardless of the character they belong to — becomes `%XX`. `URLSearchParams` on
/// the receiving side decodes that back to the original string correctly either way.
fn percent_encode_query_value(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn open_main_window(app: &tauri::AppHandle, startup: Result<u16, String>) -> tauri::Result<()> {
    let query = match startup {
        Ok(port) => format!("port={port}"),
        Err(message) => format!("error={}", percent_encode_query_value(&message)),
    };
    let url = if cfg!(debug_assertions) {
        format!("http://localhost:5173/?{query}")
    } else {
        format!("index.html?{query}")
    };
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title("Octoboard")
        .inner_size(1200.0, 760.0)
        // 280 (sidebar) + 520 (the terminal pane's own floor) + 300 (the report panel's own
        // floor) = 1100: at this minimum, both panes already sit on their floors with nothing
        // left to give up, so neither can be squeezed past usability by a narrower window. Height
        // 600 gives the terminal about 30 rows, which is comfortably usable.
        .min_inner_size(1100.0, 600.0)
        .build()?;
    Ok(())
}
