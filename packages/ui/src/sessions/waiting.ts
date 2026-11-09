import type { Console, Project, Session } from "../protocol";
import { type SessionOrder, sessionsInTreeOrder } from "./order";
import { holdsPendingLink } from "./sessionStatus";

/** Whether `session` counts as waiting for the user: its stored status says so and it shows that status. One whose
 * process cannot be reached (`holdsPendingLink`) shows *reconnecting* instead, and its stored status may be stale, so
 * the waiting badge and the jump to the next waiting session leave it out until the link is back. */
export function waitsForUser(session: Session): boolean {
  return session.status === "waiting_user" && !holdsPendingLink(session);
}

/** The sessions waiting for the user (`waitsForUser`), in the order of `sessionsInTreeOrder`. */
export function waitingSessionsInTreeOrder(consoles: Console[], projects: Project[], sessions: Session[], delegating: ReadonlySet<string>, queuedSwitches: ReadonlySet<string>, order: SessionOrder): Session[] {
  return sessionsInTreeOrder(consoles, projects, sessions, waitsForUser, delegating, queuedSwitches, order);
}

/** How many sessions `waitingSessionsInTreeOrder` lists, counted without ordering them: the waiting
 * ones that sit in a console, and for a project session in one of that console's projects. A
 * session of a console or project the store does not hold is in no list. */
export function countWaitingInTree(consoles: Map<string, Console>, projects: Map<string, Project>, sessions: Map<string, Session>): number {
  let count = 0;
  for (const session of sessions.values()) {
    if (!waitsForUser(session) || !consoles.has(session.console_id)) continue;
    if (session.kind === "console" || (session.project_id && projects.get(session.project_id)?.console_id === session.console_id)) count += 1;
  }
  return count;
}

/** The waiting session after `currentId` in `waiting`, wrapping around; the first one when the
 * current session is not among them. */
export function nextWaitingSession(waiting: Session[], currentId: string | undefined): Session | undefined {
  const index = waiting.findIndex((s) => s.id === currentId);
  return waiting[(index + 1) % waiting.length];
}

/** How many of `sessions` count as waiting for the user (`waitsForUser`), wherever they sit: the app icon's badge. */
export function countWaiting(sessions: Iterable<Session>): number {
  let count = 0;
  for (const session of sessions) if (waitsForUser(session)) count += 1;
  return count;
}
