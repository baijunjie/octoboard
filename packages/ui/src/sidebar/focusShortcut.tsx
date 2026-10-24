import { matchesCommand } from "../shortcuts/store";
import { useWindowShortcut } from "../useWindowShortcut";

/** Matched on the physical key, so a keyboard layout or a held Shift does not change it. Its default,
 * ⇧⌘F on macOS and Ctrl+Shift+F elsewhere, is a combination the terminal's agents do not use, since
 * on macOS ⌘ never reaches a terminal application, and not one of the system's own. */
function matchFocusShortcut(event: KeyboardEvent): true | undefined {
  return matchesCommand("focusMode", event) ? true : undefined;
}

/** Toggles focus mode from the keyboard: enters it for the project of the selected session, or
 * leaves it. */
export function useFocusShortcut(toggle: () => void): void {
  useWindowShortcut(matchFocusShortcut, () => toggle());
}
