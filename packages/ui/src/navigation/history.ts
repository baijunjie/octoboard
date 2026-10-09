import type { Console, Project, Session } from "../protocol";
import { belongsToFocus, resolveFocus } from "../sidebar/focus";
import type { ArchiveScope } from "../sidebar/types";

/** What the content area shows, in the terms the UI already keeps it in: the current console, the
 * selected session, the focus mode as `resolveFocus`'s key (`project:<id>` or
 * `consoleSession:<id>`), and the archive view's scope while it is open. */
export interface Location {
  console?: string;
  session?: string;
  focus?: string;
  archive?: ArchiveScope;
}

/** The locations visited, oldest first, and which one is on screen. */
export interface NavigationHistory {
  entries: readonly Location[];
  index: number;
}

/** How many locations are kept; the oldest go first. */
export const HISTORY_LIMIT = 100;

export const EMPTY_HISTORY: NavigationHistory = { entries: [], index: -1 };

export function locationKey(location: Location): string {
  const { archive } = location;
  return JSON.stringify([
    location.console,
    location.session,
    location.focus,
    archive && [archive.console, archive.project, archive.consoleSession],
  ]);
}

/** Records a location the user navigated to. The same location as the current one is not recorded
 * again, and anything that was ahead of the current one (what Forward would have gone to) is
 * dropped. */
export function push(history: NavigationHistory, location: Location): NavigationHistory {
  const current = history.entries[history.index];
  if (current && locationKey(current) === locationKey(location)) return history;
  const entries = [...history.entries.slice(0, history.index + 1), location].slice(-HISTORY_LIMIT);
  return { entries, index: entries.length - 1 };
}

/** Replaces the current entry: for a location the UI ended up at that differs from the one a move
 * was aiming at, and for one it was taken to without the user's doing (what was shown is gone). */
export function replaceCurrent(history: NavigationHistory, location: Location): NavigationHistory {
  if (history.index < 0) return history;
  const entries = history.entries.slice();
  entries[history.index] = location;
  return { entries, index: history.index };
}

/** The index of the nearest entry in `direction` (-1 back, 1 forward) that is live and shows
 * something other than what is on screen, if any. Entries that name something deleted since are
 * passed over, and so are those that are the location already shown, so a move never visibly does
 * nothing. */
function nearestLive(history: NavigationHistory, direction: -1 | 1, isLive: (location: Location) => boolean): number | undefined {
  const current = history.entries[history.index];
  const currentKey = current && locationKey(current);
  for (let i = history.index + direction; i >= 0 && i < history.entries.length; i += direction) {
    const entry = history.entries[i];
    if (isLive(entry) && locationKey(entry) !== currentKey) return i;
  }
  return undefined;
}

export function canGo(history: NavigationHistory, direction: -1 | 1, isLive: (location: Location) => boolean): boolean {
  return nearestLive(history, direction, isLive) !== undefined;
}

/** Moves to the nearest live entry in `direction` without recording anything; `undefined` when
 * there is none. */
export function go(
  history: NavigationHistory,
  direction: -1 | 1,
  isLive: (location: Location) => boolean,
): { history: NavigationHistory; location: Location } | undefined {
  const index = nearestLive(history, direction, isLive);
  if (index === undefined) return undefined;
  return { history: { entries: history.entries, index }, location: history.entries[index] };
}

/** Whether everything `location` names still exists, and can be shown together: a focus mode
 * counts by what `resolveFocus` finds, and so is gone with an archived console session, and one
 * that no longer lists the selected session cannot be shown with it (selecting a session leaves
 * such a focus mode). */
export function isLive(
  location: Location,
  data: { consoles: Map<string, Console>; projects: Map<string, Project>; sessions: Map<string, Session> },
): boolean {
  const { consoles, projects, sessions } = data;
  const { archive } = location;
  if (location.console !== undefined && !consoles.has(location.console)) return false;
  if (location.session !== undefined && !sessions.has(location.session)) return false;
  if (location.focus !== undefined) {
    const focus = resolveFocus(location.focus, location.console, projects, sessions);
    if (!focus) return false;
    const session = location.session !== undefined ? sessions.get(location.session) : undefined;
    if (session && !belongsToFocus(focus, session, sessions)) return false;
  }
  if (archive) {
    if (!consoles.has(archive.console)) return false;
    if (archive.project !== undefined && !projects.has(archive.project)) return false;
    if (archive.consoleSession !== undefined && !sessions.has(archive.consoleSession)) return false;
  }
  return true;
}
