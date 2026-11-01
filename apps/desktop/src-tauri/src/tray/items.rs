//! Building the tray's menu items from the parsed menu.

use base64::Engine;
use tauri::image::Image;
use tauri::menu::{
    IconMenuItem, IconMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder,
};
use tauri::{AppHandle, Wry};

use super::model::TrayMenu;
use super::{OPEN_ITEM_ID, QUIT_ITEM_ID, SESSION_ITEM_PREFIX};

/// The picture a line names, or none when it names none or one the table does not hold, or one that
/// does not decode: a line is better shown bare than not at all.
fn icon_of(menu: &TrayMenu, key: Option<&str>) -> Option<Image<'static>> {
    let png = base64::engine::general_purpose::STANDARD
        .decode(&menu.icons.get(key?)?.png)
        .ok()?;
    Image::from_bytes(&png).ok()
}

pub(super) fn is_template(menu: &TrayMenu, key: Option<&str>) -> bool {
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

/// The menu.
pub(super) fn build_menu(app: &AppHandle, menu: &TrayMenu) -> tauri::Result<Menu<Wry>> {
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
        if let Some(icon) = icon_of(menu, submenu.icon.as_deref()) {
            lines = lines.submenu_icon(icon);
        }
        // Plain disabled items, which `dress_submenu` gives a view once the menu is built; their
        // title is what a line reads as where that is not done, and what assistive technology
        // reads for every row drawn.
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
