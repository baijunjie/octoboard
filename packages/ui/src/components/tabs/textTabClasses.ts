/** The classes of a horizontal strip of tabs at their content's width, each a text label with at most an icon or a
 * mark beside it; shared so a strip looks the same wherever it is built, bar the compact tabs of a strip in a pane
 * header. */

/** The strip's `Tabs.ListContainer`: its start edge, as wide as its tabs, scrolling when they do not fit. */
export const TEXT_TAB_CONTAINER_CLASS = "w-max max-w-full self-start";

/** The `Tabs.List`; the padding leaves room for the focus ring and its offset inside the scroller's clip. */
export const TEXT_TAB_LIST_CLASS = "w-max min-w-0 p-1.5";

/** What every tab of such a strip has: a scroll margin (so a revealed tab keeps its ring in view), one line, and its
 * label taking the foreground colour on hover. */
const TAB_CLASS = "scroll-mx-1.5 whitespace-nowrap hover:text-foreground";

/** A tab at its content's width. */
export const TEXT_TAB_CLASS = `w-auto flex-none ${TAB_CLASS}`;

/** A compact tab, for a strip in a pane header beside the pane's title: short enough to sit in the header's row with
 * its focus ring, and narrow enough to leave the title its room. */
export const TEXT_TAB_COMPACT_CLASS = `h-6 px-1.5 ${TAB_CLASS}`;
