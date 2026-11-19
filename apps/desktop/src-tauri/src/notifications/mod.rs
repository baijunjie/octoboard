//! System notifications, which each daemon's window raises for its own sessions, and a click on
//! one bringing forward the window that raised it, or the main one when that window has closed
//! since. A notification may carry a target of the window's own (what the click should open, as
//! the window spells it), which the click hands back to that window as the `notification-chosen`
//! event. The window asks the shell to show it (`show_notification`), and the platform's part
//! shows it and hears the click:
//!
//! - `plugin.rs` (every target but macOS): the notification plugin shows it, marked with the
//!   window's label and the target, and reports a click on it to the handler `install` registers,
//!   as far as the system reports it (on Windows only while the toast is on screen, not once it is
//!   in the Action Center).
//! - `macos.rs`: the shell delivers it itself, marked with the window's label and the target, and
//!   hears the click through a delegate of the user notification center, since the plugin reports
//!   none for a notification without buttons.

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

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

/// The event a click on a notification that carries a target sends the window that raised it,
/// with the target as its payload.
const CHOSEN_EVENT: &str = "notification-chosen";

/// Shows a notification with `title` and `body`, raised by the asking window, and carrying
/// `target` for that window when given.
#[tauri::command]
pub fn show_notification(
    app: AppHandle,
    webview_window: WebviewWindow,
    title: String,
    body: String,
    target: Option<String>,
) -> Result<(), String> {
    platform::show(&app, title, body, webview_window.label(), target)
}

/// A click on a notification: brings forward the window labelled `label`, or the main one when
/// there is no such window, and hands the window that raised it the notification's `target`. A
/// target is that window's own, so it is dropped when the window has closed since.
fn chosen(app: &AppHandle, label: Option<&str>, target: Option<String>) {
    match label.and_then(|label| app.get_webview_window(label)) {
        Some(window) => {
            show_window(app, &window);
            if let Some(target) = target {
                let _ = app.emit_to(window.label(), CHOSEN_EVENT, target);
            }
        }
        None => show_main_window(app),
    }
}
