import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import type { Console, Project } from "../protocol";

/** The console the sidebar shows, and the project in focus mode, if any. Kept per window profile in
 * `localStorage`, like the panes' visibility: a convenience, so a stored id that no longer names a
 * record is simply ignored. */
const consolePreference = createPersistedPreference<string | undefined>(
  PREFERENCE_KEYS.sidebarConsole,
  (raw) => raw ?? undefined,
  (value) => value ?? null,
);
const focusPreference = createPersistedPreference<string | undefined>(
  PREFERENCE_KEYS.sidebarFocusProject,
  (raw) => raw ?? undefined,
  (value) => value ?? null,
);

export interface SidebarView {
  /** The console shown: the chosen one while it exists, otherwise the first. */
  currentConsole?: Console;
  /** The project in focus mode, while it exists and belongs to `currentConsole`. */
  focusProject?: Project;
  selectConsole: (consoleId: string) => void;
  focusProjectId: (projectId: string | undefined) => void;
}

export function useSidebarView(consoles: Console[], projects: Map<string, Project>): SidebarView {
  const consoleId = consolePreference.useValue();
  const focusId = focusPreference.useValue();
  const currentConsole = consoles.find((c) => c.id === consoleId) ?? consoles[0];
  const focused = focusId ? projects.get(focusId) : undefined;
  return {
    currentConsole,
    focusProject: focused && focused.console_id === currentConsole?.id ? focused : undefined,
    selectConsole: (id) => {
      if (id === currentConsole?.id) return;
      consolePreference.set(id);
      focusPreference.set(undefined);
    },
    focusProjectId: (id) => focusPreference.set(id),
  };
}
