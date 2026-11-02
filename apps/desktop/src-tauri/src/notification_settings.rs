//! Opening the app's own pane in System Settings > Notifications.
//!
//! On macOS the notification plugin reports "granted" without ever prompting and Octoboard keeps no
//! switch of its own, so the system's pane is the only place its notifications are turned off or
//! restyled. The Settings section links there.

#[cfg(target_os = "macos")]
use crate::system_settings::{notifications_pane_url, open_system_settings};

/// Opens the app's pane in System Settings > Notifications. Fails with a message when the system
/// declines to open it.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn macos_open_notification_settings() -> Result<(), String> {
    use objc2_foundation::{NSBundle, NSProcessInfo};

    let major_version = NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion;
    let bundle_id = NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|id| id.to_string());
    open_system_settings(&notifications_pane_url(major_version, bundle_id.as_deref()))
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn macos_open_notification_settings() -> Result<(), String> {
    Ok(())
}
