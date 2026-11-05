import type { HostFacts } from "../platform";

/** Whether a press is one that opens a context menu: the secondary button, or on macOS, where the
 * system reads it as a right-click, Ctrl with the primary one. `ctrlClickOnEveryPlatform` counts
 * Ctrl+click elsewhere too, for a target whose ordinary presses have no use for it. */
export function isSecondaryPress(event: Pick<MouseEvent, "button" | "ctrlKey">, host: Pick<HostFacts, "os">, ctrlClickOnEveryPlatform = false): boolean {
  return event.button === 2 || (event.button === 0 && event.ctrlKey && (ctrlClickOnEveryPlatform || host.os === "macos"));
}

/** Whether the platform's shortcut modifier is held, and not the other one: ⌘ on an Apple keyboard,
 * Ctrl elsewhere. */
export function isPrimaryModifier(event: Pick<KeyboardEvent, "metaKey" | "ctrlKey">, host: Pick<HostFacts, "modifier">): boolean {
  return host.modifier === "meta" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}
