//! The desktop application's Rust-side shell: launch `octoboardd` as a sidecar and hand the window
//! the port it printed (`sidecar.rs`), the native menu bar (`menu.rs`), the window's remembered
//! size and position (`window_state.rs`), and the exit-confirmation flow including its one `unsafe`
//! subsystem (`exit.rs`). Everything after the window loads talks to the daemon over WebSocket
//! only, per the architectural rule in "Why the daemon is split out" in `docs/architecture.md` — no
//! Tauri IPC command carries daemon traffic or session state, so this process reads the sidecar's
//! stdout itself and bakes the port into the window's URL as a `?port=` query parameter *before*
//! creating the window, instead of exposing an `invoke`-able command for it.
//!
//! The two IPC commands this crate does expose, `frontend_exit_heartbeat` and `confirm_quit` (both
//! in `exit.rs`), carry no daemon traffic or session data either — they are bare exit-flow signals,
//! not a channel for anything the daemon knows about.

mod exit;
mod menu;
mod sidecar;
mod window_state;

use std::thread;
use std::time::Duration;

use tauri::{Emitter, RunEvent, WebviewUrl, WebviewWindowBuilder};
#[cfg(target_os = "macos")]
use tauri::{LogicalPosition, TitleBarStyle};

use exit::{
    confirm_quit, frontend_exit_heartbeat, install_application_should_terminate_override,
    should_let_quit_through, ExitState,
};
use menu::{build_menu, SETTINGS_ITEM_ID, SETTINGS_REQUESTED_EVENT};
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
            if event.id() == SETTINGS_ITEM_ID {
                // Only a signal: the Settings dialog is the UI's to open, and it decides whether
                // now is a good time.
                let _ = app.emit(SETTINGS_REQUESTED_EVENT, ());
            } else if event.id() == "quit" {
                // `should_let_quit_through` itself emits `exit-requested` on the `false` path;
                // nothing left to do here but act on its answer.
                if should_let_quit_through(app) {
                    app.exit(0);
                }
            }
        })
        .setup(|app| {
            app.set_menu(build_menu(app.handle())?)?;
            // This closure runs once the event loop (and with it, tao's `NSApplicationDelegate`)
            // is up and running — `App::run` calls it on the runtime's `Ready` event — so the
            // delegate this installs onto already exists by now. See the function's own doc
            // comment for why the Dock icon's own Quit needs this at all.
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

    app.run(|app_handle, event| match event {
        RunEvent::ExitRequested { api, .. } => {
            if should_let_quit_through(app_handle) {
                return;
            }
            // `should_let_quit_through` has already emitted `exit-requested`; `prevent_exit` is
            // this gesture's own mechanism-specific line.
            api.prevent_exit();
        }
        // Every quit path ends here: Cmd+Q and the menu's Quit through `app.exit`, the Dock's Quit
        // and a logout through AppKit's `applicationWillTerminate:`.
        RunEvent::Exit => window_state::save(app_handle),
        _ => {}
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

/// Upper bound on how long the window stays hidden (see `.visible(false)` below) before this
/// safety net shows it regardless of what the frontend is doing. The UI reveals the window itself
/// well inside this, once it has pushed the native theme (`packages/ui/src/main.tsx`, measured at
/// 140-220 ms after the window is created); this exists only for the paths that never reach that
/// point at all — the webview failing to load the bundle at all, say — where a window left hidden
/// forever is strictly worse than one that briefly shows through in the OS's own appearance. Four
/// seconds is generous next to that measured reveal without being so long that a genuinely stuck
/// launch reads as a hung application rather than a slow one.
const REVEAL_SAFETY_NET: Duration = Duration::from_secs(4);

/// The traffic lights' inset from the window's top-left, in points, as tao applies it to the close
/// button. `Y` was tuned by eye so the buttons sit centred in the UI's 40 pt top bar.
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHT_X: f64 = 16.0;
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHT_Y: f64 = 21.0;

fn open_main_window(app: &tauri::AppHandle, startup: Result<u16, String>) -> tauri::Result<()> {
    let query = match startup {
        Ok(port) => format!("port={port}"),
        Err(message) => format!("error={}", percent_encode_query_value(&message)),
    };
    let url = if cfg!(debug_assertions) {
        format!("http://localhost:5174/?{query}")
    } else {
        format!("index.html?{query}")
    };
    // Decided before the window exists, from the saved frame and the displays connected now, so the
    // window is placed while it is still hidden and is never seen moving into place.
    let (displays, main_display) = window_state::displays(app);
    let initial =
        window_state::initial_window(window_state::load(app).as_ref(), &displays, main_display);

    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title("Octoboard")
        .inner_size(initial.size.0, initial.size.1)
        // 280 (sidebar) + 520 (the terminal pane's own floor) + 300 (the report panel's own
        // floor) = 1100: at this minimum, both panes already sit on their floors with nothing
        // left to give up, so neither can be squeezed past usability by a narrower window. Height
        // 600 gives the terminal about 30 rows, which is comfortably usable.
        .min_inner_size(window_state::MIN_SIZE.0, window_state::MIN_SIZE.1)
        // Created hidden so AppKit never paints the window in the OS's own appearance before the
        // webview has painted anything themed — with no window on screen yet, there is nothing
        // for it to paint prematurely. `packages/ui/src/main.tsx` shows it once that has
        // happened, through the `core:window:allow-show` permission this needs; `REVEAL_SAFETY_NET`
        // below is the backstop for every path that does not reach that call.
        .visible(false);
    let builder = match initial.position {
        Some((x, y)) => builder.position(x, y),
        None => builder,
    };

    // The UI draws its own top bar across the whole window (`TitleBar` in `packages/ui`), so the
    // native titlebar's background and text go and the traffic lights float over the page.
    // `TRAFFIC_LIGHT_X` / `TRAFFIC_LIGHT_Y` centre them in the bar's `--title-bar-height`, and
    // `TRAFFIC_LIGHT_INSET` in `packages/ui/src/platform/tauri.ts` is the width the bar keeps clear
    // for them; the three move together. The window keeps its title for the Dock and Mission
    // Control.
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(LogicalPosition::new(TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y));

    let window = builder.build()?;
    // Not the builder's `.maximized`. AppKit first puts a new window's frame on the main display,
    // and the builder's position is applied afterwards, queued on the main thread; tao queues a
    // builder-requested zoom while creating the window, ahead of that move, so the zoom would
    // happen on the main display and record that frame as the one to un-zoom to. Asked for here,
    // the zoom is queued behind the move and happens on the display the window belongs on.
    if initial.maximized {
        if let Err(err) = window.maximize() {
            eprintln!("octoboard: cannot maximize the window: {err}");
        }
    }
    window_state::track(&window, &initial);

    // `WebviewWindow::show` dispatches onto the window's own event loop internally, so calling it
    // from this background thread rather than the main one is safe; a plain `thread::sleep` here
    // needs neither `tauri::async_runtime` nor a direct `tokio` dependency, unlike the sidecar's
    // own async wait in `sidecar.rs`, which already had to be async to read the child's stdout.
    let safety_net_window = window.clone();
    thread::spawn(move || {
        thread::sleep(REVEAL_SAFETY_NET);
        let _ = safety_net_window.show();
    });

    Ok(())
}
