//! The menu bar icon (a status item), which keeps the app reachable while its window is closed and
//! the app is out of the Dock (`background.rs`). A click on the icon, left or right, opens its menu,
//! which lists the live sessions the UI hands over (`set_tray_menu`) and its submenus (the agents'
//! quotas), then Open and Quit; the window comes back through Open or by choosing a session, which
//! also tells the UI which one. Quit goes through the same exit flow as Cmd+Q, so it still asks
//! while a session is running.
//!
//! The icon itself has two looks: the octopus, and the octopus with an orange dot in its corner,
//! which shows while the menu's `attention` flag is set (a session is waiting for the user). Like
//! the rest of the menu, the UI decides when, and the look changes together with the menu it came
//! with. A readable selected-account quota adds a capsule over the lower tentacles
//! (`icon.rs`), with a thinner one beneath for the weekly window when the account has a 5-hour
//! one too; the UI supplies the remaining percents and the localized label, which is the icon's
//! accessible name (the icon has no tooltip). The octopus and capsule are always one template
//! image, so macOS keeps drawing them for the menu bar's light or dark look; a template image
//! cannot carry a colour, so the dot is a view of its own laid over the button, in place of the
//! octopus's cut-out corner (`show_dot`).
//!
//! Replacing the menu closes it if it is open, so a menu the UI sends while a menu is open is held
//! back and applied once it closes (`MenuTracking`). The UI is told when this menu begins to open
//! (`MENU_OPENED_EVENT`) so it can ask for fresh quotas, which reach the menu on its next update.
//!
//! Muda gives a menu item's picture no template flag, which is what makes a monochrome one follow
//! the menu's light or dark look. So the pictures flagged template get it set on their
//! `NSMenuItem`s, line by line (`finish_menu`). Tauri does not hand out a menu's `NSMenu`, and
//! tray-icon puts it on the status item (`NSStatusItem::menu`) only while it is open, so the
//! pass waits for the menu to begin tracking, whose notification carries it, and runs once for
//! each menu built (`finish::unfinished`). The same pass gives the headings of the sessions'
//! sections and the lines of each submenu their rows (`lines.rs`), as a line that cannot be
//! chosen is drawn in a grey that nothing but a view of its own can darken.

// The quota bars, the attention dot (`icon::dot_png`), the submenus' rows and the menu tracking are
// drawn and driven by macOS-only code; elsewhere the menu the UI sends is still parsed whole, so
// those parts are read by nothing. The allow reaches dead items, not unused imports: an import only
// macOS code uses carries the same `cfg` itself.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::sync::Mutex;

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter};

use crate::background::show_main_window;
use crate::exit::request_quit;
#[cfg(target_os = "macos")]
use crate::menu::APP_NAME;

mod finish;
mod icon;
mod items;
#[cfg(target_os = "macos")]
mod lines;
mod menu_tracking;
mod model;
#[cfg(target_os = "macos")]
mod openers;
mod status_item;

#[cfg(target_os = "macos")]
use finish::{finish_order, unfinished, MenuFinish};
use items::build_menu;
#[cfg(target_os = "macos")]
use menu_tracking::follow_menu_tracking;
use menu_tracking::{tracking, MenuTracking};
use model::TrayMenu;
#[cfg(target_os = "macos")]
use status_item::set_icon_label;
use status_item::{show_icon, IconState};

const TRAY_ID: &str = "main";
const OPEN_ITEM_ID: &str = "tray-open";
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
    /// What `finish_menu` is to do to the menu last set, until the menu first opens. It is replaced
    /// in the same pass as the menu, and taken only for a menu `is_tray_menu` matched, so it never
    /// finishes a menu other than the one it was made for.
    #[cfg(target_os = "macos")]
    unfinished: Mutex<Option<MenuFinish>>,
}

/// Puts the icon in the menu bar. It is a template image, so macOS tints it for a light or dark
/// menu bar itself.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon::octopus(false, None))
        .icon_as_template(true)
        .menu(&build_menu(app, &TrayMenu::default())?)
        .show_menu_on_left_click(true)
        .build(app)?;
    #[cfg(not(target_os = "macos"))]
    let _ = tray;
    #[cfg(target_os = "macos")]
    {
        set_icon_label(&tray, APP_NAME);
        follow_menu_tracking(app);
    }
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

fn apply(app: &AppHandle, menu: &TrayMenu) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "the menu bar icon does not exist".to_string())?;
    let built = build_menu(app, menu).map_err(|err| err.to_string())?;
    tray.set_menu(Some(built)).map_err(|err| err.to_string())?;
    #[cfg(target_os = "macos")]
    {
        *unfinished(app) = Some(MenuFinish {
            lines: finish_order(menu),
            submenus: menu.submenus.clone(),
        });
    }
    let quota = menu.quota_remaining.map(|remaining| icon::Quota {
        main: remaining.min(100),
        thin: menu.quota_weekly_remaining.map(|weekly| weekly.min(100)),
    });
    #[cfg(target_os = "macos")]
    set_icon_label(
        &tray,
        quota.and(menu.quota_label.as_deref()).unwrap_or(APP_NAME),
    );
    show_icon(app, &tray, menu.attention, quota)
}

/// Answers a choice in the tray menu; an id that is not one of its items is ignored.
pub fn handle_menu_event(app: &AppHandle, id: &str) {
    if id == OPEN_ITEM_ID {
        show_main_window(app);
    } else if id == QUIT_ITEM_ID {
        request_quit(app);
    } else if let Some(session) = id.strip_prefix(SESSION_ITEM_PREFIX) {
        show_main_window(app);
        let _ = app.emit(SESSION_CHOSEN_EVENT, session);
    }
}
