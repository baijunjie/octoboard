//! The quota openers: the lines of the menu bar icon's menu that open a quota submenu. They keep
//! AppKit's native interactive row. A capsule embedded in their attributed title shows the current
//! console's account quota without replacing submenu tracking, highlight or keyboard handling; the
//! menu item's accessible label gives its complete quota phrase.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAccessibility, NSBezierPath, NSColor, NSFont, NSFontAttributeName, NSImage, NSMenu,
    NSStringDrawing,
};
use objc2_foundation::{NSDictionary, NSRect, NSSize, NSString};

use super::lines::{rect, RowText, GAP, INSET, PICTURE_GAP, PICTURE_X, TEXT_INSET};
use crate::tray::model::TraySubmenu;

const OPENER_BAR_WIDTH: f64 = 56.0;
/// The opener's capsules within its picture: the 5-hour one, the weekly one three quarters of it
/// beneath, and the space between; a lone capsule is the 5-hour one's height, centred.
const OPENER_MAIN_HEIGHT: f64 = 4.0;
const OPENER_THIN_HEIGHT: f64 = 3.0;
const OPENER_GAP: f64 = 2.0;
const OPENER_BAR_HEIGHT: f64 = OPENER_THIN_HEIGHT + OPENER_GAP + OPENER_MAIN_HEIGHT;
/// Space reserved for AppKit's submenu arrow and trailing menu inset.
const OPENER_TRAILING: f64 = 32.0;

/// The widest menu the bars are placed against. A menu whose lines are wider than the screen is
/// shown at a width AppKit caps, while `NSMenu::size` still reports the width the lines asked for;
/// placing the bars by that would push them past the visible edge.
const MAX_OPENER_MENU_WIDTH: f64 = 640.0;

fn opener_gap(menu_width: f64, text_x: f64, text_width: f64) -> f64 {
    (menu_width.min(MAX_OPENER_MENU_WIDTH)
        - text_x
        - OPENER_TRAILING
        - OPENER_BAR_WIDTH
        - text_width)
        .max(GAP)
}

/// Keeps AppKit's own interactive row, including submenu tracking, highlighting and keyboard
/// handling. A title attachment is just ink inside that row, rather than a custom view that
/// would have to reimplement those interactions.
pub fn dress_quota_openers(menu: &NSMenu, models: &[TraySubmenu], mtm: MainThreadMarker) {
    let items = menu.itemArray();
    let openers: Vec<_> = items
        .iter()
        .filter(|item| item.submenu().is_some())
        .map(|item| {
            let title = item.title();
            (item, title)
        })
        .collect();
    for ((item, title), model) in openers.iter().zip(models) {
        if let Some(remaining) = model.quota_remaining {
            item.setAttributedTitle(Some(&opener_title(
                item,
                title,
                remaining,
                model.quota_weekly_remaining,
                GAP,
            )));
            if let Some(label) = &model.quota_label {
                let label = NSString::from_str(label);
                // The row's accessible name only, not a tooltip: AppKit's help tag would float over
                // the submenu that opens beside the row and repeat what it shows.
                item.setAccessibilityLabel(Some(&label));
            }
        }
    }
    // Ask AppKit to include all native titles and section views in its width calculation before
    // spacing the attachments. Pinning that width prevents a second layout from shrinking it.
    let width = menu.size().width.min(MAX_OPENER_MENU_WIDTH);
    menu.setMinimumWidth(width);
    for ((item, title), model) in openers.iter().zip(models) {
        if let Some(remaining) = model.quota_remaining {
            let measured = RowText::new(&title.to_string(), &NSColor::labelColor(), false, mtm);
            let text_width = measured.frame().size.width - TEXT_INSET * 2.0;
            let text_x = item
                .image()
                .map_or(INSET, |image| PICTURE_X + image.size().width + PICTURE_GAP);
            let gap = opener_gap(width, text_x, text_width);
            item.setAttributedTitle(Some(&opener_title(
                item,
                title,
                remaining,
                model.quota_weekly_remaining,
                gap,
            )));
        }
    }
}

fn opener_title(
    item: &objc2_app_kit::NSMenuItem,
    title: &NSString,
    remaining: u8,
    weekly_remaining: Option<u8>,
    gap: f64,
) -> Retained<objc2_foundation::NSMutableAttributedString> {
    use objc2::{rc::Weak, runtime::Bool, AnyThread};
    use objc2_app_kit::{
        NSAttachmentAttributeName, NSImageCacheMode, NSKernAttributeName, NSTextAttachment,
    };
    use objc2_foundation::{NSMutableAttributedString, NSNumber, NSRange};

    let item = Weak::new(item);
    let drawing = block2::RcBlock::new(move |bounds: NSRect| -> Bool {
        let colour = if item.load().is_some_and(|item| item.isHighlighted()) {
            NSColor::selectedMenuItemTextColor()
        } else {
            NSColor::labelColor()
        };
        if let Some(weekly) = weekly_remaining {
            draw_opener_capsule(
                rect(
                    bounds.origin.x,
                    bounds.origin.y + OPENER_THIN_HEIGHT + OPENER_GAP,
                    bounds.size.width,
                    OPENER_MAIN_HEIGHT,
                ),
                remaining,
                &colour,
            );
            draw_opener_capsule(
                rect(
                    bounds.origin.x,
                    bounds.origin.y,
                    bounds.size.width,
                    OPENER_THIN_HEIGHT,
                ),
                weekly,
                &colour,
            );
        } else {
            draw_opener_capsule(
                rect(
                    bounds.origin.x,
                    bounds.origin.y + (OPENER_BAR_HEIGHT - OPENER_MAIN_HEIGHT) / 2.0,
                    bounds.size.width,
                    OPENER_MAIN_HEIGHT,
                ),
                remaining,
                &colour,
            );
        }
        Bool::YES
    });
    let size = NSSize::new(OPENER_BAR_WIDTH, OPENER_BAR_HEIGHT);
    let image = NSImage::imageWithSize_flipped_drawingHandler(size, false, &drawing);
    // Attachments do not receive native template tint. Resolve the owning item's highlight on
    // every draw, without retaining the item through its own attributed title or caching its tint.
    image.setCacheMode(NSImageCacheMode::Never);
    let attachment = NSTextAttachment::new();
    attachment.setImage(Some(&image));
    attachment.setBounds(rect(0.0, -1.0, OPENER_BAR_WIDTH, OPENER_BAR_HEIGHT));
    // SAFETY: font, attachment and kern attributes have AppKit's required types, and the
    // ranges point at the final attachment and preceding space in this UTF-16 string.
    unsafe {
        let font = NSFont::menuFontOfSize(0.0);
        let attributes = NSDictionary::from_retained_objects(
            &[NSFontAttributeName],
            &[Retained::cast_unchecked::<AnyObject>(font)],
        );
        let string = NSString::from_str(&format!("{title} \u{fffc}"));
        let attributed = NSMutableAttributedString::initWithString_attributes(
            NSMutableAttributedString::alloc(),
            &string,
            Some(&attributes),
        );
        let end = string.length();
        attributed.addAttribute_value_range(
            NSAttachmentAttributeName,
            &attachment,
            NSRange::new(end - 1, 1),
        );
        let space = NSString::from_str(" ")
            .sizeWithAttributes(Some(&attributes))
            .width;
        let kern = NSNumber::new_f64(gap - space);
        attributed.addAttribute_value_range(NSKernAttributeName, &kern, NSRange::new(end - 2, 1));
        attributed
    }
}

fn draw_opener_capsule(bounds: NSRect, remaining: u8, colour: &NSColor) {
    let radius = bounds.size.height / 2.0;
    colour.colorWithAlphaComponent(0.22).set();
    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, radius, radius).fill();
    let width = bounds.size.width * f64::from(remaining.min(100)) / 100.0;
    if width > 0.0 {
        colour.set();
        let fill = NSRect::new(bounds.origin, NSSize::new(width, bounds.size.height));
        let radius = radius.min(width / 2.0);
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(fill, radius, radius).fill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opener_capsule_keeps_its_gap_and_clears_the_native_arrow() {
        let gap = opener_gap(340.0, 38.0, 80.0);
        assert_eq!(
            38.0 + 80.0 + gap + OPENER_BAR_WIDTH,
            340.0 - OPENER_TRAILING
        );
        assert_eq!(opener_gap(100.0, 38.0, 80.0), GAP);
    }

    #[test]
    fn opener_capsule_is_placed_within_the_capped_menu_width() {
        // A menu that asked for 1879 pt is shown narrower; the bars stay where the cap puts them.
        assert_eq!(
            opener_gap(1879.0, 38.0, 80.0),
            opener_gap(MAX_OPENER_MENU_WIDTH, 38.0, 80.0)
        );
    }
}
