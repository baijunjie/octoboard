/** The classes of a horizontal strip of tabs at their content's width, each a text label with at most an icon or a
 * mark beside it; shared so the strip looks the same wherever it is built. */

/** The strip's `Tabs.ListContainer`: its start edge, as wide as its tabs, scrolling when they do not fit. */
export const TEXT_TAB_CONTAINER_CLASS = "w-max max-w-full self-start";

/** The `Tabs.List`; the padding leaves room for the focus ring and its offset inside the scroller's clip. */
export const TEXT_TAB_LIST_CLASS = "w-max min-w-0 p-1.5";

/** A tab's scroll margin (so a revealed tab keeps its ring in view), on one line, its label taking the
 * foreground colour on hover. */
export const TEXT_TAB_BASE_CLASS = "scroll-mx-1.5 whitespace-nowrap hover:text-foreground";

/** A tab at its content's width. */
export const TEXT_TAB_CLASS = `w-auto flex-none ${TEXT_TAB_BASE_CLASS}`;
