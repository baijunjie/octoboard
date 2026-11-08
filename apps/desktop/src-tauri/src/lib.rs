//! The desktop application's Rust-side shell: it launches `octoboardd` as a sidecar, hands the
//! window the daemon's port and key as `?port=` and `?key=` query parameters (and the host's facts as
//! `?host=`, see `host_facts`) before the window is created, brings the daemon back when it dies, and
//! offers the UI the few native capabilities it cannot have itself. No IPC command carries daemon
//! traffic: the UI talks to the daemon over WebSocket (see "Why the daemon is split out" in
//! `docs/architecture.md`).
//!
//! The modules, the IPC surface and where new code goes are in `docs/structure/desktop.md`.

mod background;
mod chrome;
mod configuration_file;
mod diagnostics;
mod directory_picker;
mod exit;
mod host_facts;
mod languages;
mod lifecycle;
mod menu;
mod notification_settings;
mod page_isolation;
mod permissions;
mod recovery;
mod sidecar;
// The panes it opens are those of the macOS privacy grants and notifications.
#[cfg(target_os = "macos")]
mod system_settings;
mod tray;
mod updater;
mod window_state;

use std::thread;
use std::time::Duration;

use tauri::{Emitter, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use background::BackgroundState;
use diagnostics::diagnostic;
use exit::{confirm_quit, frontend_exit_heartbeat, request_quit, ExitState};
use menu::{set_menu_labels, ui_event_for};
use sidecar::{mark_daemon_stopping, start_daemon, window_params, DaemonState};

/// The label of the one window the application opens.
pub(crate) const MAIN_WINDOW_LABEL: &str = "main";

/// The builder the main window is made with, which the platform modules add to.
pub(crate) type MainWindowBuilder<'a> = WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle>;

/// Answers a permission probe when this process was started for one (see `permissions`), and says
/// whether it was, for `main` to exit before anything of the app starts.
pub fn answer_permission_probe() -> bool {
    permissions::answer_probe_if_asked()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = lifecycle::register_first(tauri::Builder::default())
        .plugin(tauri_plugin_shell::init())
        // Carries no daemon traffic either: the frontend asks for a notification, the plugin
        // shows it.
        .plugin(tauri_plugin_notification::init())
        // The frontend writes text to the clipboard through it (the webview's own clipboard API is
        // refused outside a secure context, which the custom scheme may not count as).
        .plugin(tauri_plugin_clipboard_manager::init())
        // The system's file and directory dialogs; the page is given none of the plugin's own commands.
        .plugin(tauri_plugin_dialog::init())
        // Driven from `updater/` through its own commands; the page is given none of the plugin's own.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(ExitState::default())
        .manage(BackgroundState::default())
        .manage(lifecycle::LifecycleState::default())
        .manage(DaemonState::default())
        .manage(tray::TrayState::default())
        .manage(updater::UpdaterState::default())
        .invoke_handler(tauri::generate_handler![
            frontend_exit_heartbeat,
            confirm_quit,
            sidecar::daemon_exit_detail,
            set_menu_labels,
            tray::set_tray_menu,
            background::bring_to_front,
            background::set_run_in_background,
            permissions::permissions_status,
            permissions::request_permission,
            permissions::open_privacy_settings,
            notification_settings::open_notification_settings,
            configuration_file::save_configuration_file,
            configuration_file::open_configuration_file,
            directory_picker::pick_directory,
            updater::check_for_update,
            updater::install_update
        ])
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if let Some(event) = ui_event_for(id) {
                // Only a signal: the Settings dialog is the UI's to open, and it decides whether
                // now is a good time.
                let _ = app.emit(event, ());
            } else if id == "quit" {
                request_quit(app);
            } else {
                tray::handle_menu_event(app, id);
            }
        })
        .on_window_event(|window, event| {
            // Closing the window leaves the app running in the background when the user chose that
            // in Settings, and quits it otherwise (see `handle_close_request`); the other ways to
            // quit are Cmd+Q, the app menu's or the menu bar icon's Quit. The UI registers no
            // close listener of its own, so the close reaches this handler rather than being
            // answered in the webview.
            if let WindowEvent::CloseRequested { api, .. } = event {
                background::handle_close_request(window, api);
            }
        })
        .setup(|app| {
            menu::install(app.handle())?;
            tray::create(app.handle())?;
            lifecycle::install(app.handle());
            // The daemon is started on a thread of its own, with retries that can take most of a
            // minute in all, and the window opens once it has. A daemon that fails to start
            // (another instance already holds its lock, a corrupt database, ...) must not leave the
            // user with no window and no explanation: the window opens either way, carrying
            // whichever of `?port=` with `?key=`, or `?error=`, applies to the daemon as it is when
            // the window is built. Until it opens nothing here assumes it exists: a quit is let
            // through (no webview has registered for the exit flow) and a reopen finds no window to
            // show.
            let handle = app.handle().clone();
            thread::spawn(move || {
                // The outcome is kept in the daemon's state, which the window is built from.
                let _ = start_daemon(&handle);
                let opening = handle.clone();
                let queued = handle.run_on_main_thread(move || open_window_for(&opening));
                if let Err(err) = queued {
                    diagnostic!("cannot open the main window: {err}");
                    handle.exit(1);
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| match event {
        RunEvent::ExitRequested { api, .. } => {
            // A deferral has already emitted `exit-requested`, or a held update's install has
            // started; `prevent_exit` is this gesture's own mechanism-specific line.
            if exit::on_exit_requested(app_handle) {
                api.prevent_exit();
            }
        }
        // Every quit path ends here: Cmd+Q and the menu's Quit through `app.exit`, the Dock's Quit
        // and a logout through AppKit's `applicationWillTerminate:`, the window's close when it
        // quits instead of hiding, and the restart of `confirm_quit`.
        RunEvent::Exit => {
            // A recovery still running must not start a daemon the application would leave behind.
            mark_daemon_stopping(app_handle);
            window_state::save(app_handle);
            // A held update is installed here only for a quit let through; the system ending the
            // session on Windows reaches this with no `ExitRequested` and leaves it for the next
            // start.
            exit::install_on_exit(app_handle);
        }
        _ => lifecycle::on_run_event(app_handle, &event),
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

/// Opens the main window for the daemon as it is when the window is built. On the main thread.
fn open_window_for(app: &tauri::AppHandle) {
    // The window's webview gets its isolation before the window exists where the platform lets it,
    // which `page_isolation` may only be able to report back asynchronously (the content rule list
    // on macOS), so the window is opened from its callback.
    let handle = app.clone();
    page_isolation::prepare(move |configuration| {
        if let Err(err) = open_main_window(&handle, configuration) {
            diagnostic!("cannot open the main window: {err}");
            handle.exit(1);
        }
    });
}

fn open_main_window(
    app: &tauri::AppHandle,
    configuration: page_isolation::Configuration,
) -> tauri::Result<()> {
    let daemon_query = window_params(app);
    let host = percent_encode_query_value(&host_facts::current_json());
    let query = match languages::preferred_languages() {
        Some(languages) => format!(
            "{daemon_query}&host={host}&languages={}",
            percent_encode_query_value(&languages)
        ),
        None => format!("{daemon_query}&host={host}"),
    };
    let url = WebviewUrl::App(format!("index.html?{query}").into());
    // Decided before the window exists, from the saved frame and the displays connected now, so the
    // window is placed while it is still hidden and is never seen moving into place.
    let (displays, main_display) = window_state::displays(app);
    let initial = window_state::initial_window(
        window_state::load(app).as_ref(),
        &displays,
        main_display,
        chrome::system_frame(),
    );

    let builder = WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, url)
        .title(menu::APP_NAME)
        .inner_size(initial.size.0, initial.size.1)
        // 48 (the left rail) + 280 (sidebar) + 520 (the terminal pane's own floor) + 320 (the
        // report panel's own floor) = 1168: at this minimum, both panes already sit on their
        // floors with nothing left to give up, so neither can be squeezed past usability by a
        // narrower window. Height 600 gives the terminal about 30 rows, which is comfortably
        // usable.
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
    let builder = page_isolation::apply(builder, configuration);
    let builder = chrome::decorate(builder);

    let window = builder.build()?;
    page_isolation::install(&window);
    chrome::install(&window);
    // A recovery that finished while the window was being built found no window to point at the
    // restarted daemon, so the window is pointed at it here instead.
    if window_params(app) != daemon_query {
        if let Some(address) = sidecar::current_address(app) {
            recovery::point_window_at(app, &address);
        }
    }
    // Not the builder's `.maximized`. AppKit first puts a new window's frame on the main display,
    // and the builder's position is applied afterwards, queued on the main thread; tao queues a
    // builder-requested zoom while creating the window, ahead of that move, so the zoom would
    // happen on the main display and record that frame as the one to un-zoom to. Asked for here,
    // the zoom is queued behind the move and happens on the display the window belongs on.
    if initial.maximized {
        if let Err(err) = window.maximize() {
            diagnostic!("cannot maximize the window: {err}");
        }
    }
    window_state::track(&window, &initial);

    // `WebviewWindow::show` dispatches onto the window's own event loop internally, so calling it
    // from this background thread rather than the main one is safe, so a plain `thread::sleep`
    // here needs neither `tauri::async_runtime` nor a direct `tokio` dependency.
    let safety_net_window = window.clone();
    let safety_net_app = app.clone();
    thread::spawn(move || {
        thread::sleep(REVEAL_SAFETY_NET);
        // Not for a window the user has closed since it was revealed: it would come back without
        // the Dock icon.
        if !background::hidden_by_close(&safety_net_app) {
            let _ = safety_net_window.show();
        }
    });

    Ok(())
}
