//! The row of tabs at the top of the icon's menu: a segmented control, one segment for each daemon
//! (the local one first, a remote one by its short name, an orange dot beside the name of one that
//! has a session waiting for the user), in a custom view on the menu's first item. Every tab's
//! lines are in the menu; those of the tabs not chosen are hidden (`show_tab`). Choosing a segment
//! shows its tab's lines while the menu stays open, which AppKit draws again as an item's hidden
//! state changes; should it not, the menu shows them the next time it opens.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSImage, NSMenu, NSSegmentSwitchTracking,
    NSSegmentedControl, NSView,
};
use objc2_foundation::{NSArray, NSData, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use super::lines::INSET;
use crate::tray::icon;

/// The space above and below the control.
const PAD: f64 = 5.0;
/// The side of the dot beside a segment's name, in points.
const DOT_SIDE: f64 = 7.0;

/// What the row of tabs shows: each tab's label, and whether its daemon raises a hand.
pub(in crate::tray) struct TabStrip {
    pub(in crate::tray) labels: Vec<String>,
    pub(in crate::tray) raised: Vec<bool>,
}

thread_local! {
    /// The tab each item of the menu last opened belongs to (`Finish::owners`), which a segment
    /// chosen while it is open shows. AppKit's views live on the main thread, and so does this.
    static OWNERS: RefCell<Vec<Option<usize>>> = const { RefCell::new(Vec::new()) };
    /// The target of the segments' action, which a control does not keep alive itself.
    static TARGET: RefCell<Option<Retained<TabTarget>>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and `TabTarget` implements no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "OctoboardTabTarget"]
    struct TabTarget;

    impl TabTarget {
        /// A segment was chosen: its tab's lines are shown, the others' hidden, in the menu the
        /// control is in.
        #[unsafe(method(chooseTab:))]
        fn choose_tab(&self, sender: &NSSegmentedControl) {
            let chosen = sender.selectedSegment();
            let Ok(chosen) = usize::try_from(chosen) else {
                return;
            };
            // SAFETY: the item is the one the control's row is the view of, in the menu being
            // shown, which keeps the item and itself alive while it is open.
            let Some(menu) = sender
                .enclosingMenuItem()
                .and_then(|item| unsafe { item.menu() })
            else {
                return;
            };
            OWNERS.with_borrow(|owners| show_tab(&menu, owners, chosen));
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for TabTarget {}
);

impl TabTarget {
    fn shared(mtm: MainThreadMarker) -> Retained<Self> {
        TARGET.with_borrow_mut(|target| {
            target
                .get_or_insert_with(|| {
                    let this = Self::alloc(mtm).set_ivars(());
                    // SAFETY: `init` is `NSObject`'s designated initialiser.
                    unsafe { msg_send![super(this), init] }
                })
                .clone()
        })
    }
}

/// The view of the row of tabs, the control centred in it.
pub(super) fn strip_row(strip: &TabStrip, mtm: MainThreadMarker) -> Retained<NSView> {
    let labels: Vec<Retained<NSString>> = strip
        .labels
        .iter()
        .map(|label| NSString::from_str(label))
        .collect();
    let labels = NSArray::from_retained_slice(&labels);
    let target = TabTarget::shared(mtm);
    let target: &AnyObject = &target;
    // SAFETY: the target answers the action's selector (`chooseTab:`), with the control as its
    // argument, and is kept alive for the life of the process (`TARGET`).
    let control = unsafe {
        NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
            &labels,
            NSSegmentSwitchTracking::SelectOne,
            Some(target),
            Some(sel!(chooseTab:)),
            mtm,
        )
    };
    let dot = raised_hand();
    for (index, raised) in strip.raised.iter().enumerate() {
        if *raised {
            control.setImage_forSegment(dot.as_deref(), index as isize);
        }
    }
    control.sizeToFit();
    let size = control.frame().size;
    let width = size.width + 2.0 * INSET;
    let height = size.height + 2.0 * PAD;
    let row = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height)),
    );
    row.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    row.setAccessibilityElement(false);
    control.setFrameOrigin(NSPoint::new(INSET, PAD));
    control.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxXMargin,
    );
    row.addSubview(&control);
    row
}

/// The orange dot beside the name of a tab whose daemon has a session waiting for the user.
fn raised_hand() -> Option<Retained<NSImage>> {
    use objc2::AnyThread;

    let png = icon::raised_hand_png()?;
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&png))?;
    image.setSize(NSSize::new(DOT_SIDE, DOT_SIDE));
    Some(image)
}

/// Chooses the tab `chosen` in `menu` as it opens: its segment selected, and its lines shown.
/// `owners` is what tab each item of the menu belongs to.
pub(super) fn choose(menu: &NSMenu, owners: &[Option<usize>], chosen: usize) {
    OWNERS.with_borrow_mut(|kept| *kept = owners.to_vec());
    if let Some(control) = menu
        .itemArray()
        .iter()
        .next()
        .and_then(|item| item.view())
        .and_then(|row| row.subviews().iter().next())
        .and_then(|view| view.downcast::<NSSegmentedControl>().ok())
    {
        control.setSelectedSegment(chosen as isize);
    }
    show_tab(menu, owners, chosen);
}

/// Shows the lines of the tab `chosen` and hides those of every other tab; a line every tab shows
/// is left as it is.
fn show_tab(menu: &NSMenu, owners: &[Option<usize>], chosen: usize) {
    for (item, owner) in menu.itemArray().iter().zip(owners) {
        if let Some(owner) = owner {
            item.setHidden(*owner != chosen);
        }
    }
}
