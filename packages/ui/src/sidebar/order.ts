import { isLive, type Console, type Project, type Session, type SessionStatus } from "../protocol";

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

/** The sessions bound to the session `ownerId` (a console session, or a project session that started
 * some), in any status, in the sidebar's order. */
export function boundSessions(sessions: Session[], ownerId: string): Session[] {
  return sessions.filter((s) => s.bound_to === ownerId).sort(compareSessions);
}

/** The archived sessions bound to `ownerId`, and those bound to an archived one of them (a lead
 * session's own sessions, two levels under a console session) — exactly what deleting an archived
 * owner takes with it. These are a third filter over the same session records the archive view
 * already serves for a project (`project_id`) and for a console's console sessions (`role`);
 * `ArchiveScope`'s `consoleSession` selects it. */
export function boundArchivedSessions(sessions: Session[], ownerId: string): Session[] {
  const direct = archivedSessions(sessions.filter((s) => s.bound_to === ownerId));
  const ids = new Set(direct.map((s) => s.id));
  return archivedSessions(sessions.filter((s) => s.bound_to === ownerId || (s.bound_to && ids.has(s.bound_to))));
}

/** The sessions that go into the archive with `ownerId`: the interrupted ones bound to it and
 * those bound to each of them (a lead session's own sessions, two levels under a console
 * session), which is how far archiving reaches. */
export function boundSessionsArchivedWith(sessions: Session[], ownerId: string): Session[] {
  const direct = boundSessions(sessions, ownerId).filter((s) => s.status === "interrupted");
  const ids = new Set(direct.map((s) => s.id));
  const nested = sessions.filter((s) => s.bound_to && ids.has(s.bound_to) && s.status === "interrupted");
  return [...direct, ...nested.sort(compareSessions)];
}

/** The sessions not bound to a console session of `owners`: unbound ones, and ones bound to a
 * project session, which is listed beside them rather than hidden behind a console session's
 * focus mode. A project's focus mode lists only these among its live sessions; its archive is not
 * filtered this way. */
export function notBoundToConsoleSession(sessions: Session[], owners: Map<string, Session>): Session[] {
  return sessions.filter((s) => !s.bound_to || !owners.has(s.bound_to));
}

/** The sessions a project's focus mode leaves out of its list: those of `sessions` (the project's)
 * that are bound to a console session and not archived, counted and grouped by the console session
 * they report to. `owners` holds the console's console sessions by id; each owner comes out once,
 * with its count, in the sidebar's order, and a session whose owner is not in it, or is archived,
 * cannot be named and is not counted. An archived owner with a live bound session should not exist
 * (archiving a console session archives its dormant bound sessions, and reopening a bound one
 * reopens its owner first), so this only keeps a link that could not be followed from being
 * offered. `undefined` when there are none, which is when the focus view shows nothing for them. */
export function boundElsewhere(
  sessions: Session[],
  owners: Map<string, Session>,
): { count: number; owners: { owner: Session; count: number }[] } | undefined {
  const named = (id: string | null | undefined) => {
    const owner = id ? owners.get(id) : undefined;
    return owner && owner.status !== "archived" ? owner : undefined;
  };
  const bound = sessions.filter((s) => s.status !== "archived" && named(s.bound_to));
  if (bound.length === 0) return undefined;
  const counts = new Map<string, number>();
  for (const s of bound) counts.set(s.bound_to!, (counts.get(s.bound_to!) ?? 0) + 1);
  const perOwner = [...counts].map(([id, count]) => ({ owner: named(id)!, count }));
  return { count: bound.length, owners: perOwner.sort((a, b) => compareSessions(a.owner, b.owner)) };
}

/** What a console session's focus mode lists: the console's projects that have a live session bound
 * to `consoleSessionId`, in the order of `sortProjects`, each with only those sessions. */
export function focusGroups(
  projects: Project[],
  sessions: Session[],
  consoleSessionId: string,
): { project: Project; sessions: Session[] }[] {
  const bound = liveSessions(boundSessions(sessions, consoleSessionId));
  const sessionsOf = (project: Project) => bound.filter((s) => s.project_id === project.id);
  return sortProjects(
    projects.filter((p) => sessionsOf(p).length > 0),
    sessionsOf,
  ).map((project) => ({ project, sessions: sessionsOf(project) }));
}

/** What a console on the rail or a project row shows: the most pressing activity among its sessions. */
export type Activity = "waiting" | "working" | "running" | undefined;

export function consoleActivity(sessions: Session[]): Activity {
  if (sessions.some((s) => s.status === "waiting_user")) return "waiting";
  if (sessions.some((s) => s.status === "working")) return "working";
  if (sessions.some((s) => s.status === "idle")) return "running";
  return undefined;
}

/** One chip of a console session's focus mode's switch strip: a console session and what is going on
 * in it and in the sessions bound to it, as far as the strip shows it. */
export interface SwitchStripEntry {
  consoleSession: Session;
  /** A raised hand beats a session at work, which beats nothing: a session merely running is not
   * worth a chip's room. */
  activity: "waiting" | "working" | undefined;
}

/** The chips of the switch strip: the console's console sessions that are not archived, each with the
 * most pressing activity among itself and its bound sessions. The order is fixed, pinned first and
 * then by when they started (ties by id), not ranked by status like the sidebar's list: a chip stays
 * where it is while statuses change, so cycling through them never skips or repeats one. */
export function switchStrip(sessions: Session[], consoleId: string): SwitchStripEntry[] {
  const consoleSessions = sessions
    .filter((s) => s.console_id === consoleId && s.role === "console" && s.status !== "archived")
    .sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.started_at - b.started_at || (a.id < b.id ? -1 : 1));
  return consoleSessions.map((consoleSession) => {
    const activity = consoleActivity([consoleSession, ...boundSessions(sessions, consoleSession.id)]);
    return { consoleSession, activity: activity === "waiting" || activity === "working" ? activity : undefined };
  });
}

/** The sessions `include` picks, in the order the sidebar tree lists them: console by console, its
 * console sessions first (in the console sessions section's own order), then its projects'
 * sessions in the sidebar's own order. */
export function sessionsInTreeOrder(
  consoles: Console[],
  projects: Project[],
  sessions: Session[],
  include: (session: Session) => boolean,
): Session[] {
  const picked = sessions.filter(include);
  return consoles.flatMap((console) => {
    const mine = picked.filter((s) => s.console_id === console.id);
    return [
      ...mine.filter((s) => s.role === "console").sort(compareSessions),
      ...sortProjects(
        projects.filter((p) => p.console_id === console.id),
        (project) => sessions.filter((s) => s.project_id === project.id),
      ).flatMap((project) => mine.filter((s) => s.role !== "console" && s.project_id === project.id).sort(compareSessions)),
    ];
  });
}
