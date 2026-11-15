//! What the flyout on Windows is handed to draw (`FlyoutModel`): the same lines `tabs::compose`
//! lays out for the macOS menu, sorted into each tab's own lines and the lines every tab shows,
//! with each line's picture and each quota submenu resolved from the menu of the tab it belongs to,
//! so the page draws them without knowing any daemon's menu.

// Only Windows draws the flyout; the model is built and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashMap;

use serde::Serialize;

use super::model::{LineIcon, TrayMenu, TraySubmenu};
use super::tabs::{raises_hand, Node, Tab};

/// The flyout's content, as its page (`packages/ui`, `TrayFlyout`) reads it.
#[derive(Clone, Debug, Serialize)]
pub struct FlyoutModel {
    pub(super) tabs: Vec<FlyoutTab>,
    /// The tab chosen when the model was built.
    pub(super) chosen: usize,
    /// What each tab lists (its sessions and quota rows), in the order of `tabs`.
    pub(super) panels: Vec<Vec<FlyoutLine>>,
    /// The lines every tab shows below its panel: Check for updates.
    pub(super) shared: Vec<FlyoutLine>,
    /// What can be done with each tab's daemon (Open board, Disconnect), in the order of `tabs`,
    /// below the shared lines.
    pub(super) actions: Vec<Vec<FlyoutLine>>,
    /// The lines every tab shows last: Quit completely.
    pub(super) footer: Vec<FlyoutLine>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(super) struct FlyoutTab {
    pub(super) key: String,
    pub(super) label: String,
    pub(super) remote: bool,
    /// Whether its daemon has a session waiting for the user.
    pub(super) raised: bool,
}

/// One line of a panel or of the shared lines.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(super) enum FlyoutLine {
    Heading {
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        icon: Option<LineIcon>,
    },
    Item {
        id: String,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        icon: Option<LineIcon>,
    },
    /// A line that cannot be chosen.
    Note {
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        icon: Option<LineIcon>,
    },
    Quota {
        submenu: FlyoutSubmenu,
    },
    Group {
        title: String,
        children: Vec<FlyoutLine>,
    },
    Separator,
}

impl std::fmt::Debug for FlyoutLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Heading { label, .. } => write!(f, "heading:{label}"),
            Self::Item { id, label, .. } => write!(f, "{label}={id}"),
            Self::Note { label, .. } => write!(f, "note:{label}"),
            Self::Quota { submenu } => write!(f, "quota:{}", submenu.title),
            Self::Group { title, children } => write!(f, "{title}{children:?}"),
            Self::Separator => write!(f, "-"),
        }
    }
}

/// A quota submenu with its picture resolved in place of the key the UI named it by.
#[derive(Clone, Serialize)]
pub(super) struct FlyoutSubmenu {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<LineIcon>,
    items: Vec<super::model::SubmenuItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quota_remaining: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quota_weekly_remaining: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quota_label: Option<String>,
}

impl FlyoutSubmenu {
    fn new(submenu: TraySubmenu, icon: Option<LineIcon>) -> Self {
        Self {
            title: submenu.title,
            icon,
            items: submenu.items,
            quota_remaining: submenu.quota_remaining,
            quota_weekly_remaining: submenu.quota_weekly_remaining,
            quota_label: submenu.quota_label,
        }
    }
}

/// The flyout's content, from the lines `compose` laid out in `Layout::Tabs` for `tabs`. The
/// application's own lines divide the menu: a tab's lines before the first of them (Check for
/// updates) are its panel and after it its actions, that line is the shared one, and the next
/// (Quit completely) is the footer. The application's separators are left out, the page drawing
/// its own between the runs. Each line's pictures are taken from the menu its tab's window sent
/// (`menus`, by window label), and the tabs' labels from `labels`.
pub(super) fn flyout_model(
    nodes: &[Node],
    tabs: &[Tab],
    menus: &HashMap<String, TrayMenu>,
    labels: &super::model::AppLabels,
    chosen: usize,
) -> FlyoutModel {
    let menu_of = |owner: Option<usize>| {
        owner
            .and_then(|index| tabs.get(index))
            .and_then(|tab| menus.get(&tab.key))
    };
    let mut panels: Vec<Vec<FlyoutLine>> = vec![Vec::new(); tabs.len()];
    let mut actions: Vec<Vec<FlyoutLine>> = vec![Vec::new(); tabs.len()];
    let mut shared = Vec::new();
    let mut footer = Vec::new();
    // How many of the application's own lines have been passed.
    let mut passed = 0;
    for node in nodes {
        let owner = node.owner();
        if owner.is_none() && matches!(node, Node::Separator { .. }) {
            continue;
        }
        let Some(line) = line_of(node, &menu_of) else {
            continue;
        };
        match owner {
            Some(index) if index < panels.len() => {
                if passed == 0 {
                    panels[index].push(line);
                } else {
                    actions[index].push(line);
                }
            }
            _ => {
                if passed == 0 {
                    shared.push(line);
                } else {
                    footer.push(line);
                }
                passed += 1;
            }
        }
    }
    FlyoutModel {
        tabs: tabs
            .iter()
            .map(|tab| FlyoutTab {
                key: tab.key.clone(),
                label: tab.label(labels),
                remote: tab.remote_name().is_some(),
                raised: raises_hand(tab, menus),
            })
            .collect(),
        chosen: chosen.min(tabs.len().saturating_sub(1)),
        panels: panels.into_iter().map(trim_separators).collect(),
        shared,
        actions,
        footer,
    }
}

/// The line `node` is drawn as, or `None` for the row of tabs, which the page draws from `tabs`.
fn line_of<'a>(
    node: &Node,
    menu_of: &dyn Fn(Option<usize>) -> Option<&'a TrayMenu>,
) -> Option<FlyoutLine> {
    let picture = |owner: Option<usize>, key: &Option<String>| {
        let key = key.as_deref()?;
        menu_of(owner)?.icons.get(key).cloned()
    };
    Some(match node {
        Node::Tabs { .. } => return None,
        Node::Line {
            owner,
            id,
            label,
            icon,
            heading,
        } => {
            let icon = picture(*owner, icon);
            match (id, heading) {
                (_, true) => FlyoutLine::Heading {
                    label: label.clone(),
                    icon,
                },
                (Some(id), false) => FlyoutLine::Item {
                    id: id.clone(),
                    label: label.clone(),
                    icon,
                },
                (None, false) => FlyoutLine::Note {
                    label: label.clone(),
                    icon,
                },
            }
        }
        Node::Quota { owner, index } => {
            let menu = menu_of(Some(*owner))?;
            let submenu = menu.submenus.get(*index)?.clone();
            let icon = picture(Some(*owner), &submenu.icon);
            FlyoutLine::Quota {
                submenu: FlyoutSubmenu::new(submenu, icon),
            }
        }
        Node::Group {
            title, children, ..
        } => FlyoutLine::Group {
            title: title.clone(),
            children: children
                .iter()
                .filter_map(|child| line_of(child, menu_of))
                .collect(),
        },
        Node::Separator { .. } => FlyoutLine::Separator,
    })
}

/// `lines` without a separator at either end or beside another, which a panel's edge makes
/// redundant: the page draws the panel and the shared lines apart itself.
fn trim_separators(lines: Vec<FlyoutLine>) -> Vec<FlyoutLine> {
    let mut kept: Vec<FlyoutLine> = Vec::with_capacity(lines.len());
    for line in lines {
        let separator = matches!(line, FlyoutLine::Separator);
        if separator
            && kept
                .last()
                .is_none_or(|last| matches!(last, FlyoutLine::Separator))
        {
            continue;
        }
        kept.push(line);
    }
    if matches!(kept.last(), Some(FlyoutLine::Separator)) {
        kept.pop();
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::super::model::fixtures::{parsed, WITH_SUBMENUS};
    use super::super::model::AppLabels;
    use super::super::tabs::{compose, tabs, Contents, Layout};
    use super::*;
    use crate::remote::ConnectedDaemon;

    fn model(attention: bool) -> FlyoutModel {
        let tabs = tabs(&[ConnectedDaemon {
            key: "daemon-1".into(),
            entry_id: "e".into(),
            name: "build.lan".into(),
            in_browser: false,
        }]);
        let mut remote = parsed(WITH_SUBMENUS);
        remote.attention = attention;
        let menus = HashMap::from([("daemon-1".to_owned(), remote)]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 1,
            },
            Layout::Tabs,
        );
        flyout_model(&nodes, &tabs, &menus, &AppLabels::default(), 1)
    }

    #[test]
    fn each_tab_s_lines_go_to_its_panel_and_the_rest_are_shared() {
        let model = model(true);
        assert_eq!(
            model.tabs,
            vec![
                FlyoutTab {
                    key: "main".into(),
                    label: AppLabels::default().this_computer,
                    remote: false,
                    raised: false
                },
                FlyoutTab {
                    key: "daemon-1".into(),
                    label: "build".into(),
                    remote: true,
                    raised: true
                },
            ]
        );
        assert_eq!(model.chosen, 1);
        assert_eq!(format!("{:?}", model.panels[0]), "[]");
        assert_eq!(
            format!("{:?}", model.panels[1]),
            "[heading:Working (1), Fix=tray:daemon-1:session:s1, note:1 more…, -, \
             quota:Codex quota, quota:Grok quota]"
        );
        assert_eq!(
            format!("{:?}", model.shared),
            "[Check for updates=tray:main:updates]"
        );
        assert_eq!(
            format!("{:?}", model.actions[0]),
            "[Open board=tray:main:open]"
        );
        assert_eq!(
            format!("{:?}", model.actions[1]),
            "[Open board=tray:daemon-1:open, Disconnect=tray:daemon-1:disconnect]"
        );
        assert_eq!(
            format!("{:?}", model.footer),
            "[Quit completely=tray:main:quit]"
        );
    }

    #[test]
    fn the_page_is_handed_the_pictures_and_the_quota_rows_themselves() {
        let model = model(false);
        let json = serde_json::to_value(&model).unwrap();
        let panel = &json["panels"][1];
        assert_eq!(panel[0]["kind"], "heading");
        assert_eq!(panel[0]["icon"]["template"], false);
        assert_eq!(panel[1]["kind"], "item");
        assert_eq!(panel[1]["icon"]["template"], true);
        assert!(panel[2].get("icon").is_none());
        let quota = &panel[4]["submenu"];
        assert_eq!(quota["title"], "Codex quota");
        assert_eq!(quota["icon"]["template"], true);
        assert_eq!(quota["items"][1]["kind"], "window");
        assert_eq!(
            quota["items"][1]["fill_colour"],
            serde_json::json!([61, 123, 245])
        );
        assert!(quota.get("quota_remaining").is_none());
        assert_eq!(json["tabs"][1]["raised"], false);
    }

    #[test]
    fn separators_never_start_end_or_double_a_list() {
        let lines = vec![
            FlyoutLine::Separator,
            FlyoutLine::Note {
                label: "a".into(),
                icon: None,
            },
            FlyoutLine::Separator,
            FlyoutLine::Separator,
            FlyoutLine::Note {
                label: "b".into(),
                icon: None,
            },
            FlyoutLine::Separator,
        ];
        assert_eq!(
            format!("{:?}", trim_separators(lines)),
            "[note:a, -, note:b]"
        );
    }

    #[test]
    fn the_chosen_tab_is_held_to_the_tabs_there_are() {
        let tabs = tabs(&[]);
        let menus = HashMap::new();
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 0,
            },
            Layout::Tabs,
        );
        let model = flyout_model(&nodes, &tabs, &menus, &AppLabels::default(), 7);
        assert_eq!(model.chosen, 0);
        assert_eq!(model.tabs.len(), 1);
        assert_eq!(model.panels.len(), 1);
        // The flyout of a single daemon is that daemon's panel, the shared lines, its actions and
        // the footer.
        assert_eq!(
            format!("{:?}", model.actions[0]),
            "[Open board=tray:main:open]"
        );
        assert!(!model.tabs[0].remote);
    }

    #[test]
    fn a_daemon_held_for_the_browser_has_a_tab_whose_actions_open_it_there_and_disconnect() {
        let tabs = tabs(&[ConnectedDaemon {
            key: "browser-e".into(),
            entry_id: "e".into(),
            name: "old".into(),
            in_browser: true,
        }]);
        let menus = HashMap::new();
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 1,
            },
            Layout::Tabs,
        );
        let model = flyout_model(&nodes, &tabs, &menus, &AppLabels::default(), 1);
        assert!(model.tabs[1].remote && !model.tabs[1].raised);
        assert_eq!(
            format!("{:?}", model.actions[1]),
            "[Open in browser=remote-window-open:e, Disconnect=tray:browser-e:disconnect]"
        );
    }

    #[test]
    fn a_tab_whose_daemon_has_no_hand_raised_is_not_marked_and_the_other_is() {
        let tabs = tabs(&[ConnectedDaemon {
            key: "daemon-1".into(),
            entry_id: "e".into(),
            name: "build.lan".into(),
            in_browser: false,
        }]);
        let mut local = parsed(WITH_SUBMENUS);
        local.attention = true;
        let menus = HashMap::from([("main".to_owned(), local)]);
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 0,
            },
            Layout::Tabs,
        );
        let model = flyout_model(&nodes, &tabs, &menus, &AppLabels::default(), 0);
        assert!(model.tabs[0].raised);
        assert!(!model.tabs[1].raised);
    }

    #[test]
    fn a_tab_that_lists_nothing_has_an_empty_panel_and_the_shared_lines_still_follow() {
        let tabs = tabs(&[]);
        let menus = HashMap::new();
        let nodes = compose(
            &Contents {
                tabs: &tabs,
                menus: &menus,
                chosen: 0,
            },
            Layout::Tabs,
        );
        let model = flyout_model(&nodes, &tabs, &menus, &AppLabels::default(), 0);
        assert!(model.panels[0].is_empty());
        assert_eq!(
            format!("{:?}", model.shared),
            "[Check for updates=tray:main:updates]"
        );
        assert_eq!(
            format!("{:?}", model.actions[0]),
            "[Open board=tray:main:open]"
        );
        assert_eq!(
            format!("{:?}", model.footer),
            "[Quit completely=tray:main:quit]"
        );
    }
}
