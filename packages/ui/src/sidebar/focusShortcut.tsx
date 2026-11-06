import { matchesCommand } from "../shortcuts/store";
import { useWindowShortcut } from "../shortcuts/useWindowShortcut";

/** Matched on the physical key, so a keyboard layout or a held Shift does not change it. Its default,
 * ⇧⌘F on macOS and Ctrl+Shift+F elsewhere, is a combination the terminal's agents do not use, since
 * on macOS ⌘ never reaches a terminal application, and not one of the system's own. */
function matchFocusShortcut(event: KeyboardEvent): { repeat: boolean } | undefined {
  return matchesCommand("focusMode", event) ? { repeat: event.repeat } : undefined;
}

/** Toggles focus mode from the keyboard: enters the selected session's context, or leaves it. */
export function useFocusShortcut(toggle: () => void): void {
  useWindowShortcut(matchFocusShortcut, ({ repeat }) => { if (!repeat) toggle(); });
}
