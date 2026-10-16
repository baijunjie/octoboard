//! The menu bar icon (a status item), which keeps the app reachable while its window is closed and
//! the app is out of the Dock (`background.rs`). A click on the icon, left or right, opens its menu,
//! which lists the live sessions the UI hands over (`set_tray_menu`), then Open and Quit; the window
//! comes back through Open or by choosing a session, which also tells the UI which one. Quit goes
//! through the same exit flow as Cmd+Q, so it still asks while a session is running.
//!
//! Replacing the menu closes it if it is open, so a menu the UI sends while a menu is open is held
//! back and applied once it closes (`MenuTracking`).
//!
//! Muda gives a menu item's picture no template flag, which is what makes a monochrome one follow the
//! menu's light or dark look. So once the menu is on the status item (`NSStatusItem::menu`, reached
//! through `TrayIcon::with_inner_tray_icon`), the pictures flagged template get it set on their
//! `NSMenuItem`s, line by line (`mark_template_images`).

use std::collections::HashMap;
use std::sync::Mutex;

use base64::Engine;
use serde::Deserialize;
use tauri::image::Image;
use tauri::menu::{IconMenuItem, IconMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

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
/// heading, then the Open and Quit labels. Text, opaque session ids and the pictures beside the
/// lines — the UI decides what is listed and how it reads, the same way it decides the Dock badge's
/// count. The pictures are drawn by the UI, which already has every agent's mark and status glyph,
/// so a new agent needs nothing here: `icons` holds each distinct one once, as a base64 PNG under a
/// key the lines name, at twice the size they show at.
#[derive(Deserialize)]
pub struct TrayMenu {
    sections: Vec<TraySection>,
    #[serde(default)]
    icons: HashMap<String, LineIcon>,
    open: String,
    quit: String,
}

/// A picture: a base64 PNG, and whether it is a template image, drawn in black for its alpha alone
/// so macOS tints it to suit the menu.
#[derive(Deserialize)]
struct LineIcon {
    png: String,
    template: bool,
}

#[derive(Deserialize)]
struct TraySection {
    heading: String,
    icon: Option<String>,
    items: Vec<TrayItem>,
}

/// An item with no `session` is a line that cannot be chosen, such as "3 more…".
#[derive(Deserialize)]
struct TrayItem {
    label: String,
    icon: Option<String>,
    session: Option<String>,
}

impl Default for TrayMenu {
    /// What the menu reads before the UI has handed over its own: no sessions, English labels.
    fn default() -> Self {
        Self {
            sections: Vec::new(),
            icons: HashMap::new(),
            open: format!("Open {APP_NAME}"),
            quit: format!("Quit {APP_NAME}"),
        }
    }
}

/// Whether a menu is open, and the latest menu the UI sent while one was. Any menu of the app counts
/// as open, not only this one: telling them apart would take the tray's own `NSMenu` out of the
/// notifications, and holding the tray's menu back while the app menu is open costs nothing anyone
/// can see.
#[derive(Default)]
struct MenuTracking {
    open: bool,
    held: Option<TrayMenu>,
}

impl MenuTracking {
    /// The menu to apply now, or `None` when it is held until the open menu closes. Only the
    /// latest is worth showing, so a menu replaces one already held, and one applied now drops it.
    fn offer(&mut self, menu: TrayMenu) -> Option<TrayMenu> {
        if self.open {
            self.held = Some(menu);
            None
        } else {
            self.held = None;
            Some(menu)
        }
    }

    fn opened(&mut self) {
        self.open = true;
    }

    /// Whether a menu was held back while the menu was open, and is now waiting for `due`.
    fn closed(&mut self) -> bool {
        self.open = false;
        self.held.is_some()
    }

    /// The held menu, unless a menu has opened again since, in which case it waits for that one
    /// to close.
    fn due(&mut self) -> Option<TrayMenu> {
        if self.open {
            None
        } else {
            self.held.take()
        }
    }
}

#[derive(Default)]
pub struct TrayState(Mutex<MenuTracking>);

fn tracking(app: &AppHandle) -> std::sync::MutexGuard<'_, MenuTracking> {
    app.state::<TrayState>()
        .inner()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Puts the icon in the menu bar. It is a template image, so macOS tints it for a light or dark
/// menu bar itself.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray-template.png"))
        .icon_as_template(true)
        .tooltip(APP_NAME)
        .menu(&build_menu(app, &TrayMenu::default())?.0)
        .show_menu_on_left_click(true)
        .build(app)?;
    #[cfg(target_os = "macos")]
    follow_menu_tracking(app);
    Ok(())
}

/// Keeps `MenuTracking` up to date from AppKit's notifications that a menu began and ended
/// tracking, both posted on the main thread.
#[cfg(target_os = "macos")]
fn follow_menu_tracking(app: &AppHandle) {
    use objc2_app_kit::{NSMenuDidBeginTrackingNotification, NSMenuDidEndTrackingNotification};
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue};

    let began = {
        let app = app.clone();
        block2::RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| tracking(&app).opened())
    };
    let ended = {
        let app = app.clone();
        block2::RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| {
            if !tracking(&app).closed() {
                return;
            }
            // Queued on the main queue rather than applied here: the item just chosen, if any, may
            // have its action sent after the menu stops tracking, and replacing the menu drops the
            // items that action belongs to. `run_on_main_thread` would not do, since on the main
            // thread it runs the task at once. The held menu is read when the block runs, so a
            // newer one sent meanwhile, or a menu opened again, is respected.
            let app = app.clone();
            let apply_held = block2::RcBlock::new(move || {
                let due = tracking(&app).due();
                if let Some(menu) = due {
                    let _ = apply(&app, &menu);
                }
            });
            // SAFETY: the main queue runs the block on the main thread, where the menu is built.
            unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&apply_held) };
        })
    };
    // SAFETY: the blocks are `'static`, no queue is given, so they run on the main thread that
    // posts these notifications, and the center keeps the observers and the blocks alive for the
    // life of the process.
    unsafe {
        let center = NSNotificationCenter::defaultCenter();
        center.addObserverForName_object_queue_usingBlock(
            Some(NSMenuDidBeginTrackingNotification),
            None,
            None,
            &began,
        );
        center.addObserverForName_object_queue_usingBlock(
            Some(NSMenuDidEndTrackingNotification),
            None,
            None,
            &ended,
        );
    }
}

/// Sets the template flag on the pictures of the menu on the tray's status item, whose items
/// (separators aside) are the lines `build_menu` made, in order; `templates` says which of them
/// have a template picture. A replaced menu is never marked by mistake, as this runs right after
/// it is set.
#[cfg(target_os = "macos")]
fn mark_template_images(tray: &tauri::tray::TrayIcon, templates: Vec<bool>) {
    use objc2::MainThreadMarker;

    let _ = tray.with_inner_tray_icon(move |inner| {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(menu) = inner.ns_status_item().and_then(|item| item.menu(mtm)) else {
            return;
        };
        let items = menu.itemArray();
        let lines = items.iter().filter(|item| !item.isSeparatorItem());
        for (item, template) in lines.zip(templates) {
            if let Some(image) = item.image().filter(|_| template) {
                image.setTemplate(true);
            }
        }
    });
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
    let (built, templates) = build_menu(app, menu).map_err(|err| err.to_string())?;
    tray.set_menu(Some(built)).map_err(|err| err.to_string())?;
    #[cfg(target_os = "macos")]
    mark_template_images(&tray, templates);
    #[cfg(not(target_os = "macos"))]
    let _ = templates;
    Ok(())
}

/// The picture a line names, or none when it names none or one the table does not hold, or one that
/// does not decode: a line is better shown bare than not at all.
fn icon_of(menu: &TrayMenu, key: Option<&str>) -> Option<Image<'static>> {
    let png = base64::engine::general_purpose::STANDARD
        .decode(&menu.icons.get(key?)?.png)
        .ok()?;
    Image::from_bytes(&png).ok()
}

fn is_template(menu: &TrayMenu, key: Option<&str>) -> bool {
    key.and_then(|key| menu.icons.get(key))
        .is_some_and(|icon| icon.template)
}

/// A label as muda takes it, where a lone `&` marks a mnemonic and would be dropped: "R&D" is
/// written "R&&D" to be shown as it is.
fn literal(label: &str) -> String {
    label.replace('&', "&&")
}

/// A line that cannot be chosen when `id` is `None`, with its picture when it has one.
fn line(
    app: &AppHandle,
    menu: &TrayMenu,
    id: Option<String>,
    label: &str,
    icon: Option<&str>,
) -> tauri::Result<IconMenuItem<Wry>> {
    let mut builder = match id {
        Some(id) => IconMenuItemBuilder::with_id(id, literal(label)),
        None => IconMenuItemBuilder::new(literal(label)).enabled(false),
    };
    if let Some(icon) = icon_of(menu, icon) {
        builder = builder.icon(icon);
    }
    builder.build(app)
}

/// The menu, and for each of its lines in order whether its picture is a template image.
fn build_menu(app: &AppHandle, menu: &TrayMenu) -> tauri::Result<(Menu<Wry>, Vec<bool>)> {
    let mut builder = MenuBuilder::new(app);
    let mut templates = Vec::new();
    for section in &menu.sections {
        let icon = section.icon.as_deref();
        builder = builder.item(&line(app, menu, None, &section.heading, icon)?);
        templates.push(is_template(menu, icon));
        for item in &section.items {
            let icon = item.icon.as_deref();
            let id = item
                .session
                .as_ref()
                .map(|session| format!("{SESSION_ITEM_PREFIX}{session}"));
            builder = builder.item(&line(app, menu, id, &item.label, icon)?);
            templates.push(is_template(menu, icon));
        }
        builder = builder.separator();
    }
    templates.extend([false, false]);
    let built = builder
        .item(&MenuItemBuilder::with_id(OPEN_ITEM_ID, literal(&menu.open)).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(QUIT_ITEM_ID, literal(&menu.quit)).build(app)?)
        .build()?;
    Ok((built, templates))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(open: &str) -> TrayMenu {
        TrayMenu {
            open: open.to_string(),
            ..TrayMenu::default()
        }
    }

    #[test]
    fn a_menu_sent_while_none_is_open_applies_at_once() {
        let mut tracking = MenuTracking::default();
        assert_eq!(tracking.offer(menu("a")).map(|m| m.open), Some("a".into()));
        assert!(!tracking.closed());
        assert!(tracking.due().is_none());
    }

    #[test]
    fn menus_sent_while_one_is_open_wait_and_only_the_latest_applies() {
        let mut tracking = MenuTracking::default();
        tracking.opened();
        assert!(tracking.offer(menu("a")).is_none());
        assert!(tracking.offer(menu("b")).is_none());
        assert!(tracking.closed());
        assert_eq!(tracking.due().map(|m| m.open), Some("b".into()));
        assert!(tracking.due().is_none());
    }

    #[test]
    fn a_held_menu_yields_to_a_newer_one_and_to_a_menu_opened_again() {
        let mut tracking = MenuTracking::default();
        tracking.opened();
        tracking.offer(menu("a"));
        assert!(tracking.closed());
        assert!(tracking.offer(menu("b")).is_some());
        assert!(tracking.due().is_none());

        tracking.opened();
        tracking.offer(menu("c"));
        tracking.closed();
        tracking.opened();
        assert!(tracking.due().is_none());
        tracking.closed();
        assert_eq!(tracking.due().map(|m| m.open), Some("c".into()));
    }
}
