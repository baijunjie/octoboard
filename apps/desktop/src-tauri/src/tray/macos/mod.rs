//! The rich menu bar icon of macOS: the attention dot and the accessible name on the status item,
//! the menu tracking that holds back a menu sent while one is open, and the finishing pass that
//! gives the menu its template pictures and its custom rows (`lines.rs`, `openers.rs`).

mod finish;
mod lines;
mod menu_tracking;
mod openers;
mod status_item;

use std::borrow::Cow;

use tauri::image::Image;
use tauri::tray::TrayIconBuilder;
use tauri::Wry;

use super::icon;

pub(super) use finish::{finish_when_opened, Unfinished};
pub(super) use menu_tracking::follow_menu_tracking;
pub(super) use status_item::{set_icon_label, show_dot};

/// The octopus is a template image, which macOS tints for a light or dark menu bar itself.
pub(super) const TEMPLATE: bool = true;

/// The octopus, with the quota capsules and the dot's cut-out corner when they apply.
pub(super) fn picture(attention: bool, quota: Option<icon::Quota>) -> Image<'static> {
    icon::octopus(attention, quota)
}

/// A line's picture as the UI drew it: AppKit tints a template one for the menu once the menu is
/// finished (`finish.rs`).
pub(super) fn menu_picture(picture: Image<'static>, _template: bool) -> Image<'static> {
    picture
}

/// AppKit lays out the bidi isolates the UI wraps names and values in and draws nothing for them,
/// so a label is shown as the UI sent it.
pub(super) fn native_text(label: &str) -> Cow<'_, str> {
    Cow::Borrowed(label)
}

/// Nothing to add to the builder: the menu's opening is heard through the status item's own menu
/// tracking (`follow_menu_tracking`), not through click events.
pub(super) fn customize(builder: TrayIconBuilder<Wry>) -> TrayIconBuilder<Wry> {
    builder
}
