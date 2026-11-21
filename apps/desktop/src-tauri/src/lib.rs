//! The desktop application's Rust-side shell: it launches `octoboardd` as a sidecar, hands the
//! window the daemon's port and key as `?port=` and `?key=` query parameters (and the host's facts
//! as `?host=`, see `host_facts`, and the settings the shell keeps as `?settings=`, see
//! `app_settings`) before the window is created, brings the daemon back when it dies, and offers
//! the UI the few native capabilities it cannot have itself. No IPC command carries daemon traffic:
//! the UI talks to the daemon over WebSocket (see "Why the daemon is split out" in
//! `docs/architecture.md`).
//!
//! The modules, the IPC surface and where new code goes are in `docs/structure/desktop.md`.

// A diagnostic is written with `diagnostic!`, so that it reaches `desktop.log` as well as stderr
// (`docs/conventions/desktop-shell.md`, "Write a diagnostic with `diagnostic!`"). A lint here rather than
// `disallowed_macros`: the root `clippy.toml` is shared by both crates, and the daemon prints to stderr legitimately.
#![deny(clippy::print_stderr)]

mod app_settings;
#[cfg(target_os = "macos")]
mod apple_event;
mod background;
mod badge;
mod chrome;
mod command_line;
mod config_dir;
mod configuration_file;
mod daemon_window;
mod diagnostics;
mod directory_picker;
mod exit;
mod host_facts;
mod languages;
mod launch_at_login;
mod lifecycle;
mod menu;
mod notification_settings;
mod notifications;
mod open_address;
mod page_isolation;
mod permissions;
mod recovery;
mod remote;
mod remote_login;
mod sidecar;
// The addresses of the System Settings panes the shell opens: the privacy grants, notifications and
// login items.
#[cfg(target_os = "macos")]
mod system_settings;
mod tray;
mod updater;
mod window_factory;
mod window_state;

use std::thread;

use tauri::{Emitter, Manager, RunEvent, WebviewWindowBuilder, WindowEvent};

use background::BackgroundState;
use diagnostics::diagnostic;
use exit::{begin_quit, confirm_quit, frontend_exit_heartbeat, request_quit, ExitState};
use menu::{set_menu_labels, ui_event_for};
use sidecar::{mark_daemon_stopping, start_daemon, DaemonState};

/// The label of the main window, the local daemon's; the windows of other daemons are
/// `daemon_window`'s.
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
        // Carries no daemon traffic either: the frontend asks for a notification, which the
        // plugin shows off macOS (on macOS the shell shows it itself, `notifications/`), and the
        // plugin answers the frontend's permission query everywhere.
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
        .manage(badge::BadgeState::default())
        .manage(updater::UpdaterState::default())
        .manage(daemon_window::DaemonWindows::default())
        .manage(window_state::TrackedWindows::default())
        .manage(remote::RemoteState::default())
        .manage(remote::RestoreState::default())
        .invoke_handler(tauri::generate_handler![
            frontend_exit_heartbeat,
            begin_quit,
            confirm_quit,
            sidecar::daemon_exit_detail,
            set_menu_labels,
            tray::set_tray_menu,
            tray::set_keep_awake_shown,
            badge::set_badge_count,
            notifications::show_notification,
            launch_at_login::launch_at_login_status,
            launch_at_login::set_launch_at_login,
            launch_at_login::open_login_items_settings,
            command_line::command_line_status,
            command_line::command_line_install,
            command_line::command_line_remove,
            background::reveal_main_window,
            background::bring_to_front,
            app_settings::set_app_settings,
            permissions::permissions_status,
            permissions::request_permission,
            permissions::open_privacy_settings,
            remote_login::remote_login_status,
            remote_login::open_remote_login_settings,
            notification_settings::open_notification_settings,
            configuration_file::save_configuration_file,
            configuration_file::open_configuration_file,
            directory_picker::pick_directory,
            updater::check_for_update,
            updater::install_update,
            remote::remote_daemons_list,
            remote::remote_daemon_save,
            remote::remote_daemon_remove,
            remote::remote_daemon_connect,
            remote::remote_daemons_status,
            remote::remote_daemon_disconnect,
            remote::page::remote_connect_pending_take,
            remote::page::remote_page_requested_take,
            remote::remote_connection_retry,
            remote::remote_link_address,
            remote::remote_forwards_list,
            remote::remote_forward_stop,
            remote::files::remote_file_begin,
            remote::files::remote_file_write,
            remote::files::remote_file_open,
            remote::files::remote_file_discard,
            remote::main_window_show,
            tray::flyout::tray_flyout_model,
            tray::flyout::tray_flyout_size,
            tray::flyout::tray_flyout_choose,
            tray::flyout::tray_flyout_close
        ])
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if let Some(event) = ui_event_for(id) {
                // Only a signal: the Settings dialog is the UI's to open, and it decides whether
                // now is a good time. To the focused window alone, which is the one whose menu the
                // user sees.
                if let Some(window) = daemon_window::menu_target(app) {
                    // With another window in front, the window the item goes to is brought forward.
                    if !window.is_focused().unwrap_or(false) {
                        background::show_window(app, &window);
                    }
                    let _ = window.emit_to(window.label(), event, ());
                }
            } else if id == "quit" {
                request_quit(app);
            } else if !remote::handle_menu_item(app, id) {
                tray::handle_menu_event(app, id);
            }
        })
        .on_window_event(|window, event| {
            // Closing the window leaves the app running in the background when the user chose that
            // in Settings, and quits it otherwise (see `handle_close_request`); the other ways to
            // quit are Cmd+Q, the app menu's or the menu bar icon's Quit. The UI registers no close
            // listener of its own, so the close reaches this handler rather than being answered in
            // the webview. The notification-area icon's flyout (Windows) is no daemon's window: it
            // only hides, and it hides as it loses the focus.
            if window.label() == tray::flyout::FLYOUT_LABEL {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        tray::flyout::hide(window.app_handle());
                    }
                    WindowEvent::Focused(false) => tray::flyout::focus_lost(window.app_handle()),
                    _ => {}
                }
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    background::handle_close_request(window, api);
                }
                WindowEvent::Focused(true) => {
                    daemon_window::note_focused(window.app_handle(), window.label());
                    // The menu bar icon's quota and eyes follow the window focused last.
                    tray::focus_changed(window.app_handle());
                }
                // Only a remote window is ever destroyed while the application runs on: the main
                // one is hidden instead. A remote window's connection ends with it.
                WindowEvent::Destroyed => {
                    daemon_window::forget(window.app_handle(), window.label());
                    tray::window_closed(window.app_handle(), window.label());
                    badge::forget(window.app_handle(), window.label());
                    remote::window_closed(window.app_handle(), window.label());
                }
                _ => {}
            }
        })
        .setup(|app| {
            // Before any window opens: the copies of remote files an earlier run opened are of no use now.
            remote::files::clear(app.handle());
            // On the main thread, inside AppKit's launch, where the event that opened the app is
            // still the current one.
            launch_at_login::capture_launch();
            background::capture_background_launch(std::env::args_os());
            menu::install(app.handle())?;
            // Before the icon, which is hidden at once when the user turned it off.
            app_settings::install(app.handle());
            tray::create(app.handle())?;
            // The notification-area icon's flyout, made hidden now so that a click shows it at
            // once.
            tray::flyout::make(app.handle());
            notifications::install(app.handle());
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
            // Which remote windows are open, for the next launch to bring back, before they end.
            remote::remember_windows(app_handle);
            // A remote window only disconnects: its daemon's sessions run on.
            remote::end_all(app_handle);
            // Tells the daemon about a quit, or passes the system ending the session on to it (on
            // Windows that reaches this with no `ExitRequested`). A held update is installed here
            // only for a quit let through, and left for the next start otherwise.
            exit::on_exit(app_handle);
        }
        _ => lifecycle::on_run_event(app_handle, &event),
    });
}

/// Percent-encodes a query-string *value* (not a full URL): letters, digits and `-_.~` pass
/// through verbatim, everything else — including UTF-8 continuation bytes, which this encodes one
/// byte at a time regardless of the character they belong to — becomes `%XX`. `URLSearchParams` on
/// the receiving side decodes that back to the original string correctly either way.
pub(crate) fn percent_encode_query_value(input: &str) -> String {
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

/// Opens the main window for the daemon as it is when the window is built. On the main thread.
fn open_window_for(app: &tauri::AppHandle) {
    // The window's webview gets its isolation before the window exists where the platform lets it,
    // which `page_isolation` may only be able to report back asynchronously (the content rule list
    // on macOS), so the window is opened from its callback.
    let handle = app.clone();
    page_isolation::prepare(move |configuration| {
        // Before the main window exists, which may be held hidden while they come back.
        remote::begin_restore(&handle, background::launch_stays_in_background(&handle));
        if let Err(err) = window_factory::open_main_window(&handle, configuration) {
            diagnostic!("cannot open the main window: {err}");
            handle.exit(1);
            return;
        }
        // A first launch from a taskbar jump list's task asks for the Remote devices page, or the
        // local window; a later one reaches the running application through the single-instance
        // plugin.
        if let Some(action) = remote::launch_action(std::env::args()) {
            remote::run_menu_action(&handle, action);
        }
    });
}

/// Held by a test while it starts a process, and by the test that takes the instance lock: a process started while the
/// lock's file is open can hold what it inherited (the lock is on the open file, and outlives the test's own handle)
/// from the fork to the exec, or until it is gone, which would make the lock look held after it was let go. Every test
/// that starts a process, and the code under test that does, takes it for the spawn, so none runs alongside that test.
#[cfg(test)]
pub(crate) fn spawning() -> std::sync::MutexGuard<'static, ()> {
    static SPAWNING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SPAWNING.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shell plugin's `open` refuses an address its validation does not match. Its default
    /// (`^((mailto:\w+)|(tel:\w+)|(https?://\w+)).+`) refuses a host written as an IPv6 literal, which the
    /// terminal and the documents link and a remote window's forward of `[::1]` opens at, so
    /// `tauri.conf.json` sets its own.
    #[test]
    fn the_shell_opens_web_addresses_with_an_ipv6_literal_host_and_still_nothing_but_its_four_schemes(
    ) {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let validation = conf["plugins"]["shell"]["open"].as_str().unwrap();
        // As the plugin compiles it.
        let open = regex::Regex::new(&format!("^{validation}$")).unwrap();
        for address in [
            "http://[::1]:54586/r2",
            "https://[2001:db8::1]/",
            "http://[::ffff:127.0.0.1]:3000",
            "http://localhost:3000/a?b#c",
            "https://example.com",
            "http://user:pw@localhost:9/",
            "mailto:someone@example.com",
            "tel:123",
        ] {
            assert!(open.is_match(address), "{address}");
        }
        for address in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.com/",
            "-flag",
            "/R",
            "C:\\Windows\\notepad.exe",
            "http://",
            "http://[evil",
            "http://[::1]/\nsecond line",
        ] {
            assert!(!open.is_match(address), "{address}");
        }
    }

    #[test]
    fn a_query_value_keeps_its_plain_characters_and_escapes_the_rest_byte_by_byte() {
        assert_eq!(percent_encode_query_value(""), "");
        assert_eq!(percent_encode_query_value("aZ09-_.~"), "aZ09-_.~");
        assert_eq!(
            percent_encode_query_value("me@host:22 &=?#/%+"),
            "me%40host%3A22%20%26%3D%3F%23%2F%25%2B"
        );
        // A multi-byte character is escaped as its UTF-8 bytes.
        assert_eq!(percent_encode_query_value("é"), "%C3%A9");
        assert_eq!(
            percent_encode_query_value("服务器"),
            "%E6%9C%8D%E5%8A%A1%E5%99%A8"
        );
    }
}
