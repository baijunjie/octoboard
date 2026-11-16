//! The app's native macOS menu bar, and the Dock icon's menu (`dock`).

use std::collections::HashMap;
use std::sync::Mutex;

use muda::accelerator::Accelerator;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::AppHandle;

use super::{ABOUT_ITEM_ID, APP_NAME, SETTINGS_ITEM_ID};

/// The Settings item's accelerator until the UI sends the one the user has chosen.
const DEFAULT_SETTINGS_ACCELERATOR: &str = "CmdOrCtrl+,";

/// The labels and the Settings accelerator the menu was last built with, so that a change of the
/// remote daemons (`remote_daemons_changed`) rebuilds it in the same language.
static LAST: Mutex<Option<(HashMap<String, String>, String)>> = Mutex::new(None);

/// The menu's labels, keyed by the UI's `menu.*` message keys (`packages/ui/src/i18n/messages/en.ts`),
/// which the UI hands over in its current language through `set_menu_labels`. A key the UI did not
/// send keeps its English default, so the menu built at startup, before the UI has loaded, reads
/// the same as one built from the English catalog.
#[derive(Clone, Default)]
pub struct Labels(HashMap<String, String>);

impl Labels {
    pub(super) fn get<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.0.get(key).map_or(default, String::as_str)
    }
}

/// Sets the menu bar the app starts with, before the UI has sent its labels, and the Dock menu.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    app.set_menu(build_menu(
        app,
        &Labels::default(),
        DEFAULT_SETTINGS_ACCELERATOR,
    )?)?;
    super::dock::install(app);
    super::dock::rebuild(&Labels::default(), &crate::remote::menu_entries(app));
    Ok(())
}

/// Nothing to add to a window: the remote daemons are in the app's own Remote menu and the Dock's.
pub fn install_window(_window: &tauri::WebviewWindow) {}

/// Rebuilds the menu bar and the Dock menu, which list the saved remote daemons, in the language
/// they were last built in. The menu bar icon's menu lists the connected daemons alone, and is not
/// rebuilt for the saved ones. On any thread.
pub fn remote_daemons_changed(app: &AppHandle) {
    let (labels, accelerator) = LAST
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| (HashMap::new(), DEFAULT_SETTINGS_ACCELERATOR.to_owned()));
    let rebuilding = app.clone();
    let _ = app.run_on_main_thread(move || {
        let labels = Labels(labels);
        match build_menu(&rebuilding, &labels, &accelerator) {
            Ok(menu) => {
                let _ = rebuilding.set_menu(menu);
            }
            Err(err) => crate::diagnostics::diagnostic!("cannot rebuild the menu bar: {err}"),
        }
        super::dock::rebuild(&labels, &crate::remote::menu_entries(&rebuilding));
    });
}

/// See `set_menu_labels`.
pub fn set_labels(
    app: &AppHandle,
    labels: HashMap<String, String>,
    settings_accelerator: Option<String>,
) -> Result<(), String> {
    let accelerator =
        settings_accelerator.unwrap_or_else(|| DEFAULT_SETTINGS_ACCELERATOR.to_owned());
    let labels = Labels(labels);
    let menu = build_menu(app, &labels, &accelerator).map_err(|err| err.to_string())?;
    app.set_menu(menu).map_err(|err| err.to_string())?;
    super::dock::rebuild(&labels, &crate::remote::menu_entries(app));
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((labels.0, accelerator));
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

    let remote_menu = remote_menu(app, labels, &crate::remote::menu_entries(app))?;

    let window_menu = SubmenuBuilder::new(app, labels.get("menu.window", "Window"))
        .minimize_with_text(labels.get("menu.minimize", "Minimize"))
        .maximize_with_text(labels.get("menu.zoom", "Zoom"))
        .separator()
        .close_window_with_text(labels.get("menu.closeWindow", "Close Window"))
        .build()?;

    MenuBuilder::new(app)
        .items(&[
            &app_menu,
            &edit_menu,
            &view_menu,
            &remote_menu,
            &window_menu,
        ])
        .build()
}

/// The Remote menu: Remote Devices, the saved remote daemons (choosing one opens or brings
/// forward its window), and the local daemon's window. Its items' ids are the remote daemons'
/// (`remote::menu_action`), which `on_menu_event` in `lib.rs` hands to `remote`.
fn remote_menu(
    app: &AppHandle,
    labels: &Labels,
    entries: &[(String, String)],
) -> tauri::Result<tauri::menu::Submenu<tauri::Wry>> {
    let mut menu = SubmenuBuilder::new(app, labels.get("menu.remote", "Remote"));
    for item in remote_items(labels, entries) {
        menu = match item {
            RemoteItem::Item { id, text, enabled } => menu.item(
                &MenuItemBuilder::with_id(id, text)
                    .enabled(enabled)
                    .build(app)?,
            ),
            RemoteItem::Separator => menu.separator(),
        };
    }
    menu.build()
}

/// One line of the Remote menu, which the Dock menu repeats.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum RemoteItem {
    Item {
        id: String,
        text: String,
        enabled: bool,
    },
    Separator,
}

/// The Remote menu's lines, in order: Remote Devices; each saved remote daemon by its name (or a
/// disabled line saying there is none); Open This Mac's Window.
pub(super) fn remote_items(labels: &Labels, entries: &[(String, String)]) -> Vec<RemoteItem> {
    use crate::remote::{DEVICES_ITEM_ID, MAIN_WINDOW_ITEM_ID, OPEN_ITEM_PREFIX};
    let item = |id: String, text: &str, enabled: bool| RemoteItem::Item {
        id,
        text: text.to_owned(),
        enabled,
    };
    let mut items = vec![
        item(
            DEVICES_ITEM_ID.to_owned(),
            labels.get("menu.remoteDevices", "Remote Devices"),
            true,
        ),
        RemoteItem::Separator,
    ];
    if entries.is_empty() {
        items.push(item(
            "remote-window-none".to_owned(),
            labels.get("menu.noRemoteDaemons", "No Remote Daemons"),
            false,
        ));
    }
    for (id, name) in entries {
        items.push(item(format!("{OPEN_ITEM_PREFIX}{id}"), name, true));
    }
    items.push(RemoteItem::Separator);
    items.push(item(
        MAIN_WINDOW_ITEM_ID.to_owned(),
        labels.get("menu.openMainWindow", "Open This Mac’s Window"),
        true,
    ));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remote_menu_lists_the_remote_devices_the_saved_daemons_and_the_main_window() {
        let entries = vec![
            ("a".to_owned(), "Staging".to_owned()),
            ("b".to_owned(), "Lab".to_owned()),
        ];
        let items = remote_items(&Labels::default(), &entries);
        let ids: Vec<Option<&str>> = items
            .iter()
            .map(|item| match item {
                RemoteItem::Item { id, .. } => Some(id.as_str()),
                RemoteItem::Separator => None,
            })
            .collect();
        assert_eq!(
            ids,
            [
                Some("remote-devices-show"),
                None,
                Some("remote-window-open:a"),
                Some("remote-window-open:b"),
                None,
                Some("main-window-show"),
            ]
        );
        assert!(matches!(&items[2], RemoteItem::Item { text, .. } if text == "Staging"));
        for item in &items {
            if let RemoteItem::Item { id, .. } = item {
                assert!(crate::remote::menu_action(id).is_some(), "{id}");
            }
        }
    }

    #[test]
    fn the_remote_menu_reads_the_ui_s_label_for_the_remote_devices_line() {
        let labels = Labels(HashMap::from([(
            "menu.remoteDevices".to_owned(),
            "远程设备".to_owned(),
        )]));
        let items = remote_items(&labels, &[]);
        assert!(matches!(
            &items[0],
            RemoteItem::Item { text, enabled: true, .. } if text == "远程设备"
        ));
    }

    #[test]
    fn an_empty_remote_menu_says_so_with_a_line_that_cannot_be_chosen() {
        let mut labels = HashMap::new();
        labels.insert(
            "menu.noRemoteDaemons".to_owned(),
            "没有远程守护进程".to_owned(),
        );
        let items = remote_items(&Labels(labels), &[]);
        assert!(items.contains(&RemoteItem::Item {
            id: "remote-window-none".to_owned(),
            text: "没有远程守护进程".to_owned(),
            enabled: false,
        }));
        assert_eq!(crate::remote::menu_action("remote-window-none"), None);
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

    fn texts(items: &[RemoteItem]) -> Vec<Option<(&str, bool)>> {
        items
            .iter()
            .map(|item| match item {
                RemoteItem::Item { text, enabled, .. } => Some((text.as_str(), *enabled)),
                RemoteItem::Separator => None,
            })
            .collect()
    }

    #[test]
    fn the_remote_menu_reads_in_english_until_the_ui_sends_its_labels() {
        let entries = vec![("a".to_owned(), "Lab".to_owned())];
        assert_eq!(
            texts(&remote_items(&Labels::default(), &entries)),
            [
                Some(("Remote Devices", true)),
                None,
                Some(("Lab", true)),
                None,
                Some(("Open This Mac’s Window", true)),
            ]
        );
        assert_eq!(
            texts(&remote_items(&Labels::default(), &[])),
            [
                Some(("Remote Devices", true)),
                None,
                Some(("No Remote Daemons", false)),
                None,
                Some(("Open This Mac’s Window", true)),
            ]
        );
    }

    #[test]
    fn the_remote_menu_reads_in_the_language_the_ui_sent() {
        let labels = Labels(HashMap::from([
            ("menu.remoteDevices".to_owned(), "远程设备".to_owned()),
            (
                "menu.openMainWindow".to_owned(),
                "打开这台 Mac 的窗口".to_owned(),
            ),
        ]));
        let entries = vec![("a".to_owned(), "实验室".to_owned())];
        assert_eq!(
            texts(&remote_items(&labels, &entries)),
            [
                Some(("远程设备", true)),
                None,
                Some(("实验室", true)),
                None,
                Some(("打开这台 Mac 的窗口", true)),
            ]
        );
    }

    #[test]
    fn a_saved_daemon_is_named_by_its_name_whatever_the_name_holds() {
        let entries = vec![("id".to_owned(), "R&D: \"lab\" #1".to_owned())];
        let items = remote_items(&Labels::default(), &entries);
        assert!(matches!(
            &items[2],
            RemoteItem::Item { id, text, enabled: true }
                if id == "remote-window-open:id" && text == "R&D: \"lab\" #1"
        ));
    }
}
