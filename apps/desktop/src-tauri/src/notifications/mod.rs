//! System notifications, which each daemon's window raises for its own sessions, and a click on
//! one bringing forward the window that raised it, or the main one when that window has closed
//! since. The window asks the shell to show it (`show_notification`), and the platform's part
//! shows it and hears the click:
//!
//! - `plugin.rs` (every target but macOS): the notification plugin shows it, marked with the
//!   window's label, and reports a click on it to the handler `install` registers, as far as the
//!   system reports it (on Windows only while the toast is on screen, not once it is in the Action
//!   Center).
//! - `macos.rs`: the shell delivers it itself, marked with the window's label, and hears the click
//!   through a delegate of the user notification center, since the plugin reports none for a
//!   notification without buttons.

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::background::{show_main_window, show_window};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod plugin;
#[cfg(not(target_os = "macos"))]
use plugin as platform;

/// Prepares hearing the clicks on the notifications, before any notification is shown.
pub fn install(app: &AppHandle) {
    platform::install(app);
}

/// Shows a notification with `title` and `body`, raised by the asking window.
#[tauri::command]
pub fn show_notification(
    app: AppHandle,
    webview_window: WebviewWindow,
    title: String,
    body: String,
) -> Result<(), String> {
    platform::show(&app, title, body, webview_window.label())
}

/// Brings forward the window labelled `label`, or the main one when there is no such window.
fn bring_forward(app: &AppHandle, label: Option<&str>) {
    match label.and_then(|label| app.get_webview_window(label)) {
        Some(window) => show_window(app, &window),
        None => show_main_window(app),
    }
}
