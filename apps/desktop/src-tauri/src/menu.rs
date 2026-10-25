//! The app's native macOS menu bar.

use std::collections::HashMap;

use muda::accelerator::Accelerator;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::AppHandle;

/// The product name, from `config/app.json` (see `build.rs`).
pub const APP_NAME: &str = env!("OCTOBOARD_APP_NAME");

/// The Settings item's id, which `on_menu_event` in `lib.rs` matches.
pub const SETTINGS_ITEM_ID: &str = "settings";

/// The About item's id, which `on_menu_event` in `lib.rs` matches.
pub const ABOUT_ITEM_ID: &str = "about";

/// The Settings item's accelerator until the UI sends the one the user has chosen.
pub const DEFAULT_SETTINGS_ACCELERATOR: &str = "CmdOrCtrl+,";

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

/// The menu's labels, keyed by the UI's `menu.*` message keys (`packages/ui/src/i18n/messages/en.ts`),
/// which the UI hands over in its current language through `set_menu_labels`. A key the UI did not
/// send keeps its English default, so the menu built at startup, before the UI has loaded, reads
/// the same as one built from the English catalog.
#[derive(Default)]
pub struct Labels(HashMap<String, String>);

impl Labels {
    fn get<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.0.get(key).map_or(default, String::as_str)
    }
}

/// Rebuilds the menu bar from the labels the UI sent, and the Settings item's accelerator when it
/// sent one, and makes it the app's menu. An accelerator that does not parse leaves the item with
/// the default one (see `usable_accelerator`).
#[tauri::command]
pub fn set_menu_labels(
    app: AppHandle,
    labels: HashMap<String, String>,
    settings_accelerator: Option<String>,
) -> Result<(), String> {
    let menu = build_menu(
        &app,
        &Labels(labels),
        settings_accelerator
            .as_deref()
            .unwrap_or(DEFAULT_SETTINGS_ACCELERATOR),
    )
    .map_err(|err| err.to_string())?;
    app.set_menu(menu).map_err(|err| err.to_string())?;
    Ok(())
}

/// `accelerator` when the menu can parse it, the default one otherwise. Tauri's menu items drop an
/// accelerator that does not parse and leave the item without a key, so it is parsed here first.
fn usable_accelerator(accelerator: &str) -> &str {
    if accelerator.parse::<Accelerator>().is_ok() {
        accelerator
    } else {
        DEFAULT_SETTINGS_ACCELERATOR
    }
}

/// Builds the app's menu bar. It is close to the framework's own default macOS menu, with one
/// difference that matters: Quit is this crate's own `MenuItem`, not
/// `PredefinedMenuItem::quit`. The predefined one sends the native `terminate:` selector straight
/// to the app, which ends the process through no `RunEvent` any confirmation flow can intercept; a
/// plain menu item instead reaches `on_menu_event` in `lib.rs`, which routes it into the same
/// `exit-requested` path as Cmd+Q and the menu bar icon's Quit. `settings_accelerator` is the
/// Settings item's key, which the user can change (the default one stands in for a key that does
/// not parse); Quit's is fixed.
///
/// The Dock icon's own Quit sends that same native `terminate:` straight to the process, with no
/// menu item of this crate's own in the way — `exit::install_application_should_terminate_override`
/// is how it is caught instead.
pub fn build_menu(
    app: &AppHandle,
    labels: &Labels,
    settings_accelerator: &str,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let quit =
        MenuItemBuilder::with_id("quit", labels.get("menu.quit", &format!("Quit {APP_NAME}")))
            .accelerator("CmdOrCtrl+Q")
            .build(app)?;

    // A plain item for the same reason as Quit: the UI owns what opening settings means.
    let settings =
        MenuItemBuilder::with_id(SETTINGS_ITEM_ID, labels.get("menu.settings", "Settings…"))
            .accelerator(usable_accelerator(settings_accelerator))
            .build(app)?;

    // A plain item as well, not the native About panel: About is a section of the UI's Settings.
    let about = MenuItemBuilder::with_id(
        ABOUT_ITEM_ID,
        labels.get("menu.about", &format!("About {APP_NAME}")),
    )
    .build(app)?;

    // The submenu title is the product name, which no language translates.
    let app_menu = SubmenuBuilder::new(app, APP_NAME)
        .item(&about)
        .separator()
        .item(&settings)
        .separator()
        .services_with_text(labels.get("menu.services", "Services"))
        .separator()
        .hide_with_text(labels.get("menu.hide", &format!("Hide {APP_NAME}")))
        .hide_others_with_text(labels.get("menu.hideOthers", "Hide Others"))
        .show_all_with_text(labels.get("menu.showAll", "Show All"))
        .separator()
        .item(&quit)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, labels.get("menu.edit", "Edit"))
        .undo_with_text(labels.get("menu.undo", "Undo"))
        .redo_with_text(labels.get("menu.redo", "Redo"))
        .separator()
        .cut_with_text(labels.get("menu.cut", "Cut"))
        .copy_with_text(labels.get("menu.copy", "Copy"))
        .paste_with_text(labels.get("menu.paste", "Paste"))
        .select_all_with_text(labels.get("menu.selectAll", "Select All"))
        .build()?;

    let view_menu = SubmenuBuilder::new(app, labels.get("menu.view", "View"))
        .fullscreen_with_text(labels.get("menu.fullscreen", "Toggle Full Screen"))
        .build()?;

    let window_menu = SubmenuBuilder::new(app, labels.get("menu.window", "Window"))
        .minimize_with_text(labels.get("menu.minimize", "Minimize"))
        .maximize_with_text(labels.get("menu.zoom", "Zoom"))
        .separator()
        .close_window_with_text(labels.get("menu.closeWindow", "Close Window"))
        .build()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &edit_menu, &view_menu, &window_menu])
        .build()
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

    #[test]
    fn a_key_the_menu_cannot_parse_gives_way_to_the_default() {
        for accelerator in [
            "CmdOrCtrl+,",
            "CmdOrCtrl+Shift+Backslash",
            "Ctrl+Alt+IntlBackslash",
            "CmdOrCtrl+Numpad5",
            "CmdOrCtrl+F13",
        ] {
            assert_eq!(usable_accelerator(accelerator), accelerator);
        }
        for accelerator in ["", "CmdOrCtrl+", "Hyperdrive+K", "CmdOrCtrl+Nonsense"] {
            assert_eq!(
                usable_accelerator(accelerator),
                DEFAULT_SETTINGS_ACCELERATOR
            );
        }
    }
}
