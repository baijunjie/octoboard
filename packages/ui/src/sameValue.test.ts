import { expect, it } from "vitest";

import { sameValue } from "./sameValue";

it("tells copies of the same data from changed data", () => {
  expect(sameValue({ a: 1, b: [1, { c: null }] }, { b: [1, { c: null }], a: 1 })).toBe(true);
  expect(sameValue({ a: 1, b: [1, 2] }, { a: 1, b: [1, 3] })).toBe(false);
  expect(sameValue([1, 2], [1, 2, 3])).toBe(false);
  expect(sameValue({ a: undefined }, {})).toBe(true);
  expect(sameValue(undefined, { a: 1 })).toBe(false);
  expect(sameValue(null, {})).toBe(false);
});
