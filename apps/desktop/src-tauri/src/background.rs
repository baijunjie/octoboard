//! Running in the background: with the user's Settings switch on (`set_run_in_background`), the
//! window's close button hides the window and takes the app out of the Dock instead of quitting.
//! Cmd+W and the Window menu's Close Window item are the same gesture: `menu/macos.rs` builds that
//! item with Tauri's predefined `close_window_with_text`, which carries ⌘W and arrives as the same
//! `CloseRequested` as the button. The app stays reachable through the menu bar icon (`tray/`), the
//! Dock icon or a reopen from the Finder or Spotlight, all of which bring the window back through
//! `show_main_window`. The UI's `bringToFront` does too, through `bring_to_front`, for the exit
//! confirmation of a quit that came while the window was hidden and for the notice that the daemon
//! exited. Activating the app while the window is hidden by a close, such as by clicking a system
//! notification, brings it back too. With the switch off, the default, all three are an ordinary
//! quit (`exit::request_quit_from_window`), confirmation included; and with no daemon left running
//! there is nothing to keep in the background, so they quit at once whatever the switch says. A
//! launch at login with the switch on starts in this state, the window never shown
//! (`start_in_background`). What the platform adds to hiding and showing the window (the Dock,
//! fullscreen, activation on macOS) is in `lifecycle`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::{AppHandle, CloseRequestApi, Manager, Window};

use crate::exit::request_quit_from_window;
use crate::launch_at_login::take_login_launch;
use crate::lifecycle::{before_show, hide_after_leaving_fullscreen, window_hidden};
use crate::sidecar::daemon_stopped;
use crate::MAIN_WINDOW_LABEL;

#[derive(Default)]
pub struct BackgroundState {
    /// Bumped by every show of the window, so a hide that waited for something (a fullscreen
    /// transition, the Dock's hold-off) can tell the window was shown again in the meantime and
    /// leave it alone.
    pub(crate) shown: AtomicU64,
    /// Whether the window is hidden by the close button, as opposed to not shown yet or hidden by
    /// the system (Cmd+H): the state in which the app is out of the Dock and activating it brings
    /// the window back.
    hidden_by_close: AtomicBool,
    /// Whether the close button hides the window rather than quitting: the UI's Settings choice,
    /// which it pushes at startup and on every change. Off until it says otherwise.
    run_in_background: AtomicBool,
}

/// Whether the window is hidden by the close button; false until the first close.
pub fn hidden_by_close(app: &AppHandle) -> bool {
    app.state::<BackgroundState>()
        .hidden_by_close
        .load(Ordering::SeqCst)
}

/// Sets whether the main window's close button keeps the app running in the background, which is
/// the UI's Settings switch; see `handle_close_request`.
#[tauri::command]
pub fn set_run_in_background(app: AppHandle, enabled: bool) {
    app.state::<BackgroundState>()
        .run_in_background
        .store(enabled, Ordering::SeqCst);
}

/// Answers the main window's close request, whether it came from the close button, Cmd+W or the
/// Window menu's Close Window item: hides the window and takes the app out of the Dock if the user
/// chose to run in the background and there is a daemon to keep running, and quits otherwise,
/// through the same confirmation as any quit while a session is live.
pub fn handle_close_request(window: &Window, api: &CloseRequestApi) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }
    // Always prevented: the window is only ever closed by the app exiting, and a quit that the
    // confirmation defers must leave it on screen.
    api.prevent_close();
    let app = window.app_handle();
    let run_in_background = app
        .state::<BackgroundState>()
        .run_in_background
        .load(Ordering::SeqCst);
    if run_in_background && !daemon_stopped(app) {
        hide_main_window(window);
    } else {
        // Quits at once when the daemon is stopped; `exit.rs` owns that rule.
        request_quit_from_window(app);
    }
}

fn hide_main_window(window: &Window) {
    let shown = window
        .app_handle()
        .state::<BackgroundState>()
        .shown
        .load(Ordering::SeqCst);
    if hide_after_leaving_fullscreen(window, shown) {
        return;
    }
    hide(window, shown);
}

/// Hides the window unless it has been shown again since `shown` was read.
pub(crate) fn hide(window: &Window, shown: u64) {
    let app = window.app_handle();
    if app.state::<BackgroundState>().shown.load(Ordering::SeqCst) != shown {
        return;
    }
    let _ = window.hide();
    app.state::<BackgroundState>()
        .hidden_by_close
        .store(true, Ordering::SeqCst);
    window_hidden(app, shown);
}

/// Leaves the window, which the shell created hidden and has not shown, hidden for good, in the
/// state a window closed into the background is in: the app is out of the Dock and waits behind its
/// menu bar icon, and a reopen or the menu bar icon brings the window back. Where the app was
/// launched at login with the user's choice to keep running in the background
/// (`reveal_main_window`), which is also made the shell's own choice from here, as the UI's push
/// would.
pub fn start_in_background(app: &AppHandle) {
    let state = app.state::<BackgroundState>();
    state.run_in_background.store(true, Ordering::SeqCst);
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let shown = state.shown.load(Ordering::SeqCst);
    hide(&window.as_ref().window(), shown);
}

/// Shows the window the shell created hidden, once the UI has painted it, except after a launch at
/// login while the user keeps the app running in the background (`run_in_background`, the UI's
/// choice): there the window stays hidden and the app waits behind its menu bar icon, as after the
/// window is closed into the background. Only the first reveal of a process can be that launch.
#[tauri::command]
pub fn reveal_main_window(app: AppHandle, run_in_background: bool) -> Result<(), String> {
    if take_login_launch() && run_in_background {
        start_in_background(&app);
        return Ok(());
    }
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "there is no main window".to_string())?;
    window.show().map_err(|err| err.to_string())
}

/// `show_main_window` for the UI, which the exit confirmation uses to raise a window that may be
/// hidden. Carries nothing.
#[tauri::command]
pub fn bring_to_front(app: AppHandle) {
    show_main_window(&app);
}

/// Brings the window to the front, restoring it first if it was hidden or minimized, and puts the
/// app back in the Dock if the window had taken it out.
pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let state = app.state::<BackgroundState>();
    state.shown.fetch_add(1, Ordering::SeqCst);
    state.hidden_by_close.store(false, Ordering::SeqCst);
    before_show(app, &window);
    let _ = window.unminimize();
    let _ = window.show();
    // Only activates the app for a window that is visible and not minimized, so it comes last.
    let _ = window.set_focus();
}
