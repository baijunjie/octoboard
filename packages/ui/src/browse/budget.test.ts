import { expect, it } from "vitest";

import { BROWSE_BOUND, BROWSE_SHARES, documentImageLane, listingLane, listLane } from "./budget";

it("keeps every share and its replacement units within the daemon's bound", () => {
  const units = Object.values(BROWSE_SHARES).reduce((sum, share) => sum + share.places + share.replacementUnits, 0);
  expect(units).toBeLessThanOrEqual(BROWSE_BOUND);
});

it("sizes each lane by its share", () => {
  expect(listingLane.places).toBe(BROWSE_SHARES.listings.places);
  expect(listLane.places).toBe(BROWSE_SHARES.lists.places);
  expect(documentImageLane.places).toBe(BROWSE_SHARES.documentImages.places);
});
