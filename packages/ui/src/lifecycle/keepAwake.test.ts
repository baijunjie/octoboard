import { expect, it } from "vitest";

import { shouldKeepAwake } from "./keepAwake";

it("holds only with Prevent sleep on, and then always or while a session is working", () => {
  expect(shouldKeepAwake(false, "always", true)).toBe(false);
  expect(shouldKeepAwake(true, "always", false)).toBe(true);
  expect(shouldKeepAwake(true, "working", false)).toBe(false);
  expect(shouldKeepAwake(true, "working", true)).toBe(true);
});
