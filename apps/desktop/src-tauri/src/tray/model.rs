//! The tray menu as the UI sends it, parsed whole on every platform.

use std::collections::HashMap;

use serde::Deserialize;

/// The tray menu as the UI words it, already in its language: sections of sessions, each under a
/// heading, submenus of rows (headers, quota windows, notes) that cannot be chosen, then the Updates,
/// Open and Quit labels, whether the icon should show its attention dot, and its selected-account quota.
/// Text, opaque session ids and the pictures beside the lines — the UI decides what is listed and
/// how it reads, the same way it decides the Dock badge's count. The pictures are drawn by the UI,
/// which already has every agent's mark, status glyph and bar colour, so a new agent needs nothing
/// here: `icons` holds each distinct picture once, as a base64 PNG under a key the lines name, at
/// twice the size they show at, and a quota bar's colour is sent as sRGB bytes.
#[derive(Clone, Deserialize)]
pub struct TrayMenu {
    pub(super) sections: Vec<TraySection>,
    #[serde(default)]
    pub(super) submenus: Vec<TraySubmenu>,
    #[serde(default)]
    pub(super) icons: HashMap<String, LineIcon>,
    pub(super) open: String,
    #[serde(default = "default_updates_label")]
    pub(super) updates: String,
    pub(super) quit: String,
    /// Whether the icon shows the attention dot, which asks for the user's attention.
    #[serde(default)]
    pub(super) attention: bool,
    /// The selected session account's remaining quota, already chosen and rounded by the UI: the
    /// 5-hour window's when the account has a weekly one too, else its one window's.
    #[serde(default)]
    pub(super) quota_remaining: Option<u8>,
    /// The weekly window's remainder, drawn as a thinner capsule beneath when the account has a
    /// 5-hour window as well.
    #[serde(default)]
    pub(super) quota_weekly_remaining: Option<u8>,
    /// The UI's localized account, window and remainder for the icon's accessible name.
    #[serde(default)]
    pub(super) quota_label: Option<String>,
}

/// A picture: a base64 PNG, and whether it is a template image, drawn in black for its alpha alone
/// so macOS tints it to suit the menu.
#[derive(Clone, Deserialize)]
pub(super) struct LineIcon {
    pub(super) png: String,
    pub(super) template: bool,
}

#[derive(Clone, Deserialize)]
pub(super) struct TraySection {
    pub(super) heading: String,
    pub(super) icon: Option<String>,
    pub(super) items: Vec<TrayItem>,
}

/// A submenu, opened from a line of the menu that shows its title.
#[derive(Clone, Deserialize)]
pub struct TraySubmenu {
    pub(super) title: String,
    /// The picture beside the title, such as the agent's mark: a key of `icons`.
    #[serde(default)]
    pub(super) icon: Option<String>,
    pub items: Vec<SubmenuItem>,
    /// The current console's account quota for this agent, selected by the UI.
    #[serde(default)]
    pub quota_remaining: Option<u8>,
    /// Weekly remainder beneath the 5-hour bar, absent for single-window accounts.
    #[serde(default)]
    pub quota_weekly_remaining: Option<u8>,
    #[serde(default)]
    pub quota_label: Option<String>,
}

/// An item with no `session` is a line that cannot be chosen, such as "3 more…".
#[derive(Clone, Deserialize)]
pub(super) struct TrayItem {
    pub(super) label: String,
    pub(super) icon: Option<String>,
    pub(super) session: Option<String>,
}

/// One window of a two-layer meter: its name and detail, and how much of its bar is filled (0 to 1).
#[derive(Clone, Deserialize)]
pub struct MeterLayer {
    pub name: String,
    pub detail: String,
    pub fraction: f64,
}

#[derive(Clone, Deserialize)]
pub struct HeaderStatus {
    pub label: String,
    #[serde(default)]
    pub foreground_colour: Option<[u8; 3]>,
    #[serde(default)]
    pub background_colour: Option<[u8; 4]>,
}

/// What a submenu holds, none of it choosable: an account's header (with when its reading was
/// taken), a quota window's row with the bar the shell draws (`fraction` of it filled, in
/// `fill_colour`, or in `used_up_colour` when the account's quota is `used_up`), the 5-hour and weekly
/// windows as one two-layer meter, a note, and the rules between accounts. `label` is a line's whole
/// phrase (for a header, when it says more than `title`): its menu item's title, which is what
/// assistive technology reads.
#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SubmenuItem {
    Header {
        title: String,
        #[serde(default)]
        status: Option<HeaderStatus>,
        #[serde(default)]
        suffix: Option<String>,
        /// When the account's reading was taken, drawn right-aligned on its line.
        #[serde(default)]
        detail: Option<String>,
        #[serde(default)]
        label: Option<String>,
    },
    Window {
        name: String,
        detail: String,
        label: String,
        fraction: f64,
        /// The bar's colour, as sRGB bytes from the UI stylesheet.
        #[serde(default)]
        fill_colour: Option<[u8; 3]>,
        #[serde(default)]
        used_up: bool,
        /// The bar's colour when `used_up`, as sRGB bytes from the UI stylesheet.
        #[serde(default)]
        used_up_colour: Option<[u8; 3]>,
    },
    Meter {
        session: MeterLayer,
        weekly: MeterLayer,
        label: String,
        /// The bar's colour, as sRGB bytes from the UI stylesheet.
        #[serde(default)]
        fill_colour: Option<[u8; 3]>,
        #[serde(default)]
        used_up: bool,
        /// The bars' colour when `used_up`, as sRGB bytes from the UI stylesheet.
        #[serde(default)]
        used_up_colour: Option<[u8; 3]>,
    },
    Note {
        text: String,
    },
    Separator,
}

impl SubmenuItem {
    /// The title the shell gives the item of its line: what it reads as in a menu that cannot
    /// draw it as a row, and what assistive technology reads for every row drawn.
    pub(super) fn text(&self) -> Option<&str> {
        match self {
            Self::Header { title, label, .. } => Some(label.as_deref().unwrap_or(title)),
            Self::Window { label, .. } | Self::Meter { label, .. } => Some(label),
            Self::Note { text } => Some(text),
            Self::Separator => None,
        }
    }
}

fn default_updates_label() -> String {
    "Check for updates".to_string()
}

impl Default for TrayMenu {
    /// What the menu reads before the UI has handed over its own: no sessions, English labels.
    fn default() -> Self {
        Self {
            sections: Vec::new(),
            submenus: Vec::new(),
            icons: HashMap::new(),
            open: "Open board".to_string(),
            updates: default_updates_label(),
            quit: "Quit completely".to_string(),
            attention: false,
            quota_remaining: None,
            quota_weekly_remaining: None,
            quota_label: None,
        }
    }
}

/// Menus the tests of several modules parse.
#[cfg(test)]
pub(super) mod fixtures {
    use super::TrayMenu;

    pub fn parsed(json: &str) -> TrayMenu {
        serde_json::from_str(json).expect("a menu")
    }

    pub const WITH_SUBMENUS: &str = r#"{
        "sections": [
            {"heading": "Working (1)", "icon": "status", "items": [
                {"label": "Fix", "icon": "codex", "session": "s1"},
                {"label": "1 more…"}
            ]}
        ],
        "submenus": [
            {"title": "Codex quota", "icon": "codex", "items": [
                {"kind": "header", "title": "Default · Plus", "detail": "Updated 1 minute ago",
                 "label": "Default · Plus · updated 1 minute ago"},
                {"kind": "window", "name": "5-hour", "detail": "82% remaining · resets in 1h",
                 "label": "5-hour 82% remaining · resets in 1h", "fraction": 0.82, "fill_colour": [61, 123, 245]},
                {"kind": "meter",
                 "session": {"name": "5-hour", "detail": "82% remaining · resets in 1h", "fraction": 0.82},
                 "weekly": {"name": "Weekly", "detail": "40% remaining · resets in 2d", "fraction": 0.4},
                 "label": "5-hour 82% remaining · resets in 1h, Weekly 40% remaining · resets in 2d",
                 "fill_colour": [61, 123, 245]},
                {"kind": "separator"},
                {"kind": "header", "title": "Work"},
                {"kind": "note", "text": "Quota unavailable"}
            ]},
            {"title": "Grok quota", "icon": "grok", "items": [{"kind": "note", "text": "Reading quota…"}]}
        ],
        "icons": {
            "status": {"png": "", "template": false},
            "grok": {"png": "", "template": false},
            "codex": {"png": "", "template": true}
        },
        "open": "Open",
        "quit": "Quit"
    }"#;
}

#[cfg(test)]
mod tests {
    use super::fixtures::{parsed, WITH_SUBMENUS};
    use super::*;

    #[test]
    fn a_menu_asks_for_attention_only_when_the_ui_says_so() {
        let bare = parsed(r#"{"sections": [], "open": "Open", "quit": "Quit"}"#);
        assert!(!bare.attention);
        let raised =
            parsed(r#"{"sections": [], "open": "Open", "quit": "Quit", "attention": true}"#);
        assert!(raised.attention);
        assert!(!TrayMenu::default().attention);
    }

    #[test]
    fn selected_quota_is_optional_and_keeps_both_endpoints() {
        let bare = parsed(r#"{"sections": [], "open": "Open", "quit": "Quit"}"#);
        assert_eq!(bare.quota_remaining, None);
        assert_eq!(bare.quota_weekly_remaining, None);
        assert_eq!(bare.quota_label, None);
        let layered = parsed(
            r#"{"sections": [], "open": "Open", "quit": "Quit", "quota_remaining": 80, "quota_weekly_remaining": 30}"#,
        );
        assert_eq!(layered.quota_remaining, Some(80));
        assert_eq!(layered.quota_weekly_remaining, Some(30));
        for remaining in [0, 100] {
            let menu = parsed(&format!(
                r#"{{"sections": [], "open": "Open", "quit": "Quit", "quota_remaining": {remaining}, "quota_label": "Work quota"}}"#
            ));
            assert_eq!(menu.quota_remaining, Some(remaining));
            assert_eq!(menu.quota_label.as_deref(), Some("Work quota"));
        }
    }

    #[test]
    fn submenu_quota_is_optional_and_preserves_zero_remaining() {
        let legacy = parsed(WITH_SUBMENUS);
        assert_eq!(legacy.submenus[0].quota_remaining, None);
        assert_eq!(legacy.submenus[0].quota_weekly_remaining, None);
        assert_eq!(legacy.submenus[0].quota_label, None);
        let menu = parsed(
            r#"{
            "sections": [], "open": "Open", "quit": "Quit",
            "submenus": [{"title": "Claude quota", "items": [],
                "quota_remaining": 0,
                "quota_weekly_remaining": 100,
                "quota_label": "Claude · Work · 5-hour · 0% remaining"}]
        }"#,
        );
        assert_eq!(menu.submenus[0].quota_remaining, Some(0));
        assert_eq!(menu.submenus[0].quota_weekly_remaining, Some(100));
        assert_eq!(
            menu.submenus[0].quota_label.as_deref(),
            Some("Claude · Work · 5-hour · 0% remaining")
        );
    }

    #[test]
    fn a_line_reads_as_its_whole_phrase_where_it_is_not_drawn_as_a_row() {
        let menu = parsed(WITH_SUBMENUS);
        let texts: Vec<_> = menu.submenus[0]
            .items
            .iter()
            .map(SubmenuItem::text)
            .collect();
        assert_eq!(
            texts,
            vec![
                Some("Default · Plus · updated 1 minute ago"),
                Some("5-hour 82% remaining · resets in 1h"),
                Some("5-hour 82% remaining · resets in 1h, Weekly 40% remaining · resets in 2d"),
                None,
                Some("Work"),
                Some("Quota unavailable"),
            ]
        );
    }

    #[test]
    fn quota_bars_are_used_up_only_when_the_item_says_so() {
        let window = |used_up: Option<bool>| {
            let mut value = serde_json::json!({
                "kind": "window", "name": "5h", "detail": "0% remaining", "label": "5h 0% remaining",
                "fraction": 0.0, "fill_colour": [212, 99, 58]
            });
            if let Some(used_up) = used_up {
                value["used_up"] = serde_json::json!(used_up);
                value["used_up_colour"] = serde_json::json!([123, 123, 133]);
            }
            serde_json::from_value::<SubmenuItem>(value).unwrap()
        };
        for (sent, expected) in [(None, false), (Some(false), false), (Some(true), true)] {
            assert!(
                matches!(window(sent), SubmenuItem::Window { used_up, .. } if used_up == expected)
            );
        }
        assert!(matches!(
            window(Some(true)),
            SubmenuItem::Window {
                used_up_colour: Some([123, 123, 133]),
                ..
            }
        ));
        assert!(matches!(
            window(None),
            SubmenuItem::Window {
                used_up_colour: None,
                fill_colour: Some([212, 99, 58]),
                ..
            }
        ));
        let meter: SubmenuItem = serde_json::from_value(serde_json::json!({
            "kind": "meter", "label": "x", "fill_colour": [4, 5, 6], "used_up": true,
            "used_up_colour": [1, 2, 3],
            "session": {"name": "5h", "detail": "d", "fraction": 0.0},
            "weekly": {"name": "Weekly", "detail": "d", "fraction": 0.5}
        }))
        .unwrap();
        assert!(matches!(
            meter,
            SubmenuItem::Meter {
                used_up: true,
                used_up_colour: Some([1, 2, 3]),
                fill_colour: Some([4, 5, 6]),
                ..
            }
        ));
    }
}
