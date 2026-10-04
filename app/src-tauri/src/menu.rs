//! The app's native macOS menu bar.

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::AppHandle;

/// Builds the app's menu bar. It is close to the framework's own default macOS menu, with one
/// difference that matters: Quit is this crate's own `MenuItem`, not
/// `PredefinedMenuItem::quit`. The predefined one sends the native `terminate:` selector straight
/// to the app, which ends the process through no `RunEvent` any confirmation flow can intercept; a
/// plain menu item instead reaches `on_menu_event` in `lib.rs`, which routes it into the same
/// `exit-requested` path as Cmd+Q and the window's close button.
///
/// The Dock icon's own Quit sends that same native `terminate:` straight to the process, with no
/// menu item of this crate's own in the way — `exit::install_application_should_terminate_override`
/// is how it is caught instead.
pub fn build_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let quit = MenuItemBuilder::with_id("quit", "Quit Octoboard")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "Octoboard")
        .about(None)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .item(&quit)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View").fullscreen().build()?;

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &edit_menu, &view_menu, &window_menu])
        .build()
}
