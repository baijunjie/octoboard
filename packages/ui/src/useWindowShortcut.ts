import { useEffect, useRef } from "react";

import { MODAL_OPEN } from "./layout/useRegionCycle";

/** Whether the keyboard is an Apple one, where ⌘ is the shortcut modifier. */
export const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

/**
 * A window-wide keyboard shortcut. `match` says what, if anything, a key press means (it is told
 * nothing else, so it can be a plain function of the event); a match calls `run` with it.
 *
 * Listened for on the window's capture phase so the key never reaches the terminal (and through it
 * the agent), and swallowed (not run) while a dialog or menu is open and during an input method
 * composition. `enabled` is false where the shortcut does not exist, and the key is then left alone.
 */
export function useWindowShortcut<T>(
  match: (event: KeyboardEvent) => T | undefined,
  run: (matched: T) => void,
  enabled = true,
): void {
  const latest = useRef({ match, run });
  latest.current = { match, run };
  useEffect(() => {
    if (!enabled) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      const matched = latest.current.match(event);
      if (matched === undefined) return;
      event.preventDefault();
      event.stopPropagation();
      if (document.querySelector(MODAL_OPEN)) return;
      latest.current.run(matched);
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [enabled]);
}
