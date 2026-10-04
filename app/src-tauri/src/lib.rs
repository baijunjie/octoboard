//! The desktop application's Rust-side shell: launch `octoboardd` as a sidecar and hand the
//! window the port it printed (`sidecar.rs`), the native menu bar (`menu.rs`), and the
//! exit-confirmation flow including its one `unsafe` subsystem (`exit.rs`). Everything after the
//! window loads talks to the daemon over WebSocket only, per the architectural rule in "Why the
//! daemon is split out in the MVP" in `docs/mvp.md` — no Tauri IPC command carries daemon traffic
//! or session state, so this process reads the sidecar's stdout itself and bakes the port into the
//! window's URL as a `?port=` query parameter *before* creating the window, instead of exposing an
//! `invoke`-able command for it.
//!
//! The two IPC commands this crate does expose, `frontend_exit_heartbeat` and `confirm_quit` (both
//! in `exit.rs`), carry no daemon traffic or session data either — they are bare exit-flow signals,
//! not a channel for anything the daemon knows about.

mod exit;
mod menu;
mod sidecar;

use tauri::{RunEvent, WebviewUrl, WebviewWindowBuilder};

use exit::{
    confirm_quit, frontend_exit_heartbeat, install_application_should_terminate_override,
    should_let_quit_through, ExitState,
};
use menu::build_menu;
use sidecar::spawn_daemon_and_wait_for_port;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // Carries no daemon traffic either: the frontend asks for a notification, the plugin
        // shows it.
        .plugin(tauri_plugin_notification::init())
        .manage(ExitState::default())
        .invoke_handler(tauri::generate_handler![
            frontend_exit_heartbeat,
            confirm_quit
        ])
        .on_menu_event(|app, event| {
            if event.id() == "quit" {
                // `should_let_quit_through` itself emits `exit-requested` on the `false` path;
                // nothing left to do here but act on its answer.
                if should_let_quit_through(app) {
                    app.exit(0);
                }
            }
        })
        .setup(|app| {
            app.set_menu(build_menu(app.handle())?)?;
            // The event loop (and with it, tao's `NSApplicationDelegate`) is built by
            // `Builder::build`, which calls this closure from inside itself — so the delegate this
            // installs onto already exists by now. See the function's own doc comment for why the
            // Dock icon's own Quit needs this at all.
            install_application_should_terminate_override(app.handle());
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
            if should_let_quit_through(app_handle) {
                return;
            }
            // `should_let_quit_through` has already emitted `exit-requested`; `prevent_exit` is
            // this gesture's own mechanism-specific line.
            api.prevent_exit();
        }
    });
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
