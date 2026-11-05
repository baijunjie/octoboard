//! The status item's own views: its accessible name and the attention dot laid over its button.

use super::lines::picture_view;
use crate::tray::icon;

/// The whole quota phrase is the status button's accessible name; the icon has no tooltip.
pub(in crate::tray) fn set_icon_label(tray: &tauri::tray::TrayIcon, label: &str) {
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
pub(in crate::tray) fn show_dot(tray: &tauri::tray::TrayIcon, shown: bool) -> bool {
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

/// The dot shares the octopus's complete canvas, including the transparent space where the
/// quota pill sits, so the same image rectangle aligns both with and without a bar.
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
