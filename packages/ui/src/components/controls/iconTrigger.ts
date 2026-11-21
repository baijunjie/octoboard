/** What every small icon button of a row or a strip shares, with no tint of its own: the size and shape, and the
 * `sidebar-fills` the tints are drawn from. */
export const ICON_TRIGGER_BASE_CLASS = "sidebar-fills inline-flex row-icon-control items-center justify-center rounded-md p-0 text-app-muted";

/** How the actions menu's trigger looks: the one look of a small icon button in a row or a strip, which the other icon
 * buttons beside it take too (`ICON_TRIGGER_CLASS` on a `Button`), so they read as one set. It carries `sidebar-fills`,
 * the tints the menu's own wrapper gives the trigger, so each hovers alike on a dialog's surface as on the sidebar's.
 * Transparent with a muted glyph at rest, a subtle `bg-default` and the foreground glyph while hovered or open, never a
 * solid fill or an inverted glyph; a caller chooses no colours of its own for the same action. The glyph keeps 3:1
 * against its actual background in every state, while the hover tint, not the control's boundary, is held to none. */
export const ICON_TRIGGER_CLASS = `${ICON_TRIGGER_BASE_CLASS} hover:bg-default hover:text-foreground aria-expanded:bg-default aria-expanded:text-foreground`;
