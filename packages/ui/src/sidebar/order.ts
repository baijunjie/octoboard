import { type Console, type Project, type Session, type Settings } from "../protocol";
import { sessionStatusKind, type StatusKind } from "../sessionLabel";

/** How urgently a session asks for attention: a raised hand, a prompt, background work,
 * foreground work, then no process. Archived sessions are listed separately. */
const STATUS_RANK: Record<StatusKind, number> = {
  waiting_user: 0,
  starting: 1,
  idle: 1,
  background: 2,
  working: 3,
  interrupted: 4,
  archived: 5,
};

/** Whether the sidebar ranks sessions and projects by status (`Settings.sidebar_order_by_status`);
 * a daemon that does not send the setting leaves it on. */
export function ordersByStatus(settings: Pick<Settings, "sidebar_order_by_status">): boolean {
  return settings.sidebar_order_by_status !== false;
}

/** Whether a session has work under way, including work left running between turns. */
export function countsAsWorking(session: Pick<Session, "status" | "background_tasks">): boolean {
  const kind = sessionStatusKind(session);
  return kind === "working" || kind === "background";
}

function rankOf(session: Session): number {
  return STATUS_RANK[sessionStatusKind(session)];
}

/** Pinned first; then by status (`STATUS_RANK`) unless `byStatus` is off (the user's
 * `Settings.sidebar_order_by_status`); then newest first. */
export function compareSessions(a: Session, b: Session, byStatus = true): number {
  return (
    Number(b.pinned) - Number(a.pinned) ||
    (byStatus ? rankOf(a) - rankOf(b) : 0) ||
    b.started_at - a.started_at
  );
}

/** `compareSessions` with `byStatus` bound, for `Array.prototype.sort`. */
function sessionOrder(byStatus: boolean): (a: Session, b: Session) => number {
  return (a, b) => compareSessions(a, b, byStatus);
}

/** The rank of a project with no unarchived session, which ends the list. */
export const INACTIVE_PROJECT_RANK = 4;

/** A project's rank by its most urgent unarchived session: a raised hand, work, a running
 * session, then an interrupted session. Projects with no unarchived session end the list. */
export function projectRank(sessions: Session[]): number {
  if (sessions.some((s) => s.status === "waiting_user")) return 0;
  if (sessions.some(countsAsWorking)) return 1;
  if (sessions.some((s) => s.status === "idle" || s.status === "starting")) return 2;
  if (sessions.some((s) => s.status === "interrupted")) return 3;
  return INACTIVE_PROJECT_RANK;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Projects with unarchived sessions first; within each group, pinned, then rank (unless
 * `byStatus` is off), then name. */
export function sortProjects(projects: Project[], sessionsOf: (project: Project) => Session[], byStatus = true): Project[] {
  return sortProjectsByRank(projects, new Map(projects.map((p) => [p.id, projectRank(sessionsOf(p))])), byStatus);
}

/** `sortProjects` for a caller that already holds each project's `projectRank`, by project id.
 * With `byStatus` off the rank still splits off the inactive projects, which end the list. */
export function sortProjectsByRank(projects: Project[], rank: ReadonlyMap<string, number>, byStatus = true): Project[] {
  return [...projects].sort(
    (a, b) =>
      Number(rank.get(a.id) === INACTIVE_PROJECT_RANK) - Number(rank.get(b.id) === INACTIVE_PROJECT_RANK) ||
      Number(b.pinned) - Number(a.pinned) ||
      (byStatus ? rank.get(a.id)! - rank.get(b.id)! : 0) ||
      collator.compare(a.name, b.name),
  );
}

/** A project's sessions in sidebar order: the ones not archived, ranked by `compareSessions`. */
export function liveSessions(sessions: Session[], byStatus = true): Session[] {
  return sessions.filter((s) => s.status !== "archived").sort(sessionOrder(byStatus));
}

/** Archived sessions, most recently ended first. */
export function archivedSessions(sessions: Session[]): Session[] {
  return sessions
    .filter((s) => s.status === "archived")
    .sort((a, b) => (b.ended_at ?? b.started_at) - (a.ended_at ?? a.started_at));
}

/** The newest `limit` sessions an archive preview lists: those the archive lists as rows of their
 * own (`archivedSessionRows`), so a child shows while its owner is not archived and is left to the
 * full archive once it sits under its archived owner. */
export function archivePreviewSessions(archived: Session[], limit = 10): Session[] {
  const ids = new Set(archived.map((session) => session.id));
  return archivedSessions(archived)
    .filter((session) => !session.bound_to || !ids.has(session.bound_to))
    .slice(0, limit);
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
export function liveSessionRows(sessions: Session[], byStatus = true): NestedSession[] {
  return nest(liveSessions(sessions, byStatus));
}

/** The archived sessions, most recently ended first (`archivedSessions`), with the ones bound to an
 * archived project session nested under it. One whose owner is not archived is listed on its own:
 * its owner has no row here to go under. */
export function archivedSessionRows(sessions: Session[]): NestedSession[] {
  return nest(archivedSessions(sessions));
}

/** A console's live console sessions (not archived, and not one still starting), newest first.
 * Several may exist at once; this is the new-session dialog's list of owners to choose from, not
 * the sidebar's order (see "Order of projects and sessions" in
 * `docs/product/order-of-projects-and-sessions.md` for that one, `compareSessions` via `liveSessions`). */
export function liveConsoleSessions(sessions: Session[], consoleId: string): Session[] {
  return sessions
    .filter((s) => s.console_id === consoleId && s.role === "console" && s.status !== "archived" && s.status !== "starting")
    .sort((a, b) => b.started_at - a.started_at);
}

/** The sessions bound to the session `ownerId` (a console session, or a project session that started
 * some), in any status: pinned first, then most pressing (`compareSessions`, by status). */
export function boundSessions(sessions: Session[], ownerId: string): Session[] {
  return sessions.filter((s) => s.bound_to === ownerId).sort(compareSessions);
}

/** Whether any session is bound to the session `ownerId`, in any status. */
export function hasBoundSessions(sessions: Iterable<Session>, ownerId: string): boolean {
  for (const s of sessions) if (s.bound_to === ownerId) return true;
  return false;
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

/** Whether `session` has never been used, so that it is deleted outright: no conversation on the
 * agent's side (no turn taken, none resumed at open), no session bound to it, and either bound to
 * no owner or opened by the user. One an agent started was dispatched by its owner, which handed it
 * work and expects it to exist, so deleting it would silently cancel that task: it keeps Archive,
 * and so does one still starting, which the daemon refuses to delete. */
export function isUnusedSession(session: Session, hasBoundSessions: boolean): boolean {
  return session.status !== "starting" && !session.has_conversation && !hasBoundSessions
    && (!session.bound_to || session.origin === "user");
}

/** Whether archiving `session` asks the user first, as far as the sessions say. One sitting at its
 * prompt (`idle`) or with no process (`interrupted`) is archived at once, since nothing is cut
 * short; one at work, raising a hand or with work left running in the background asks, and so
 * does one with an unarchived session bound to it or to one bound to it, since the archive
 * reaches both levels and ends their processes too. A bound branch or worktree git refuses to
 * remove makes it ask as well, which only the daemon can say. */
export function archiveNeedsConfirmation(sessions: Iterable<Session>, session: Session): boolean {
  if (session.status !== "idle" && session.status !== "interrupted") return true;
  // Its background work ends with it.
  if (sessionStatusKind(session) === "background") return true;
  // One pass, so that a one-shot iterator such as a Map's `values()` serves too.
  const archivedChildren = new Set<string>();
  const unarchivedElsewhere: string[] = [];
  for (const s of sessions) {
    if (!s.bound_to || s.status === "archived") {
      if (s.bound_to === session.id) archivedChildren.add(s.id);
      continue;
    }
    if (s.bound_to === session.id) return true;
    unarchivedElsewhere.push(s.bound_to);
  }
  return unarchivedElsewhere.some((owner) => archivedChildren.has(owner));
}

/** The unarchived sessions bound to `ownerId` or to one bound to it. Every process in
 * this group ends on confirmation, regardless of how the session was opened or bound. An
 * archived lead still links its descendants to the owner. */
export function boundSessionsArchivedWith(sessions: Session[], ownerId: string): Session[] {
  const direct = boundSessions(sessions, ownerId);
  const ids = new Set(direct.map((s) => s.id));
  const nested = sessions.filter((s) => s.bound_to && ids.has(s.bound_to));
  return [...direct, ...nested.sort(compareSessions)].filter((s) => s.status !== "archived");
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
  byStatus = true,
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
  return { count, owners: [...counts.values()].sort((a, b) => compareSessions(a.owner, b.owner, byStatus)) };
}

/** The sessions a console session's focus mode lists, in whatever project and status: those bound
 * to `consoleSessionId`, and those bound to them. */
function focusListed(sessions: Session[], consoleSessionId: string): Session[] {
  const bound = new Set(boundSessions(sessions, consoleSessionId).map((s) => s.id));
  return sessions.filter((s) => bound.has(s.id) || (!!s.bound_to && bound.has(s.bound_to)));
}

const liveInProject = (listed: Session[], project: Project) =>
  listed.filter((s) => s.project_id === project.id && s.status !== "archived");

/** What a console session's focus mode lists: the console's projects that have a live session bound
 * to `consoleSessionId`, in the order of `sortProjects`, each with only those sessions — a lead
 * session among them carrying the sessions bound to it, nested under it, since the console session
 * sees the team but deals with the lead session alone. A team is in the lead session's own project,
 * so it falls under the same project heading. */
export function focusGroups(
  projects: Project[],
  sessions: Session[],
  consoleSessionId: string,
  byStatus = true,
): { project: Project; sessions: NestedSession[] }[] {
  const listed = focusListed(sessions, consoleSessionId);
  const sessionsOf = (project: Project) => liveInProject(listed, project);
  return sortProjects(
    projects.filter((p) => sessionsOf(p).length > 0),
    sessionsOf,
    byStatus,
  ).map((project) => ({ project, sessions: liveSessionRows(sessionsOf(project), byStatus) }));
}

/** One group of `focusGroups`: the rows of `project` alone, out of every session of the console. */
export function focusGroupRows(sessions: Session[], project: Project, consoleSessionId: string, byStatus = true): NestedSession[] {
  return liveSessionRows(liveInProject(focusListed(sessions, consoleSessionId), project), byStatus);
}

/** Every activity there is, most pressing first. The type is derived from the list rather than
 * written beside it, so whoever covers them all cannot be left behind by a new one. */
export const ACTIVITIES = ["waiting", "working", "running"] as const;

/** What a console on the rail or a project row shows: the most pressing activity among its sessions. */
export type Activity = (typeof ACTIVITIES)[number] | undefined;

export function consoleActivity(sessions: Session[]): Activity {
  if (sessions.some((s) => s.status === "waiting_user")) return "waiting";
  if (sessions.some(countsAsWorking)) return "working";
  if (sessions.some((s) => s.status === "idle" || s.status === "starting")) return "running";
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
 * sessions in the sidebar's own order; `byStatus` is the sidebar's `ordersByStatus`. */
export function sessionsInTreeOrder(
  consoles: Console[],
  projects: Project[],
  sessions: Session[],
  include: (session: Session) => boolean,
  byStatus = true,
): Session[] {
  const order = sessionOrder(byStatus);
  const picked = sessions.filter(include);
  return consoles.flatMap((console) => {
    const mine = picked.filter((s) => s.console_id === console.id);
    return [
      ...mine.filter((s) => s.role === "console").sort(order),
      ...sortProjects(
        projects.filter((p) => p.console_id === console.id),
        (project) => sessions.filter((s) => s.project_id === project.id),
        byStatus,
      ).flatMap((project) =>
        // Nested as the project's list nests them, so a session bound to a project session follows
        // its owner rather than being ranked against it.
        nest(mine.filter((s) => s.role !== "console" && s.project_id === project.id).sort(order)).map((row) => row.session),
      ),
    ];
  });
}
