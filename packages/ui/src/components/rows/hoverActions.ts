/** The classes of a strip of actions clipped to no width until its row (a `group`) is hovered or holds
 * focus, and always shown on a device that cannot hover, where nothing would reveal them before a tap.
 * Hidden by clipping rather than removed, so they take no room yet stay reachable with Tab; focus lifts
 * the clip so the focus ring can paint. */
export const HOVER_ACTIONS_CLASS =
  "flex max-w-0 shrink-0 overflow-hidden opacity-0 transition-opacity motion-reduce:transition-none [@media(hover:none)]:max-w-none [@media(hover:none)]:overflow-visible [@media(hover:none)]:opacity-100 group-hover:max-w-none group-hover:opacity-100 group-focus-within:max-w-none group-focus-within:overflow-visible group-focus-within:opacity-100";
