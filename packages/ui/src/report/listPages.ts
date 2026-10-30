import type { Daemon } from "../store";

/** The listings asked for, per daemon and then per `snapshotEpoch` and session: the answer is one
 * full `page_list` reply with every page's HTML, so it is asked for once and shared. */
const listings = new WeakMap<Daemon["request"], Map<string, Promise<void>>>();

/**
 * Asks the daemon to list `session`'s pages, which arrive through the store (`page_list` in
 * `store.ts`), unless a listing for this `epoch` is already out or has answered: the window's
 * check for a project session's Report panel (`useProjectReportSession.ts`) and the panel's own
 * (`ReportPanel.tsx`) then cost one reply between them. A page pushed or deleted since reaches the
 * store as an event, so a held answer is current until the next `snapshot`, which bumps `epoch`.
 * A failed listing is not held, so Try again lists afresh.
 */
export function listPages(request: Daemon["request"], session: string, epoch: number): Promise<void> {
  let held = listings.get(request);
  if (!held) listings.set(request, (held = new Map()));
  const key = `${epoch}\0${session}`;
  const known = held.get(key);
  if (known) return known;
  for (const other of held.keys()) if (!other.startsWith(`${epoch}\0`)) held.delete(other);
  const listing = request({ type: "list_pages", session }).then(() => undefined);
  held.set(key, listing);
  listing.catch(() => {
    if (held.get(key) === listing) held.delete(key);
  });
  return listing;
}
