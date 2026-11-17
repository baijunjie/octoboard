import type { Daemon } from "../store";
import type { FormValues } from "./page/formModel";

/** What the user has entered into a session's pages, by page id. */
export type SessionDrafts = Readonly<Record<string, FormValues>>;

// Held per daemon store, so that another daemon (or a test's own) starts with none, and outside any component, so that
// a half-filled form outlives its report panel being closed, or the session being switched away from, and is there
// when the panel is opened again. Only what this window's user typed: no other window sees it, and a reload loses it.
const held = new WeakMap<Daemon["store"], Map<string, SessionDrafts>>();

/** The drafts held for `session`'s pages, none if nothing has been entered. */
export function heldDrafts(store: Daemon["store"], session: string): SessionDrafts {
  return held.get(store)?.get(session) ?? {};
}

/** Holds `drafts` as `session`'s, and lets go of the drafts of sessions the store no longer has, whose panel cannot be
 * opened again. */
export function holdDrafts(store: Daemon["store"], session: string, drafts: SessionDrafts): void {
  let sessions = held.get(store);
  if (sessions === undefined) {
    sessions = new Map();
    held.set(store, sessions);
  }
  if (Object.keys(drafts).length === 0) sessions.delete(session);
  else sessions.set(session, drafts);
  const known = store.getState().sessions;
  for (const id of sessions.keys()) {
    if (!known.has(id)) sessions.delete(id);
  }
}
