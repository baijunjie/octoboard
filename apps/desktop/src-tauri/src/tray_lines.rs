//! The rows of the menu bar icon's menus. A line that cannot be chosen is a disabled menu item,
//! which AppKit draws in the disabled grey and which a title colour cannot change; a menu item
//! with a view of its own is not dimmed and never highlights. So once muda has built the menu
//! (`finish_menu` in `tray.rs`, after every rebuild), the lines that cannot be chosen become
//! views: a session section's heading (`heading_row`) is its picture and its text in the
//! secondary colour; in a submenu (`dress_submenu`) an account's header is one line of the same in
//! the small size of a section header, a note one line at the menu's size, and a window's row
//! is its name and its detail (right-aligned, secondary colour) over a bar the shell draws.
//!
//! A row is read by assistive technology as its menu item alone: the item keeps the whole phrase
//! as its title, and nothing inside the row is an accessibility element. The texts are therefore
//! drawn (`RowText`), not `NSTextField`s, whose cells stay exposed as static texts of their own
//! whatever the field is marked as.
//!
//! Every colour is a dynamic system colour, resolved as the view is drawn, so a menu follows the
//! light or dark look it is shown in, and a switch between them while it is open.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSBezierPath, NSColor, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSImage, NSImageView, NSMenu, NSStringDrawing, NSView,
};
use objc2_foundation::{NSDictionary, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use crate::tray::{AgentKind, SubmenuItem};

/// The narrowest a row is, in points; a row whose text needs more is as wide as it needs, and the
/// menu is as wide as its widest row.
const MIN_WIDTH: f64 = 280.0;
/// The space left of and right of a row's content, as far in as a standard menu item's text.
const INSET: f64 = 14.0;
/// The space between a window's name and its detail where they come close.
const GAP: f64 = 12.0;
/// The space above a window's text, between its text and its bar, and below its bar.
const TEXT_TOP: f64 = 4.0;
const TEXT_TO_BAR: f64 = 5.0;
const BAR_BOTTOM: f64 = 7.0;
const BAR_HEIGHT: f64 = 6.0;
/// The space between a row text's frame and its text, left and right: the inset a text field's
/// cell has, so the texts sit where they did as fields.
const TEXT_INSET: f64 = 2.0;
/// The space above and below the single line of a note or an account's header.
const LINE_PAD: f64 = 4.0;
/// The space above and below a session section's heading, meant to make it, with its picture, as
/// tall as a standard menu item (about 22 pt).
/// TODO: this and the heading's horizontal spacing (`INSET`, `PICTURE_GAP`) are estimates of a
/// standard item's layout; confirm them against the session lines in the live menu.
const HEADING_PAD: f64 = 2.0;
/// The space between a heading's picture and its text.
const PICTURE_GAP: f64 = 6.0;

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

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

struct TextIvars {
    string: Retained<NSString>,
    /// The font and colour, as the attributes the string is measured and drawn with.
    attributes: Retained<NSDictionary<NSString, AnyObject>>,
}

define_class!(
    // SAFETY: `NSView` has no subclassing requirements beyond those `drawRect:` meets, and
    // `RowText` implements no `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "OctoboardRowText"]
    #[ivars = TextIvars]
    struct RowText;

    impl RowText {
        /// The string, in the font and the colour it was made with; the colour is dynamic, so it
        /// is resolved here, for the appearance the text is drawn in.
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let ivars = self.ivars();
            let origin = NSPoint::new(TEXT_INSET, 0.0);
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
            true
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for RowText {}
);

impl RowText {
    /// A text in the `colour` given, in the menu's font at its size, or `small` in the system
    /// font at the small size a section header has; sized to fit with the inset a text field's
    /// cell had, and not an accessibility element: its row is read as a whole, as its menu item.
    fn new(string: &str, colour: &NSColor, small: bool, mtm: MainThreadMarker) -> Retained<Self> {
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
            size.width.ceil() + TEXT_INSET * 2.0,
            size.height.ceil(),
        );
        let this = Self::alloc(mtm).set_ivars(TextIvars { string, attributes });
        // SAFETY: `initWithFrame:` is `NSView`'s designated initialiser.
        let text: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        text.setAccessibilityElement(false);
        text
    }
}

/// The colour of an agent's bar fill: mid-tones of the colours the app uses for the same bars
/// (`.quota-meter--*` in `style.css`), which suit a light and a dark menu; the text colour for Grok
/// Build and for an agent the shell does not know.
fn fill_colour(agent: AgentKind) -> Retained<NSColor> {
    match agent {
        AgentKind::Claude => NSColor::colorWithSRGBRed_green_blue_alpha(0.831, 0.388, 0.227, 1.0),
        AgentKind::Codex => NSColor::colorWithSRGBRed_green_blue_alpha(0.239, 0.482, 0.961, 1.0),
        AgentKind::Grok | AgentKind::Other => NSColor::labelColor(),
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

/// A window's row: its name left and its detail right-aligned above a bar of `fraction` of its
/// width, which grows with the row.
fn window_row(
    name: &str,
    detail: &str,
    fraction: f64,
    agent: AgentKind,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let name = RowText::new(name, &NSColor::labelColor(), false, mtm);
    let detail = RowText::new(detail, &NSColor::secondaryLabelColor(), false, mtm);
    let (name_size, detail_size) = (name.frame().size, detail.frame().size);
    let width = (INSET * 2.0 + name_size.width + GAP + detail_size.width).max(MIN_WIDTH);
    let line = name_size.height.max(detail_size.height);
    let height = TEXT_TOP + line + TEXT_TO_BAR + BAR_HEIGHT + BAR_BOTTOM;
    let row = row_view(width, height, mtm);

    let y = height - TEXT_TOP - line;
    name.setFrameOrigin(NSPoint::new(INSET, y));
    detail.setFrameOrigin(NSPoint::new(width - INSET - detail_size.width, y));
    detail.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
    row.addSubview(&name);
    row.addSubview(&detail);

    let bar = QuotaBar::new(
        rect(INSET, BAR_BOTTOM, width - INSET * 2.0, BAR_HEIGHT),
        fraction.clamp(0.0, 1.0),
        fill_colour(agent),
        mtm,
    );
    bar.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    row.addSubview(&bar);
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

/// The row of a session section's heading: its `picture` (the one its menu item shows, so it
/// keeps whether it is a template image) where a standard item's picture sits, and its text in
/// the secondary colour after it. The row is as wide as its content, so it does not widen the menu,
/// and grows with it.
pub fn heading_row(
    heading: &str,
    picture: Option<&NSImage>,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let heading_text = RowText::new(heading, &NSColor::secondaryLabelColor(), false, mtm);
    let text_size = heading_text.frame().size;
    let picture = picture.map(|image| NSImageView::imageViewWithImage(image, mtm));
    let picture_size = picture
        .as_ref()
        .map_or(NSSize::ZERO, |view| view.frame().size);
    let text_x = match picture {
        Some(_) => INSET + picture_size.width + PICTURE_GAP,
        None => INSET,
    };
    let line = text_size.height.max(picture_size.height);
    let height = line + HEADING_PAD * 2.0;
    let row = row_view(text_x + text_size.width + INSET, height, mtm);
    if let Some(view) = picture {
        view.setAccessibilityElement(false);
        if let Some(cell) = view.cell() {
            cell.setAccessibilityElement(false);
        }
        view.setFrameOrigin(NSPoint::new(INSET, (height - picture_size.height) / 2.0));
        row.addSubview(&view);
    }
    heading_text.setFrameOrigin(NSPoint::new(text_x, (height - text_size.height) / 2.0));
    row.addSubview(&heading_text);
    row
}

/// Dresses the lines of `menu`, a submenu muda built from `items` (one line to each, a separator
/// for a separator), as the rows they stand for.
pub fn dress_submenu(menu: &NSMenu, items: &[SubmenuItem], mtm: MainThreadMarker) {
    for (item, model) in menu.itemArray().iter().zip(items) {
        match model {
            SubmenuItem::Separator => {}
            SubmenuItem::Header { title } => item.setView(Some(&single_line_row(title, true, mtm))),
            SubmenuItem::Note { text } => item.setView(Some(&single_line_row(text, false, mtm))),
            SubmenuItem::Window {
                name,
                detail,
                fraction,
                agent,
                ..
            } => item.setView(Some(&window_row(name, detail, *fraction, *agent, mtm))),
        }
    }
}
