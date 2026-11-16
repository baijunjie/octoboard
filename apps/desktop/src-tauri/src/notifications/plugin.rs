//! Every target but macOS: the notification plugin shows the notification, its extra payload
//! naming the window that raised it, and reports a click on it to the handler `install` registers.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

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
        super::bring_forward(&handle, label.as_deref());
    });
}

/// Shows the notification, marked with the label of the window that raised it.
pub fn show(app: &AppHandle, title: String, body: String, label: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .extra(WINDOW_KEY, label)
        .show()
        .map_err(|err| err.to_string())
}

/// The label of the window a notification's extra payload names.
fn window_of(extra: &std::collections::HashMap<String, serde_json::Value>) -> Option<String> {
    extra.get(WINDOW_KEY)?.as_str().map(str::to_owned)
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
