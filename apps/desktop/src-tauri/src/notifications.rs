//! System notifications, which each daemon's window raises for its own sessions, and a click on
//! one bringing forward the window that raised it. The window asks the shell to show it
//! (`show_notification`), which marks it with the window's label; the notification plugin reports
//! a click on it to the handler `install` registers, as far as the system reports it (on Windows
//! only while the toast is on screen, not once it is in the Action Center), and the handler brings
//! that window forward, or the main one when that window has closed since.

use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

use crate::background::{show_main_window, show_window};

/// The key of a notification's extra payload that holds the label of the window that raised it.
const WINDOW_KEY: &str = "window";

/// The action the plugin reports for a click on the notification itself.
const TAP: &str = "tap";

/// Registers the handler for clicks on the notifications. Before any notification is shown: the
/// plugin reports nothing of one shown before the first handler existed.
pub fn install(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.notification().on_action(move |performed| {
        if performed.action_id() != TAP {
            return;
        }
        let label = performed
            .notification()
            .and_then(|notification| window_of(notification.extra()));
        bring_forward(&handle, label.as_deref());
    });
}

/// The label of the window a notification's extra payload names.
fn window_of(extra: &std::collections::HashMap<String, serde_json::Value>) -> Option<String> {
    extra.get(WINDOW_KEY)?.as_str().map(str::to_owned)
}

fn bring_forward(app: &AppHandle, label: Option<&str>) {
    match label.and_then(|label| app.get_webview_window(label)) {
        Some(window) => show_window(app, &window),
        None => show_main_window(app),
    }
}

/// Shows a notification with `title` and `body`, raised by the asking window.
#[tauri::command]
pub fn show_notification(
    app: AppHandle,
    webview_window: WebviewWindow,
    title: String,
    body: String,
) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .extra(WINDOW_KEY, webview_window.label())
        .show()
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notification_names_the_window_that_raised_it() {
        let mut extra = std::collections::HashMap::new();
        assert_eq!(window_of(&extra), None);
        extra.insert(WINDOW_KEY.to_owned(), serde_json::json!("daemon-2"));
        assert_eq!(window_of(&extra).as_deref(), Some("daemon-2"));
        extra.insert(WINDOW_KEY.to_owned(), serde_json::json!(2));
        assert_eq!(window_of(&extra), None);
    }
}
