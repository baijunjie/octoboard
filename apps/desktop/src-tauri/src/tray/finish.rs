//! What is left to do to the tray's native menu once muda has built it, as it first begins to open:
//! what each line needs, worked out from the menu the UI sent. The finishing itself is native
//! (`macos/finish.rs`).

use super::items::is_template;
use super::model::TrayMenu;

/// What `finish_menu` does to a line of the menu once muda has built it.
pub(super) struct LineFinish {
    /// Whether the line's picture is a template image.
    pub(super) template: bool,
    /// The text of the line when it is a section's heading, which gets a row of its own.
    pub(super) heading: Option<String>,
}

/// For each line of the menu `build_menu` makes, in order: the sessions' lines, then each
/// submenu's opening line (with its agent's mark), then Updates, Open and Quit. Separators are not lines,
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
    lines.extend([plain(), plain(), plain()]);
    lines
}

#[cfg(test)]
mod tests {
    use super::super::model::fixtures::{parsed, WITH_SUBMENUS};
    use super::*;

    #[test]
    fn a_menu_without_submenus_reads_and_has_only_the_open_updates_and_quit_lines_to_mark() {
        let menu = parsed(r#"{"sections": [], "open": "Open", "quit": "Quit"}"#);
        assert!(menu.submenus.is_empty());
        assert_eq!(finish_order(&menu).len(), 3);
    }

    #[test]
    fn a_submenu_opener_is_finished_as_a_template_when_its_picture_is_one() {
        let menu = parsed(WITH_SUBMENUS);
        // The heading, the Codex session and "1 more…", then the Codex and Grok openers, then Updates, Open
        // and Quit.
        let templates: Vec<_> = finish_order(&menu)
            .into_iter()
            .map(|line| line.template)
            .collect();
        assert_eq!(
            templates,
            vec![false, true, false, true, false, false, false, false]
        );
    }

    #[test]
    fn lines_to_finish_follow_the_lines_in_menu_order() {
        // Two sections (the second an empty state with no session line), then the two submenus'
        // opening lines, then Updates, Open and Quit; a submenu's own lines are rows without pictures.
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
                (false, None), // Updates
                (false, None), // Open
                (false, None), // Quit
            ]
        );
    }
}
