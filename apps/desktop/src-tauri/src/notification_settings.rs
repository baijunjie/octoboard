//! Opening the system's notification settings: the app's own pane in System Settings on macOS, the
//! Notifications page of Settings on Windows.
//!
//! On macOS the notification plugin reports "granted" without ever prompting and Octoboard keeps no
//! switch of its own, so the system's pane is the only place its notifications are turned off or
//! restyled; the same holds on Windows. The Settings section links there.

#[cfg(target_os = "macos")]
mod macos {
    use crate::system_settings::{notifications_pane_url, open_system_settings};

    pub fn open_notification_settings() -> Result<(), String> {
        use objc2_foundation::{NSBundle, NSProcessInfo};

        let major_version = NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion;
        let bundle_id = NSBundle::mainBundle()
            .bundleIdentifier()
            .map(|id| id.to_string());
        open_system_settings(&notifications_pane_url(major_version, bundle_id.as_deref()))
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

    /// Opens Settings > System > Notifications through the shell, as the user's default handler for
    /// the `ms-settings:` scheme. Not through `tauri-plugin-shell`, whose `open` only accepts
    /// `http:`, `https:`, `mailto:` and `tel:` addresses.
    #[cfg(windows)]
    pub fn open_notification_settings() -> Result<(), String> {
        use std::ptr::{null, null_mut};
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain([0]).collect() };
        let (verb, page) = (wide("open"), wide(NOTIFICATIONS_PAGE));
        // SAFETY: both strings are NUL-terminated and outlive the call.
        let result = unsafe {
            ShellExecuteW(
                null_mut(),
                verb.as_ptr(),
                page.as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            )
        };
        // The call reports success with a value above 32 and an error code at or below it.
        if result as usize > 32 {
            Ok(())
        } else {
            Err(format!(
                "the system did not open {NOTIFICATIONS_PAGE} (code {})",
                result as usize
            ))
        }
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
