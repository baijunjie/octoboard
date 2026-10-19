//! The menu bar icon (a status item), which keeps the app reachable while its window is closed and
//! the app is out of the Dock (`background.rs`). A click on the icon, left or right, opens its menu,
//! which lists the live sessions the UI hands over (`set_tray_menu`) and its submenus (the agents'
//! quotas), then Open and Quit; the window comes back through Open or by choosing a session, which
//! also tells the UI which one. Quit goes through the same exit flow as Cmd+Q, so it still asks
//! while a session is running.
//!
//! The icon itself has two looks, both template images: the octopus, and the octopus with a
//! raised hand in its corner, which shows while the menu's `attention` flag is set (a session is
//! waiting for the user). Like the rest of the menu, the UI decides when, and the look changes
//! together with the menu it came with.
//!
//! Replacing the menu closes it if it is open, so a menu the UI sends while a menu is open is held
//! back and applied once it closes (`MenuTracking`). The UI is told when this menu begins to open
//! (`MENU_OPENED_EVENT`) so it can ask for fresh quotas, which reach the menu on its next update.
//!
//! Muda gives a menu item's picture no template flag, which is what makes a monochrome one follow
//! the menu's light or dark look. So once the menu is on the status item (`NSStatusItem::menu`,
//! reached through `TrayIcon::with_inner_tray_icon`), the pictures flagged template get it set on
//! their `NSMenuItem`s, line by line (`finish_menu`). The same pass gives the lines of each
//! submenu their rows (`tray_lines.rs`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use base64::Engine;
use serde::Deserialize;
use tauri::image::Image;
use tauri::menu::{
    IconMenuItem, IconMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder,
};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::background::show_main_window;
use crate::exit::request_quit;
use crate::menu::APP_NAME;
#[cfg(target_os = "macos")]
use crate::tray_lines::dress_submenu;

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

/// The tray menu as the UI words it, already in its language: sections of sessions, each under a
/// heading, submenus of rows (headers, quota windows, notes) that cannot be chosen, then the Open
/// and Quit labels, and whether the icon should raise its hand. Text, opaque session ids and the
/// pictures beside the lines — the UI decides what is listed and how it reads, the same way it
/// decides the Dock badge's count. The pictures are drawn by the UI, which already has every
/// agent's mark and status glyph, so a new agent needs nothing here for its pictures: `icons` holds
/// each distinct one once, as a base64 PNG under a key the lines name, at twice the size they show
/// at. Only a quota bar's colour is the shell's, and an agent it does not know gets the text colour
/// (`AgentKind::Other`).
#[derive(Deserialize)]
pub struct TrayMenu {
    sections: Vec<TraySection>,
    #[serde(default)]
    submenus: Vec<TraySubmenu>,
    #[serde(default)]
    icons: HashMap<String, LineIcon>,
    open: String,
    quit: String,
    /// Whether the icon shows the raised hand, which asks for the user's attention.
    #[serde(default)]
    attention: bool,
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

/// A submenu, opened from a line of the menu that shows its title.
#[derive(Clone, Deserialize)]
pub struct TraySubmenu {
    title: String,
    pub items: Vec<SubmenuItem>,
}

/// An item with no `session` is a line that cannot be chosen, such as "3 more…".
#[derive(Deserialize)]
struct TrayItem {
    label: String,
    icon: Option<String>,
    session: Option<String>,
}

/// The agent a window belongs to, which decides the colour of its bar; the UI's `Agent`.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
    Grok,
    /// An agent added to the UI before the shell knows its colour.
    #[serde(other)]
    Other,
}

/// What a submenu holds, none of it choosable: an account's header, a quota window's row with
/// the bar the shell draws (`fraction` of it filled, in the colour of `agent`), a note, and the
/// rules between accounts. `label` is a window's whole line as one phrase, for assistive
/// technology.
#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SubmenuItem {
    Header {
        title: String,
    },
    Window {
        name: String,
        detail: String,
        label: String,
        fraction: f64,
        agent: AgentKind,
    },
    Note {
        text: String,
    },
    Separator,
}

impl SubmenuItem {
    /// The text the item reads as in a menu that cannot draw it as a row: the title the shell
    /// first gives its line, which is also what stays where a row is not made.
    fn text(&self) -> Option<&str> {
        match self {
            Self::Header { title } => Some(title),
            Self::Window { label, .. } => Some(label),
            Self::Note { text } => Some(text),
            Self::Separator => None,
        }
    }
}

impl Default for TrayMenu {
    /// What the menu reads before the UI has handed over its own: no sessions, English labels.
    fn default() -> Self {
        Self {
            sections: Vec::new(),
            submenus: Vec::new(),
            icons: HashMap::new(),
            open: format!("Open {APP_NAME}"),
            quit: format!("Quit {APP_NAME}"),
            attention: false,
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

/// The menu tracking, and whether the icon now shows the raised hand, so that the picture is
/// replaced only when that changes.
#[derive(Default)]
pub struct TrayState {
    tracking: Mutex<MenuTracking>,
    attention_shown: AtomicBool,
}

fn tracking(app: &AppHandle) -> std::sync::MutexGuard<'_, MenuTracking> {
    app.state::<TrayState>()
        .inner()
        .tracking
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// The icon's picture for whether it raises its hand. Both are template images, drawn in black for
/// their alpha alone.
fn icon_image(attention: bool) -> Image<'static> {
    if attention {
        tauri::include_image!("icons/tray-attention-template.png")
    } else {
        tauri::include_image!("icons/tray-template.png")
    }
}

/// Puts the icon in the menu bar. It is a template image, so macOS tints it for a light or dark
/// menu bar itself.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon_image(false))
        .icon_as_template(true)
        .tooltip(APP_NAME)
        .menu(&build_menu(app, &TrayMenu::default())?)
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
        block2::RcBlock::new(move |notification: std::ptr::NonNull<NSNotification>| {
            tracking(&app).opened();
            // SAFETY: the notification is alive for the duration of its delivery.
            if is_tray_menu(&app, unsafe { notification.as_ref() }) {
                let _ = app.emit(MENU_OPENED_EVENT, ());
            }
        })
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

/// Whether `notification` is about the menu on the tray's status item rather than another menu of
/// the app.
#[cfg(target_os = "macos")]
fn is_tray_menu(app: &AppHandle, notification: &objc2_foundation::NSNotification) -> bool {
    use objc2::{rc::Retained, MainThreadMarker};

    // The address stands for the object, as the closure has to be `Send`.
    let Some(notified) = notification
        .object()
        .map(|object| Retained::as_ptr(&object) as usize)
    else {
        return false;
    };
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return false;
    };
    tray.with_inner_tray_icon(move |inner| {
        let menu = MainThreadMarker::new()
            .and_then(|mtm| inner.ns_status_item().and_then(|item| item.menu(mtm)));
        menu.is_some_and(|menu| Retained::as_ptr(&menu) as usize == notified)
    })
    .unwrap_or(false)
}

/// Finishes the menu on the tray's status item after muda has built it: sets the template flag on
/// the pictures of its lines that have it, whose items (separators aside) are the lines
/// `build_menu` made, in the order `template_order` gives; then dresses each submenu's lines as
/// the rows they stand for (`dress_submenu`). A replaced menu is never touched by mistake, as this
/// runs right after it is set.
#[cfg(target_os = "macos")]
fn finish_menu(tray: &tauri::tray::TrayIcon, templates: Vec<bool>, submenus: Vec<TraySubmenu>) {
    use objc2::MainThreadMarker;

    let _ = tray.with_inner_tray_icon(move |inner| {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(menu) = inner.ns_status_item().and_then(|item| item.menu(mtm)) else {
            return;
        };
        let items = menu.itemArray();
        let lines: Vec<_> = items
            .iter()
            .filter(|item| !item.isSeparatorItem())
            .collect();
        for (item, template) in lines.iter().zip(templates) {
            if let Some(image) = item.image().filter(|_| template) {
                image.setTemplate(true);
            }
        }
        let opened = lines.iter().filter_map(|item| item.submenu());
        for (submenu, model) in opened.zip(&submenus) {
            dress_submenu(&submenu, &model.items, mtm);
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
    let built = build_menu(app, menu).map_err(|err| err.to_string())?;
    tray.set_menu(Some(built)).map_err(|err| err.to_string())?;
    #[cfg(target_os = "macos")]
    finish_menu(&tray, template_order(menu), menu.submenus.clone());
    show_attention(app, &tray, menu.attention)
}

/// Swaps the icon's picture when `attention` differs from the one shown, keeping it a template
/// image, which a plain `set_icon` would drop.
fn show_attention(
    app: &AppHandle,
    tray: &tauri::tray::TrayIcon,
    attention: bool,
) -> Result<(), String> {
    let shown = &app.state::<TrayState>().inner().attention_shown;
    if shown.load(Ordering::Relaxed) == attention {
        return Ok(());
    }
    tray.set_icon_with_as_template(Some(icon_image(attention)), true)
        .map_err(|err| err.to_string())?;
    shown.store(attention, Ordering::Relaxed);
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

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
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

/// For each line of the menu `build_menu` makes, in order, whether its picture is a template
/// image: the sessions' lines, then each submenu's opening line (which has no picture), then Open
/// and Quit. Separators are not lines, and a submenu's own lines have no pictures.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn template_order(menu: &TrayMenu) -> Vec<bool> {
    let mut templates = Vec::new();
    for section in &menu.sections {
        templates.push(is_template(menu, section.icon.as_deref()));
        templates.extend(
            section
                .items
                .iter()
                .map(|item| is_template(menu, item.icon.as_deref())),
        );
    }
    templates.extend(menu.submenus.iter().map(|_| false));
    templates.extend([false, false]);
    templates
}

/// The menu.
fn build_menu(app: &AppHandle, menu: &TrayMenu) -> tauri::Result<Menu<Wry>> {
    let mut builder = MenuBuilder::new(app);
    for section in &menu.sections {
        let icon = section.icon.as_deref();
        builder = builder.item(&line(app, menu, None, &section.heading, icon)?);
        for item in &section.items {
            let icon = item.icon.as_deref();
            let id = item
                .session
                .as_ref()
                .map(|session| format!("{SESSION_ITEM_PREFIX}{session}"));
            builder = builder.item(&line(app, menu, id, &item.label, icon)?);
        }
        builder = builder.separator();
    }
    for submenu in &menu.submenus {
        let mut lines = SubmenuBuilder::new(app, literal(&submenu.title));
        // Plain disabled items, which `dress_submenu` replaces or gives a view once the menu is
        // built; they are what a line reads as where that is not done.
        for item in &submenu.items {
            lines = match item.text() {
                Some(text) => lines.item(&line(app, menu, None, text, None)?),
                None => lines.separator(),
            };
        }
        builder = builder.item(&lines.build()?);
    }
    if !menu.submenus.is_empty() {
        builder = builder.separator();
    }
    builder
        .item(&MenuItemBuilder::with_id(OPEN_ITEM_ID, literal(&menu.open)).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(QUIT_ITEM_ID, literal(&menu.quit)).build(app)?)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(json: &str) -> TrayMenu {
        serde_json::from_str(json).expect("a menu")
    }

    fn menu(open: &str) -> TrayMenu {
        TrayMenu {
            open: open.to_string(),
            ..TrayMenu::default()
        }
    }

    #[test]
    fn a_menu_asks_for_attention_only_when_the_ui_says_so() {
        let bare = parsed(r#"{"sections": [], "open": "Open", "quit": "Quit"}"#);
        assert!(!bare.attention);
        let raised =
            parsed(r#"{"sections": [], "open": "Open", "quit": "Quit", "attention": true}"#);
        assert!(raised.attention);
        assert!(!TrayMenu::default().attention);
    }

    #[test]
    fn both_looks_of_the_icon_are_decodable_template_pictures() {
        for attention in [false, true] {
            let image = icon_image(attention);
            assert_eq!((image.width(), image.height()), (44, 44));
            assert!(image.rgba().chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
        }
        assert_ne!(icon_image(true).rgba(), icon_image(false).rgba());
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

    const WITH_SUBMENUS: &str = r#"{
        "sections": [
            {"heading": "Working (1)", "icon": "status", "items": [
                {"label": "Fix", "icon": "codex", "session": "s1"},
                {"label": "1 more…"}
            ]}
        ],
        "submenus": [
            {"title": "Codex quota", "items": [
                {"kind": "header", "title": "Default · Plus"},
                {"kind": "window", "name": "5-hour", "detail": "82% remaining · resets in 1h",
                 "label": "5-hour 82% remaining · resets in 1h", "fraction": 0.82, "agent": "codex"},
                {"kind": "separator"},
                {"kind": "header", "title": "Work"},
                {"kind": "note", "text": "Quota unavailable"}
            ]},
            {"title": "Grok quota", "items": [{"kind": "note", "text": "Reading quota…"}]}
        ],
        "icons": {
            "status": {"png": "", "template": false},
            "codex": {"png": "", "template": true}
        },
        "open": "Open",
        "quit": "Quit"
    }"#;

    #[test]
    fn a_menu_without_submenus_reads_and_has_only_the_open_and_quit_lines_to_mark() {
        let menu = parsed(r#"{"sections": [], "open": "Open", "quit": "Quit"}"#);
        assert!(menu.submenus.is_empty());
        assert_eq!(template_order(&menu), vec![false, false]);
    }

    #[test]
    fn a_line_reads_as_its_whole_phrase_where_it_is_not_drawn_as_a_row() {
        let menu = parsed(WITH_SUBMENUS);
        let texts: Vec<_> = menu.submenus[0]
            .items
            .iter()
            .map(SubmenuItem::text)
            .collect();
        assert_eq!(
            texts,
            vec![
                Some("Default · Plus"),
                Some("5-hour 82% remaining · resets in 1h"),
                None,
                Some("Work"),
                Some("Quota unavailable"),
            ]
        );
    }

    #[test]
    fn template_flags_follow_the_lines_in_menu_order() {
        // The sessions' heading and two lines, then the two submenus' opening lines, then Open
        // and Quit; a submenu's own lines are rows without pictures.
        assert_eq!(
            template_order(&parsed(WITH_SUBMENUS)),
            vec![
                false, true, false, // heading, session, "1 more…"
                false, false, // Codex and Grok submenus
                false, false, // Open, Quit
            ]
        );
    }
}
