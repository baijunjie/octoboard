//! What is left to do to the tray's native menu once muda has built it, as it begins to open: what
//! each line needs, worked out from the lines `tabs::compose` made. The finishing itself is native
//! (`macos/finish.rs`).

use super::items::is_template;
use super::model::{TrayMenu, TraySubmenu};
use super::tabs::Node;

/// What `finish_menu` does to a line of the menu once muda has built it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct LineFinish {
    /// Whether the line's picture is a template image.
    pub(super) template: bool,
    /// The text of the line when it is a section's heading, which gets a row of its own.
    pub(super) heading: Option<String>,
    /// Whether it is the row of tabs, which gets the tabs' control.
    pub(super) tabs: bool,
}

/// What finishing the menu takes.
pub(super) struct Finish {
    /// For each line of the top level, separators aside, in order.
    pub(super) lines: Vec<LineFinish>,
    /// The tab each item of the top level belongs to, separators included, in order: what is
    /// hidden while another tab is chosen.
    pub(super) owners: Vec<Option<usize>>,
    /// The quota submenus, in the order their opening lines come: the only submenus of the top
    /// level in the tabbed layout (`tabs::Layout::Tabs`) the macOS menu is composed in.
    pub(super) quotas: Vec<TraySubmenu>,
}

/// What finishing the menu made of `nodes` takes; `menu_of` gives the menu of a tab.
pub(super) fn finish_order<'a>(
    nodes: &[Node],
    menu_of: &dyn Fn(usize) -> Option<&'a TrayMenu>,
) -> Finish {
    let mut finish = Finish {
        lines: Vec::new(),
        owners: Vec::new(),
        quotas: Vec::new(),
    };
    let plain = || LineFinish {
        template: false,
        heading: None,
        tabs: false,
    };
    for node in nodes {
        finish.owners.push(node.owner());
        match node {
            Node::Separator { .. } => {}
            Node::Tabs { .. } => finish.lines.push(LineFinish {
                tabs: true,
                ..plain()
            }),
            Node::Line {
                owner,
                label,
                icon,
                heading,
                ..
            } => finish.lines.push(LineFinish {
                template: owner
                    .and_then(menu_of)
                    .is_some_and(|menu| is_template(menu, icon.as_deref())),
                heading: heading.then(|| label.clone()),
                tabs: false,
            }),
            Node::Quota { owner, index } => {
                let submenu =
                    menu_of(*owner).and_then(|menu| Some((menu, menu.submenus.get(*index)?)));
                finish.lines.push(LineFinish {
                    template: submenu
                        .is_some_and(|(menu, submenu)| is_template(menu, submenu.icon.as_deref())),
                    ..plain()
                });
                if let Some((_, submenu)) = submenu {
                    finish.quotas.push(submenu.clone());
                }
            }
            Node::Group { .. } => finish.lines.push(plain()),
        }
    }
    finish
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::model::fixtures::{parsed, WITH_SUBMENUS};
    use super::super::tabs::{compose, tabs, Contents, Layout};
    use super::*;
    use crate::remote::ConnectedDaemon;

    #[test]
    fn every_item_is_owned_by_its_tab_and_the_lines_say_what_each_needs() {
        let tabs = tabs(&[ConnectedDaemon {
            key: "daemon-1".into(),
            entry_id: "e".into(),
            name: "lab".into(),
            in_browser: false,
        }]);
        let menus = HashMap::from([("daemon-1".to_owned(), parsed(WITH_SUBMENUS))]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 0,
            },
            Layout::Tabs,
        );
        let menu_of = |index: usize| menus.get(&tabs[index].key);
        let finish = finish_order(&nodes, &menu_of);
        assert_eq!(finish.owners.len(), nodes.len());
        assert_eq!(
            finish.lines.len(),
            nodes
                .iter()
                .filter(|node| !matches!(node, Node::Separator { .. }))
                .collect::<Vec<_>>()
                .len()
        );
        assert!(finish.lines[0].tabs);
        // The remote tab's heading, its Codex session line (a template picture) and "1 more…".
        let remote: Vec<_> = finish.lines[1..4]
            .iter()
            .map(|line| (line.template, line.heading.clone()))
            .collect();
        assert_eq!(
            remote,
            vec![
                (false, Some("Working (1)".to_string())),
                (true, None),
                (false, None)
            ]
        );
        // Its two quota submenus, the Codex one a template, in order.
        let titles: Vec<_> = finish.quotas.iter().map(|q| q.title.as_str()).collect();
        assert_eq!(titles, ["Codex quota", "Grok quota"]);
        assert_eq!(finish.owners[0], None);
        assert_eq!(finish.owners.last(), Some(&None));
    }
}
