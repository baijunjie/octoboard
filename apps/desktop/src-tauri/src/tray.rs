//! The menu bar icon (a status item), which keeps the app reachable while its window is closed and
//! the app is out of the Dock (`background.rs`). A left click on the icon brings the window back;
//! a right click opens its menu, which lists the live sessions the UI hands over (`set_tray_menu`),
//! then Open and Quit. Choosing a session brings the window back and tells the UI which one. Quit
//! goes through the same exit flow as Cmd+Q, so it still asks while a session is running.

use serde::Deserialize;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Wry};

use crate::background::show_main_window;
use crate::exit::request_quit;
use crate::menu::APP_NAME;

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

/// The tray menu as the UI words it, already in its language: sections of sessions, each under a
/// heading, then the Open and Quit labels. Text and opaque session ids only — the UI decides what
/// is listed and how it reads, the same way it decides the Dock badge's count.
#[derive(Deserialize)]
pub struct TrayMenu {
    sections: Vec<TraySection>,
    open: String,
    quit: String,
}

#[derive(Deserialize)]
struct TraySection {
    heading: String,
    items: Vec<TrayItem>,
}

/// An item with no `session` is a line that cannot be chosen, such as "3 more…".
#[derive(Deserialize)]
struct TrayItem {
    label: String,
    session: Option<String>,
}

impl Default for TrayMenu {
    /// What the menu reads before the UI has handed over its own: no sessions, English labels.
    fn default() -> Self {
        Self {
            sections: Vec::new(),
            open: format!("Open {APP_NAME}"),
            quit: format!("Quit {APP_NAME}"),
        }
    }
}

/// Puts the icon in the menu bar. It is a template image, so macOS tints it for a light or dark
/// menu bar itself.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray-template.png"))
        .icon_as_template(true)
        .tooltip(APP_NAME)
        .menu(&build_menu(app, &TrayMenu::default())?)
        // The menu belongs to the right click; the left click is the quick way back to the window.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Rebuilds the tray menu from what the UI sent.
#[tauri::command]
pub fn set_tray_menu(app: AppHandle, menu: TrayMenu) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "the menu bar icon does not exist".to_string())?;
    let menu = build_menu(&app, &menu).map_err(|err| err.to_string())?;
    tray.set_menu(Some(menu)).map_err(|err| err.to_string())
}

fn build_menu(app: &AppHandle, menu: &TrayMenu) -> tauri::Result<Menu<Wry>> {
    let mut builder = MenuBuilder::new(app);
    for section in &menu.sections {
        builder = builder.item(
            &MenuItemBuilder::new(&section.heading)
                .enabled(false)
                .build(app)?,
        );
        for item in &section.items {
            let entry = match &item.session {
                Some(session) => {
                    MenuItemBuilder::with_id(format!("{SESSION_ITEM_PREFIX}{session}"), &item.label)
                        .build(app)?
                }
                None => MenuItemBuilder::new(&item.label)
                    .enabled(false)
                    .build(app)?,
            };
            builder = builder.item(&entry);
        }
        builder = builder.separator();
    }
    builder
        .item(&MenuItemBuilder::with_id(OPEN_ITEM_ID, &menu.open).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(QUIT_ITEM_ID, &menu.quit).build(app)?)
        .build()
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
