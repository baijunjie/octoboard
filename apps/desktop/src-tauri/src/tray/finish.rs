//! What is left to do to the tray's native menu once muda has built it, as it first begins to open.

#[cfg(target_os = "macos")]
use tauri::{AppHandle, Manager};

use super::items::is_template;
#[cfg(target_os = "macos")]
use super::lines::{dress_submenu, heading_row};
use super::model::TrayMenu;
#[cfg(target_os = "macos")]
use super::model::TraySubmenu;
#[cfg(target_os = "macos")]
use super::openers::dress_quota_openers;
#[cfg(target_os = "macos")]
use super::TrayState;

/// What `finish_menu` does to a menu `build_menu` made.
#[cfg(target_os = "macos")]
pub(super) struct MenuFinish {
    pub(super) lines: Vec<LineFinish>,
    pub(super) submenus: Vec<TraySubmenu>,
}

#[cfg(target_os = "macos")]
pub(super) fn unfinished(app: &AppHandle) -> std::sync::MutexGuard<'_, Option<MenuFinish>> {
    app.state::<TrayState>()
        .inner()
        .unfinished
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Finishes the tray's menu as it begins to open, when it is the first time since it was built:
/// `notification` is the one that it began tracking, and carries the menu.
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
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

/// What `finish_menu` does to a line of the menu once muda has built it.
pub(super) struct LineFinish {
    /// Whether the line's picture is a template image.
    pub(super) template: bool,
    /// The text of the line when it is a section's heading, which gets a row of its own.
    pub(super) heading: Option<String>,
}

/// For each line of the menu `build_menu` makes, in order: the sessions' lines, then each
/// submenu's opening line (with its agent's mark), then Open and Quit. Separators are not lines,
/// and a submenu's own lines have no pictures.
pub(super) fn finish_order(menu: &TrayMenu) -> Vec<LineFinish> {
    let plain = || LineFinish {
        template: false,
        heading: None,
    };
    let mut lines = Vec::new();
    for section in &menu.sections {
        lines.push(LineFinish {
            template: is_template(menu, section.icon.as_deref()),
            heading: Some(section.heading.clone()),
        });
        lines.extend(section.items.iter().map(|item| LineFinish {
            template: is_template(menu, item.icon.as_deref()),
            heading: None,
        }));
    }
    lines.extend(menu.submenus.iter().map(|submenu| LineFinish {
        template: is_template(menu, submenu.icon.as_deref()),
        heading: None,
    }));
    lines.extend([plain(), plain()]);
    lines
}

#[cfg(test)]
mod tests {
    use super::super::model::fixtures::{parsed, WITH_SUBMENUS};
    use super::*;

    #[test]
    fn a_menu_without_submenus_reads_and_has_only_the_open_and_quit_lines_to_mark() {
        let menu = parsed(r#"{"sections": [], "open": "Open", "quit": "Quit"}"#);
        assert!(menu.submenus.is_empty());
        assert_eq!(finish_order(&menu).len(), 2);
    }

    #[test]
    fn a_submenu_opener_is_finished_as_a_template_when_its_picture_is_one() {
        let menu = parsed(WITH_SUBMENUS);
        // The heading, the Codex session and "1 more…", then the Codex and Grok openers, then Open
        // and Quit.
        let templates: Vec<_> = finish_order(&menu)
            .into_iter()
            .map(|line| line.template)
            .collect();
        assert_eq!(
            templates,
            vec![false, true, false, true, false, false, false]
        );
    }

    #[test]
    fn lines_to_finish_follow_the_lines_in_menu_order() {
        // Two sections (the second an empty state with no session line), then the two submenus'
        // opening lines, then Open and Quit; a submenu's own lines are rows without pictures.
        let menu = parsed(
            r#"{
                "sections": [
                    {"heading": "Working (1)", "icon": "status", "items": [
                        {"label": "Fix it", "icon": "codex", "session": "a"}
                    ]},
                    {"heading": "No sessions running", "items": []}
                ],
                "submenus": [{"title": "Codex quota", "items": []}],
                "icons": {
                    "status": {"png": "", "template": false},
                    "codex": {"png": "", "template": true}
                },
                "open": "Open",
                "quit": "Quit"
            }"#,
        );
        let finishes: Vec<_> = finish_order(&menu)
            .into_iter()
            .map(|line| (line.template, line.heading))
            .collect();
        assert_eq!(
            finishes,
            vec![
                (false, Some("Working (1)".to_string())),
                (true, None),
                (false, Some("No sessions running".to_string())),
                (false, None), // Codex submenu
                (false, None), // Open
                (false, None), // Quit
            ]
        );
    }
}
