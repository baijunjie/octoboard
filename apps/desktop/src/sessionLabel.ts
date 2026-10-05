import type { Console, Project, Session, SessionStatus } from "./protocol";

/** Where to tell the user a session is, since the daemon's `Session` record itself only carries
 * ids. A hub session has no project, so it is named for its console instead. */
export function sessionLocation(session: Session, consoles: Map<string, Console>, projects: Map<string, Project>): string {
  if (session.project_id) return projects.get(session.project_id)?.name ?? "a project";
  return `${consoles.get(session.console_id)?.name ?? "its console"}'s hub`;
}

/** Human labels for the "Session statuses" table in docs/product/sessions.md. A row's own
 * `aria-label` wins over one on an element nested inside it, so the wire enum must never be the
 * only place a status is put into words. */
export const STATUS_LABEL: Record<SessionStatus, string> = {
  working: "working",
  waiting_user: "waiting for you",
  idle: "awaiting instructions",
  interrupted: "interrupted",
  archived: "archived",
};
