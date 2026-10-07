import type { Console, Project, Session } from "./protocol";
import { compareSessions, sortProjects } from "./sidebar/order";

/** The sessions waiting for the user, in the order the sidebar tree lists them: console by console,
 * its console sessions first (in the console sessions section's own order), then its projects'
 * sessions in the sidebar's own order. */
export function waitingSessionsInTreeOrder(consoles: Console[], projects: Project[], sessions: Session[]): Session[] {
  const waiting = sessions.filter((s) => s.status === "waiting_user");
  return consoles.flatMap((console) => {
    const mine = waiting.filter((s) => s.console_id === console.id);
    return [
      ...mine.filter((s) => s.role === "console").sort(compareSessions),
      ...sortProjects(
        projects.filter((p) => p.console_id === console.id),
        (project) => sessions.filter((s) => s.project_id === project.id),
      ).flatMap((project) => mine.filter((s) => s.role !== "console" && s.project_id === project.id).sort(compareSessions)),
    ];
  });
}

/** The waiting session after `currentId` in `waiting`, wrapping around; the first one when the
 * current session is not among them. */
export function nextWaitingSession(waiting: Session[], currentId: string | undefined): Session | undefined {
  const index = waiting.findIndex((s) => s.id === currentId);
  return waiting[(index + 1) % waiting.length];
}
