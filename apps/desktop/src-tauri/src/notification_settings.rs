//! Opening the system's notification settings: the app's own pane in System Settings on macOS, the
//! Notifications page of Settings on Windows.
//!
//! On macOS the notification plugin reports "granted" without ever prompting and Octoboard keeps no
//! switch of its own, so the system's pane is the only place its notifications are turned off or
//! restyled; the same holds on Windows. The Settings section links there.

#[cfg(target_os = "macos")]
mod macos {
    use crate::system_settings::notifications_pane_url;

    pub fn open_notification_settings() -> Result<(), String> {
        use objc2_foundation::{NSBundle, NSProcessInfo};

        let major_version = NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion;
        let bundle_id = NSBundle::mainBundle()
            .bundleIdentifier()
            .map(|id| id.to_string());
        crate::open_address::open(&notifications_pane_url(major_version, bundle_id.as_deref()))
    }
}
#[cfg(target_os = "macos")]
use macos as platform;

/// Every target but macOS, Linux included.
#[cfg(not(target_os = "macos"))]
mod windows {
    /// The Settings page for notifications; the page lists every app, the app's own entry among them.
    #[cfg(windows)]
    const NOTIFICATIONS_PAGE: &str = "ms-settings:notifications";

    /// Opens Settings > System > Notifications.
    #[cfg(windows)]
    pub fn open_notification_settings() -> Result<(), String> {
        crate::open_address::open(NOTIFICATIONS_PAGE)
    }

    /// Does nothing: the host facts announce no notification settings, so the UI offers no link.
    #[cfg(not(windows))]
    pub fn open_notification_settings() -> Result<(), String> {
        Ok(())
    }
}
#[cfg(not(target_os = "macos"))]
use windows as platform;

/// Opens the app's notification settings in the system's settings app: its pane in System Settings >
/// Notifications on macOS, the Notifications page of Settings on Windows. Fails with a message when
/// the system declines to open it.
#[tauri::command]
pub fn open_notification_settings() -> Result<(), String> {
    platform::open_notification_settings()
}
