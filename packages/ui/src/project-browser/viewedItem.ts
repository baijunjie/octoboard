// How the viewer follows the item it shows through a list, for the Files tree and the Git mode's
// change list alike: its neighbours in the list, and whether what the list now says of it calls for
// reading it again. Pure, so the two lists keep one rule.

/** The items before and after `current` among `items` (the list in order). An item the list does not
 * have is placed where it would be (`compare` orders any two items as the list does), so moving on
 * from it still reaches its neighbours; the ends do not wrap. */
export function neighboursIn<T>(
  items: readonly T[],
  current: T,
  same: (a: T, b: T) => boolean,
  compare: (a: T, b: T) => number,
): { previous?: T; next?: T } {
  const index = items.findIndex((item) => same(item, current));
  if (index !== -1) return { previous: items[index - 1], next: items[index + 1] };
  const after = items.findIndex((item) => compare(current, item) < 0);
  const insertAt = after === -1 ? items.length : after;
  return { previous: items[insertAt - 1], next: items[insertAt] };
}

/** What the lists held say about the item the viewer shows: listed, at a version (null when the
 * list does not tell); gone from a complete list; under something that cannot be listed; or nothing
 * yet (a list still loading, refreshing or cut short before the item). */
export type Evidence = { state: "listed"; version: string | null } | { state: "gone" } | { state: "unreadable" } | { state: "unknown" };

/** What the viewer shows of an item, as far as reading it again goes: its body at `version` (when
 * the list's versions are comparable with it), a failure (`changing` when the item kept changing
 * while it was read), or nothing yet. */
export type Shown = { state: "read"; version?: string } | { state: "error"; changing?: boolean } | { state: "loading" };

const evidenceKey = (evidence: Evidence | undefined): string | undefined => (evidence?.state === "listed" ? `listed:${evidence.version}` : evidence?.state);

/**
 * Whether the item the viewer shows has to be read again, given what the lists now say of it
 * (`evidence`) and what they said when this was last asked for the same item (`previous`). Only news
 * counts, so a read that fails again is not repeated until the item changes once more. An item that
 * kept changing as it was read is read again at every fresh listing (`freshListing`) that has it, so
 * it shows once it settles.
 *
 * Otherwise a body is read again when its item is gone, cannot be reached, or is listed at another
 * version than the body's; a failure is read again when the item is listed anew. With `newsOnly`,
 * for a list whose versions say nothing of the body, any news against a `previous` that exists
 * calls for a read.
 */
export function rereadFor(shown: Shown, evidence: Evidence, previous: Evidence | undefined, freshListing: boolean, newsOnly = false): boolean {
  if (shown.state === "loading" || evidence.state === "unknown") return false;
  if (shown.state === "error" && shown.changing) return freshListing && evidence.state === "listed";
  if (evidenceKey(evidence) === evidenceKey(previous)) return false;
  if (newsOnly) return previous !== undefined;
  if (shown.state === "error") return evidence.state === "listed";
  if (evidence.state !== "listed") return true;
  return evidence.version !== null && evidence.version !== shown.version;
}
