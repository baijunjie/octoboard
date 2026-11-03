import { createBrowseLane } from "./lane";

/** The daemon's bound of outstanding browse requests per connection: one more is refused with `limit_exceeded`
 * (apps/daemon/PROTOCOL.md, "Browse budgets"). A window talks to the daemon over one connection, so every browse
 * request it sends counts against it. */
export const BROWSE_BOUND = 16;

/** A share of the bound: how many requests it has out at once, and the units reserved beside them for a request
 * replacing another in its slot. The daemon counts the newer request before it gives the older one up, and the older
 * one keeps its unit until it stops, so for that moment both are outstanding. */
export interface BrowseShare {
  places: number;
  replacementUnits: number;
}

/**
 * The window's shares of `BROWSE_BOUND`, whose places and replacement units add up to no more than it
 * (`browse/budget.test.ts`). That holds on the assumption that a read superseded or cancelled in the daemon returns
 * promptly: one still held by the operating system (a volume that stopped answering) keeps its daemon unit until it
 * returns, beyond what a share reserves. Every browse request the window sends goes through one of them: a new kind
 * of request takes its share from these.
 *
 * - `listings`: directory listings, every project browser mounted counted, those a browser left out included (on a
 *   volume slow to answer, say). A listing replaced in its slot waits for a place like any other, so it needs no
 *   unit of its own.
 * - `lists`: recursive file searches, Git lists and project sources. A request in a slot that already has one out
 *   goes past the places, so each place has a unit for it.
 * - `documentImages`: the images a document names, one at a time in one slot, whose next read cancels the one let go
 *   of.
 * - `viewer`: the viewer's read, file or change, in one slot for the whole window.
 * - `viewerBodies`: the read of a change's whole bodies, in a slot apart from the viewer's so a click on a separator
 *   never cancels the read of a patch still on its way.
 */
export const BROWSE_SHARES = {
  listings: { places: 6, replacementUnits: 0 },
  lists: { places: 2, replacementUnits: 2 },
  documentImages: { places: 1, replacementUnits: 1 },
  viewer: { places: 1, replacementUnits: 1 },
  viewerBodies: { places: 1, replacementUnits: 1 },
} as const satisfies Record<string, BrowseShare>;

/** The slots shared across the window. A window shows one subject at a time, whichever project and mode it is from,
 * so a read left behind by a viewer of another project is cancelled too. */
export const BROWSE_SLOTS = {
  viewer: "viewer",
  viewerBodies: "viewer-bodies",
  documentImage: "document-image",
} as const;

/** The lane of directory listings. */
export const listingLane = createBrowseLane(BROWSE_SHARES.listings.places);
/** The lane of file searches, Git lists and project sources, each in a slot of its own per project. */
export const listLane = createBrowseLane(BROWSE_SHARES.lists.places, { replacing: true });
/** The lane of a document's images, all in `BROWSE_SLOTS.documentImage`: they wait for each other rather than replace
 * each other. */
export const documentImageLane = createBrowseLane(BROWSE_SHARES.documentImages.places, { slot: BROWSE_SLOTS.documentImage });
