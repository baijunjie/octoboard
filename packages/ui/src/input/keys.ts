/** Whether the keyboard is an Apple one, where ⌘ is the shortcut modifier. */
export const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

/** Whether a press is one that opens a context menu: the secondary button, or on macOS, where the
 * system reads it as a right-click, Ctrl with the primary one. `ctrlClickOnEveryPlatform` counts
 * Ctrl+click elsewhere too, for a target whose ordinary presses have no use for it. */
export function isSecondaryPress(event: Pick<MouseEvent, "button" | "ctrlKey">, ctrlClickOnEveryPlatform = false): boolean {
  return event.button === 2 || (event.button === 0 && event.ctrlKey && (ctrlClickOnEveryPlatform || IS_MAC));
}

/** Whether the platform's shortcut modifier is held, and not the other one: ⌘ on an Apple keyboard,
 * Ctrl elsewhere. */
export function isPrimaryModifier(event: Pick<KeyboardEvent, "metaKey" | "ctrlKey">): boolean {
  return IS_MAC ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}
