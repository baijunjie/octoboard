//! The rows of the menu bar icon's menus. A line that cannot be chosen is a disabled menu item,
//! which AppKit draws in the disabled grey and which a title colour cannot change; a menu item
//! with a view of its own is not dimmed and never highlights. So once muda has built the menu
//! (`finish_menu` in `tray/finish.rs`, the first time it opens after each rebuild), the lines that
//! cannot be chosen become views: a session section's heading (`heading_row`) is its picture and
//! its text in the secondary colour; in a submenu (`dress_submenu`) an account's header is one
//! line of the same in the small size of a section header, with when its reading was taken
//! right-aligned, a note one line at the menu's size, and a window's row is its name and its
//! detail (right-aligned, secondary colour) over a bar the shell draws; a 5-hour window with the
//! account-wide weekly one is a single row (`meter_row`) of the 5-hour row, a thinner bar beneath
//! its bar, and the weekly name and detail under that.
//!
//! A row is read by assistive technology as its menu item alone: the item keeps the whole phrase
//! as its title, and nothing inside the row is an accessibility element. The texts are therefore
//! drawn (`RowText`), not `NSTextField`s, whose cells stay exposed as static texts of their own
//! whatever the field is marked as. The non-element picture view (`picture_view`) is shared with
//! the attention dot `tray/status_item.rs` lays over the menu bar icon.
//!
//! A bar is filled in the colour the UI sends for its agent as sRGB bytes, or, for an account whose
//! quota is used up, in the grey the UI stylesheet defines for that, which it sends the same way.
//! Where it sent none the bar is filled in the text colour.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSBezierPath, NSColor, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSImage, NSImageView, NSMenu, NSStringDrawing, NSView,
};
use objc2_foundation::{NSDictionary, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use super::model::{HeaderStatus, MeterLayer, SubmenuItem};

/// The narrowest a row is, in points; a row whose text needs more is as wide as it needs, and the
/// menu is as wide as its widest row.
const MIN_WIDTH: f64 = 280.0;
/// The space left of and right of a row's content, as far in as a standard menu item's text.
pub(super) const INSET: f64 = 14.0;
/// The space between a window's name and its detail where they come close.
pub(super) const GAP: f64 = 12.0;
/// The space above a window's text, between its text and its bar, and below its bar; a two-layer
/// meter's rows (`meter_row`) are spaced the same.
const TEXT_TOP: f64 = 4.0;
const TEXT_TO_BAR: f64 = 5.0;
const BAR_BOTTOM: f64 = 7.0;
const BAR_HEIGHT: f64 = 6.0;
/// A two-layer meter's weekly bar, three quarters of the 5-hour bar above it as on every two-layer
/// bar, and the space between the two, which brings the pair back to a whole point.
const THIN_BAR_HEIGHT: f64 = 4.5;
const THIN_BAR_GAP: f64 = 2.5;
/// The space between a row text's frame and its text, left and right: the inset a text field's
/// cell has, so the texts sit where they did as fields.
pub(super) const TEXT_INSET: f64 = 2.0;
/// The space above and below the single line of a note or an account's header.
const LINE_PAD: f64 = 4.0;
/// Where AppKit lays out a standard menu item, which a session section's heading copies so that its
/// picture and text line up with the session lines' below it: 24 pt tall, an 18 pt picture 16 pt in
/// and centred, and the title's text field 4 pt after the picture, or `INSET` in on a line with no
/// picture, centred too. Read from the views `NSMenu` lays its items out in on macOS 26, with the
/// 18 pt pictures the UI draws for the menu (`SIZE` in
/// `packages/ui/src/statusItem/statusItemIcons.ts`); another version, or a picture of another size,
/// may be placed otherwise.
const ITEM_HEIGHT: f64 = 24.0;
pub(super) const PICTURE_X: f64 = 16.0;
pub(super) const PICTURE_GAP: f64 = 4.0;

struct BarIvars {
    /// How much of the bar is filled, 0 to 1.
    fraction: f64,
    fill: Retained<NSColor>,
}

define_class!(
    // SAFETY: `NSView` has no subclassing requirements beyond those `drawRect:` meets, and
    // `QuotaBar` implements no `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "OctoboardQuotaBar"]
    #[ivars = BarIvars]
    struct QuotaBar;

    impl QuotaBar {
        /// A rounded track with `fraction` of its width filled from the start. The colours are
        /// dynamic, so they are resolved here, for the appearance the bar is drawn in.
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let bounds = self.bounds();
            let radius = bounds.size.height / 2.0;
            NSColor::tertiaryLabelColor().set();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, radius, radius).fill();
            let width = bounds.size.width * self.ivars().fraction;
            if width > 0.0 {
                let filled = NSRect::new(bounds.origin, NSSize::new(width, bounds.size.height));
                let radius = radius.min(width / 2.0);
                self.ivars().fill.set();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(filled, radius, radius)
                    .fill();
            }
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for QuotaBar {}
);

impl QuotaBar {
    fn new(
        frame: NSRect,
        fraction: f64,
        fill: Retained<NSColor>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(BarIvars { fraction, fill });
        // SAFETY: `initWithFrame:` is `NSView`'s designated initialiser.
        let bar: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        bar.setAccessibilityElement(false);
        bar
    }
}

pub(super) fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

pub(super) struct TextIvars {
    string: Retained<NSString>,
    /// The font and colour, as the attributes the string is measured and drawn with.
    attributes: Retained<NSDictionary<NSString, AnyObject>>,
    background: Option<Retained<NSColor>>,
}

define_class!(
    // SAFETY: `NSView` has no subclassing requirements beyond those `drawRect:` meets, and
    // `RowText` implements no `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "OctoboardRowText"]
    #[ivars = TextIvars]
    pub(super) struct RowText;

    impl RowText {
        /// The string, in the font and the colour it was made with; the colour is dynamic, so it
        /// is resolved here, for the appearance the text is drawn in.
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let ivars = self.ivars();
            if let Some(background) = &ivars.background {
                background.set();
                let radius = self.bounds().size.height / 2.0;
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(), radius, radius).fill();
            }
            let origin = if ivars.background.is_some() {
                NSPoint::new(6.0, 2.0)
            } else {
                NSPoint::new(TEXT_INSET, 0.0)
            };
            // SAFETY: the attributes hold an `NSFont` and an `NSColor` under the keys for them.
            unsafe {
                ivars
                    .string
                    .drawAtPoint_withAttributes(origin, Some(&ivars.attributes));
            }
        }

        /// Like a text field's label, the text takes part in the menu's vibrancy, so the
        /// secondary colour blends with the menu's background as the system's own lines do.
        #[unsafe(method(allowsVibrancy))]
        fn allows_vibrancy(&self) -> bool {
            self.ivars().background.is_none()
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for RowText {}
);

impl RowText {
    /// A text in the `colour` given, in the menu's font at its size, or `small` in the system
    /// font at the small size a section header has; sized to fit with the inset a text field's
    /// cell had, and not an accessibility element: its row is read as a whole, as its menu item.
    pub(super) fn new(
        string: &str,
        colour: &NSColor,
        small: bool,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        Self::with_background(string, colour, small, None, mtm)
    }

    fn with_background(
        string: &str,
        colour: &NSColor,
        small: bool,
        background: Option<Retained<NSColor>>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let font = if small {
            NSFont::systemFontOfSize(NSFont::smallSystemFontSize())
        } else {
            NSFont::menuFontOfSize(0.0)
        };
        // SAFETY: the keys are the constants AppKit defines, read on the main thread, and the
        // objects are the font and the colour they take.
        let attributes = unsafe {
            NSDictionary::from_retained_objects(
                &[NSFontAttributeName, NSForegroundColorAttributeName],
                &[
                    Retained::cast_unchecked::<AnyObject>(font),
                    Retained::cast_unchecked::<AnyObject>(colour.retain()),
                ],
            )
        };
        let string = NSString::from_str(string);
        // SAFETY: the attributes are the font and the colour, as above.
        let size = unsafe { string.sizeWithAttributes(Some(&attributes)) };
        let frame = rect(
            0.0,
            0.0,
            size.width.ceil()
                + if background.is_some() {
                    12.0
                } else {
                    TEXT_INSET * 2.0
                },
            size.height.ceil() + if background.is_some() { 4.0 } else { 0.0 },
        );
        let this = Self::alloc(mtm).set_ivars(TextIvars {
            string,
            attributes,
            background,
        });
        // SAFETY: `initWithFrame:` is `NSView`'s designated initialiser.
        let text: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        text.setAccessibilityElement(false);
        text
    }
}

fn srgb([r, g, b]: [u8; 3]) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
        1.0,
    )
}

/// The fill of a row's bars: the grey the UI sends for an account whose quota is used up, else the
/// agent's colour it sends. Each falls back to a text colour where the UI sent none.
fn bar_fill(
    used_up: bool,
    used_up_colour: Option<[u8; 3]>,
    fill_colour: Option<[u8; 3]>,
) -> Retained<NSColor> {
    if used_up {
        used_up_colour.map_or_else(NSColor::secondaryLabelColor, srgb)
    } else {
        fill_colour.map_or_else(NSColor::labelColor, srgb)
    }
}

/// The view of a row, `width` wide at least. It is not an accessibility element, and nothing in
/// it is: the row is read as its menu item, whose title is the whole phrase. Were the view the
/// element instead, the item's title would be read too, so the phrase would be exposed twice;
/// were the item's title cleared, the line would have no title to a menu search.
fn row_view(width: f64, height: f64, mtm: MainThreadMarker) -> Retained<NSView> {
    let row = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, height));
    row.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    row.setAccessibilityElement(false);
    row
}

/// A name at the left and a detail right-aligned at the right of one line, the detail in the
/// secondary colour and the name in `name_colour`, both `small` or at the menu's size; the line a
/// window's bar sits beneath, or an account's header.
struct TextLine {
    name: Retained<RowText>,
    detail: Retained<RowText>,
    /// The line's height, and the width its content needs, insets included.
    height: f64,
    width: f64,
}

impl TextLine {
    fn new(
        name: &str,
        detail: &str,
        name_colour: &NSColor,
        small: bool,
        mtm: MainThreadMarker,
    ) -> Self {
        let name = RowText::new(name, name_colour, small, mtm);
        let detail = RowText::new(detail, &NSColor::secondaryLabelColor(), small, mtm);
        let (name_size, detail_size) = (name.frame().size, detail.frame().size);
        Self {
            height: name_size.height.max(detail_size.height),
            width: INSET * 2.0 + name_size.width + GAP + detail_size.width,
            name,
            detail,
        }
    }

    /// Adds the line to `row`, `width` wide, with its bottom at `y`; the detail keeps to the right
    /// edge as the row grows.
    fn place(&self, row: &NSView, width: f64, y: f64) {
        self.name.setFrameOrigin(NSPoint::new(INSET, y));
        self.detail.setFrameOrigin(NSPoint::new(
            width - INSET - self.detail.frame().size.width,
            y,
        ));
        self.detail
            .setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
        row.addSubview(&self.name);
        row.addSubview(&self.detail);
    }
}

/// Adds to `row`, `width` wide, a bar `thickness` tall with its bottom at `y`, `fraction` of its
/// width filled in `fill`; it grows with the row.
fn add_bar(row: &NSView, width: f64, y: f64, thickness: f64, fraction: f64, fill: &NSColor) {
    let bar = QuotaBar::new(
        rect(INSET, y, width - INSET * 2.0, thickness),
        fraction.clamp(0.0, 1.0),
        fill.retain(),
        row.mtm(),
    );
    bar.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    row.addSubview(&bar);
}

/// A window's row: its name left and its detail right-aligned above a bar of `fraction` of its
/// width, which grows with the row.
fn window_row(
    name: &str,
    detail: &str,
    fraction: f64,
    fill: &NSColor,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let line = TextLine::new(name, detail, &NSColor::labelColor(), false, mtm);
    let width = line.width.max(MIN_WIDTH);
    let height = TEXT_TOP + line.height + TEXT_TO_BAR + BAR_HEIGHT + BAR_BOTTOM;
    let row = row_view(width, height, mtm);
    line.place(&row, width, height - TEXT_TOP - line.height);
    add_bar(&row, width, BAR_BOTTOM, BAR_HEIGHT, fraction, fill);
    row
}

/// The 5-hour and weekly windows as one item: a window's row for the 5-hour one, a thinner bar of
/// the weekly window directly beneath its bar, and the weekly name and detail under that in the
/// secondary colour.
fn meter_row(
    session: &MeterLayer,
    weekly: &MeterLayer,
    fill: &NSColor,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let session_line = TextLine::new(
        &session.name,
        &session.detail,
        &NSColor::labelColor(),
        false,
        mtm,
    );
    let weekly_line = TextLine::new(
        &weekly.name,
        &weekly.detail,
        &NSColor::secondaryLabelColor(),
        false,
        mtm,
    );
    let width = session_line.width.max(weekly_line.width).max(MIN_WIDTH);
    let thin_y = BAR_BOTTOM + weekly_line.height + TEXT_TO_BAR;
    let thick_y = thin_y + THIN_BAR_HEIGHT + THIN_BAR_GAP;
    let height = thick_y + BAR_HEIGHT + TEXT_TO_BAR + session_line.height + TEXT_TOP;
    let row = row_view(width, height, mtm);
    session_line.place(&row, width, height - TEXT_TOP - session_line.height);
    add_bar(&row, width, thick_y, BAR_HEIGHT, session.fraction, fill);
    add_bar(&row, width, thin_y, THIN_BAR_HEIGHT, weekly.fraction, fill);
    weekly_line.place(&row, width, BAR_BOTTOM);
    row
}

/// One line of secondary text: a note, or, `small`, an account's header.
fn single_line_row(string: &str, small: bool, mtm: MainThreadMarker) -> Retained<NSView> {
    let line_text = RowText::new(string, &NSColor::secondaryLabelColor(), small, mtm);
    let size = line_text.frame().size;
    let width = (INSET * 2.0 + size.width).max(MIN_WIDTH);
    let row = row_view(width, size.height + LINE_PAD * 2.0, mtm);
    line_text.setFrameOrigin(NSPoint::new(INSET, LINE_PAD));
    row.addSubview(&line_text);
    row
}

/// An account's header: one line of small secondary text, its title at the left and, when there is
/// one, when its reading was taken right-aligned.
fn header_row(
    title: &str,
    detail: Option<&str>,
    status: Option<&HeaderStatus>,
    suffix: Option<&str>,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let line = TextLine::new(
        title,
        detail.unwrap_or(""),
        &NSColor::secondaryLabelColor(),
        true,
        mtm,
    );
    let tag = status.map(|status| {
        let colour = NSColor::systemOrangeColor();
        let foreground = status
            .foreground_colour
            .map_or_else(NSColor::labelColor, srgb);
        let background = status.background_colour.map_or_else(
            || colour.colorWithAlphaComponent(0.15),
            |[r, g, b, a]| srgb([r, g, b]).colorWithAlphaComponent(f64::from(a) / 255.0),
        );
        RowText::with_background(&status.label, &foreground, true, Some(background), mtm)
    });
    let suffix = suffix.map(|text| RowText::new(text, &NSColor::secondaryLabelColor(), true, mtm));
    let suffix_width = suffix
        .as_ref()
        .map_or(0.0, |text| text.frame().size.width + 6.0);
    let tag_width = tag.as_ref().map_or(0.0, |tag| tag.frame().size.width + 6.0);
    let height = tag
        .as_ref()
        .map_or(line.height, |tag| line.height.max(tag.frame().size.height));
    let width = (line.width + tag_width + suffix_width).max(MIN_WIDTH);
    let row = row_view(width, height + LINE_PAD * 2.0, mtm);
    line.place(&row, width, LINE_PAD + (height - line.height) / 2.0);
    if let Some(suffix) = suffix {
        suffix.setFrameOrigin(NSPoint::new(
            INSET + line.name.frame().size.width + tag_width + 6.0,
            LINE_PAD + (height - suffix.frame().size.height) / 2.0,
        ));
        row.addSubview(&suffix);
    }
    if let Some(tag) = tag {
        tag.setFrameOrigin(NSPoint::new(
            INSET + line.name.frame().size.width + 6.0,
            LINE_PAD + (height - tag.frame().size.height) / 2.0,
        ));
        row.addSubview(&tag);
    }
    row
}

/// A view that draws `image` at its own size, laid out by frames, and that is not an accessibility
/// element, nor is its cell: the picture is part of the look of what it sits in, which is read as a
/// whole. `imageViewWithImage:` leaves the frame empty, to be sized by Auto Layout from the image,
/// so the view takes the image's size itself, or it draws nothing.
pub fn picture_view(image: &NSImage, mtm: MainThreadMarker) -> Retained<NSImageView> {
    let view = NSImageView::imageViewWithImage(image, mtm);
    view.setFrameSize(image.size());
    view.setAccessibilityElement(false);
    if let Some(cell) = view.cell() {
        cell.setAccessibilityElement(false);
    }
    view
}

/// The row of a session section's heading: its `picture` (the one its menu item shows, so it
/// keeps whether it is a template image) where a standard item's picture sits, and its text in
/// the secondary colour where a standard item's title sits (`ITEM_HEIGHT` and the constants with
/// it). The row is as wide as its content, so it does not widen the menu, and grows with it.
pub fn heading_row(
    heading: &str,
    picture: Option<&NSImage>,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let heading_text = RowText::new(heading, &NSColor::secondaryLabelColor(), false, mtm);
    let text_size = heading_text.frame().size;
    let picture = picture.map(|image| picture_view(image, mtm));
    let picture_size = picture
        .as_ref()
        .map_or(NSSize::ZERO, |view| view.frame().size);
    let text_x = match picture {
        Some(_) => PICTURE_X + picture_size.width + PICTURE_GAP,
        None => INSET,
    };
    let height = ITEM_HEIGHT.max(text_size.height).max(picture_size.height);
    let centred = |content: f64| ((height - content) / 2.0).round();
    let row = row_view(text_x + text_size.width + INSET, height, mtm);
    if let Some(view) = picture {
        view.setFrameOrigin(NSPoint::new(PICTURE_X, centred(picture_size.height)));
        row.addSubview(&view);
    }
    heading_text.setFrameOrigin(NSPoint::new(text_x, centred(text_size.height)));
    row.addSubview(&heading_text);
    row
}

/// Dresses the lines of `menu`, a submenu muda built from `items` (one line to each, a separator
/// for a separator), as the rows they stand for.
pub fn dress_submenu(menu: &NSMenu, items: &[SubmenuItem], mtm: MainThreadMarker) {
    for (item, model) in menu.itemArray().iter().zip(items) {
        match model {
            SubmenuItem::Separator => {}
            SubmenuItem::Header {
                title,
                detail,
                status,
                suffix,
                ..
            } => item.setView(Some(&header_row(
                title,
                detail.as_deref(),
                status.as_ref(),
                suffix.as_deref(),
                mtm,
            ))),
            SubmenuItem::Note { text } => item.setView(Some(&single_line_row(text, false, mtm))),
            SubmenuItem::Window {
                name,
                detail,
                fraction,
                used_up,
                used_up_colour,
                fill_colour,
                ..
            } => item.setView(Some(&window_row(
                name,
                detail,
                *fraction,
                &bar_fill(*used_up, *used_up_colour, *fill_colour),
                mtm,
            ))),
            SubmenuItem::Meter {
                session,
                weekly,
                used_up,
                used_up_colour,
                fill_colour,
                ..
            } => item.setView(Some(&meter_row(
                session,
                weekly,
                &bar_fill(*used_up, *used_up_colour, *fill_colour),
                mtm,
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_is_filled_in_the_colour_sent_for_its_state() {
        let grey = [123, 123, 133];
        let orange = [212, 99, 58];
        assert!(bar_fill(true, Some(grey), Some(orange)).isEqual(Some(&srgb(grey))));
        assert!(bar_fill(false, Some(grey), Some(orange)).isEqual(Some(&srgb(orange))));
        assert!(bar_fill(true, None, Some(orange)).isEqual(Some(&NSColor::secondaryLabelColor())));
        assert!(bar_fill(false, Some(grey), None).isEqual(Some(&NSColor::labelColor())));
        let expected = NSColor::colorWithSRGBRed_green_blue_alpha(
            123.0 / 255.0,
            123.0 / 255.0,
            133.0 / 255.0,
            1.0,
        );
        assert!(srgb(grey).isEqual(Some(&expected)));
    }
}
