//! The rows of the menu bar icon's submenus. A line that cannot be chosen is a disabled menu item,
//! which AppKit draws in the disabled grey and which a title colour cannot change; a menu item
//! with a view of its own is not dimmed and never highlights. So once muda has built a submenu
//! (`dress_submenu`, run after every rebuild by `tray.rs`), an account's header becomes the
//! system's section header where the system has one, and a window's row and a note become views:
//! the window's name and its detail (right-aligned, secondary colour) over a bar the shell draws.
//!
//! Every colour is a dynamic system colour, resolved as the view is drawn, so a menu follows the
//! light or dark look it is shown in, and a switch between them while it is open.

use objc2::rc::Retained;
use objc2::{
    define_class, msg_send, sel, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityStaticTextRole, NSAutoresizingMaskOptions, NSBezierPath,
    NSColor, NSFont, NSMenu, NSMenuItem, NSTextField, NSView,
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
/// The space above and below a note's single line.
const NOTE_PAD: f64 = 4.0;

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

/// A text of a row in the menu's font and the colour given, sized to fit and not read out on its
/// own: its row is read as a whole.
fn text(string: &str, colour: &NSColor, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(string), mtm);
    field.setFont(Some(&NSFont::menuFontOfSize(0.0)));
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
    let name = text(name, &NSColor::labelColor(), mtm);
    let detail = text(detail, &NSColor::secondaryLabelColor(), mtm);
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
    let note_text = text(note, &NSColor::secondaryLabelColor(), mtm);
    let size = note_text.frame().size;
    let width = (INSET * 2.0 + size.width).max(MIN_WIDTH);
    let row = row_view(width, size.height + NOTE_PAD * 2.0, note, mtm);
    note_text.setFrameOrigin(NSPoint::new(INSET, NOTE_PAD));
    row.addSubview(&note_text);
    row
}

/// Dresses the lines of `menu`, a submenu muda built from `items` (one line to each, a separator
/// for a separator), as the rows they stand for. A header becomes the system's section header
/// where macOS has it (14 and later) and stays a plain disabled line before that.
pub fn dress_submenu(menu: &NSMenu, items: &[SubmenuItem], mtm: MainThreadMarker) {
    // A class method, so it is the class object that is asked.
    let section_header = NSMenuItem::class()
        .metaclass()
        .responds_to(sel!(sectionHeaderWithTitle:));
    for (index, (item, model)) in menu.itemArray().iter().zip(items).enumerate() {
        match model {
            SubmenuItem::Header { title } if section_header => {
                let header = NSMenuItem::sectionHeaderWithTitle(&NSString::from_str(title), mtm);
                menu.removeItemAtIndex(index as isize);
                menu.insertItem_atIndex(&header, index as isize);
            }
            SubmenuItem::Header { .. } | SubmenuItem::Separator => {}
            SubmenuItem::Window {
                name,
                detail,
                label,
                fraction,
                agent,
            } => item.setView(Some(&window_row(
                name, detail, label, *fraction, *agent, mtm,
            ))),
            SubmenuItem::Note { text } => item.setView(Some(&note_row(text, mtm))),
        }
    }
}
