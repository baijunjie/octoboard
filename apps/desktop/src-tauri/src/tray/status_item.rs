//! The icon the menu bar shows: its picture, the layers laid over it, and its accessible name.
//! There is one icon for every daemon: its attention dot is up while any daemon has a session
//! waiting for the user, and its quota capsule, its keep-awake eyes and its accessible name follow
//! the window the user focused last (`look_of`).

use std::collections::HashMap;
use std::sync::Mutex;

use super::icon;
use super::model::TrayMenu;
use super::names;
use super::platform::{picture, set_icon_label, show_layers, TEMPLATE};
use crate::menu::APP_NAME;

/// What the icon is to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Look {
    attention: bool,
    quota: Option<icon::Quota>,
    awake: bool,
    /// Its accessible name, which is its tooltip on Windows.
    label: String,
}

/// The look of the icon: the dot while any window's menu raises a hand, and the quota and the hold
/// of the window labelled `focus`, whose daemon's name (`name`, when more than one daemon is
/// shown) leads the accessible name. A window whose UI has sent nothing shows the octopus alone.
pub(super) fn look_of(
    menus: &HashMap<String, TrayMenu>,
    awake: &HashMap<String, bool>,
    focus: &str,
    name: Option<&str>,
) -> Look {
    let attention = menus.values().any(|menu| menu.attention);
    let held = awake.get(focus).copied().unwrap_or(false);
    let menu = menus.get(focus);
    let quota = menu.and_then(|menu| {
        menu.quota_remaining.map(|remaining| icon::Quota {
            main: remaining.min(100),
            thin: menu.quota_weekly_remaining.map(|weekly| weekly.min(100)),
        })
    });
    let label = match menu {
        Some(menu) if held => menu.keep_awake_label.clone(),
        Some(menu) => quota
            .and(menu.quota_label.as_deref())
            .unwrap_or(APP_NAME)
            .to_owned(),
        None if held => super::model::default_keep_awake_label(),
        None => APP_NAME.to_owned(),
    };
    Look {
        attention,
        quota,
        awake: held,
        label: names::named(name, &label),
    }
}

/// The look the icon has now, so an update that changes nothing it shows leaves it alone.
pub(super) type Shown = Mutex<Option<Look>>;

/// Puts `look` on the icon: its accessible name, then its picture. Swaps the picture and shows or
/// hides the layers (the dot and the eyes) together, keeping the picture a template image, which a
/// plain `set_icon` would drop; both happen on the main thread in the same pass, so the menu bar
/// never draws the cut-out octopus without its layers, or a layer over the whole octopus. When a
/// layer cannot be put up, the icon stays the octopus alone, which still reads as the octopus where
/// the cut-out one would look broken, and the look shown is left unknown, so the next update tries
/// again.
pub(super) fn show_look(
    shown: &Shown,
    tray: &tauri::tray::TrayIcon,
    look: Look,
) -> Result<(), String> {
    let mut shown = shown.lock().unwrap_or_else(|err| err.into_inner());
    if shown.as_ref() == Some(&look) {
        return Ok(());
    }
    set_icon_label(tray, &look.label);
    let (attention, awake, quota) = (look.attention, look.awake, look.quota);
    tray.set_icon_with_as_template(Some(picture(attention, awake, quota)), TEMPLATE)
        .map_err(|err| err.to_string())?;
    let layers_up = show_layers(tray, attention, awake);
    if (attention || awake) && !layers_up {
        tray.set_icon_with_as_template(Some(picture(false, false, quota)), TEMPLATE)
            .map_err(|err| err.to_string())?;
        show_layers(tray, false, false);
        *shown = None;
        return Ok(());
    }
    *shown = Some(look);
    Ok(())
}

#[cfg(test)]
mod tests {
    use tauri::image::Image;

    use super::super::model::{fixtures, TrayMenu};
    use super::*;

    #[test]
    fn both_looks_of_the_icon_and_the_dot_are_decodable_template_pictures() {
        let dot = Image::from_bytes(&icon::dot_png().expect("dot PNG")).expect("the dot decodes");
        for image in [
            icon::octopus(false, false, None),
            icon::octopus(true, false, None),
            dot,
        ] {
            assert_eq!((image.width(), image.height()), (36, 36));
            assert!(image.rgba().chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
        }
        assert_ne!(
            icon::octopus(true, false, None).rgba(),
            icon::octopus(false, false, None).rgba()
        );
    }

    #[test]
    fn the_dot_falls_inside_the_corner_cut_out_of_the_octopus() {
        let dot = Image::from_bytes(&icon::dot_png().expect("dot PNG")).expect("the dot decodes");
        let octopus = icon::octopus(true, false, None);
        let alphas = |image: &Image<'_>| -> Vec<u8> {
            image.rgba().chunks(4).map(|pixel| pixel[3]).collect()
        };
        let (dot, octopus) = (alphas(&dot), alphas(&octopus));
        assert!(dot.iter().any(|&alpha| alpha > 0));
        assert!(dot
            .iter()
            .zip(&octopus)
            .all(|(&dot, &octopus)| dot == 0 || octopus == 0));
    }

    fn menu(json: &str) -> TrayMenu {
        fixtures::parsed(json)
    }

    const WORK: &str = r#"{"sections": [], "open": "Open", "quit": "Quit", "quota_remaining": 40,
        "quota_label": "Work quota", "keep_awake_label": "Work quota. Awake"}"#;

    #[test]
    fn the_look_follows_the_focused_window_s_quota_and_hold() {
        let menus = HashMap::from([
            ("main".to_owned(), menu(WORK)),
            (
                "daemon-1".to_owned(),
                menu(r#"{"sections": [], "open": "Open", "quit": "Quit", "attention": true}"#),
            ),
        ]);
        let awake = HashMap::from([("daemon-1".to_owned(), true)]);
        let local = look_of(&menus, &awake, "main", None);
        assert_eq!(local.quota.map(|quota| quota.main), Some(40));
        assert!(!local.awake);
        assert_eq!(local.label, "Work quota");
        // The dot is up for a raised hand on any daemon, whichever window has the focus.
        assert!(local.attention);
        let remote = look_of(&menus, &awake, "daemon-1", Some("lab"));
        assert_eq!(remote.quota, None);
        assert!(remote.awake);
        assert_eq!(
            remote.label,
            "lab — Octoboard is keeping the computer awake"
        );
    }

    #[test]
    fn a_focused_window_that_sent_no_menu_shows_the_octopus_alone() {
        let look = look_of(&HashMap::new(), &HashMap::new(), "daemon-4", None);
        assert!(!look.attention && !look.awake && look.quota.is_none());
        assert_eq!(look.label, "Octoboard");
    }

    #[test]
    fn the_hold_names_the_icon_by_the_awake_label() {
        let menus = HashMap::from([("main".to_owned(), menu(WORK))]);
        let awake = HashMap::from([("main".to_owned(), true)]);
        assert_eq!(
            look_of(&menus, &awake, "main", None).label,
            "Work quota. Awake"
        );
        assert_eq!(
            look_of(&menus, &awake, "main", Some("This Mac")).label,
            "This Mac — Work quota. Awake"
        );
    }

    #[test]
    fn a_quota_the_ui_sends_beyond_a_whole_bar_is_drawn_as_a_whole_bar() {
        let menus = HashMap::from([(
            "main".to_owned(),
            menu(
                r#"{"sections": [], "open": "Open", "quit": "Quit", "quota_remaining": 250,
                    "quota_weekly_remaining": 180, "quota_label": "Q"}"#,
            ),
        )]);
        let look = look_of(&menus, &HashMap::new(), "main", None);
        let quota = look.quota.expect("a quota");
        assert_eq!((quota.main, quota.thin), (100, Some(100)));
    }

    #[test]
    fn a_quota_label_names_the_icon_only_while_a_quota_is_drawn() {
        let menus = HashMap::from([(
            "main".to_owned(),
            menu(r#"{"sections": [], "open": "Open", "quit": "Quit", "quota_label": "Orphan"}"#),
        )]);
        assert_eq!(
            look_of(&menus, &HashMap::new(), "main", None).label,
            "Octoboard"
        );
    }

    #[test]
    fn the_dot_stays_up_while_the_window_focused_is_one_whose_daemon_raises_no_hand() {
        let raised = menu(r#"{"sections": [], "open": "Open", "quit": "Quit", "attention": true}"#);
        let calm = menu(r#"{"sections": [], "open": "Open", "quit": "Quit"}"#);
        let menus = HashMap::from([("main".to_owned(), calm), ("daemon-2".to_owned(), raised)]);
        for focus in ["main", "daemon-2", "daemon-9"] {
            assert!(
                look_of(&menus, &HashMap::new(), focus, None).attention,
                "{focus}"
            );
        }
        let none = HashMap::from([(
            "main".to_owned(),
            menu(r#"{"sections": [], "open": "Open", "quit": "Quit"}"#),
        )]);
        assert!(!look_of(&none, &HashMap::new(), "main", None).attention);
    }

    #[test]
    fn the_eyes_follow_the_focused_window_s_own_hold_and_not_another_s() {
        let menus = HashMap::from([("main".to_owned(), menu(WORK))]);
        let awake = HashMap::from([("daemon-1".to_owned(), true)]);
        assert!(!look_of(&menus, &awake, "main", None).awake);
        assert!(look_of(&menus, &awake, "daemon-1", None).awake);
        // A window that closed has nothing held: the eyes go with it.
        assert!(!look_of(&menus, &HashMap::new(), "daemon-1", None).awake);
    }
}
