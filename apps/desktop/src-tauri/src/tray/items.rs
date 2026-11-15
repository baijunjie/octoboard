//! Building the native menu from the lines `tabs::compose` worked out.

use base64::Engine;
use tauri::image::Image;
use tauri::menu::{
    IconMenuItem, IconMenuItemBuilder, IsMenuItem, Menu, MenuBuilder, PredefinedMenuItem, Submenu,
    SubmenuBuilder,
};
use tauri::{AppHandle, Wry};

use super::model::TrayMenu;
use super::platform::native_text;
use super::tabs::Node;

/// The picture a line names, or none when it names none or one the table does not hold, or one that
/// does not decode: a line is better shown bare than not at all.
fn icon_of(menu: &TrayMenu, key: Option<&str>) -> Option<Image<'static>> {
    let icon = menu.icons.get(key?)?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(&icon.png)
        .ok()?;
    Image::from_bytes(&png).ok()
}

pub(super) fn is_template(menu: &TrayMenu, key: Option<&str>) -> bool {
    key.and_then(|key| menu.icons.get(key))
        .is_some_and(|icon| icon.template)
}

/// A label as muda takes it, where a lone `&` marks a mnemonic and would be dropped: "R&D" is
/// written "R&&D" to be shown as it is. The platform first takes out what its menu cannot draw
/// (`native_text`).
fn literal(label: &str) -> String {
    native_text(label).replace('&', "&&")
}

/// A line that cannot be chosen when `id` is `None`, with its picture when it has one.
fn line(
    app: &AppHandle,
    menu: Option<&TrayMenu>,
    id: Option<&str>,
    label: &str,
    icon: Option<&str>,
) -> tauri::Result<IconMenuItem<Wry>> {
    let mut builder = match id {
        Some(id) => IconMenuItemBuilder::with_id(id, literal(label)),
        None => IconMenuItemBuilder::new(literal(label)).enabled(false),
    };
    if let Some(icon) = menu.and_then(|menu| icon_of(menu, icon)) {
        builder = builder.icon(icon);
    }
    builder.build(app)
}

/// A native item, of whichever kind.
enum Built {
    Line(IconMenuItem<Wry>),
    Submenu(Submenu<Wry>),
    Separator(PredefinedMenuItem<Wry>),
}

impl Built {
    fn item(&self) -> &dyn IsMenuItem<Wry> {
        match self {
            Self::Line(item) => item,
            Self::Submenu(item) => item,
            Self::Separator(item) => item,
        }
    }
}

/// The menu holding `nodes`. `menu_of` gives the menu of a tab, whose pictures its lines name.
pub(super) fn build_menu<'a>(
    app: &AppHandle,
    nodes: &[Node],
    menu_of: &dyn Fn(usize) -> Option<&'a TrayMenu>,
) -> tauri::Result<Menu<Wry>> {
    let built = build_all(app, nodes, menu_of)?;
    let items: Vec<&dyn IsMenuItem<Wry>> = built.iter().map(Built::item).collect();
    MenuBuilder::new(app).items(&items).build()
}

fn build_all<'a>(
    app: &AppHandle,
    nodes: &[Node],
    menu_of: &dyn Fn(usize) -> Option<&'a TrayMenu>,
) -> tauri::Result<Vec<Built>> {
    nodes.iter().map(|node| build(app, node, menu_of)).collect()
}

fn build<'a>(
    app: &AppHandle,
    node: &Node,
    menu_of: &dyn Fn(usize) -> Option<&'a TrayMenu>,
) -> tauri::Result<Built> {
    Ok(match node {
        Node::Tabs { label } => Built::Line(line(app, None, None, label, None)?),
        Node::Line {
            owner,
            id,
            label,
            icon,
            ..
        } => Built::Line(line(
            app,
            owner.and_then(menu_of),
            id.as_deref(),
            label,
            icon.as_deref(),
        )?),
        Node::Quota { owner, index } => {
            // `compose` names only the submenus a tab's menu has.
            let Some((menu, submenu)) =
                menu_of(*owner).and_then(|menu| Some((menu, menu.submenus.get(*index)?)))
            else {
                return Ok(Built::Separator(PredefinedMenuItem::separator(app)?));
            };
            let mut lines = SubmenuBuilder::new(app, literal(&submenu.title));
            if let Some(icon) = icon_of(menu, submenu.icon.as_deref()) {
                lines = lines.submenu_icon(icon);
            }
            // Plain disabled items, which `dress_submenu` gives a view once the menu is built;
            // their title is what a line reads as where that is not done, and what assistive
            // technology reads for every row drawn.
            for item in &submenu.items {
                lines = match item.text() {
                    Some(text) => lines.item(&line(app, None, None, text, None)?),
                    None => lines.separator(),
                };
            }
            Built::Submenu(lines.build()?)
        }
        Node::Group {
            title, children, ..
        } => {
            let built = build_all(app, children, menu_of)?;
            let items: Vec<&dyn IsMenuItem<Wry>> = built.iter().map(Built::item).collect();
            Built::Submenu(
                SubmenuBuilder::new(app, literal(title))
                    .items(&items)
                    .build()?,
            )
        }
        Node::Separator { .. } => Built::Separator(PredefinedMenuItem::separator(app)?),
    })
}
