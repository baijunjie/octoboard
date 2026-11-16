//! The row of tabs at the top of the icon's menu: a segmented control, one segment for each daemon
//! (the local one first, a remote one by its short name, an orange dot after the name of one that
//! has a session waiting for the user), in a custom view on the menu's first item, its leading edge
//! where the lines' text begins. A segment cannot place an image after its label, so the label
//! of a raised tab is followed by a run of no-break spaces that keeps the dot's room (after its
//! name is cut to fit, never with it), and the dot is a view over the control, laid over that room. Every tab's
//! lines are in the menu; those of the tabs not chosen are hidden (`show_tab`). Choosing a segment
//! shows its tab's lines while the menu stays open, which AppKit draws again as an item's hidden
//! state changes; should it not, the menu shows them the next time it opens.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSFont, NSFontAttributeName, NSImage, NSImageView,
    NSMenu, NSSegmentSwitchTracking, NSSegmentedControl, NSStringDrawing, NSView,
};
use objc2_foundation::{
    NSArray, NSData, NSDictionary, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use super::lines::{INSET, MIN_WIDTH, TEXT_INSET};
use crate::tray::icon;

/// The space above and below the control.
const PAD: f64 = 5.0;
/// Where the control's visible background starts: where the lines' text does, `TEXT_INSET` in from
/// `INSET`, the control's frame having no bezel inset of its own (measured on macOS 26, where
/// the background started at `INSET`, 2 pt left of the text).
const ORIGIN: f64 = INSET + TEXT_INSET;
/// The side of the dot after a segment's name, in points.
const DOT_SIDE: f64 = 7.0;
/// The space between a raised tab's name and its dot, at least; the segment's own padding is what
/// follows the dot, as it precedes the name.
const DOT_GAP: f64 = 6.0;
/// What follows a raised tab's name in its label, which keeps the room the dot is laid over.
const DOT_ROOM: char = '\u{00A0}';

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

define_class!(
    // SAFETY: `NSImageView` has no subclassing requirements for `hitTest:`, and `DotView`
    // implements no `Drop`.
    #[unsafe(super(NSImageView))]
    #[thread_kind = MainThreadOnly]
    #[name = "OctoboardTabDot"]
    struct DotView;

    impl DotView {
        /// The dot lies over a segment without taking its clicks: they reach the control below.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<&NSView> {
            None
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for DotView {}
);

impl DotView {
    fn new(image: &NSImage, frame: NSRect, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `initWithFrame:` is `NSView`'s designated initialiser, which `NSImageView` keeps.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.setImage(Some(image));
        view.setAccessibilityElement(false);
        view
    }
}

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

/// The width of `string` in `font`, in points.
fn text_width(string: &str, font: &Retained<NSFont>) -> f64 {
    // SAFETY: the key is the constant AppKit defines, read on the main thread, and the object is
    // the font it takes.
    let attributes = unsafe {
        NSDictionary::from_retained_objects(
            &[NSFontAttributeName],
            &[Retained::cast_unchecked::<AnyObject>(font.clone())],
        )
    };
    // SAFETY: the attributes are the font, as above.
    unsafe { NSString::from_str(string).sizeWithAttributes(Some(&attributes)) }.width
}

/// Whether `c` continues the character before it into one cluster: a combining mark, a variation
/// selector, a zero-width joiner, an emoji skin tone, or a Devanagari or Thai vowel sign or mark (not
/// the Devanagari avagraha, U+093D, which is a letter).
fn extends_previous(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{0900}'..='\u{0903}'
        | '\u{093A}'..='\u{093C}'
        | '\u{093E}'..='\u{094F}'
        | '\u{0951}'..='\u{0957}'
        | '\u{0962}'..='\u{0963}'
        | '\u{0E31}'
        | '\u{0E33}'
        | '\u{0E34}'..='\u{0E3A}'
        | '\u{0E47}'..='\u{0E4E}'
        | '\u{200D}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE00}'..='\u{FE0F}'
        | '\u{FE20}'..='\u{FE2F}'
        | '\u{1F3FB}'..='\u{1F3FF}'
        | '\u{E0100}'..='\u{E01EF}')
}

fn is_regional_indicator(c: char) -> bool {
    matches!(c, '\u{1F1E6}'..='\u{1F1FF}')
}

/// Drops the last cluster of `kept`: its base with the marks, selectors and joined characters that
/// follow, so a cut never leaves a base without its accent or half of a joined emoji. (No grapheme
/// segmentation is a dependency; this covers what a name is made of.) A flag is two regional indicators,
/// popped together; in a run of them the pairs are counted from its start. A Devanagari conjunct (a
/// consonant, a virama, a consonant) is one cluster.
fn pop_cluster(kept: &mut Vec<char>) {
    while let Some(c) = kept.pop() {
        if extends_previous(c) {
            continue;
        }
        // The second of a pair of regional indicators takes the first with it.
        if is_regional_indicator(c) {
            let run = kept
                .iter()
                .rev()
                .take_while(|c| is_regional_indicator(**c))
                .count();
            if run % 2 == 1 {
                kept.pop();
            }
        }
        // A base: a joiner or a virama before it joins it to the cluster before, which goes too.
        if matches!(kept.last(), Some('\u{200D}' | '\u{094D}')) {
            continue;
        }
        break;
    }
}

/// `name` cut to at most `max` points wide in `font`, ending in "…" when cut, with no space before it.
fn cut_to(name: &str, max: f64, font: &Retained<NSFont>) -> String {
    if text_width(name, font) <= max {
        return name.to_owned();
    }
    let mut kept: Vec<char> = name.chars().collect();
    while !kept.is_empty() {
        pop_cluster(&mut kept);
        let cut: String = kept.iter().collect::<String>().trim_end().to_owned() + "\u{2026}";
        if text_width(&cut, font) <= max {
            return cut;
        }
    }
    "\u{2026}".to_owned()
}

/// The segmented control with one segment for each of `shown`.
fn control_of(
    shown: &[String],
    font: &Retained<NSFont>,
    mtm: MainThreadMarker,
) -> Retained<NSSegmentedControl> {
    let labels: Vec<Retained<NSString>> = shown
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
    control.setFont(Some(font));
    control.sizeToFit();
    control
}

/// The view of the row of tabs: the control at its natural width, its visible background starting
/// where the lines' text does, and the dots of the raised tabs laid over their segments.
///
/// The names are cut here, to what fits the narrowest the menu is, before the room for a dot is
/// added after them: left to AppKit, a label is cut as a whole, room included, and the ellipsis
/// would come after a stray space.
pub(super) fn strip_row(strip: &TabStrip, mtm: MainThreadMarker) -> Retained<NSView> {
    let font = NSFont::systemFontOfSize(NSFont::systemFontSize());
    let room = {
        let one = text_width(&DOT_ROOM.to_string(), &font).max(1.0);
        ((DOT_GAP + DOT_SIDE) / one).ceil() as usize
    };
    let room = DOT_ROOM.to_string().repeat(room);
    let shown_of = |names: &[String]| -> Vec<String> {
        names
            .iter()
            .zip(&strip.raised)
            .map(|(name, raised)| {
                if *raised {
                    format!("{name}{room}")
                } else {
                    name.clone()
                }
            })
            .collect()
    };
    let mut names = strip.labels.clone();
    let mut shown = shown_of(&names);
    let mut control = control_of(&shown, &font, mtm);
    // AppKit gives every segment the same padding around its text, which is what is left of the
    // control's width once the texts are taken out, shared out by segment.
    let padding = |control: &NSSegmentedControl, shown: &[String]| {
        let texts: f64 = shown.iter().map(|label| text_width(label, &font)).sum();
        (control.frame().size.width - texts) / shown.len().max(1) as f64
    };
    let budget = MIN_WIDTH - 2.0 * ORIGIN;
    while control.frame().size.width > budget {
        let widths: Vec<f64> = shown.iter().map(|label| text_width(label, &font)).collect();
        let Some((widest, width)) = widths
            .iter()
            .copied()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
        else {
            break;
        };
        let over = control.frame().size.width - budget;
        let name_width = text_width(&names[widest], &font);
        let trimmed = cut_to(&names[widest], name_width - over - 1.0, &font);
        if trimmed == names[widest] || width <= 0.0 {
            break;
        }
        names[widest] = trimmed;
        shown = shown_of(&names);
        control = control_of(&shown, &font, mtm);
    }
    for (index, (name, full)) in names.iter().zip(&strip.labels).enumerate() {
        if name != full {
            control.setToolTip_forSegment(Some(&NSString::from_str(full)), index as isize);
        }
    }
    let size = control.frame().size;
    let width = size.width + 2.0 * ORIGIN;
    let height = size.height + 2.0 * PAD;
    let row = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height)),
    );
    row.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    row.setAccessibilityElement(false);
    control.setFrameOrigin(NSPoint::new(ORIGIN, PAD));
    control.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMaxXMargin);
    row.addSubview(&control);
    if let Some(dot) = raised_hand() {
        let padding = padding(&control, &shown);
        let mut edge = 0.0;
        for (label, raised) in shown.iter().zip(&strip.raised) {
            edge += text_width(label, &font) + padding;
            if *raised {
                // Where the padding after the room's end begins, as the name's padding precedes it.
                let x = ORIGIN + edge - padding / 2.0 - DOT_SIDE;
                let y = PAD + (size.height - DOT_SIDE) / 2.0;
                let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(DOT_SIDE, DOT_SIDE));
                row.addSubview(&DotView::new(&dot, frame, mtm));
            }
        }
    }
    row
}

/// The orange dot after the name of a tab whose daemon has a session waiting for the user.
fn raised_hand() -> Option<Retained<NSImage>> {
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

#[cfg(test)]
mod tests {
    use super::pop_cluster;

    fn popped(text: &str) -> String {
        let mut kept: Vec<char> = text.chars().collect();
        pop_cluster(&mut kept);
        kept.into_iter().collect()
    }

    #[test]
    fn drops_a_plain_character() {
        assert_eq!(popped("abc"), "ab");
        assert_eq!(popped(""), "");
    }

    #[test]
    fn drops_a_base_with_its_combining_marks() {
        assert_eq!(popped("ae\u{0301}"), "a");
        assert_eq!(popped("a\u{0915}\u{0308}\u{0301}"), "a");
    }

    #[test]
    fn drops_an_emoji_with_its_selector_or_skin_tone() {
        assert_eq!(popped("a\u{2764}\u{FE0F}"), "a");
        assert_eq!(popped("a\u{1F44D}\u{1F3FD}"), "a");
    }

    #[test]
    fn drops_a_devanagari_or_thai_base_with_its_vowel_signs_and_marks() {
        // क + virama (U+094D), then the vowel sign i (U+093F) after a base and a candrabindu (U+0901).
        assert_eq!(popped("a\u{0915}\u{094D}"), "a");
        assert_eq!(popped("a\u{0915}\u{093F}\u{0901}"), "a");
        assert_eq!(popped("a\u{0915}\u{0947}\u{0962}"), "a");
        // ก + sara i (U+0E34) + mai ek (U+0E48), and a base with mai han-akat (U+0E31).
        assert_eq!(popped("a\u{0E01}\u{0E34}\u{0E48}"), "a");
        assert_eq!(popped("a\u{0E01}\u{0E31}"), "a");
        // ก + sara am (U+0E33).
        assert_eq!(popped("a\u{0E01}\u{0E33}"), "a");
    }

    #[test]
    fn drops_a_devanagari_avagraha_alone() {
        // ऽ (U+093D) is a letter of its own, not a mark of the base before it.
        assert_eq!(popped("a\u{0915}\u{093D}"), "a\u{0915}");
    }

    #[test]
    fn drops_a_devanagari_conjunct_whole() {
        // क्ष: ka, virama, ssa; and the conjunct with a vowel sign after it.
        assert_eq!(popped("a\u{0915}\u{094D}\u{0937}"), "a");
        assert_eq!(popped("a\u{0915}\u{094D}\u{0937}\u{093F}"), "a");
        // Only the conjunct goes, not the consonant before it.
        assert_eq!(popped("\u{0915}\u{0915}\u{094D}\u{0937}"), "\u{0915}");
    }

    #[test]
    fn drops_a_flag_whole_and_a_lone_indicator_alone() {
        assert_eq!(popped("a\u{1F1EF}\u{1F1F5}"), "a");
        // Two flags: the second goes, then the first.
        assert_eq!(
            popped("\u{1F1EF}\u{1F1F5}\u{1F1FA}\u{1F1F8}"),
            "\u{1F1EF}\u{1F1F5}"
        );
        // An odd run ends in a lone indicator.
        assert_eq!(popped("\u{1F1EF}\u{1F1F5}\u{1F1FA}"), "\u{1F1EF}\u{1F1F5}");
    }

    #[test]
    fn drops_a_joined_emoji_sequence_whole() {
        assert_eq!(popped("a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"), "a");
    }
}
