//! The rows of the menu bar icon's menus. A line that cannot be chosen is a disabled menu item,
//! which AppKit draws in the disabled grey and which a title colour cannot change; a menu item
//! with a view of its own is not dimmed and never highlights. So once muda has built the menu
//! (`finish_menu` in `tray.rs`, after every rebuild), the lines that cannot be chosen become
//! views: a session section's heading (`heading_row`) is its picture and its text in bold
//! secondary colour; in a submenu (`dress_submenu`) an account's header is one line of the same,
//! a note one line of regular secondary text, and a window's row is its name and its detail
//! (right-aligned, secondary colour) over a bar the shell draws.
//!
//! Every colour is a dynamic system colour, resolved as the view is drawn, so a menu follows the
//! light or dark look it is shown in, and a switch between them while it is open.

use objc2::rc::Retained;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityStaticTextRole, NSAutoresizingMaskOptions, NSBezierPath,
    NSColor, NSFont, NSImage, NSImageView, NSMenu, NSTextField, NSView,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

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

/// A text of a row in the menu's font (bold when `bold`, at the menu font's size) and the colour
/// given, sized to fit and not read out on its own: its row is read as a whole.
fn text(
    string: &str,
    colour: &NSColor,
    bold: bool,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(string), mtm);
    let font = NSFont::menuFontOfSize(0.0);
    let font = if bold {
        NSFont::boldSystemFontOfSize(font.pointSize())
    } else {
        font
    };
    field.setFont(Some(&font));
    field.setTextColor(Some(colour));
    field.sizeToFit();
    field.setAccessibilityElement(false);
    field
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

/// The view of a row, `width` wide at least, read as `label` by assistive technology.
fn row_view(width: f64, height: f64, label: &str, mtm: MainThreadMarker) -> Retained<NSView> {
    let row = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, height));
    row.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    row.setAccessibilityElement(true);
    // Static text, read as its whole phrase: the view stands for the line, not its title.
    // SAFETY: a constant string AppKit defines, read on the main thread.
    row.setAccessibilityRole(Some(unsafe { NSAccessibilityStaticTextRole }));
    row.setAccessibilityLabel(Some(&NSString::from_str(label)));
    row
}

/// A window's row: its name left and its detail right-aligned above a bar of `fraction` of its
/// width, which grows with the row.
fn window_row(
    name: &str,
    detail: &str,
    label: &str,
    fraction: f64,
    agent: AgentKind,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let name = text(name, &NSColor::labelColor(), false, mtm);
    let detail = text(detail, &NSColor::secondaryLabelColor(), false, mtm);
    let (name_size, detail_size) = (name.frame().size, detail.frame().size);
    let width = (INSET * 2.0 + name_size.width + GAP + detail_size.width).max(MIN_WIDTH);
    let line = name_size.height.max(detail_size.height);
    let height = TEXT_TOP + line + TEXT_TO_BAR + BAR_HEIGHT + BAR_BOTTOM;
    let row = row_view(width, height, label, mtm);

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

/// A note's row: one line of secondary text.
fn note_row(note: &str, mtm: MainThreadMarker) -> Retained<NSView> {
    single_line_row(note, false, mtm)
}

/// An account's header: a note's row in bold.
fn account_row(title: &str, mtm: MainThreadMarker) -> Retained<NSView> {
    single_line_row(title, true, mtm)
}

/// One line of secondary text, regular or bold, read as that line.
fn single_line_row(string: &str, bold: bool, mtm: MainThreadMarker) -> Retained<NSView> {
    let line_text = text(string, &NSColor::secondaryLabelColor(), bold, mtm);
    let size = line_text.frame().size;
    let width = (INSET * 2.0 + size.width).max(MIN_WIDTH);
    let row = row_view(width, size.height + LINE_PAD * 2.0, string, mtm);
    line_text.setFrameOrigin(NSPoint::new(INSET, LINE_PAD));
    row.addSubview(&line_text);
    row
}

/// The row of a session section's heading: its `picture` (the one its menu item shows, so it
/// keeps whether it is a template image) where a standard item's picture sits, and its text in
/// the bold secondary colour after it. The row is as wide as its content, so it does not widen
/// the menu, and grows with it.
pub fn heading_row(
    heading: &str,
    picture: Option<&NSImage>,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let heading_text = text(heading, &NSColor::secondaryLabelColor(), true, mtm);
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
    let row = row_view(text_x + text_size.width + INSET, height, heading, mtm);
    if let Some(view) = picture {
        view.setAccessibilityElement(false);
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
            SubmenuItem::Header { title } => item.setView(Some(&account_row(title, mtm))),
            SubmenuItem::Note { text } => item.setView(Some(&note_row(text, mtm))),
            SubmenuItem::Window {
                name,
                detail,
                label,
                fraction,
                agent,
            } => item.setView(Some(&window_row(
                name, detail, label, *fraction, *agent, mtm,
            ))),
        }
    }
}
