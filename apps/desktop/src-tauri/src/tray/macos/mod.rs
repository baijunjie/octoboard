//! The rich menu bar icon of macOS: the attention dot, the red eyes and the accessible name on the
//! status item, the menu tracking that holds back a rebuild while a menu is open, and the finishing
//! pass that gives the menu its row of tabs (`tabs.rs`), its template pictures and its custom rows
//! (`lines.rs`, `openers.rs`).

mod finish;
mod lines;
mod menu_tracking;
mod openers;
mod status_item;
mod tabs;

use std::borrow::Cow;

use tauri::image::Image;
use tauri::tray::TrayIconBuilder;
use tauri::Wry;

use super::icon;

pub(super) use finish::{finish_when_opened, Unfinished};
pub(super) use menu_tracking::follow_menu_tracking;
pub(super) use status_item::{set_icon_label, set_visible, show_layers};

/// The tabs are a row at the top of the menu, every tab's lines below it.
pub(super) const LAYOUT: super::tabs::Layout = super::tabs::Layout::Tabs;

/// The icon opens its native menu, not a flyout.
pub(super) const FLYOUT: bool = false;

/// The octopus is a template image, which macOS tints for a light or dark menu bar itself.
pub(super) const TEMPLATE: bool = true;

/// The octopus, with the quota capsules, the dot's cut-out corner and the eyes' cut-outs when they
/// apply.
pub(super) fn picture(attention: bool, awake: bool, quota: Option<icon::Quota>) -> Image<'static> {
    icon::octopus(attention, awake, quota)
}

/// AppKit lays out the bidi isolates the UI wraps names and values in and draws nothing for them,
/// so a label is shown as the UI sent it.
pub(super) fn native_text(label: &str) -> Cow<'_, str> {
    Cow::Borrowed(label)
}

/// The tooltip names the application; the status item's accessible name carries its live state.
pub(super) fn customize(builder: TrayIconBuilder<Wry>) -> TrayIconBuilder<Wry> {
    builder.tooltip(crate::menu::APP_NAME)
}
