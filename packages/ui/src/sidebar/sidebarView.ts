import { useEffect, useMemo } from "react";
import { useShallow } from "zustand/react/shallow";

import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import type { Console } from "../protocol";
import { useDaemonStore } from "../store";
import { focusKey, resolveFocus } from "./focus";
import type { FocusTarget } from "./types";

/** The current console, which the sidebar shows, and what is in focus mode, if anything. Kept per window profile in
 * `localStorage`, like the panes' visibility: a convenience, so a stored id that no longer names a
 * record is simply ignored. */
const consolePreference = createPersistedPreference<string | undefined>(
  PREFERENCE_KEYS.sidebarConsole,
  (raw) => raw ?? undefined,
  (value) => value ?? null,
);
/** The focused thing as `project:<id>` or `consoleSession:<id>`. */
const focusPreference = createPersistedPreference<string | undefined>(
  PREFERENCE_KEYS.sidebarFocus,
  (raw) => raw ?? undefined,
  (value) => value ?? null,
);

/** The console shown: the chosen one while it exists, otherwise the first. */
function currentConsoleOf(consoles: Console[], chosenId: string | undefined): Console | undefined {
  return consoles.find((c) => c.id === chosenId) ?? consoles[0];
}

/** The console the sidebar shows, for what acts on "the console in hand" from outside it. The
 * daemon's consoles in the order the store holds them, which is the order the sidebar is given. */
export function useCurrentConsole(): Console | undefined {
  const consoles = useDaemonStore((s) => s.consoles);
  const chosenId = consolePreference.useValue();
  return useMemo(() => currentConsoleOf(Array.from(consoles.values()), chosenId), [consoles, chosenId]);
}

export interface SidebarView {
  /** The console shown: the chosen one while it exists, otherwise the first. */
  currentConsole?: Console;
  /** What is in focus mode, while it exists and belongs to `currentConsole`. */
  focus?: FocusTarget;
  selectConsole: (consoleId: string) => void;
  setFocus: (target: FocusTarget | undefined) => void;
}

/** The sidebar's view of `consoles`. The projects and sessions its focus mode names are read from
 * the store here, as the one record in focus, so a change of any other session leaves it alone. */
export function useSidebarView(consoles: Console[]): SidebarView {
  const consoleId = consolePreference.useValue();
  const focusId = focusPreference.useValue();
  const currentConsole = currentConsoleOf(consoles, consoleId);
  const currentId = currentConsole?.id;
  // `resolveFocus` wraps the record in a new object each call; shallowly equal, it is the same one.
  const focus = useDaemonStore(useShallow((s) => resolveFocus(focusId, currentId, s.projects, s.sessions)));
  // A console session archived while in focus is left for good: were the memory kept, reopening it
  // later would put the sidebar straight back into its focus mode. A record that is merely not
  // known yet (before the first snapshot) is kept, as a project's is.
  const archivedInFocus = useDaemonStore(
    (s) => focusId?.startsWith("consoleSession:") === true && s.sessions.get(focusId.slice("consoleSession:".length))?.status === "archived",
  );
  useEffect(() => {
    if (archivedInFocus) focusPreference.set(undefined);
  }, [archivedInFocus]);
  return {
    currentConsole,
    focus,
    selectConsole: (id) => {
      if (id === currentConsole?.id) return;
      consolePreference.set(id);
      focusPreference.set(undefined);
    },
    setFocus: (target) => focusPreference.set(target && focusKey(target)),
  };
}
