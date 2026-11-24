import { expect, it } from "vitest";

import { fadeMask } from "./fadeMask";

it("creates a horizontal alpha ramp only at the clipped edge", () => {
  expect(fadeMask("left", 0, 8)).toBe(
    "linear-gradient(to left, #000 0, #000 calc(100% - 8px), transparent 100%)",
  );
});

it("can instead paint only the edges of a vertical viewport", () => {
  expect(fadeMask("bottom", 4, 8, "edge")).toBe(
    "linear-gradient(to bottom, #000 0, transparent 4px, transparent calc(100% - 8px), #000 100%)",
  );
});
