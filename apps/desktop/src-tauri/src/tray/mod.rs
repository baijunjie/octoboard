//! The menu bar icon (a status item), which keeps the app reachable while its windows are closed
//! and the app is out of the Dock (`background.rs`). There is one for the whole application, and
//! its menu has a tab for each daemon it shows (`tabs.rs`): the local daemon's ("This Mac", "This
//! PC") first, then each connected remote daemon's in the order it was connected in, whether shown
//! in a window or held for the browser. Each daemon's window's UI hands over what its tab lists
//! (`set_tray_menu`): its live sessions under their sections' headings and its agents' quota
//! submenus. A tab ends with Open board, which brings that daemon's window forward, and for a
//! remote daemon Disconnect, which closes its window and with it its connection
//! (`background::disconnect_window`); a daemon held for the browser has no window to list its
//! sessions, and offers opening it there instead. Below the tabs come the application's lines, the
//! same whichever tab is chosen: Check for updates, which brings the main window back at Settings'
//! Updates section, and, after a separator, Quit completely, which goes through the same exit flow
//! as Cmd+Q. Connecting to a remote daemon is not offered here: that is the Remote app menu's, the
//! Dock menu's, the Windows system menu's and the jump list's.
//!
//! On macOS the tabs are a row at the top of the menu (`macos/tabs.rs`), every tab's lines are in
//! the menu and those of the tabs not chosen are hidden, so choosing a tab shows its lines while
//! the menu stays open. The tab chosen as the menu opens is the daemon of the frontmost window of
//! the application, else the local daemon. On Windows the icon opens the application's own flyout
//! (`flyout.rs`) with the same tabs; only a development build on Linux has a popup menu, which can
//! hold no tabs, the local daemon's lines at the top and each remote daemon's in a submenu named
//! after it (`tabs::Layout`).
//!
//! The icon has two attention looks: the octopus, and the octopus with an orange dot in its corner,
//! which shows while any daemon has a session waiting for the user. Its eyes turn red while the
//! computer is kept awake (`set_keep_awake_shown`): on macOS the eye sockets keep their shape and
//! go from translucent to solid red, and only on Windows does the picture draw larger eyes, so that
//! reads by shape and not only by colour. A readable selected-account quota adds a capsule over the
//! lower tentacles (`icon.rs`), with a thinner one beneath for the weekly window when the account
//! has a 5-hour one too; the UI supplies the remaining percents and the localized label, which is
//! the icon's accessible name. The quota and the eyes follow the window the user focused last,
//! whichever daemon it shows, and keep it while every window is hidden (`status_item::look_of`).
//! The octopus and capsule are always one template image, so macOS keeps drawing them for the menu
//! bar's light or dark look; a template image cannot carry a colour, so the dot and the eyes are
//! views of their own laid over the button, in place of the octopus's cut-out corner and eyes
//! (`show_layers`). On Windows the icon is the app icon in its own colours, the dot and the larger
//! eyes are part of the picture, a quota is not drawn, and the label is the icon's tooltip, which
//! names the daemon whose state it shows.
//!
//! The user can hide the icon from Settings (`set_icon_visible`, reached through
//! `set_app_settings`, in the local daemon's window only). The choice is kept in `settings.json`
//! (`app_settings`) and read when the icon is created, which hides it before the first frame is
//! drawn, so a hidden icon never flashes. While it is hidden its menu and its look are still kept
//! up to date, so it comes back as it should be.
//!
//! Replacing the menu closes it if it is open, so a change that comes while a menu is open is kept
//! and the menu is built again once it closes (`MenuTracking`), which macOS's menu tracking
//! (`follow_menu_tracking`) and the flyout's opening and closing report; choosing a tab is the one
//! change shown while the menu is open. The Linux popup menu reports nothing, so it is replaced as
//! the change comes. Every daemon's window is told when the menu begins to open
//! (`MENU_OPENED_EVENT`: macOS's menu tracking, the click that opens the flyout or the popup menu
//! elsewhere) so it can ask for fresh quotas, which reach the menu on its next update.
//!
//! Muda gives a menu item's picture no template flag, which is what makes a monochrome one follow
//! the menu's light or dark look. So the pictures flagged template get it set on their
//! `NSMenuItem`s, line by line (`finish_menu`). Tauri does not hand out a menu's `NSMenu`, and
//! tray-icon puts it on the status item (`NSStatusItem::menu`) only while it is open, so the
//! pass waits for the menu to begin tracking, whose notification carries it, and runs once for
//! each menu built (`macos/finish.rs`). The same pass gives the row of tabs its control, the
//! headings of the sessions' sections and the lines of each submenu their rows (`lines.rs`), as a
//! line that cannot be chosen is drawn in a grey that nothing but a view of its own can darken.
//!
//! All of that native work is the platform module's (`macos/`, `windows.rs`); this module and the
//! files beside it hold what every platform shares: the model, the tabs and the lines, the plain
//! items, the icon's pictures and when a menu is held back.

// The quota bars, the attention dot (`icon::dot_png`), the row of tabs and the submenus' rows are
// drawn by macOS-only code; elsewhere the menu the UI sends is still parsed whole, so those parts
// are read by nothing. The allow reaches dead items, not unused imports: the imports only macOS
// code uses are in `macos/`.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::app_settings;
use crate::background::{disconnect_window, show_main_window, show_window};
use crate::daemon_window;
use crate::diagnostics::diagnostic;
use crate::exit::request_quit;
use crate::menu::APP_NAME;
use crate::MAIN_WINDOW_LABEL;

mod finish;
pub mod flyout;
mod flyout_model;
mod icon;
mod items;
mod menu_tracking;
mod model;
mod names;
mod status_item;
mod tabs;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

use finish::finish_order;
use items::build_menu;
use menu_tracking::{tracking, MenuTracking};
use model::TrayMenu;
use platform::{
    customize, finish_when_opened, follow_menu_tracking, picture, set_icon_label, set_visible,
    Unfinished, FLYOUT, LAYOUT, TEMPLATE,
};
use status_item::{look_of, show_look, Shown};
use tabs::{compose, tab_of, Contents, Tab};

/// The icon's tray id.
const TRAY_ID: &str = "main";

/// What every menu item of a daemon's lines starts with, followed by the key of its tab (its
/// window's label), a colon and what the item is (`ItemId`). The application's Updates and Quit
/// are named after the local daemon's tab.
const ITEM_PREFIX: &str = "tray:";
const OPEN_ITEM: &str = "open";
const UPDATES_ITEM: &str = "updates";
/// The Quit item, which goes down the same exit flow as the app menu's.
const QUIT_ITEM: &str = "quit";
/// The Disconnect item, which ends the connection to a remote daemon.
const DISCONNECT_ITEM: &str = "disconnect";
/// A session item is this followed by the session's id, which is what the UI gets back in
/// `SESSION_CHOSEN_EVENT`.
const SESSION_ITEM: &str = "session:";

/// What the shell emits to a daemon's window, with the session's id as its payload, when one of
/// its sessions is chosen in the menu; the platform adapter's `tauri.ts` listens for it.
const SESSION_CHOSEN_EVENT: &str = "tray-session-chosen";

/// What the shell emits to every daemon's window, with no payload, when the menu begins to open;
/// the platform adapter's `tauri.ts` listens for it.
const MENU_OPENED_EVENT: &str = "tray-menu-opened";

/// What the menu and the icon are made from, and what is left to finish of the menu last set.
#[derive(Default)]
pub struct TrayState {
    tracking: Mutex<MenuTracking>,
    /// The menu each daemon's window's UI last sent, by the window's label.
    menus: Mutex<HashMap<String, TrayMenu>>,
    /// Whether each daemon's window's daemon keeps its computer awake, by the window's label.
    awake: Mutex<HashMap<String, bool>>,
    /// The tabs of the menu last built.
    tabs: Mutex<Vec<Tab>>,
    shown: Shown,
    /// The flyout's model and whether it is open (Windows).
    flyout: Mutex<flyout::FlyoutState>,
    /// What `finish_menu` is to do to the menu last set, until it first opens (macOS); nothing
    /// elsewhere.
    unfinished: Unfinished,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Puts the icon in the menu bar. Its picture and whether it is a template image, which macOS
/// tints for a light or dark menu bar itself, are the platform's.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let tray = customize(
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(picture(false, false, None))
            .icon_as_template(TEMPLATE)
            .show_menu_on_left_click(true),
    )
    .build(app)?;
    set_icon_label(&tray, APP_NAME);
    if !app_settings::current(app).show_menu_bar_icon {
        if let Err(err) = set_visible(&tray, false) {
            diagnostic!("cannot hide the menu bar icon: {err}");
        }
    }
    build(app);
    follow_menu_tracking(app);
    Ok(())
}

/// Builds the menu again from what it is made from now, or once the open menu closes. On the main
/// thread.
fn rebuild(app: &AppHandle) {
    if tracking(app).rebuild_now() {
        build(app);
    }
}

/// The window the menu is about as it opens: the frontmost window of the application, else the
/// main window.
fn frontmost(app: &AppHandle) -> Option<String> {
    daemon_window::menu_target(app).map(|window| window.label().to_owned())
}

/// The tab chosen as the menu opens: the frontmost window's daemon, else the local daemon.
pub(super) fn chosen_on_open(app: &AppHandle) -> usize {
    let tabs = lock(&app.state::<TrayState>().tabs).clone();
    tab_of(&tabs, frontmost(app).as_deref())
}

/// Builds the menu and sets it on the icon. On the main thread.
fn build(app: &AppHandle) {
    let state = app.state::<TrayState>();
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let tabs = tabs::tabs(&crate::remote::connected_daemons(app));
    let menus = lock(&state.menus).clone();
    let chosen = tab_of(&tabs, frontmost(app).as_deref());
    let nodes = compose(
        &Contents {
            tabs: &tabs,
            menus: &menus,
            chosen,
        },
        LAYOUT,
    );
    let local = menus.get(MAIN_WINDOW_LABEL).cloned().unwrap_or_default();
    if FLYOUT {
        let model = flyout_model::flyout_model(&nodes, &tabs, &menus, &local.app, chosen);
        flyout::set_model(app, model);
        *lock(&state.tabs) = tabs;
        update_look(app);
        return;
    }
    let menu_of = |index: usize| tabs.get(index).and_then(|tab| menus.get(&tab.key));
    match build_menu(app, &nodes, &menu_of) {
        Ok(menu) => {
            if let Err(err) = tray.set_menu(Some(menu)) {
                diagnostic!("cannot set the menu bar icon's menu: {err}");
            }
        }
        Err(err) => diagnostic!("cannot build the menu bar icon's menu: {err}"),
    }
    finish_when_opened(
        &state,
        finish_order(&nodes, &menu_of),
        &tabs,
        &menus,
        &local.app,
    );
    *lock(&state.tabs) = tabs;
    update_look(app);
}

/// Puts the look the icon is to have now on it (`status_item::look_of`): its quota and eyes those
/// of the window the user focused last, else the main window's.
fn update_look(app: &AppHandle) {
    let state = app.state::<TrayState>();
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let focus = daemon_window::last_focused_or_main(app)
        .map_or_else(|| MAIN_WINDOW_LABEL.to_owned(), |w| w.label().to_owned());
    let tabs = lock(&state.tabs).clone();
    let menus = lock(&state.menus).clone();
    let awake = lock(&state.awake).clone();
    let labels = menus
        .get(MAIN_WINDOW_LABEL)
        .map(|menu| menu.app.clone())
        .unwrap_or_default();
    // With more than one daemon shown, the icon's name says whose state it shows.
    let name = (tabs.len() > 1).then(|| {
        let tab = &tabs[tab_of(&tabs, Some(&focus))];
        tab.remote_name()
            .map_or_else(|| labels.this_computer.clone(), str::to_owned)
    });
    let look = look_of(&menus, &awake, &focus, name.as_deref());
    if let Err(err) = show_look(&state.shown, &tray, look) {
        diagnostic!("cannot show the menu bar icon's look: {err}");
    }
}

/// Keeps what the UI of the asking window sent as its daemon's lines, and builds the menu again
/// with them, or once the open menu closes.
#[tauri::command]
pub fn set_tray_menu(
    app: AppHandle,
    webview_window: tauri::WebviewWindow,
    menu: TrayMenu,
) -> Result<(), String> {
    lock(&app.state::<TrayState>().menus).insert(webview_window.label().to_owned(), menu);
    rebuild(&app);
    Ok(())
}

/// Keeps whether the daemon of the asking window is now keeping its computer awake (Prevent sleep,
/// which the daemon holds itself: the UI hands over the `keep_awake_held` it reports), which the
/// icon shows at once while that window is the one it follows, whether or not a menu is open.
#[tauri::command]
pub fn set_keep_awake_shown(
    app: AppHandle,
    webview_window: tauri::WebviewWindow,
    held: bool,
) -> Result<(), String> {
    lock(&app.state::<TrayState>().awake).insert(webview_window.label().to_owned(), held);
    update_look(&app);
    Ok(())
}

/// The connections to the remote daemons changed: the menu lists their tabs again. On any thread.
pub fn daemons_changed(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || rebuild(&handle));
}

/// The window labelled `label` closed: its daemon's lines and hold go with it. On the main thread.
pub fn window_closed(app: &AppHandle, label: &str) {
    let state = app.state::<TrayState>();
    let had_menu = lock(&state.menus).remove(label).is_some();
    let had_hold = lock(&state.awake).remove(label).is_some();
    if had_menu || had_hold {
        rebuild(app);
    } else {
        update_look(app);
    }
}

/// The user focused another window: the icon follows its daemon's quota and hold.
pub fn focus_changed(app: &AppHandle) {
    update_look(app);
}

/// Shows or hides the icon, which is the Settings switch (`create` applies the stored choice at
/// launch). Hiding changes nothing else: the menu and the look are kept and are there again when
/// the icon is shown.
pub fn set_icon_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
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

/// A menu item's id: the tab it is in (a window's label, or the key of a connection held for the
/// browser), and what it is.
#[derive(Debug, PartialEq, Eq)]
struct ItemId<'a> {
    window: &'a str,
    item: Item<'a>,
}

#[derive(Debug, PartialEq, Eq)]
enum Item<'a> {
    Open,
    Updates,
    Quit,
    Disconnect,
    Session(&'a str),
}

impl<'a> ItemId<'a> {
    fn format(window: &str, item: Item<'_>) -> String {
        let item = match item {
            Item::Open => OPEN_ITEM.to_owned(),
            Item::Updates => UPDATES_ITEM.to_owned(),
            Item::Quit => QUIT_ITEM.to_owned(),
            Item::Disconnect => DISCONNECT_ITEM.to_owned(),
            Item::Session(session) => format!("{SESSION_ITEM}{session}"),
        };
        format!("{ITEM_PREFIX}{window}:{item}")
    }

    /// The item `id` names, or `None` for an id that is not one of the menu's own items. A tab's
    /// key holds no colon, so the first one ends it; a session's id may hold any.
    fn parse(id: &'a str) -> Option<Self> {
        let (window, item) = id.strip_prefix(ITEM_PREFIX)?.split_once(':')?;
        let item = match item {
            OPEN_ITEM => Item::Open,
            UPDATES_ITEM => Item::Updates,
            QUIT_ITEM => Item::Quit,
            DISCONNECT_ITEM => Item::Disconnect,
            other => Item::Session(other.strip_prefix(SESSION_ITEM)?),
        };
        Some(Self { window, item })
    }
}

/// Answers a choice in the menu; an id that is not one of its own items is ignored (the line that
/// opens a daemon held for the browser has the Remote menu's id, which `remote::handle_menu_item`
/// answers before this). Open and
/// a session bring that daemon's window forward, a session telling it which one; Updates is the
/// application's and goes to the main window; Quit quits the application; Disconnect closes a
/// remote daemon's window, which ends its connection, or ends the connection held for the browser.
pub fn handle_menu_event(app: &AppHandle, id: &str) {
    let Some(ItemId { window, item }) = ItemId::parse(id) else {
        return;
    };
    match item {
        Item::Updates => {
            show_main_window(app);
            let _ = app.emit_to(MAIN_WINDOW_LABEL, "tray-updates-requested", ());
        }
        Item::Quit => request_quit(app),
        Item::Disconnect => disconnect(app, window),
        Item::Open => {
            show_tab_window(app, window);
        }
        Item::Session(session) => {
            if show_tab_window(app, window) {
                let _ = app.emit_to(window, SESSION_CHOSEN_EVENT, session);
            }
        }
    }
}

/// Ends the connection of the tab `key`: a window's by closing the window, one held for the
/// browser by letting it go.
fn disconnect(app: &AppHandle, key: &str) {
    let held = crate::remote::connected_daemons(app)
        .into_iter()
        .find(|daemon| daemon.key == key && daemon.in_browser);
    match held {
        Some(daemon) => crate::remote::remote_daemon_disconnect(app.clone(), daemon.entry_id),
        None => disconnect_window(app, key),
    }
}

/// Brings the window labelled `label` forward, and answers whether it exists.
fn show_tab_window(app: &AppHandle, label: &str) -> bool {
    match app.get_webview_window(label) {
        Some(window) => {
            show_window(app, &window);
            true
        }
        None => false,
    }
}

/// Tells every daemon's window's UI that the menu begins to open.
fn menu_opened(app: &AppHandle) {
    let labels: Vec<String> = lock(&app.state::<TrayState>().menus)
        .keys()
        .cloned()
        .collect();
    for label in labels {
        let _ = app.emit_to(label.as_str(), MENU_OPENED_EVENT, ());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_id_names_its_window_and_reads_back() {
        for (window, item) in [
            ("main", Item::Open),
            ("daemon-2", Item::Updates),
            ("daemon-2", Item::Quit),
            ("daemon-2", Item::Disconnect),
            ("daemon-12", Item::Session("a:b")),
        ] {
            let id = ItemId::format(window, item);
            let parsed = ItemId::parse(&id).expect("an item of an icon");
            assert_eq!(parsed.window, window);
        }
        assert_eq!(ItemId::format("main", Item::Open), "tray:main:open");
        assert_eq!(
            ItemId::parse("tray:daemon-1:session:s1:x"),
            Some(ItemId {
                window: "daemon-1",
                item: Item::Session("s1:x")
            })
        );
    }

    #[test]
    fn ids_that_are_not_an_icon_s_items_are_ignored() {
        for id in [
            "quit",
            "settings",
            "tray:main",
            "tray:main:other",
            "tray-open",
        ] {
            assert_eq!(ItemId::parse(id), None, "{id}");
        }
    }
}
