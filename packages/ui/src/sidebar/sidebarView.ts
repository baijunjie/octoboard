import { useEffect } from "react";
import { useShallow } from "zustand/react/shallow";

import { consolePreference, currentConsoleOf } from "../store";
import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import type { Console } from "../protocol";
import { useDaemonStore } from "../store";
import { focusKey, resolveFocus } from "./focus";
import type { FocusTarget } from "./types";

/** What is in focus mode, if anything, as `project:<id>` or `consoleSession:<id>`; kept like the current
 * console (`currentConsole.ts`). */
const focusPreference = createPersistedPreference<string | undefined>(
  PREFERENCE_KEYS.sidebarFocus,
  (raw) => raw ?? undefined,
  (value) => value ?? null,
);

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
