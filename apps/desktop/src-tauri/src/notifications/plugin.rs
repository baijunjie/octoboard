//! Every target but macOS: the notification plugin shows the notification, its extra payload
//! naming the window that raised it and the target it carries, and reports a click on it to the
//! handler `install` registers.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// The key of a notification's extra payload that holds the label of the window that raised it.
const WINDOW_KEY: &str = "window";

/// The key of a notification's extra payload that holds the target it carries for that window.
const TARGET_KEY: &str = "target";

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
        let extra = performed
            .notification()
            .map(|notification| notification.extra());
        let label = extra.and_then(window_of);
        let target = extra.and_then(target_of);
        super::chosen(&handle, label.as_deref(), target);
    });
}

/// Shows the notification, marked with the label of the window that raised it and the target it
/// carries.
pub fn show(
    app: &AppHandle,
    title: String,
    body: String,
    label: &str,
    target: Option<String>,
) -> Result<(), String> {
    let mut builder = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .extra(WINDOW_KEY, label);
    if let Some(target) = target {
        builder = builder.extra(TARGET_KEY, target);
    }
    builder.show().map_err(|err| err.to_string())
}

/// The label of the window a notification's extra payload names.
fn window_of(extra: &std::collections::HashMap<String, serde_json::Value>) -> Option<String> {
    extra.get(WINDOW_KEY)?.as_str().map(str::to_owned)
}

/// The target a notification's extra payload carries.
fn target_of(extra: &std::collections::HashMap<String, serde_json::Value>) -> Option<String> {
    extra.get(TARGET_KEY)?.as_str().map(str::to_owned)
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

    #[test]
    fn a_notification_hands_back_the_target_it_carries() {
        let mut extra = std::collections::HashMap::new();
        assert_eq!(target_of(&extra), None);
        extra.insert(
            TARGET_KEY.to_owned(),
            serde_json::json!("scheduled-task:t-1"),
        );
        assert_eq!(target_of(&extra).as_deref(), Some("scheduled-task:t-1"));
    }
}
