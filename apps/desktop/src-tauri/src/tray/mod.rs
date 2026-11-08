//! The menu bar icon (a status item), which keeps the app reachable while its window is closed and
//! the app is out of the Dock (`background.rs`). A click on the icon, left or right, opens its menu,
//! which lists the live sessions the UI hands over (`set_tray_menu`) and its submenus (the agents'
//! quotas), then Updates, Open and Quit; the window comes back through Open or by choosing a
//! session, which also tells the UI which one. Updates brings it back at Settings’ About section.
//! Quit goes through the same exit flow as Cmd+Q, so it still asks while a session is running.
//!
//! The icon itself has two attention looks: the octopus, and the octopus with an orange dot in its
//! corner, which shows while the menu's `attention` flag is set (a session is waiting for the
//! user). Like the rest of the menu, the UI decides when, and the look changes together with the
//! menu it came with. Whichever it is, the octopus's eyes turn red while the computer is kept awake
//! (`keep_awake_changed`): on macOS the eye sockets keep their shape and go from translucent to
//! solid red, and only on Windows does the picture draw larger eyes, so that reads by shape and not
//! only by colour. Unlike the rest it follows the hold at once, not the menu, and the menu's
//! `keep_awake_label` becomes the accessible name. A readable selected-account quota adds a capsule
//! over the lower tentacles (`icon.rs`), with a thinner one beneath for the weekly window when the
//! account has a 5-hour one too; the UI supplies the remaining percents and the localized label,
//! which is the icon's accessible name (the icon has no tooltip). The octopus and capsule are
//! always one template image, so macOS keeps drawing them for the menu bar's light or dark look; a
//! template image cannot carry a colour, so the dot and the eyes are views of their own laid over
//! the button, in place of the octopus's cut-out corner and eyes (`show_layers`). On Windows the
//! icon is the app icon in its own colours, the dot and the larger eyes are part of the picture, a
//! quota is not drawn, and the label is the icon's tooltip.
//!
//! The user can hide the icon from Settings (`set_tray_icon_visible`). While it is hidden the menu
//! and the icon's look are still kept up to date, so the icon comes back as it should be.
//!
//! Replacing the menu closes it if it is open, so a menu the UI sends while a menu is open is held
//! back and applied once it closes (`MenuTracking`), which macOS's menu tracking and, on Windows,
//! the menu loop of tray-icon's window report (`follow_menu_tracking`). The UI
//! is told when this menu begins to open (`MENU_OPENED_EVENT`: macOS's menu tracking, the click
//! that opens it on Windows) so it can ask for fresh quotas, which reach the menu on its next update.
//!
//! Muda gives a menu item's picture no template flag, which is what makes a monochrome one follow
//! the menu's light or dark look. So the pictures flagged template get it set on their
//! `NSMenuItem`s, line by line (`finish_menu`). Tauri does not hand out a menu's `NSMenu`, and
//! tray-icon puts it on the status item (`NSStatusItem::menu`) only while it is open, so the
//! pass waits for the menu to begin tracking, whose notification carries it, and runs once for
//! each menu built (`macos/finish.rs`). The same pass gives the headings of the sessions'
//! sections and the lines of each submenu their rows (`lines.rs`), as a line that cannot be
//! chosen is drawn in a grey that nothing but a view of its own can darken.
//!
//! All of that native work is the platform module's (`macos/`, `windows.rs`); this module and the
//! files beside it hold what every platform shares: the model, the plain items, the icon's pictures
//! and when a menu is held back.

// The quota bars, the attention dot (`icon::dot_png`) and the submenus' rows are drawn by
// macOS-only code, and the menu tracking is followed on macOS and Windows only; elsewhere the menu
// the UI sends is still parsed whole, so those parts are read by nothing. The allow reaches dead items, not unused imports: the imports
// only macOS code uses are in `macos/`.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::sync::Mutex;

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter};

use crate::background::show_main_window;
use crate::exit::request_quit;
use crate::menu::APP_NAME;

mod finish;
mod icon;
mod items;
mod menu_tracking;
mod model;
mod status_item;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

use items::build_menu;
use menu_tracking::{tracking, MenuTracking};
use model::TrayMenu;
use platform::{
    customize, finish_when_opened, follow_menu_tracking, picture, set_icon_label, set_visible,
    Unfinished, TEMPLATE,
};
use status_item::{show_look, IconState, Look};

const TRAY_ID: &str = "main";
const OPEN_ITEM_ID: &str = "tray-open";
const UPDATES_ITEM_ID: &str = "tray-updates";
/// The Quit item, which goes down the same exit flow as the app menu's.
const QUIT_ITEM_ID: &str = "tray-quit";

/// A session item's id is this prefix followed by the session's id, which is what the UI gets back
/// in `SESSION_CHOSEN_EVENT`.
const SESSION_ITEM_PREFIX: &str = "tray-session:";

/// What the shell emits to the webview, with the session's id as its payload, when a session is
/// chosen in the tray menu; the platform adapter's `tauri.ts` listens for it.
const SESSION_CHOSEN_EVENT: &str = "tray-session-chosen";

/// What the shell emits to the webview, with no payload, when the tray menu begins to open; the
/// platform adapter's `tauri.ts` listens for it.
const MENU_OPENED_EVENT: &str = "tray-menu-opened";

/// The menu tracking, the icon currently drawn, so repeated menu updates keep its picture, and
/// what is left to finish of the menu last set.
#[derive(Default)]
pub struct TrayState {
    tracking: Mutex<MenuTracking>,
    icon_shown: Mutex<IconState>,
    /// What the icon is asked to show, from the menu last applied and the keep-awake hold.
    look: Mutex<Look>,
    /// What `finish_menu` is to do to the menu last set, until the menu first opens. It is replaced
    /// in the same pass as the menu, and taken only for a menu `is_tray_menu` matched, so it never
    /// finishes a menu other than the one it was made for. Nothing off macOS, where no menu is
    /// finished.
    unfinished: Unfinished,
}

/// Puts the icon in the menu bar. Its picture and whether it is a template image, which macOS
/// tints for a light or dark menu bar itself, are the platform's.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let tray = customize(
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(picture(false, false, None))
            .icon_as_template(TEMPLATE)
            .menu(&build_menu(app, &TrayMenu::default())?)
            .show_menu_on_left_click(true),
    )
    .build(app)?;
    set_icon_label(&tray, APP_NAME);
    follow_menu_tracking(app);
    Ok(())
}

/// Rebuilds the tray menu from what the UI sent, or holds it until the open menu closes.
#[tauri::command]
pub fn set_tray_menu(app: AppHandle, menu: TrayMenu) -> Result<(), String> {
    let Some(menu) = tracking(&app).offer(menu) else {
        return Ok(());
    };
    apply(&app, &menu)
}

/// Shows or hides the icon, which is the UI's Settings switch. The icon starts shown on every
/// launch and the UI hands its choice over once it has loaded, so a window whose UI never loads
/// keeps it. Hiding changes nothing else: the menu and the look
/// the UI last sent are kept and are there again when the icon is shown.
#[tauri::command]
pub fn set_tray_icon_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "the menu bar icon does not exist".to_string())?;
    set_visible(&tray, visible).map_err(|err| err.to_string())
}

/// Takes the icon out of the notification area now, for a process that is about to end without
/// the exit event, which is what otherwise removes it: an icon left behind stays drawn until the
/// pointer passes over it. Hiding waits for the main thread to have done it, which dropping the
/// icon would not.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn remove(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(false);
    }
}

fn apply(app: &AppHandle, menu: &TrayMenu) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "the menu bar icon does not exist".to_string())?;
    let built = build_menu(app, menu).map_err(|err| err.to_string())?;
    tray.set_menu(Some(built)).map_err(|err| err.to_string())?;
    finish_when_opened(app, menu);
    show_look(app, &tray, |look| {
        *look = Look::from_menu(menu, look.awake())
    })
}

/// Tells the icon whether the computer is now kept awake (`keep_awake::set_keep_awake`), which it
/// shows at once, whether or not a menu is open: the hold is not part of the menu's snapshot.
pub fn keep_awake_changed(app: &AppHandle, held: bool) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "the menu bar icon does not exist".to_string())?;
    show_look(app, &tray, |look| look.set_awake(held))
}

/// Answers a choice in the tray menu; an id that is not one of its items is ignored.
pub fn handle_menu_event(app: &AppHandle, id: &str) {
    if id == OPEN_ITEM_ID {
        show_main_window(app);
    } else if id == UPDATES_ITEM_ID {
        show_main_window(app);
        let _ = app.emit("tray-updates-requested", ());
    } else if id == QUIT_ITEM_ID {
        request_quit(app);
    } else if let Some(session) = id.strip_prefix(SESSION_ITEM_PREFIX) {
        show_main_window(app);
        let _ = app.emit(SESSION_CHOSEN_EVENT, session);
    }
}
