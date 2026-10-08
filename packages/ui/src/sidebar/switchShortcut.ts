import type { Session } from "../protocol";
import { MODAL_OPEN } from "../layout/useRegionCycle";
import { usePlatform } from "../platform/react";
import { useWindowShortcut } from "../useWindowShortcut";
import { cycleConsoleSession } from "./focus";
import type { FocusTarget } from "./types";

/** ⌃Tab and ⌃⇧Tab: the next and the previous console session. It exists only in the desktop app (macOS),
 * where Ctrl rather than ⌘ is the browsers' own choice for switching tabs; matched on the key, since
 * Tab is not a layout-dependent one. A held key is matched too, to be kept from the terminal, but
 * `repeat` has it not move on, so it cannot race through the strip. */
function matchSwitchShortcut(event: KeyboardEvent): { backward: boolean; repeat: boolean } | undefined {
  if (event.key !== "Tab" || !event.ctrlKey || event.metaKey || event.altKey) return undefined;
  return { backward: event.shiftKey, repeat: event.repeat };
}

/**
 * Moves through the switch strip's console sessions (`strip`, in its order) from the keyboard, going
 * round at either end, handing the one to show to `onSwitch`. It exists only while a console
 * session's focus mode is on screen (`shown`) with another console session to go to, and only where
 * the window has `windowChrome` (the desktop app): a browser keeps ⌃Tab for its own tabs. Elsewhere
 * the key is left alone, for the terminal's agent.
 *
 * Returns the move itself, for the same key press arriving another way (the report page's relay);
 * it does nothing where the shortcut does not exist, or while a dialog or menu is open.
 */
export function useSwitchShortcut({
  focus,
  strip,
  shown,
  onSwitch,
}: {
  focus: FocusTarget | undefined;
  strip: Session[];
  shown: boolean;
  onSwitch: (session: Session) => void;
}): (backward: boolean) => void {
  const { windowChrome } = usePlatform();
  const currentId = focus && "consoleSession" in focus ? focus.consoleSession.id : undefined;
  const available = windowChrome !== undefined && shown && currentId !== undefined && strip.length > 1;
  const move = (backward: boolean) => {
    if (!available || document.querySelector(MODAL_OPEN)) return;
    const next = cycleConsoleSession(strip, currentId, backward);
    if (next) onSwitch(next);
  };
  useWindowShortcut(
    matchSwitchShortcut,
    ({ backward, repeat }) => {
      if (!repeat) move(backward);
    },
    available,
  );
  return move;
}
