//! macOS: the shell shows the notification itself and waits for the click on it.
//!
//! The plugin shows a notification here through `notify-rust`, which waits for what the user does
//! with it only when it has buttons: one without is delivered and given up on at once, so a click
//! on it later reaches the delegate with nothing waiting for it, and only activates the app, which
//! brings forward whichever window was in front. Here the notification is shown the same way,
//! through `mac-notification-sys`, but waited on for a click.
//!
//! This relies on how `mac-notification-sys` ends a wait: a notification keeps its wait, and its
//! thread, until it is clicked, closed, or gone from the notification center's delivered list,
//! which it polls; a notification left in Notification Center is taken to stay on that list, so a
//! click on it there is still heard.

use mac_notification_sys::{Notification, NotificationResponse};
use tauri::AppHandle;

/// Nothing to register: each notification's own thread waits for its click.
pub fn install(_app: &AppHandle) {}

/// Shows the notification and waits on a thread of its own until it is clicked or goes away: the
/// delegate hears the click on the main thread, which the wait must leave free (a synchronous
/// command runs there).
pub fn show(app: &AppHandle, title: String, body: String, label: &str) -> Result<(), String> {
    // Set once for the process; later calls report it already set. Without it the first
    // notification attributes itself to whatever application AppleScript resolves, Finder by
    // default.
    let _ = mac_notification_sys::set_application(application(
        tauri::is_dev(),
        &app.config().identifier,
    ));
    let app = app.clone();
    let label = label.to_owned();
    std::thread::Builder::new()
        .name("notification".into())
        .spawn(move || {
            let response = Notification::new()
                .title(&title)
                .message(&body)
                .wait_for_click(true)
                .send();
            if let Ok(NotificationResponse::Click) = response {
                super::bring_forward(&app, Some(&label));
            }
        })
        .map(drop)
        .map_err(|err| err.to_string())
}

/// The application the notifications are attributed to, which a click activates: the app's own
/// bundle, but Terminal for a development build, whose binary is in no bundle LaunchServices knows
/// (the plugin does the same).
fn application(is_dev: bool, identifier: &str) -> &str {
    if is_dev {
        "com.apple.Terminal"
    } else {
        identifier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notification_is_attributed_to_the_app_except_in_a_development_build() {
        assert_eq!(
            application(false, "dev.octoboard.desktop"),
            "dev.octoboard.desktop"
        );
        assert_eq!(
            application(true, "dev.octoboard.desktop"),
            "com.apple.Terminal"
        );
    }
}
