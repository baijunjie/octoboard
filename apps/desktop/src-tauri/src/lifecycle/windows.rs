//! The Windows side of the application's life. This module serves every target but macOS, Linux
//! included. The window's close request and the tray's Quit go through the shared exit flow alone
//! (`handle_close_request` and `request_quit`), and running in the background hides the window and
//! leaves the tray icon, from which `show_main_window` brings it back. There is no Dock to leave or
//! come back to, so the hooks around them are empty. What the platform adds is a second launch: it
//! is turned back and brings the running window forward.

use tauri::{AppHandle, RunEvent, WebviewWindow, Window, Wry};

use crate::background::show_main_window;

/// Nothing to keep. Not a unit struct, so `run` manages it the same way on every platform.
#[derive(Default)]
pub struct LifecycleState {}

/// Registers the single-instance plugin, which has to come before every other plugin: a second
/// launch hands the running instance its arguments and ends, and the running instance shows its
/// window. Without a window yet (the daemon is still starting) there is nothing to show.
pub fn register_first(builder: tauri::Builder<Wry>) -> tauri::Builder<Wry> {
    builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        show_main_window(app);
    }))
}

pub fn install(_app: &AppHandle) {}

pub fn on_run_event(_app: &AppHandle, _event: &RunEvent) {}

/// Never: the window is hidden as it is.
pub fn hide_after_leaving_fullscreen(_window: &Window, _shown: u64) -> bool {
    false
}

pub fn window_hidden(_app: &AppHandle, _shown: u64) {}

pub fn before_show(_app: &AppHandle, _window: &WebviewWindow) {}
