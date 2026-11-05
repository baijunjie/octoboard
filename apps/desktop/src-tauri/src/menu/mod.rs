//! The app's native menu bar: built on macOS, and not at all elsewhere, where the host facts
//! announce none (`appMenu: false`) and Settings, About and Quit are reachable from the rail and
//! the tray. What differs by platform is in `macos.rs` and `windows.rs`; this module holds the
//! ids and events the menus' items share with `on_menu_event` in `lib.rs`.

use std::collections::HashMap;

use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

pub use platform::install;

/// The product name, from `config/app.json` (see `build.rs`).
pub const APP_NAME: &str = env!("OCTOBOARD_APP_NAME");

/// The Settings item's id, which `on_menu_event` in `lib.rs` matches.
pub const SETTINGS_ITEM_ID: &str = "settings";

/// The About item's id, which `on_menu_event` in `lib.rs` matches.
pub const ABOUT_ITEM_ID: &str = "about";

/// What `on_menu_event` emits to the webview when the Settings item is chosen; the platform
/// adapter's `tauri.ts` listens for it.
pub const SETTINGS_REQUESTED_EVENT: &str = "settings-requested";

/// What `on_menu_event` emits to the webview when the About item is chosen; the platform adapter's
/// `tauri.ts` listens for it. The UI opens Settings at its About section, so the native About panel
/// is never shown.
pub const ABOUT_REQUESTED_EVENT: &str = "about-requested";

/// The event a menu item that only asks the UI to do something emits to the webview, `None` for any
/// other item. Both are bare signals: what the UI does with them, and whether now is a good time, is
/// its own to decide.
pub fn ui_event_for(item_id: &str) -> Option<&'static str> {
    match item_id {
        SETTINGS_ITEM_ID => Some(SETTINGS_REQUESTED_EVENT),
        ABOUT_ITEM_ID => Some(ABOUT_REQUESTED_EVENT),
        _ => None,
    }
}

/// Rebuilds the menu bar from the labels the UI sent, and the Settings item's accelerator when it
/// sent one, and makes it the app's menu. Where there is no menu bar it does nothing, so the UI
/// can send its labels on every platform.
#[tauri::command]
pub fn set_menu_labels(
    app: AppHandle,
    labels: HashMap<String, String>,
    settings_accelerator: Option<String>,
) -> Result<(), String> {
    platform::set_labels(&app, labels, settings_accelerator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_items_that_ask_the_ui_to_open_something_each_have_their_own_event() {
        assert_eq!(
            ui_event_for(SETTINGS_ITEM_ID),
            Some(SETTINGS_REQUESTED_EVENT)
        );
        assert_eq!(ui_event_for(ABOUT_ITEM_ID), Some(ABOUT_REQUESTED_EVENT));
        assert_ne!(SETTINGS_REQUESTED_EVENT, ABOUT_REQUESTED_EVENT);
        assert_eq!(ui_event_for("quit"), None);
    }
}
