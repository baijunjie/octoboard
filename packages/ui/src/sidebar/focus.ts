import type { Project, Session } from "../protocol";
import type { FocusTarget } from "./types";

/** The id of the project or console session in focus. */
export function focusTargetId(target: FocusTarget): string {
  return "project" in target ? target.project.id : target.consoleSession.id;
}

/** The focused thing as the stored key `resolveFocus` reads. */
export function focusKey(target: FocusTarget): string {
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

/** The focus mode to be in while `session` is selected: `focus`, unless it does not show `session`
 * (`belongsToFocus`, which reads `sessions`), which leaves it. Every way of selecting a session
 * applies this rule. */
export function focusFor(
  focus: FocusTarget | undefined,
  session: Session | undefined,
  sessions: Map<string, Session>,
): FocusTarget | undefined {
  return focus && session && !belongsToFocus(focus, session, sessions) ? undefined : focus;
}

/** The focus mode to be in after selecting `session`: that console session's own when `enterFocus`
 * asks for it, otherwise what `focusFor` leaves of `focus`. */
export function focusAfterSelect(
  focus: FocusTarget | undefined,
  session: Session,
  enterFocus: boolean,
  sessions: Map<string, Session>,
): FocusTarget | undefined {
  return enterFocus ? { consoleSession: session } : focusFor(focus, session, sessions);
}

/** Whether a window that approved a request for a console session moves to the console session it
 * started: only one in the focus mode of the requesting session's own project (`requestingProject`)
 * does, into the console session's own. One in another project's or a console session's focus mode,
 * or not in focus mode, stays as it is. */
export function followsStartedConsoleSession(focus: FocusTarget | undefined, requestingProject: string | null): boolean {
  return focus !== undefined && "project" in focus && focus.project.id === requestingProject;
}

/** How ⌃Tab selects the console session it moves to: its focus mode is entered, as with a press on
 * its chip, but an interrupted session is only shown, not resumed — a held or repeated ⌃Tab passes
 * over console sessions on its way, and must not start an agent in each. */
export const KEY_SWITCH_SELECTION = { enterFocus: true, resume: false } as const;

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

/** The console session ⌃Tab (`backward`: ⌃⇧Tab) moves to from `currentId`, going round the strip's
 * console sessions (`switchStrip`) at either end. `undefined` when there is nowhere to go: fewer
 * than two console sessions, or `currentId` is not one of them. */
export function cycleConsoleSession(strip: Session[], currentId: string, backward: boolean): Session | undefined {
  const index = strip.findIndex((s) => s.id === currentId);
  if (strip.length < 2 || index < 0) return undefined;
  return strip[(index + (backward ? -1 : 1) + strip.length) % strip.length];
}
