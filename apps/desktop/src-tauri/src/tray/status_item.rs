//! The icon the menu bar shows: its picture, the attention dot laid over it, and its accessible name.

use tauri::{AppHandle, Manager};

use super::icon;
use super::platform::{picture, show_dot, TEMPLATE};
use super::TrayState;

/// Both values change the picture; an account's live quota can move while attention stays put.
#[derive(Default, PartialEq, Eq)]
pub(super) struct IconState {
    attention: bool,
    quota: Option<icon::Quota>,
}

/// Swaps the icon's picture and shows or hides the dot when attention or quota changes,
/// keeping the picture a template image, which a plain `set_icon` would drop. Both happen on
/// the main thread in the same pass (a command and the held menu's block both run there), so the
/// menu bar never draws the cut-out octopus without its dot, or the dot over the whole octopus.
/// When the dot cannot be put up, the icon stays the octopus alone, which still reads as the
/// octopus where the cut-out one would look broken, and the look shown is left as it was, so the
/// next menu tries again.
pub(super) fn show_icon(
    app: &AppHandle,
    tray: &tauri::tray::TrayIcon,
    attention: bool,
    quota: Option<icon::Quota>,
) -> Result<(), String> {
    let state = app.state::<TrayState>();
    let mut shown = state
        .icon_shown
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let next = IconState { attention, quota };
    if *shown == next {
        return Ok(());
    }
    tray.set_icon_with_as_template(Some(picture(attention, quota)), TEMPLATE)
        .map_err(|err| err.to_string())?;
    let dot_up = show_dot(tray, attention);
    if attention && !dot_up {
        tray.set_icon_with_as_template(Some(picture(false, quota)), TEMPLATE)
            .map_err(|err| err.to_string())?;
        *shown = IconState {
            attention: false,
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

    use super::*;

    #[test]
    fn both_looks_of_the_icon_and_the_dot_are_decodable_template_pictures() {
        let dot = Image::from_bytes(&icon::dot_png().expect("dot PNG")).expect("the dot decodes");
        for image in [icon::octopus(false, None), icon::octopus(true, None), dot] {
            assert_eq!((image.width(), image.height()), (44, 44));
            assert!(image.rgba().chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
        }
        assert_ne!(
            icon::octopus(true, None).rgba(),
            icon::octopus(false, None).rgba()
        );
    }

    #[test]
    fn the_dot_falls_inside_the_corner_cut_out_of_the_octopus() {
        let dot = Image::from_bytes(&icon::dot_png().expect("dot PNG")).expect("the dot decodes");
        let octopus = icon::octopus(true, None);
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
}
