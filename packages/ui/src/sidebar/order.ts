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

/** A session as a list nests it. The lists are flat and in display order — an owner immediately
 * followed by the sessions bound to it — so a renderer indents the ones that carry `under` and a
 * list shown a page at a time still slices rows without separating a team from its owner. */
export interface NestedSession {
  session: Session;
  /** The project session this one is bound to, when that owner is listed right above it. */
  under?: Session;
}

/** `ordered` nested, keeping the order it already has: every session that is not bound to another
 * of them stays where it is, with the sessions bound to it after it. A session whose owner is not
 * in the list (a lead session's console session, or an owner the list filtered out) is one of the
 * list's own, not nested. Orchestration is two levels deep, so nothing is nested under a nested
 * session; one that somehow is — a third level, or a binding leading back on itself — ends the
 * list on its own rather than being dropped from it, since a list that quietly loses a session
 * would hide a running agent from the sidebar, the waiting list and the menu bar alike. */
function nest(ordered: Session[]): NestedSession[] {
  const listed = new Map(ordered.map((session) => [session.id, session]));
  const ownerOf = (session: Session) => (session.bound_to ? listed.get(session.bound_to) : undefined);
  const rows = ordered
    .filter((session) => ownerOf(session) === undefined)
    .flatMap((session) => [
      { session },
      ...ordered.filter((member) => ownerOf(member)?.id === session.id).map((member) => ({ session: member, under: session })),
    ]);
  const placed = new Set(rows.map((row) => row.session.id));
  return [...rows, ...ordered.filter((session) => !placed.has(session.id)).map((session) => ({ session }))];
}

/** The sessions that are not archived, in the sidebar's order (`liveSessions`), with the sessions
 * bound to a project session nested under it. */
export function liveSessionRows(sessions: Session[]): NestedSession[] {
  return nest(liveSessions(sessions));
}

/** The archived sessions, most recently ended first (`archivedSessions`), with the ones bound to an
 * archived project session nested under it. One whose owner is not archived is listed on its own:
 * its owner has no row here to go under. */
export function archivedSessionRows(sessions: Session[]): NestedSession[] {
  return nest(archivedSessions(sessions));
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

/** The console session above `session`, which it reports to directly or through the lead session
 * that owns it. `lookup` resolves an owner's id as far as the caller knows the console's sessions;
 * `undefined` from it, as from this function, means none was found — an unbound session, one under
 * an unbound project session, or one whose owner the caller does not hold. The walk up carries a
 * seen set rather than trusting that orchestration is two levels deep, so a binding that somehow
 * leads back on itself ends the walk instead of hanging the render. */
export function consoleSessionAbove(session: Session, lookup: (id: string) => Session | undefined): Session | undefined {
  const seen = new Set<string>();
  let at: Session | undefined = session;
  while (at?.bound_to && !seen.has(at.id)) {
    seen.add(at.id);
    const owner = lookup(at.bound_to);
    if (owner?.role === "console") return owner;
    at = owner;
  }
  return undefined;
}

/** The sessions of a project that no console session is above: unbound ones, and the sessions bound
 * to one of those, which are listed under their owner rather than hidden behind a console session's
 * focus mode. A lead session (bound to a console session of `owners`) and the sessions bound to it
 * are left out together: a team goes where its owner goes, which is its console session's focus
 * mode. A project's focus mode lists only these among its live sessions; its archive is not
 * filtered this way. */
export function notUnderConsoleSession(sessions: Session[], owners: Map<string, Session>): Session[] {
  const byId = new Map(sessions.map((s) => [s.id, s]));
  return sessions.filter((s) => consoleSessionAbove(s, (id) => byId.get(id) ?? owners.get(id)) === undefined);
}

/** The sessions a project's focus mode leaves out of its list: those of `sessions` (the project's)
 * that are not archived and have a console session above them — bound to it, or bound to a lead
 * session that is, since a team is listed with its owner and so is counted with it — grouped by
 * that console session. `owners` holds the console's console sessions by id; each owner comes out
 * once, with its count, in the sidebar's order, and a session whose console session is not in it,
 * or is archived, cannot be named and is not counted. An archived owner with a live bound session
 * should not exist (archiving a console session archives its dormant bound sessions, and reopening
 * a bound one reopens its owner first), so this only keeps a link that could not be followed from
 * being offered. `undefined` when there are none, which is when the focus view shows nothing for
 * them. */
export function boundElsewhere(
  sessions: Session[],
  owners: Map<string, Session>,
): { count: number; owners: { owner: Session; count: number }[] } | undefined {
  const byId = new Map(sessions.map((s) => [s.id, s]));
  const counts = new Map<string, { owner: Session; count: number }>();
  let count = 0;
  for (const session of sessions) {
    if (session.status === "archived") continue;
    const owner = consoleSessionAbove(session, (id) => byId.get(id) ?? owners.get(id));
    if (!owner || owner.status === "archived") continue;
    count++;
    const entry = counts.get(owner.id);
    if (entry) entry.count++;
    else counts.set(owner.id, { owner, count: 1 });
  }
  if (count === 0) return undefined;
  return { count, owners: [...counts.values()].sort((a, b) => compareSessions(a.owner, b.owner)) };
}

/** What a console session's focus mode lists: the console's projects that have a live session bound
 * to `consoleSessionId`, in the order of `sortProjects`, each with only those sessions — a lead
 * session among them carrying the sessions bound to it, nested under it, since the console session
 * sees the team but deals with the lead session alone. A team is in the lead session's own project,
 * so it falls under the same project heading. */
export function focusGroups(
  projects: Project[],
  sessions: Session[],
  consoleSessionId: string,
): { project: Project; sessions: NestedSession[] }[] {
  const bound = new Set(boundSessions(sessions, consoleSessionId).map((s) => s.id));
  const listed = sessions.filter((s) => bound.has(s.id) || (!!s.bound_to && bound.has(s.bound_to)));
  const sessionsOf = (project: Project) => listed.filter((s) => s.project_id === project.id && s.status !== "archived");
  return sortProjects(
    projects.filter((p) => sessionsOf(p).length > 0),
    sessionsOf,
  ).map((project) => ({ project, sessions: liveSessionRows(sessionsOf(project)) }));
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
      ).flatMap((project) =>
        // Nested as the project's list nests them, so a session bound to a project session follows
        // its owner rather than being ranked against it.
        nest(mine.filter((s) => s.role !== "console" && s.project_id === project.id).sort(compareSessions)).map((row) => row.session),
      ),
    ];
  });
}
