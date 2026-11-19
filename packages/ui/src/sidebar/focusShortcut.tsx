import { useHost, useOptionalPlatform } from "../platform/react";
import type { ModifierHost } from "../shortcuts/binding";
import { matchesCommand } from "../shortcuts/store";
import { KEEP_DEFAULT, useWindowShortcut } from "../shortcuts/useWindowShortcut";

/** What a focus-mode press is: which command, and whether it changes focus mode (a held key's repeat never does). */
interface FocusPress {
  action: "enter" | "exit";
  changes: boolean;
}

/** Matched on the physical key, so a keyboard layout or a held Shift does not change it. The defaults, ⌥⌘→ and ⌥⌘←
 * (Ctrl+Alt+→ and Ctrl+Alt+← elsewhere), are combinations neither the application nor its terminals' programs use
 * (see `REBINDABLE` in `shortcuts/commands.ts`), but a browser may: Chrome switches tabs with ⌥⌘← and ⌥⌘→. So in a
 * browser a press is taken only when it changes focus mode (entering while out of it, leaving while in it); any other
 * keeps its default for the browser and the system, and goes no further into the page (`KEEP_DEFAULT`), so the terminal
 * never sends it to the agent. The desktop app has no tabs, and a key it lets through can make the system beep: there
 * every press is taken. A key held down repeats only what its first press did, so its repeats are taken and do nothing:
 * a first press the browser took has moved focus away from the page by then. */
function matchFocusShortcut(event: KeyboardEvent, host: ModifierHost, can: { enter: boolean; exit: boolean }, desktopApp: boolean): FocusPress | typeof KEEP_DEFAULT | undefined {
  const action = matchesCommand("enterFocusMode", event, host) ? "enter" : matchesCommand("exitFocusMode", event, host) ? "exit" : undefined;
  if (action === undefined) return undefined;
  const changes = !event.repeat && can[action];
  if (!changes && !event.repeat && !desktopApp) return KEEP_DEFAULT;
  return { action, changes };
}

/** Enters focus mode on the selected session's context, or leaves it, from the keyboard. `canEnter` and `canExit` say
 * whether a press would change anything; one that would not runs neither (`matchFocusShortcut`). */
export function useFocusShortcut({ canEnter, canExit, enter, exit }: { canEnter: boolean; canExit: boolean; enter: () => void; exit: () => void }): void {
  // The window's own kind decides, not the daemon's machine: the desktop app is the client with no browser around it.
  const desktopApp = useOptionalPlatform()?.kind === "tauri";
  const host = useHost();
  useWindowShortcut((event) => matchFocusShortcut(event, host, { enter: canEnter, exit: canExit }, desktopApp), ({ action, changes }) => {
    if (!changes) return;
    if (action === "enter") enter();
    else exit();
  });
}
