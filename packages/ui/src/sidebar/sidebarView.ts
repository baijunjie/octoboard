import { useEffect } from "react";

import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import type { Console, Project, Session } from "../protocol";
import { focusKey, resolveFocus } from "./focus";
import type { FocusTarget } from "./types";

/** The console the sidebar shows, and what is in focus mode, if anything. Kept per window profile in
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

export interface SidebarView {
  /** The console shown: the chosen one while it exists, otherwise the first. */
  currentConsole?: Console;
  /** What is in focus mode, while it exists and belongs to `currentConsole`. */
  focus?: FocusTarget;
  selectConsole: (consoleId: string) => void;
  setFocus: (target: FocusTarget | undefined) => void;
}

export function useSidebarView(
  consoles: Console[],
  projects: Map<string, Project>,
  sessions: Map<string, Session>,
): SidebarView {
  const consoleId = consolePreference.useValue();
  const focusId = focusPreference.useValue();
  const currentConsole = consoles.find((c) => c.id === consoleId) ?? consoles[0];
  const focus = resolveFocus(focusId, currentConsole?.id, projects, sessions);
  // A console session archived while in focus is left for good: were the memory kept, reopening it
  // later would put the sidebar straight back into its focus mode. A record that is merely not
  // known yet (before the first snapshot) is kept, as a project's is.
  const archivedInFocus = focusId?.startsWith("consoleSession:") && sessions.get(focusId.slice("consoleSession:".length))?.status === "archived";
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
