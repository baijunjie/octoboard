import type { Session } from "../protocol";

/** The sessions of a store's `sessions` map grouped the ways the lists read them, so a component
 * takes its own group instead of filtering every session on every daemon event. */
export interface SessionIndex {
  /** By `project_id`; a session of no project is in none. */
  byProject: ReadonlyMap<string, Session[]>;
  /** By `console_id`: every session of the console, console sessions included. */
  byConsole: ReadonlyMap<string, Session[]>;
  /** The ids of the sessions some session is bound to (`bound_to`), in any status. */
  boundOwnerIds: ReadonlySet<string>;
}

/** Shared and never changed: the arrays an index holds are read, not edited. */
const EMPTY_SESSIONS: Session[] = [];

/** A group of an index that is not there: one array for all of them, so it is never a new value. */
export function sessionsOf(group: ReadonlyMap<string, Session[]>, id: string): Session[] {
  return group.get(id) ?? EMPTY_SESSIONS;
}

const indexes = new WeakMap<ReadonlyMap<string, Session>, SessionIndex>();
let latest: SessionIndex | undefined;

function group(sessions: Iterable<Session>, keyOf: (session: Session) => string | null | undefined): Map<string, Session[]> {
  const groups = new Map<string, Session[]>();
  for (const session of sessions) {
    const key = keyOf(session);
    if (!key) continue;
    const members = groups.get(key);
    if (members) members.push(session);
    else groups.set(key, [session]);
  }
  return groups;
}

const sameMembers = (a: Session[], b: Session[]) => a.length === b.length && a.every((s, i) => s === b[i]);

/** `next`, with every group that holds the same sessions in the same order replaced by the array
 * `previous` had for it, so a group no event touched keeps its identity. */
function reuseGroups(next: Map<string, Session[]>, previous: ReadonlyMap<string, Session[]> | undefined): Map<string, Session[]> {
  if (!previous) return next;
  for (const [key, members] of next) {
    const before = previous.get(key);
    if (before && sameMembers(before, members)) next.set(key, before);
  }
  return next;
}

/** The index of `sessions`, kept for as long as that map is the store's current one. Building it
 * is one pass over the sessions, once per map: every daemon event that touches a session replaces
 * the map, and the groups come out as the same arrays as before wherever nothing in them changed,
 * so only what depends on a changed session sees a new value. */
export function indexSessions(sessions: ReadonlyMap<string, Session>): SessionIndex {
  const cached = indexes.get(sessions);
  if (cached) return cached;
  const values = Array.from(sessions.values());
  const owners = new Set<string>();
  for (const session of values) if (session.bound_to) owners.add(session.bound_to);
  const previous = latest;
  const index: SessionIndex = {
    byProject: reuseGroups(group(values, (s) => s.project_id), previous?.byProject),
    byConsole: reuseGroups(group(values, (s) => s.console_id), previous?.byConsole),
    boundOwnerIds:
      previous && previous.boundOwnerIds.size === owners.size && [...owners].every((id) => previous.boundOwnerIds.has(id))
        ? previous.boundOwnerIds
        : owners,
  };
  indexes.set(sessions, index);
  latest = index;
  return index;
}
