import { expect, it } from "vitest";

import { downloadFraction } from "./downloadFraction";

it("has no fraction while there is no progress", () => {
  expect(downloadFraction(undefined)).toBeUndefined();
});

it.each([
  ["no total", { downloaded: 40 }],
  ["an unknown total", { downloaded: 40, total: undefined }],
  ["a total of zero", { downloaded: 0, total: 0 }],
])("has no fraction with %s, which a bar shows as indeterminate", (_name, progress) => {
  expect(downloadFraction(progress)).toBeUndefined();
});

it.each([
  [0, 100, 0],
  [25, 100, 0.25],
  [1, 3, 1 / 3],
  [100, 100, 1],
])("is the share downloaded of the total: %i of %i", (downloaded, total, expected) => {
  expect(downloadFraction({ downloaded, total })).toBeCloseTo(expected, 10);
});

it("stops at 1 when more has arrived than the total said", () => {
  expect(downloadFraction({ downloaded: 150, total: 100 })).toBe(1);
});

it("stops at 0 when the count is below it", () => {
  expect(downloadFraction({ downloaded: -10, total: 100 })).toBe(0);
});
