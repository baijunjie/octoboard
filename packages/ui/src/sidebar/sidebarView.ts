import { useEffect } from "react";

import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import type { Console, Project, Session } from "../protocol";
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

/** The id of the project or console session in focus. */
export function focusTargetId(target: FocusTarget): string {
  return "project" in target ? target.project.id : target.consoleSession.id;
}

function focusKey(target: FocusTarget): string {
  return `${"project" in target ? "project" : "consoleSession"}:${focusTargetId(target)}`;
}

/** The focused thing a stored `key` names, while it exists and belongs to the console shown. A
 * console session that is archived no longer has a row in the sidebar, so it counts as gone. */
export function resolveFocus(
  key: string | undefined,
  consoleId: string | undefined,
  projects: Map<string, Project>,
  sessions: Map<string, Session>,
): FocusTarget | undefined {
  const separator = key?.indexOf(":") ?? -1;
  if (!key || separator < 0) return undefined;
  const id = key.slice(separator + 1);
  switch (key.slice(0, separator)) {
    case "project": {
      const project = projects.get(id);
      return project && project.console_id === consoleId ? { project } : undefined;
    }
    case "consoleSession": {
      const consoleSession = sessions.get(id);
      return consoleSession?.role === "console" && consoleSession.console_id === consoleId && consoleSession.status !== "archived"
        ? { consoleSession }
        : undefined;
    }
    default:
      return undefined;
  }
}

/** Whether `session` is something the focused thing shows: a project's sessions not bound to a
 * console session (`sessions` is every session, to tell which owner is one) and, archived, all of
 * its sessions; a console session itself and the sessions bound to it. Selecting a session that is
 * not leaves focus mode, so what is selected is always on screen in the sidebar. */
export function belongsToFocus(target: FocusTarget, session: Session, sessions: Map<string, Session>): boolean {
  if ("project" in target) {
    const consoleOwned = !!session.bound_to && sessions.get(session.bound_to)?.role === "console";
    return session.project_id === target.project.id && (session.status === "archived" || !consoleOwned);
  }
  return session.id === target.consoleSession.id || session.bound_to === target.consoleSession.id;
}

/** What the focus-mode shortcut does: enter the focus mode of the selected session's context — a
 * project session's project, or a console session itself — or, in focus mode, leave it. `undefined`
 * when it does nothing, otherwise the focus mode to be in afterwards. */
export function shortcutOutcome(
  focus: FocusTarget | undefined,
  selected: Session | undefined,
  projects: Map<string, Project>,
): { focus: FocusTarget | undefined } | undefined {
  if (focus) return { focus: undefined };
  if (!selected) return undefined;
  if (selected.role === "console") {
    return selected.status === "archived" ? undefined : { focus: { consoleSession: selected } };
  }
  const project = selected.project_id ? projects.get(selected.project_id) : undefined;
  return project ? { focus: { project } } : undefined;
}

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
