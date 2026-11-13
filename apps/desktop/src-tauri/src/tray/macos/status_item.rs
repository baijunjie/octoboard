//! The status item's own views: its accessible name and the attention dot and red eyes laid over
//! its button.

use super::lines::picture_view;
use crate::tray::icon;

/// The whole quota phrase is the status button's accessible name; its tooltip names the application.
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

/// Shows or hides the status item. The item is hidden rather than removed from the status bar,
/// which tray-icon's own `set_visible` does by making a new one, so the button's views (the dot and
/// the eyes) and accessible name stay as they are.
pub(in crate::tray) fn set_visible(
    tray: &tauri::tray::TrayIcon,
    visible: bool,
) -> tauri::Result<()> {
    tray.with_inner_tray_icon(move |inner| {
        if let Some(item) = inner.ns_status_item() {
            item.setVisible(visible);
        }
    })
}

/// A template layer drawn over the button's octopus in a colour of its own, which the template
/// octopus cannot carry.
#[derive(Clone, Copy)]
enum Layer {
    /// The orange attention dot, at the upper right.
    Dot,
    /// The red eye sockets, over the octopus's own, while the computer is kept awake.
    Eyes,
}

impl Layer {
    fn png(self) -> Option<Vec<u8>> {
        match self {
            Self::Dot => icon::dot_png(),
            Self::Eyes => icon::eyes_png(),
        }
    }

    /// A dynamic system colour, resolved as the layer is drawn, so it suits a light or dark menu
    /// bar.
    fn colour(self) -> objc2::rc::Retained<objc2_app_kit::NSColor> {
        use objc2_app_kit::NSColor;

        match self {
            Self::Dot => NSColor::systemOrangeColor(),
            Self::Eyes => NSColor::systemRedColor(),
        }
    }
}

/// The views that draw the layers on the status item's button, once made.
struct LayerViews {
    dot: Option<objc2::rc::Retained<objc2_app_kit::NSImageView>>,
    eyes: Option<objc2::rc::Retained<objc2_app_kit::NSImageView>>,
}

impl LayerViews {
    fn slot(
        &mut self,
        layer: Layer,
    ) -> &mut Option<objc2::rc::Retained<objc2_app_kit::NSImageView>> {
        match layer {
            Layer::Dot => &mut self.dot,
            Layer::Eyes => &mut self.eyes,
        }
    }
}

thread_local! {
    /// AppKit's views live on the main thread, and so does this.
    static LAYER_VIEWS: std::cell::RefCell<LayerViews> =
        const { std::cell::RefCell::new(LayerViews { dot: None, eyes: None }) };
}

/// Shows or hides the attention dot and the red eyes over the icon, and answers whether every layer
/// asked for is up. Each is a template picture in an image view whose tint is a system colour
/// (`Layer::colour`), so it suits a light or dark menu bar like the octopus under it. A view is
/// made the first time its layer shows (`make_layer_view`), and is laid over the rectangle the
/// button draws the octopus in each time it shows.
pub(in crate::tray) fn show_layers(tray: &tauri::tray::TrayIcon, dot: bool, eyes: bool) -> bool {
    use objc2::MainThreadMarker;

    tray.with_inner_tray_icon(move |inner| {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let Some(button) = inner.ns_status_item().and_then(|item| item.button(mtm)) else {
            return false;
        };
        LAYER_VIEWS.with_borrow_mut(|views| {
            // Both are set, so a layer that cannot be put up still lets the other be hidden.
            let dot = show_layer(views, &button, Layer::Dot, dot, mtm);
            let eyes = show_layer(views, &button, Layer::Eyes, eyes, mtm);
            dot && eyes
        })
    })
    .unwrap_or(false)
}

/// Puts one layer in the requested state and answers whether it is there: a layer asked to show
/// answers false when its picture could not be made, and one asked to hide answers true once
/// hidden.
fn show_layer(
    views: &mut LayerViews,
    button: &objc2_app_kit::NSStatusBarButton,
    layer: Layer,
    shown: bool,
    mtm: objc2::MainThreadMarker,
) -> bool {
    let slot = views.slot(layer);
    let image = shown.then(|| layer_image(button, layer)).flatten();
    if shown && image.is_none() {
        if let Some(view) = slot.as_ref() {
            view.setHidden(true);
        }
        return false;
    }
    if slot.is_none() {
        if let Some(image) = image.as_ref() {
            *slot = Some(make_layer_view(button, image, layer, mtm));
        }
    }
    let Some(view) = slot.as_ref() else {
        // Nothing to hide: the layer was never shown.
        return !shown;
    };
    if shown {
        view.setImage(image.as_deref());
        if let Some(cell) = button.cell() {
            view.setFrame(cell.imageRectForBounds(button.bounds()));
        }
    }
    view.setHidden(!shown);
    // The requested state is in place, whether the layer is up or hidden.
    true
}

/// A layer shares the octopus's complete canvas, including the transparent space where the
/// quota pill sits, so the same image rectangle aligns both with and without a bar.
fn layer_image(
    button: &objc2_app_kit::NSStatusBarButton,
    layer: Layer,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSImage>> {
    use objc2::AnyThread;
    use objc2_app_kit::NSImage;
    use objc2_foundation::NSData;

    let png = layer.png()?;
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&png))?;
    image.setSize(button.image()?.size());
    image.setTemplate(true);
    Some(image)
}

/// Puts the layer below the button's other subviews: the topmost receives the clicks that open
/// the menu.
fn make_layer_view(
    button: &objc2_app_kit::NSStatusBarButton,
    image: &objc2_app_kit::NSImage,
    layer: Layer,
    mtm: objc2::MainThreadMarker,
) -> objc2::rc::Retained<objc2_app_kit::NSImageView> {
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSImageScaling, NSWindowOrderingMode};

    let view = picture_view(image, mtm);
    view.setImageScaling(NSImageScaling::ScaleNone);
    view.setContentTintColor(Some(&layer.colour()));
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewMinXMargin
            | NSAutoresizingMaskOptions::ViewMaxXMargin
            | NSAutoresizingMaskOptions::ViewMinYMargin
            | NSAutoresizingMaskOptions::ViewMaxYMargin,
    );
    button.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, None);
    view
}
