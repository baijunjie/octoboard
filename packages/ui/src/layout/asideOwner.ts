import type { PlainMessageKey } from "../i18n/catalog";
import type { Project, Session } from "../protocol";

/**
 * Who the aside belongs to: a console session, whose report panel it shows, or a project, whose
 * browser it shows. It is set by what the user does — selecting a session, opening a project's
 * files — and never derived from whatever else is on screen, so a project's browser can be open
 * with no session at all and a terminal of another project beside it. No owner, no aside.
 */
export type AsideOwner = { kind: "report"; consoleSession: string } | { kind: "project"; project: string };

/** The owner selecting `session` gives the aside: a console session's report, a project
 * session's project. */
export function ownerForSession(session: Session | undefined): AsideOwner | undefined {
  if (!session) return undefined;
  if (session.role === "console") return { kind: "report", consoleSession: session.id };
  return session.project_id ? { kind: "project", project: session.project_id } : undefined;
}

/** `owner` while what it names still exists, otherwise nothing: an owner is never replaced by
 * another project or session on its own. */
export function liveOwner(
  owner: AsideOwner | undefined,
  projects: ReadonlyMap<string, Project>,
  sessions: ReadonlyMap<string, Session>,
): AsideOwner | undefined {
  if (!owner) return undefined;
  if (owner.kind === "project") return projects.has(owner.project) ? owner : undefined;
  return sessions.get(owner.consoleSession)?.role === "console" ? owner : undefined;
}

/** What is left of `owner` once `consoleId` is the current console: a browser of a project of
 * another console is closed. A report stays with the console session that is selected, as before. */
export function ownerAfterConsoleSwitch(
  owner: AsideOwner | undefined,
  consoleId: string | undefined,
  projects: ReadonlyMap<string, Project>,
): AsideOwner | undefined {
  if (owner?.kind !== "project") return owner;
  return projects.get(owner.project)?.console_id === consoleId ? owner : undefined;
}

/**
 * What owns the aside once `consoleId` is the current console with `selected` selected, after
 * anything that may have moved either (another console made current on the rail, Back and Forward):
 * `owner` with a browser of another console's project closed, and when that leaves nothing, the
 * selected session's own owner: a console session's report whichever console is current, as making
 * another console current never closes one, and a project session's browser while its console is
 * current. So a browser closed by making another console current comes back with the console,
 * whichever way the window got back there.
 */
export function ownerAfterMove(
  owner: AsideOwner | undefined,
  selected: Session | undefined,
  consoleId: string | undefined,
  projects: ReadonlyMap<string, Project>,
): AsideOwner | undefined {
  const kept = ownerAfterConsoleSwitch(owner, consoleId, projects);
  if (kept) return kept;
  const own = ownerForSession(selected);
  // A report is never closed by making another console current, so it comes back from anywhere.
  return own?.kind === "report" || selected?.console_id === consoleId ? own : undefined;
}

/** An owner's identity, as text: two owners are the same owner when their keys are equal. */
export function ownerKey(owner: AsideOwner | undefined): string | undefined {
  if (!owner) return undefined;
  return owner.kind === "report" ? `report:${owner.consoleSession}` : `project:${owner.project}`;
}

export function sameOwner(a: AsideOwner | undefined, b: AsideOwner | undefined): boolean {
  return ownerKey(a) === ownerKey(b);
}

/** How the aside's controls are named for each kind of owner: the rail's toggle, the scrim that
 * closes its drawer and its resize handle. A project's are named for the project pane, not its
 * files, as the same pane also shows the project's Git review. */
export const ASIDE_LABELS: Record<
  AsideOwner["kind"],
  { show: PlainMessageKey; hide: PlainMessageKey; close: PlainMessageKey; resize: PlainMessageKey }
> = {
  report: { show: "rail.report.show", hide: "rail.report.hide", close: "app.closeReport", resize: "pane.resizeReport" },
  project: {
    show: "rail.projectPane.show",
    hide: "rail.projectPane.hide",
    close: "app.closeProjectPane",
    resize: "pane.resizeProjectPane",
  },
};
