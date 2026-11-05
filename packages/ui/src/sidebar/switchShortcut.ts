import type { Session } from "../protocol";
import { MODAL_OPEN } from "../components/focus/selectors";
import type { HostFacts } from "../platform/host";
import { usePlatform } from "../platform/react";
import { commandExists } from "../shortcuts/commands";
import { matchesCommand } from "../shortcuts/store";
import { useWindowShortcut } from "../shortcuts/useWindowShortcut";
import { cycleConsoleSession } from "./focus";
import type { FocusTarget } from "./types";

/** The keys of the next and the previous console session, Control with Tab and with Shift+Tab unless rebound (⌃Tab and
 * ⌃⇧Tab on an Apple keyboard, Ctrl+Tab and Ctrl+Shift+Tab elsewhere), the browsers' own keys for switching tabs; matched
 * on the physical key. A held key is matched too, to be kept from the terminal, but `repeat` has it not move on, so it
 * cannot race through the strip. */
function matchSwitchShortcut(event: KeyboardEvent, host: HostFacts): { backward: boolean; repeat: boolean } | undefined {
  if (matchesCommand("nextConsoleSession", event, host)) return { backward: false, repeat: event.repeat };
  if (matchesCommand("previousConsoleSession", event, host)) return { backward: true, repeat: event.repeat };
  return undefined;
}

/**
 * Moves through the switch strip's console sessions (`strip`, in its order) from the keyboard, going
 * round at either end, handing the one to show to `onSwitch`. It exists only while a console
 * session's focus mode is on screen (`shown`) with another console session to go to, and only in the
 * desktop app (the `app` shortcuts' client), on any system: a browser keeps the key for its own tabs
 * and never hands it to the page. Elsewhere the key is left alone, for the terminal's agent. Where it
 * exists it is taken on the window's capture phase (`useWindowShortcut`), before a focused terminal
 * sees it, on every system alike.
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
  const platform = usePlatform();
  const { host } = platform;
  const currentId = focus && "consoleSession" in focus ? focus.consoleSession.id : undefined;
  const available = commandExists("nextConsoleSession", platform) && shown && currentId !== undefined && strip.length > 1;
  const move = (backward: boolean) => {
    if (!available || document.querySelector(MODAL_OPEN)) return;
    const next = cycleConsoleSession(strip, currentId, backward);
    if (next) onSwitch(next);
  };
  useWindowShortcut(
    (event) => matchSwitchShortcut(event, host),
    ({ backward, repeat }) => {
      if (!repeat) move(backward);
    },
    available,
  );
  return move;
}
