//! The icon the menu bar shows: its picture, the attention dot laid over it, and its accessible name.

use tauri::{AppHandle, Manager};

use super::icon;
#[cfg(target_os = "macos")]
use super::lines::picture_view;
use super::TrayState;

/// Both values change the picture; an account's live quota can move while attention stays put.
#[derive(Default, PartialEq, Eq)]
pub(super) struct IconState {
    attention: bool,
    quota: Option<icon::Quota>,
}

/// The whole quota phrase is the status button's accessible name; the icon has no tooltip.
#[cfg(target_os = "macos")]
pub(super) fn set_icon_label(tray: &tauri::tray::TrayIcon, label: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSAccessibility;
    use objc2_foundation::NSString;

    let label = label.to_string();
    let _ = tray.with_inner_tray_icon(move |inner| {
        if let Some(button) = MainThreadMarker::new()
            .and_then(|mtm| inner.ns_status_item().and_then(|item| item.button(mtm)))
        {
            button.setAccessibilityLabel(Some(&NSString::from_str(&label)));
        }
    });
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
    tray.set_icon_with_as_template(Some(icon::octopus(attention, quota)), true)
        .map_err(|err| err.to_string())?;
    let dot_up = show_dot(tray, attention);
    if attention && !dot_up {
        tray.set_icon_with_as_template(Some(icon::octopus(false, quota)), true)
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

#[cfg(target_os = "macos")]
thread_local! {
    /// The view that draws the attention dot on the status item's button, once made. AppKit's views
    /// live on the main thread, and so does this.
    static DOT_VIEW: std::cell::RefCell<Option<objc2::rc::Retained<objc2_app_kit::NSImageView>>> =
        const { std::cell::RefCell::new(None) };
}

/// Shows or hides the attention dot over the icon, and answers whether it is up. The dot is a
/// template picture in an image view whose tint is the system orange, a dynamic colour resolved as
/// it is drawn, so it suits a light or dark menu bar like the octopus under it. The view is made
/// the first time the dot shows (`make_dot_view`), and is laid over the rectangle the button
/// draws the octopus in each time it shows.
#[cfg(target_os = "macos")]
fn show_dot(tray: &tauri::tray::TrayIcon, shown: bool) -> bool {
    use objc2::MainThreadMarker;

    tray.with_inner_tray_icon(move |inner| {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let Some(button) = inner.ns_status_item().and_then(|item| item.button(mtm)) else {
            return false;
        };
        DOT_VIEW.with_borrow_mut(|dot| {
            let image = shown.then(|| dot_image(&button)).flatten();
            if shown && image.is_none() {
                if let Some(view) = dot.as_ref() {
                    view.setHidden(true);
                }
                return false;
            }
            if dot.is_none() {
                if let Some(image) = image.as_ref() {
                    *dot = Some(make_dot_view(&button, image, mtm));
                }
            }
            let Some(view) = dot.as_ref() else {
                return false;
            };
            if shown {
                view.setImage(image.as_deref());
                if let Some(cell) = button.cell() {
                    view.setFrame(cell.imageRectForBounds(button.bounds()));
                }
            }
            view.setHidden(!shown);
            shown
        })
    })
    .unwrap_or(false)
}

/// The dot is drawn only on macOS; elsewhere it is never up, so the icon keeps the octopus alone.
#[cfg(not(target_os = "macos"))]
fn show_dot(_tray: &tauri::tray::TrayIcon, _shown: bool) -> bool {
    false
}

/// The dot shares the octopus's complete canvas, including the transparent space where the
/// quota pill sits, so the same image rectangle aligns both with and without a bar.
#[cfg(target_os = "macos")]
fn dot_image(
    button: &objc2_app_kit::NSStatusBarButton,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSImage>> {
    use objc2::AnyThread;
    use objc2_app_kit::NSImage;
    use objc2_foundation::NSData;

    let png = icon::dot_png()?;
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&png))?;
    image.setSize(button.image()?.size());
    image.setTemplate(true);
    Some(image)
}

/// Puts the dot below the button's other subviews: the topmost receives the clicks that open
/// the menu. Its orange tint is a dynamic system colour, resolved in the menu bar's appearance.
#[cfg(target_os = "macos")]
fn make_dot_view(
    button: &objc2_app_kit::NSStatusBarButton,
    image: &objc2_app_kit::NSImage,
    mtm: objc2::MainThreadMarker,
) -> objc2::rc::Retained<objc2_app_kit::NSImageView> {
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSColor, NSImageScaling, NSWindowOrderingMode};

    let view = picture_view(image, mtm);
    view.setImageScaling(NSImageScaling::ScaleNone);
    view.setContentTintColor(Some(&NSColor::systemOrangeColor()));
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewMinXMargin
            | NSAutoresizingMaskOptions::ViewMaxXMargin
            | NSAutoresizingMaskOptions::ViewMinYMargin
            | NSAutoresizingMaskOptions::ViewMaxYMargin,
    );
    button.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, None);
    view
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
