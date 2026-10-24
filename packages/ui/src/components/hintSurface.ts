// What every floating hint is drawn with and placed by.

/** Where a tooltip opens relative to what it belongs to, and how far from it. Both are HeroUI's own
 * defaults for a tooltip with no arrow, spelled out so the touch popover takes the same ones. */
export const PLACEMENT = "top";
export const OFFSET = 3;

/** The one visual definition of every floating surface that says something: HeroUI's own `tooltip`
 * class (surface, padding, text size, radius, width cap, word breaking, and the dialog fills of
 * `style.css`) with the application's `control-fills`, which a surface portalled out of the pane or
 * dialog that carries the fills needs. The tooltip, the touch popover and the hover popover are all
 * drawn with it; they differ in the element underneath, since a tooltip cannot hold focus or a button
 * and so cannot be what holds a Try again or selectable text. */
export const TOOLTIP_SURFACE = "tooltip control-fills outline-none";
