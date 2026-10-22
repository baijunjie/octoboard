import type { Console, Project, Session } from "./protocol";
import { sessionsInTreeOrder } from "./sidebar/order";

/** The sessions waiting for the user, in the order of `sessionsInTreeOrder`. */
export function waitingSessionsInTreeOrder(consoles: Console[], projects: Project[], sessions: Session[], byStatus: boolean): Session[] {
  return sessionsInTreeOrder(consoles, projects, sessions, (s) => s.status === "waiting_user", byStatus);
}

/** How many sessions `waitingSessionsInTreeOrder` lists, counted without ordering them: the waiting
 * ones that sit in a console, and for a project session in one of that console's projects. A
 * session of a console or project the store does not hold is in no list. */
export function countWaitingInTree(consoles: Map<string, Console>, projects: Map<string, Project>, sessions: Map<string, Session>): number {
  let count = 0;
  for (const session of sessions.values()) {
    if (session.status !== "waiting_user" || !consoles.has(session.console_id)) continue;
    if (session.role === "console" || (session.project_id && projects.get(session.project_id)?.console_id === session.console_id)) count += 1;
  }
  return count;
}

/** The waiting session after `currentId` in `waiting`, wrapping around; the first one when the
 * current session is not among them. */
export function nextWaitingSession(waiting: Session[], currentId: string | undefined): Session | undefined {
  const index = waiting.findIndex((s) => s.id === currentId);
  return waiting[(index + 1) % waiting.length];
}
