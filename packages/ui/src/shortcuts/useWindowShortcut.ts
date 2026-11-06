import { useEffect } from "react";

import { runMenuCommand } from "../components/menus/menuCommands";
import { MODAL_OPEN } from "../components/focus/selectors";
import { useLatest } from "../components/hooks/useLatest";

/**
 * A window-wide keyboard shortcut. `match` says what, if anything, a key press means (it is told
 * nothing else, so it can be a plain function of the event); a match calls `run` with it.
 *
 * Listened for on the window's capture phase so the key never reaches the terminal (and through it
 * the agent). A modal suppresses the window action; an open menu may take a command its own item
 * advertises. Input-method composition is left alone. `enabled` is false where the shortcut does not exist,
 * and the key is then left alone.
 */
export function useWindowShortcut<T>(
  match: (event: KeyboardEvent) => T | undefined,
  run: (matched: T) => void,
  enabled = true,
): void {
  const latest = useLatest({ match, run });
  useEffect(() => {
    if (!enabled) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      const matched = latest.current.match(event);
      if (matched === undefined) return;
      event.preventDefault();
      event.stopPropagation();
      if (document.querySelector(MODAL_OPEN)) {
        runMenuCommand(event);
        return;
      }
      latest.current.run(matched);
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [enabled]);
}
