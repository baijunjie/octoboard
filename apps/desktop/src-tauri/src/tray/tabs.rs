//! What the one menu bar icon's menu holds, worked out on every platform from the daemons the
//! application shows and the menus their windows' UIs sent: a tab for each daemon (the local one
//! first, then each connected remote daemon in the order it was connected in, whether shown in a
//! window or held for the browser), and the lines of the menu (`Node`) in the layout the platform
//! can show (`Layout`). `items.rs` builds the native menu from those lines.

use std::collections::HashMap;

use super::model::{AppLabels, TrayMenu};
use super::names;
use super::{Item, ItemId};
use crate::remote::{ConnectedDaemon, OPEN_ITEM_PREFIX};
use crate::MAIN_WINDOW_LABEL;

/// One daemon of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Tab {
    /// The label of the daemon's window, or for a daemon held for the browser the key its
    /// connection is kept under.
    pub(super) key: String,
    pub(super) kind: TabKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TabKind {
    /// The local daemon, in the main window.
    Local,
    /// A remote daemon in a window of its own.
    Window { name: String },
    /// A remote daemon held for the system's browser, which no window of the application shows.
    Browser { entry_id: String, name: String },
}

impl Tab {
    /// The remote daemon's name, `None` for the local daemon.
    pub(super) fn remote_name(&self) -> Option<&str> {
        match &self.kind {
            TabKind::Local => None,
            TabKind::Window { name } | TabKind::Browser { name, .. } => Some(name),
        }
    }

    /// What the tab is labelled: the local daemon's by `this_computer`, a remote one by its short
    /// name.
    pub(super) fn label(&self, labels: &AppLabels) -> String {
        match self.remote_name() {
            Some(name) => names::short_name(name),
            None => labels.this_computer.clone(),
        }
    }
}

/// The tabs: the local daemon's, then each connected remote daemon's in the order it was connected
/// in.
pub(super) fn tabs(connected: &[ConnectedDaemon]) -> Vec<Tab> {
    let local = Tab {
        key: MAIN_WINDOW_LABEL.to_owned(),
        kind: TabKind::Local,
    };
    std::iter::once(local)
        .chain(connected.iter().map(|daemon| Tab {
            key: daemon.key.clone(),
            kind: if daemon.in_browser {
                TabKind::Browser {
                    entry_id: daemon.entry_id.clone(),
                    name: daemon.name.clone(),
                }
            } else {
                TabKind::Window {
                    name: daemon.name.clone(),
                }
            },
        }))
        .collect()
}

/// The tab of the window labelled `window`, else the local daemon's (the first).
pub(super) fn tab_of(tabs: &[Tab], window: Option<&str>) -> usize {
    window
        .and_then(|window| tabs.iter().position(|tab| tab.key == window))
        .unwrap_or(0)
}

/// Whether the daemon of `tab` has a session waiting for the user, as its window's UI last said.
pub(super) fn raises_hand(tab: &Tab, menus: &HashMap<String, TrayMenu>) -> bool {
    menus.get(&tab.key).is_some_and(|menu| menu.attention)
}

/// How the platform's menu can show the tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Layout {
    /// A row of tabs at the top (`Node::Tabs`), every tab's lines below it, then the application's
    /// lines: the macOS menu, which hides the lines of all but the chosen tab, and the Windows
    /// flyout, which shows the chosen tab's panel (`flyout_model`).
    Tabs,
    /// A popup menu that can hold no tabs (a development build on Linux): the local daemon's lines
    /// at the top, each remote daemon's in a submenu named after it, then the application's lines.
    #[cfg_attr(
        any(target_os = "macos", windows),
        allow(dead_code, reason = "only a build for Linux lays the menu out so")
    )]
    Nested,
}

/// One line of the menu, with the tab it belongs to (`owner`, `None` for a line every tab shows).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Node {
    /// The row of tabs, which the platform draws (macOS); a plain line reading the chosen tab's
    /// label until it does.
    Tabs {
        label: String,
    },
    Line {
        owner: Option<usize>,
        /// What choosing it does, or `None` for a line that cannot be chosen.
        id: Option<String>,
        label: String,
        /// The picture beside it: a key of the `icons` of the menu of the tab `owner` names.
        icon: Option<String>,
        /// Whether it is a section's heading, which the platform may give a row of its own.
        heading: bool,
    },
    /// An agent's quota submenu, the `index`th of the menu of the tab `owner` names.
    Quota {
        owner: usize,
        index: usize,
    },
    /// A submenu of plain lines: a remote daemon's in the nested layout.
    Group {
        owner: Option<usize>,
        title: String,
        children: Vec<Node>,
    },
    Separator {
        owner: Option<usize>,
    },
}

impl Node {
    pub(super) fn owner(&self) -> Option<usize> {
        match self {
            Self::Tabs { .. } => None,
            Self::Line { owner, .. } | Self::Group { owner, .. } | Self::Separator { owner } => {
                *owner
            }
            Self::Quota { owner, .. } => Some(*owner),
        }
    }
}

fn line(owner: Option<usize>, id: Option<String>, label: &str) -> Node {
    Node::Line {
        owner,
        id,
        label: label.to_owned(),
        icon: None,
        heading: false,
    }
}

/// What the menu is made from.
pub(super) struct Contents<'a> {
    pub(super) tabs: &'a [Tab],
    /// The menu each window's UI last sent, by the window's label.
    pub(super) menus: &'a HashMap<String, TrayMenu>,
    /// The tab chosen when the menu is built, whose label the row of tabs reads until it is drawn.
    pub(super) chosen: usize,
}

impl Contents<'_> {
    /// The menu the application's own lines and labels are taken from: the local daemon's
    /// window's, or the English one before it has sent any.
    fn local(&self) -> std::borrow::Cow<'_, TrayMenu> {
        match self.menus.get(MAIN_WINDOW_LABEL) {
            Some(menu) => std::borrow::Cow::Borrowed(menu),
            None => std::borrow::Cow::Owned(TrayMenu::default()),
        }
    }
}

/// The lines of the menu, in order.
pub(super) fn compose(contents: &Contents<'_>, layout: Layout) -> Vec<Node> {
    let local = contents.local();
    let mut nodes = Vec::new();
    match layout {
        Layout::Tabs => {
            if contents.tabs.len() > 1 {
                let chosen = contents.chosen.min(contents.tabs.len() - 1);
                nodes.push(Node::Tabs {
                    label: contents.tabs[chosen].label(&local.app),
                });
            }
            for (index, tab) in contents.tabs.iter().enumerate() {
                let before = nodes.len();
                tab_lines(&mut nodes, contents, index, tab, true);
                // The separator under a tab's lines is the tab's own, so it shows only with them: a
                // tab that lists nothing leaves none under the row of tabs.
                if nodes.len() > before {
                    nodes.push(Node::Separator { owner: Some(index) });
                }
            }
        }
        Layout::Nested => {
            for (index, tab) in contents.tabs.iter().enumerate() {
                if index == 0 {
                    tab_lines(&mut nodes, contents, index, tab, false);
                    continue;
                }
                let mut children = Vec::new();
                tab_lines(&mut children, contents, index, tab, false);
                if !children.is_empty() {
                    children.push(Node::Separator { owner: Some(index) });
                }
                action_lines(&mut children, &local, index, tab);
                nodes.push(Node::Group {
                    owner: None,
                    title: tab.remote_name().unwrap_or_default().to_owned(),
                    children,
                });
            }
        }
    }
    if layout == Layout::Nested && !nodes.is_empty() {
        nodes.push(Node::Separator { owner: None });
    }
    let id = |item| Some(ItemId::format(MAIN_WINDOW_LABEL, item));
    nodes.push(line(None, id(Item::Updates), &local.updates));
    match layout {
        Layout::Tabs => {
            for (index, tab) in contents.tabs.iter().enumerate() {
                action_lines(&mut nodes, &local, index, tab);
            }
        }
        Layout::Nested => {
            if let Some(tab) = contents.tabs.first() {
                action_lines(&mut nodes, &local, 0, tab);
            }
        }
    }
    nodes.push(Node::Separator { owner: None });
    nodes.push(line(None, id(Item::Quit), &local.quit));
    nodes
}

/// The lines of one daemon, `tab`, the `index`th, that list what it holds: its sessions (each
/// section after its heading) and its quota submenus where `quotas` (the top level of the menu),
/// with a separator between the runs and none after the last. A daemon held for the browser has
/// no window to list sessions from, so it has none.
fn tab_lines(
    nodes: &mut Vec<Node>,
    contents: &Contents<'_>,
    index: usize,
    tab: &Tab,
    quotas: bool,
) {
    let owner = Some(index);
    let id = |item| Some(ItemId::format(&tab.key, item));
    if let Some(menu) = contents.menus.get(&tab.key) {
        for section in &menu.sections {
            nodes.push(Node::Line {
                owner,
                id: None,
                label: section.heading.clone(),
                icon: section.icon.clone(),
                heading: true,
            });
            for item in &section.items {
                nodes.push(Node::Line {
                    owner,
                    id: item
                        .session
                        .as_deref()
                        .and_then(|session| id(Item::Session(session))),
                    label: item.label.clone(),
                    icon: item.icon.clone(),
                    heading: false,
                });
            }
            nodes.push(Node::Separator { owner });
        }
        if quotas && !menu.submenus.is_empty() {
            nodes.extend((0..menu.submenus.len()).map(|submenu| Node::Quota {
                owner: index,
                index: submenu,
            }));
            nodes.push(Node::Separator { owner });
        }
    }
    if matches!(nodes.last(), Some(Node::Separator { owner: last }) if *last == owner) {
        nodes.pop();
    }
}

/// What can be done with one daemon, `tab`, the `index`th: Open board, and for a remote daemon
/// Disconnect. A daemon held for the browser offers opening it there instead of Open board.
fn action_lines(nodes: &mut Vec<Node>, local: &TrayMenu, index: usize, tab: &Tab) {
    let owner = Some(index);
    let id = |item| Some(ItemId::format(&tab.key, item));
    if let TabKind::Browser { entry_id, .. } = &tab.kind {
        nodes.push(line(
            owner,
            Some(format!("{OPEN_ITEM_PREFIX}{entry_id}")),
            &local.app.open_in_browser,
        ));
    } else {
        nodes.push(line(owner, id(Item::Open), &local.open));
    }
    if tab.remote_name().is_some() {
        nodes.push(line(owner, id(Item::Disconnect), &local.disconnect));
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::fixtures::parsed;
    use super::*;

    fn daemon(key: &str, name: &str, in_browser: bool) -> ConnectedDaemon {
        ConnectedDaemon {
            key: key.into(),
            entry_id: format!("e-{name}"),
            name: name.into(),
            in_browser,
        }
    }

    fn menu(attention: bool, session: &str) -> TrayMenu {
        parsed(&format!(
            r#"{{"sections": [{{"heading": "Working (1)", "items": [
                {{"label": "{session}", "session": "{session}"}}]}}],
              "submenus": [{{"title": "Codex quota", "items": []}}],
              "open": "Open", "quit": "Quit", "attention": {attention}}}"#
        ))
    }

    /// The lines' ids and labels, a separator as "-", a submenu as its title followed by its lines
    /// in brackets.
    fn outline(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|node| match node {
                Node::Tabs { label } => format!("tabs:{label}"),
                Node::Line { id, label, .. } => match id {
                    Some(id) => format!("{label}={id}"),
                    None => label.clone(),
                },
                Node::Quota { owner, index } => format!("quota:{owner}:{index}"),
                Node::Group {
                    title, children, ..
                } => format!("{title}[{}]", outline(children).join(", ")),
                Node::Separator { .. } => "-".into(),
            })
            .collect()
    }

    #[test]
    fn the_local_daemon_comes_first_then_the_remote_ones_as_connected() {
        let tabs = tabs(&[
            daemon("daemon-3", "build.lan", false),
            daemon("browser-x", "old", true),
        ]);
        assert_eq!(tabs[0].kind, TabKind::Local);
        assert_eq!(tabs[0].key, "main");
        assert_eq!(
            tabs[1].kind,
            TabKind::Window {
                name: "build.lan".into()
            }
        );
        assert!(matches!(&tabs[2].kind, TabKind::Browser { entry_id, .. } if entry_id == "e-old"));
        let labels = AppLabels::default();
        assert_eq!(tabs[1].label(&labels), "build");
        assert_eq!(tabs[0].label(&labels), labels.this_computer);
    }

    #[test]
    fn the_chosen_tab_is_the_window_s_daemon_else_the_local_one() {
        let tabs = tabs(&[daemon("daemon-3", "a", false)]);
        assert_eq!(tab_of(&tabs, Some("daemon-3")), 1);
        assert_eq!(tab_of(&tabs, Some("daemon-9")), 0);
        assert_eq!(tab_of(&tabs, None), 0);
    }

    #[test]
    fn a_tab_raises_its_hand_from_its_own_window_s_menu() {
        let tabs = tabs(&[daemon("daemon-3", "a", false)]);
        let menus = HashMap::from([("daemon-3".to_owned(), menu(true, "s"))]);
        assert!(!raises_hand(&tabs[0], &menus));
        assert!(raises_hand(&tabs[1], &menus));
    }

    #[test]
    fn with_only_the_local_daemon_the_menu_has_no_tabs_and_reads_as_before() {
        let tabs = tabs(&[]);
        let menus = HashMap::from([("main".to_owned(), menu(false, "s1"))]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 0,
            },
            Layout::Tabs,
        );
        assert_eq!(
            outline(&nodes),
            vec![
                "Working (1)",
                "s1=tray:main:session:s1",
                "-",
                "quota:0:0",
                "-",
                "Check for updates=tray:main:updates",
                "Open=tray:main:open",
                "-",
                "Quit=tray:main:quit",
            ]
        );
    }

    #[test]
    fn on_macos_every_tab_s_lines_follow_the_row_of_tabs_owned_by_their_tab() {
        let tabs = tabs(&[
            daemon("daemon-3", "build.lan", false),
            daemon("browser-e-old", "old", true),
        ]);
        let menus = HashMap::from([
            ("main".to_owned(), menu(false, "s1")),
            ("daemon-3".to_owned(), menu(true, "r1")),
        ]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 1,
            },
            Layout::Tabs,
        );
        assert_eq!(
            outline(&nodes),
            vec![
                "tabs:build",
                "Working (1)",
                "s1=tray:main:session:s1",
                "-",
                "quota:0:0",
                "-",
                "Working (1)",
                "r1=tray:daemon-3:session:r1",
                "-",
                "quota:1:0",
                "-",
                "Check for updates=tray:main:updates",
                "Open=tray:main:open",
                "Open=tray:daemon-3:open",
                "Disconnect=tray:daemon-3:disconnect",
                "Open in browser=remote-window-open:e-old",
                "Disconnect=tray:browser-e-old:disconnect",
                "-",
                "Quit=tray:main:quit",
            ]
        );
        // Every tab's lines sit where the chosen tab's are shown, the application's between them.
        let owners: Vec<_> = nodes.iter().map(Node::owner).collect();
        assert_eq!(owners[0], None);
        assert!(owners[1..6].iter().all(|owner| *owner == Some(0)));
        assert!(owners[6..11].iter().all(|owner| *owner == Some(1)));
        assert_eq!(owners[11], None);
        assert_eq!(owners[12], Some(0));
        assert!(owners[13..15].iter().all(|owner| *owner == Some(1)));
        assert!(owners[15..17].iter().all(|owner| *owner == Some(2)));
        assert!(owners[17..].iter().all(Option::is_none));
    }

    #[test]
    fn on_windows_each_remote_daemon_is_a_submenu_without_its_quotas() {
        let tabs = tabs(&[daemon("daemon-3", "build.lan", false)]);
        let menus = HashMap::from([
            ("main".to_owned(), menu(false, "s1")),
            ("daemon-3".to_owned(), menu(true, "r1")),
        ]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 0,
            },
            Layout::Nested,
        );
        assert_eq!(
            outline(&nodes),
            [
                "Working (1)",
                "s1=tray:main:session:s1",
                "build.lan[Working (1), r1=tray:daemon-3:session:r1, -, Open=tray:daemon-3:open, Disconnect=tray:daemon-3:disconnect]",
                "-",
                "Check for updates=tray:main:updates",
                "Open=tray:main:open",
                "-",
                "Quit=tray:main:quit",
            ]
        );
    }

    #[test]
    fn a_window_whose_ui_sent_nothing_yet_still_offers_open_and_disconnect() {
        let tabs = tabs(&[daemon("daemon-3", "a", false)]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &HashMap::new(),
                chosen: 0,
            },
            Layout::Tabs,
        );
        assert_eq!(
            outline(&nodes),
            [
                format!("tabs:{}", AppLabels::default().this_computer),
                "Check for updates=tray:main:updates".into(),
                "Open board=tray:main:open".into(),
                "Open board=tray:daemon-3:open".into(),
                "Disconnect=tray:daemon-3:disconnect".into(),
                "-".into(),
                "Quit completely=tray:main:quit".into(),
            ]
        );
    }

    #[test]
    fn a_daemon_held_for_the_browser_has_no_window_to_raise_a_hand_from() {
        let tabs = tabs(&[daemon("browser-e-old", "old", true)]);
        // A stale menu under its key does not make it raise one: nothing but a window sends a menu.
        let menus = HashMap::from([("browser-e-old".to_owned(), menu(false, "s"))]);
        assert!(!raises_hand(&tabs[1], &menus));
        assert_eq!(tab_of(&tabs, Some("browser-e-old")), 1);
    }

    #[test]
    fn a_chosen_tab_past_the_last_is_the_last_and_the_row_reads_its_label() {
        let tabs = tabs(&[daemon("daemon-3", "build.lan", false)]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &HashMap::new(),
                chosen: 9,
            },
            Layout::Tabs,
        );
        assert_eq!(outline(&nodes)[0], "tabs:build");
    }

    #[test]
    fn the_row_of_tabs_reads_this_computer_when_the_local_daemon_is_chosen() {
        let tabs = tabs(&[daemon("daemon-3", "build.lan", false)]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &HashMap::new(),
                chosen: 0,
            },
            Layout::Tabs,
        );
        assert_eq!(
            outline(&nodes)[0],
            format!("tabs:{}", AppLabels::default().this_computer)
        );
    }

    #[test]
    fn the_application_s_lines_and_every_tab_s_labels_are_the_local_windows_language() {
        // The interface language is the client's, shared by every window: the shared lines and the
        // remote tabs' Disconnect read in the language the local window's UI sent.
        let local = parsed(
            r#"{"sections": [], "open": "打开", "quit": "完全退出", "updates": "检查更新",
                "disconnect": "断开连接",
                "app": {"this_computer": "这台 Mac"}}"#,
        );
        let tabs = tabs(&[daemon("daemon-3", "build.lan", false)]);
        let menus = HashMap::from([("main".to_owned(), local)]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 0,
            },
            Layout::Tabs,
        );
        let lines = outline(&nodes);
        assert_eq!(lines[0], "tabs:这台 Mac");
        for expected in [
            "打开=tray:main:open",
            "打开=tray:daemon-3:open",
            "断开连接=tray:daemon-3:disconnect",
            "检查更新=tray:main:updates",
            "完全退出=tray:main:quit",
        ] {
            assert!(
                lines.contains(&expected.to_owned()),
                "{expected}: {lines:?}"
            );
        }
    }

    #[test]
    fn a_remote_daemon_offers_no_update_check_and_no_quit_of_its_own() {
        let tabs = tabs(&[
            daemon("daemon-3", "a", false),
            daemon("daemon-4", "b", false),
        ]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &HashMap::new(),
                chosen: 0,
            },
            Layout::Tabs,
        );
        for node in &nodes {
            if let Node::Line {
                id: Some(id),
                owner: Some(owner),
                ..
            } = node
            {
                if *owner > 0 {
                    assert!(
                        !id.ends_with(":quit") && !id.ends_with(":updates"),
                        "{id} belongs to a remote tab"
                    );
                }
            }
        }
        // Check for updates and Quit completely are the application's, once, below the tabs.
        let ids: Vec<String> = nodes
            .iter()
            .filter_map(|node| match node {
                Node::Line { id: Some(id), .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        for shared in ["tray:main:updates", "tray:main:quit"] {
            assert_eq!(ids.iter().filter(|id| *id == shared).count(), 1, "{shared}");
        }
    }

    #[test]
    fn in_the_nested_layout_a_daemon_held_for_the_browser_is_a_submenu_that_opens_it_there() {
        let tabs = tabs(&[daemon("browser-e-old", "old", true)]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &HashMap::new(),
                chosen: 0,
            },
            Layout::Nested,
        );
        assert_eq!(
            outline(&nodes)[..3],
            [
                "old[Open in browser=remote-window-open:e-old, Disconnect=tray:browser-e-old:disconnect]",
                "-",
                "Check for updates=tray:main:updates",
            ]
        );
    }

    #[test]
    fn the_menu_offers_no_way_to_connect_to_a_remote_daemon() {
        // Connecting is the Remote app menu's, the Dock menu's, the Windows system menu's and the jump
        // list's: the icon's menu lists only the connected daemons' tabs.
        let tabs = tabs(&[]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &HashMap::new(),
                chosen: 0,
            },
            Layout::Tabs,
        );
        assert_eq!(
            outline(&nodes),
            vec![
                "Check for updates=tray:main:updates",
                "Open board=tray:main:open",
                "-",
                "Quit completely=tray:main:quit",
            ]
        );
    }

    #[test]
    fn whichever_tab_is_shown_no_separator_starts_its_lines_or_follows_another() {
        // An empty tab (nothing sent yet, or a daemon held for the browser) beside a full one.
        let tabs = tabs(&[
            daemon("daemon-3", "build.lan", false),
            daemon("browser-e-old", "old", true),
        ]);
        let none = HashMap::new();
        let some = HashMap::from([
            ("main".to_owned(), menu(false, "s1")),
            ("daemon-3".to_owned(), menu(true, "r1")),
        ]);
        for menus in [&none, &some] {
            let nodes = compose(
                &Contents {
                    tabs: &tabs,
                    menus,
                    chosen: 0,
                },
                Layout::Tabs,
            );
            for shown in 0..tabs.len() {
                let visible: Vec<&Node> = nodes
                    .iter()
                    .filter(|node| node.owner().is_none_or(|owner| owner == shown))
                    .filter(|node| !matches!(node, Node::Tabs { .. }))
                    .collect();
                let separator = |node: &&Node| matches!(node, Node::Separator { .. });
                assert!(
                    !separator(&visible[0]),
                    "tab {shown} starts with a separator"
                );
                assert!(
                    visible
                        .windows(2)
                        .all(|pair| !(separator(&pair[0]) && separator(&pair[1]))),
                    "tab {shown} doubles a separator"
                );
                assert!(!separator(visible.last().unwrap()));
            }
        }
    }
}
