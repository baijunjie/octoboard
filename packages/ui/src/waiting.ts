import type { Console, Project, Session } from "./protocol";
import { sessionsInTreeOrder } from "./sidebar/order";

/** The sessions waiting for the user, in the order of `sessionsInTreeOrder`. */
export function waitingSessionsInTreeOrder(consoles: Console[], projects: Project[], sessions: Session[]): Session[] {
  return sessionsInTreeOrder(consoles, projects, sessions, (s) => s.status === "waiting_user");
}

/** The waiting session after `currentId` in `waiting`, wrapping around; the first one when the
 * current session is not among them. */
export function nextWaitingSession(waiting: Session[], currentId: string | undefined): Session | undefined {
  const index = waiting.findIndex((s) => s.id === currentId);
  return waiting[(index + 1) % waiting.length];
}
