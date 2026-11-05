//! Finishing the tray's native menu as it first begins to open, from what `finish_order` worked
//! out when it was built.

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use super::lines::{dress_submenu, heading_row};
use super::openers::dress_quota_openers;
use crate::tray::finish::{finish_order, LineFinish};
use crate::tray::model::{TrayMenu, TraySubmenu};
use crate::tray::TrayState;

/// `TrayState::unfinished`.
pub(in crate::tray) type Unfinished = Mutex<Option<MenuFinish>>;

/// What `finish_menu` does to a menu `build_menu` made.
pub(in crate::tray) struct MenuFinish {
    pub(super) lines: Vec<LineFinish>,
    pub(super) submenus: Vec<TraySubmenu>,
}

pub(super) fn unfinished(app: &AppHandle) -> std::sync::MutexGuard<'_, Option<MenuFinish>> {
    app.state::<TrayState>()
        .inner()
        .unfinished
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Finishes the tray's menu as it begins to open, when it is the first time since it was built:
/// `notification` is the one that it began tracking, and carries the menu.
pub(super) fn finish_opening_menu(
    app: &AppHandle,
    notification: &objc2_foundation::NSNotification,
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
    let finish = unfinished(app).take();
    if let Some(finish) = finish {
        finish_menu(&menu, &finish, mtm);
    }
}

/// Finishes `menu`, the tray's menu as muda built it: sets the template flag on the pictures of
/// its lines that have it, whose items (separators aside) are the lines `build_menu` made, in the
/// order `finish_order` gives; gives the lines that are a section's heading their row, with the
/// picture just made a template where it is one; then dresses each submenu's lines as the rows
/// they stand for (`dress_submenu`). Opening lines keep their native menu item and receive title
/// attachments for available console-account quotas.
fn finish_menu(menu: &objc2_app_kit::NSMenu, finish: &MenuFinish, mtm: objc2::MainThreadMarker) {
    let items = menu.itemArray();
    let lines: Vec<_> = items
        .iter()
        .filter(|item| !item.isSeparatorItem())
        .collect();
    for (item, line) in lines.iter().zip(&finish.lines) {
        let image = item.image();
        if let Some(image) = image.as_ref().filter(|_| line.template) {
            image.setTemplate(true);
        }
        if let Some(heading) = &line.heading {
            item.setView(Some(&heading_row(heading, image.as_deref(), mtm)));
        }
    }
    dress_quota_openers(menu, &finish.submenus, mtm);
    let opened = lines.iter().filter_map(|item| item.submenu());
    for (submenu, model) in opened.zip(&finish.submenus) {
        dress_submenu(&submenu, &model.items, mtm);
    }
}

/// Keeps what `finish_menu` is to do to `menu`, the menu just set, for when it first opens.
pub(in crate::tray) fn finish_when_opened(app: &AppHandle, menu: &TrayMenu) {
    *unfinished(app) = Some(MenuFinish {
        lines: finish_order(menu),
        submenus: menu.submenus.clone(),
    });
}
