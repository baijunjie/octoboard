import { isLive, type Project, type Session, type SessionStatus } from "../protocol";

/** How urgently a status asks for the user's attention: a raised hand first, then a session at
 * work, then one sitting at its prompt, then one with no process. Archived sessions are listed
 * apart and never ranked against the others. */
const STATUS_RANK: Record<SessionStatus, number> = {
  waiting_user: 0,
  working: 1,
  idle: 2,
  interrupted: 3,
  archived: 4,
};

/** Pinned first; then by status (`STATUS_RANK`); then newest first. */
export function compareSessions(a: Session, b: Session): number {
  return (
    Number(b.pinned) - Number(a.pinned) ||
    STATUS_RANK[a.status] - STATUS_RANK[b.status] ||
    b.started_at - a.started_at
  );
}

/** A project's rank by the most urgent of its sessions: one with a raised hand, then one with a
 * session at work, then one with a session merely running, then the inactive ones (no session
 * with a running process), which therefore end the list after the pinned ones. */
function projectRank(sessions: Session[]): number {
  if (sessions.some((s) => s.status === "waiting_user")) return 0;
  if (sessions.some((s) => s.status === "working")) return 1;
  if (sessions.some((s) => s.status === "idle")) return 2;
  return 3;
}

/** Whether a project has no session with a running process. */
export function isInactiveProject(sessions: Session[]): boolean {
  return !sessions.some((s) => isLive(s.status));
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** The projects in sidebar order: pinned first; then by `projectRank`; then by name. */
export function sortProjects(projects: Project[], sessionsOf: (project: Project) => Session[]): Project[] {
  const rank = new Map(projects.map((p) => [p.id, projectRank(sessionsOf(p))]));
  return [...projects].sort(
    (a, b) =>
      Number(b.pinned) - Number(a.pinned) ||
      rank.get(a.id)! - rank.get(b.id)! ||
      collator.compare(a.name, b.name),
  );
}

/** A project's sessions in sidebar order: the ones not archived, ranked by `compareSessions`. */
export function liveSessions(sessions: Session[]): Session[] {
  return sessions.filter((s) => s.status !== "archived").sort(compareSessions);
}

/** Archived sessions, most recently ended first. */
export function archivedSessions(sessions: Session[]): Session[] {
  return sessions
    .filter((s) => s.status === "archived")
    .sort((a, b) => (b.ended_at ?? b.started_at) - (a.ended_at ?? a.started_at));
}

/** A console's live console sessions (not archived), newest first. Several may exist at once; this
 * is the new-session dialog's list of owners to choose from, not the sidebar's order (see "The
 * console sessions section and the project list" in `docs/product/sidebar.md` for that one,
 * `compareSessions` via `liveSessions`). */
export function liveConsoleSessions(sessions: Session[], consoleId: string): Session[] {
  return sessions
    .filter((s) => s.console_id === consoleId && s.role === "console" && s.status !== "archived")
    .sort((a, b) => b.started_at - a.started_at);
}

/** The sessions bound to the console session `consoleSessionId`, in any status, in the sidebar's order. */
export function boundSessions(sessions: Session[], consoleSessionId: string): Session[] {
  return sessions.filter((s) => s.bound_to === consoleSessionId).sort(compareSessions);
}

/** A console session's own archived bound sessions — the project sessions that are bound to it and
 * have since been archived. This is a third filter over the same session records the archive view
 * already serves for a project (`project_id`) and for a console's console sessions (`role`).
 * `ArchiveScope`'s `consoleSession` selects it, but nothing in the sidebar opens that scope yet.
 *
 * TODO(docs/plans/20261008-console-sessions-and-agent-accounts/13-focus-modes.md): a console
 * session's focus mode is what reaches this scope. */
export function boundArchivedSessions(sessions: Session[], consoleSessionId: string): Session[] {
  return archivedSessions(sessions.filter((s) => s.bound_to === consoleSessionId));
}

/** What a console in the switcher or a project row shows: the most pressing activity among its sessions. */
export type Activity = "waiting" | "working" | "running" | undefined;

export function consoleActivity(sessions: Session[]): Activity {
  if (sessions.some((s) => s.status === "waiting_user")) return "waiting";
  if (sessions.some((s) => s.status === "working")) return "working";
  if (sessions.some((s) => s.status === "idle")) return "running";
  return undefined;
}
