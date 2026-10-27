//! Opening the app's own pane in System Settings > Notifications.
//!
//! On macOS the notification plugin reports "granted" without ever prompting and Octoboard keeps no
//! switch of its own, so the system's pane is the only place its notifications are turned off or
//! restyled. The Settings section links there.

/// The pane of System Settings (macOS 13 and later) every app's notification settings live in.
#[cfg(any(target_os = "macos", test))]
const SETTINGS_PANE_URL: &str =
    "x-apple.systempreferences:com.apple.Notifications-Settings.extension";

/// The same pane in System Preferences (macOS 11 and 12).
#[cfg(any(target_os = "macos", test))]
const PREFERENCES_PANE_URL: &str = "x-apple.systempreferences:com.apple.preference.notifications";

/// The address of the pane for `bundle_id`, as the macOS `major_version` names it, or of the pane's
/// generic page when there is no identifier (a binary run outside a bundle, such as `tauri dev`,
/// has none to select). Both panes take `?id=` to select one app.
#[cfg(any(target_os = "macos", test))]
fn pane_url(major_version: isize, bundle_id: Option<&str>) -> String {
    let pane = if major_version >= 13 {
        SETTINGS_PANE_URL
    } else {
        PREFERENCES_PANE_URL
    };
    match bundle_id {
        Some(id) if !id.is_empty() => format!("{pane}?id={id}"),
        _ => pane.to_string(),
    }
}

/// Goes through `NSWorkspace`, not `tauri-plugin-shell`: the plugin's `open` only accepts `http:`,
/// `https:`, `mailto:` and `tel:` addresses and would reject this scheme. Fails with a message when
/// the address cannot be built or the system declines to open it.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn macos_open_notification_settings() -> Result<(), String> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSBundle, NSProcessInfo, NSString, NSURL};

    let major_version = NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion;
    let bundle_id = NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|id| id.to_string());
    let url = pane_url(major_version, bundle_id.as_deref());
    let Some(parsed) = NSURL::URLWithString(&NSString::from_str(&url)) else {
        return Err(format!("not a valid address: {url}"));
    };
    if NSWorkspace::sharedWorkspace().openURL(&parsed) {
        Ok(())
    } else {
        Err(format!("the system did not open {url}"))
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn macos_open_notification_settings() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_app_in_system_settings_from_macos_13() {
        for major in [13, 14, 26] {
            assert_eq!(
                pane_url(major, Some("dev.octoboard.desktop")),
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=dev.octoboard.desktop"
            );
        }
    }

    #[test]
    fn selects_the_app_in_system_preferences_on_macos_11_and_12() {
        for major in [11, 12] {
            assert_eq!(
                pane_url(major, Some("dev.octoboard.desktop")),
                "x-apple.systempreferences:com.apple.preference.notifications?id=dev.octoboard.desktop"
            );
        }
    }

    #[test]
    fn falls_back_to_the_generic_pane_without_an_identifier() {
        assert_eq!(pane_url(14, None), SETTINGS_PANE_URL);
        assert_eq!(pane_url(14, Some("")), SETTINGS_PANE_URL);
        assert_eq!(pane_url(12, None), PREFERENCES_PANE_URL);
    }
}
