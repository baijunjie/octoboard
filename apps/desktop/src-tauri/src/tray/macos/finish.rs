//! Finishing the tray's native menu as it begins to open, from what `finish_order` worked out when
//! it was built: once for each menu built, the pictures, the rows and the row of tabs; at every
//! opening, the tab chosen.

use std::collections::HashMap;
use std::sync::Mutex;

use super::lines::{dress_submenu, heading_row};
use super::openers::dress_quota_openers;
use super::tabs::{self, TabStrip};
use crate::tray::finish::Finish;
use crate::tray::model::{AppLabels, TrayMenu};
use crate::tray::tabs::{raises_hand, Tab};
use crate::tray::TrayState;

/// `TrayState::unfinished`: what is left to do to the menu last set, and the tabs it has.
pub(in crate::tray) type Unfinished = Mutex<Option<MenuFinish>>;

/// What `finish_menu` does to a menu `build_menu` made.
pub(in crate::tray) struct MenuFinish {
    finish: Finish,
    strip: TabStrip,
    /// Whether the pictures and rows have been put on the menu, which happens once.
    dressed: bool,
}

/// Finishes the icon's menu as it begins to open: the first time since it was built, its pictures
/// and rows; every time, the row of tabs with `chosen` chosen, and only that tab's lines shown.
/// `notification` is the one that it began tracking, and carries the menu.
pub(super) fn finish_opening_menu(
    state: &TrayState,
    notification: &objc2_foundation::NSNotification,
    chosen: usize,
) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSMenu;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(menu) = notification
        .object()
        .and_then(|object| object.downcast::<NSMenu>().ok())
    else {
        return;
    };
    let mut unfinished = state.unfinished.lock().unwrap_or_else(|e| e.into_inner());
    let Some(finish) = unfinished.as_mut() else {
        return;
    };
    if !finish.dressed {
        finish.dressed = true;
        dress_menu(&menu, &finish.finish, &finish.strip, mtm);
    }
    tabs::choose(&menu, &finish.finish.owners, chosen);
}

/// Dresses `menu`, the tray's menu as muda built it: sets the template flag on the pictures of
/// its lines that have it, whose items (separators aside) are the lines `build_menu` made, in the
/// order `finish_order` gives; gives the row of tabs its control; gives the lines that are a
/// section's heading their row, with the picture just made a template where it is one; then
/// dresses each quota submenu's lines as the rows they stand for (`dress_submenu`). Opening lines
/// keep their native menu item and receive title attachments for available console-account
/// quotas. The quota submenus come before any other submenu, so they are the first ones.
fn dress_menu(
    menu: &objc2_app_kit::NSMenu,
    finish: &Finish,
    strip: &TabStrip,
    mtm: objc2::MainThreadMarker,
) {
    let items = menu.itemArray();
    let lines: Vec<_> = items
        .iter()
        .filter(|item| !item.isSeparatorItem())
        .collect();
    for (item, line) in lines.iter().zip(&finish.lines) {
        if line.tabs {
            item.setView(Some(&tabs::strip_row(strip, mtm)));
            continue;
        }
        let image = item.image();
        if let Some(image) = image.as_ref().filter(|_| line.template) {
            image.setTemplate(true);
        }
        if let Some(heading) = &line.heading {
            item.setView(Some(&heading_row(heading, image.as_deref(), mtm)));
        }
    }
    dress_quota_openers(menu, &finish.quotas, mtm);
    let opened = lines.iter().filter_map(|item| item.submenu());
    for (submenu, model) in opened.zip(&finish.quotas) {
        dress_submenu(&submenu, &model.items, mtm);
    }
}

/// Keeps what finishing the menu just set takes, for when it opens: `finish`, and the tabs' labels
/// and raised hands for the row of tabs.
pub(in crate::tray) fn finish_when_opened(
    state: &TrayState,
    finish: Finish,
    tabs: &[Tab],
    menus: &HashMap<String, TrayMenu>,
    labels: &AppLabels,
) {
    let strip = TabStrip {
        labels: tabs.iter().map(|tab| tab.label(labels)).collect(),
        raised: tabs.iter().map(|tab| raises_hand(tab, menus)).collect(),
    };
    *state.unfinished.lock().unwrap_or_else(|e| e.into_inner()) = Some(MenuFinish {
        finish,
        strip,
        dressed: false,
    });
}
