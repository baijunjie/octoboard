//! The icon the menu bar shows: its picture, the layers laid over it, and its accessible name.

use tauri::{AppHandle, Manager};

use super::icon;
use super::model::default_keep_awake_label;
use super::platform::{picture, set_icon_label, show_layers, TEMPLATE};
use super::TrayState;
use crate::menu::APP_NAME;

/// What the icon is to show: what the last menu applied says, and whether the computer is kept
/// awake, which the UI forwards from the daemon's held state (`set_keep_awake_shown`).
pub(super) struct Look {
    attention: bool,
    quota: Option<icon::Quota>,
    /// The accessible name from the quota, or the application's name.
    label: String,
    /// The accessible name while the computer is kept awake.
    awake_label: String,
    awake: bool,
}

impl Default for Look {
    /// The octopus alone, as the icon is before the UI sends its first menu.
    fn default() -> Self {
        Self {
            attention: false,
            quota: None,
            label: APP_NAME.to_string(),
            awake_label: default_keep_awake_label(),
            awake: false,
        }
    }
}

impl Look {
    pub(super) fn from_menu(menu: &super::model::TrayMenu, awake: bool) -> Self {
        let quota = menu.quota_remaining.map(|remaining| icon::Quota {
            main: remaining.min(100),
            thin: menu.quota_weekly_remaining.map(|weekly| weekly.min(100)),
        });
        Self {
            attention: menu.attention,
            quota,
            label: quota
                .and(menu.quota_label.as_deref())
                .unwrap_or(APP_NAME)
                .to_string(),
            awake_label: menu.keep_awake_label.clone(),
            awake,
        }
    }

    pub(super) fn awake(&self) -> bool {
        self.awake
    }

    pub(super) fn set_awake(&mut self, awake: bool) {
        self.awake = awake;
    }

    fn label(&self) -> &str {
        if self.awake {
            &self.awake_label
        } else {
            &self.label
        }
    }
}

/// Both values change the picture; an account's live quota can move while attention stays put.
#[derive(Default, PartialEq, Eq)]
pub(super) struct IconState {
    attention: bool,
    awake: bool,
    quota: Option<icon::Quota>,
}

/// Puts `look` on the icon: its accessible name, then its picture. Taking the lock for the whole of
/// it keeps the menu's look and the hold's from landing out of order.
pub(super) fn show_look(
    app: &AppHandle,
    tray: &tauri::tray::TrayIcon,
    change: impl FnOnce(&mut Look),
) -> Result<(), String> {
    let state = app.state::<TrayState>();
    let mut look = state.look.lock().unwrap_or_else(|err| err.into_inner());
    change(&mut look);
    set_icon_label(tray, look.label());
    show_icon(app, tray, look.attention, look.awake, look.quota)
}

/// Swaps the icon's picture and shows or hides the layers (the dot and the eyes) when attention,
/// the hold or quota changes, keeping the picture a template image, which a plain `set_icon`
/// would drop. Both happen on the main thread in the same pass (a command and the held menu's
/// block both run there), so the menu bar never draws the cut-out octopus without its layers, or
/// a layer over the whole octopus. When a layer cannot be put up, the icon stays the octopus
/// alone, which still reads as the octopus where the cut-out one would look broken, and the look
/// shown is left as it was, so the next menu tries again.
fn show_icon(
    app: &AppHandle,
    tray: &tauri::tray::TrayIcon,
    attention: bool,
    awake: bool,
    quota: Option<icon::Quota>,
) -> Result<(), String> {
    let state = app.state::<TrayState>();
    let mut shown = state
        .icon_shown
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let next = IconState {
        attention,
        awake,
        quota,
    };
    if *shown == next {
        return Ok(());
    }
    tray.set_icon_with_as_template(Some(picture(attention, awake, quota)), TEMPLATE)
        .map_err(|err| err.to_string())?;
    let layers_up = show_layers(tray, attention, awake);
    if (attention || awake) && !layers_up {
        tray.set_icon_with_as_template(Some(picture(false, false, quota)), TEMPLATE)
            .map_err(|err| err.to_string())?;
        show_layers(tray, false, false);
        *shown = IconState {
            attention: false,
            awake: false,
            quota,
        };
        return Ok(());
    }
    *shown = next;
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

    #[test]
    fn the_icon_is_named_by_the_awake_label_only_while_the_hold_is_up() {
        let menu = fixtures::parsed(
            r#"{"sections": [], "open": "Open", "quit": "Quit", "quota_remaining": 40,
                "quota_label": "Work quota", "keep_awake_label": "Work quota. Awake"}"#,
        );
        let mut look = Look::from_menu(&menu, false);
        assert_eq!(look.label(), "Work quota");
        look.set_awake(true);
        assert_eq!(look.label(), "Work quota. Awake");
        // A menu that carries no quota names only the application, and a later one keeps the hold.
        let bare = Look::from_menu(&TrayMenu::default(), look.awake());
        assert_eq!(bare.label(), "Octoboard is keeping the computer awake");
        assert_eq!(
            Look::from_menu(&TrayMenu::default(), false).label(),
            "Octoboard"
        );
    }
}
