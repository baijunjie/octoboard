import { useMemo } from "react";

import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import type { Console } from "../protocol";
import { useDaemonStore } from "./react";

/** The current console, which the sidebar shows. Kept per window profile in `localStorage`, like the panes'
 * visibility: a convenience, so a stored id that no longer names a record is simply ignored. */
export const consolePreference = createPersistedPreference<string | undefined>(
  PREFERENCE_KEYS.sidebarConsole,
  (raw) => raw ?? undefined,
  (value) => value ?? null,
);

/** The console shown: the chosen one while it exists, otherwise the first. */
export function currentConsoleOf(consoles: Console[], chosenId: string | undefined): Console | undefined {
  return consoles.find((c) => c.id === chosenId) ?? consoles[0];
}

/** The console the sidebar shows, for what acts on "the console in hand" from outside it. The
 * daemon's consoles in the order the store holds them, which is the order the sidebar is given. */
export function useCurrentConsole(): Console | undefined {
  const consoles = useDaemonStore((s) => s.consoles);
  const chosenId = consolePreference.useValue();
  return useMemo(() => currentConsoleOf(Array.from(consoles.values()), chosenId), [consoles, chosenId]);
}
