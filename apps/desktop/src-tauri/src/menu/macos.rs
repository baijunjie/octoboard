//! The app's native macOS menu bar.

use std::collections::HashMap;

use muda::accelerator::Accelerator;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::AppHandle;

use super::{ABOUT_ITEM_ID, APP_NAME, SETTINGS_ITEM_ID};

/// The Settings item's accelerator until the UI sends the one the user has chosen.
const DEFAULT_SETTINGS_ACCELERATOR: &str = "CmdOrCtrl+,";

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

/// Sets the menu bar the app starts with, before the UI has sent its labels.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    app.set_menu(build_menu(
        app,
        &Labels::default(),
        DEFAULT_SETTINGS_ACCELERATOR,
    )?)?;
    Ok(())
}

/// See `set_menu_labels`.
pub fn set_labels(
    app: &AppHandle,
    labels: HashMap<String, String>,
    settings_accelerator: Option<String>,
) -> Result<(), String> {
    let menu = build_menu(
        app,
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
/// menu item of this crate's own in the way — `install_application_should_terminate_override` in
/// `lifecycle/macos.rs` is how it is caught instead.
fn build_menu(
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
