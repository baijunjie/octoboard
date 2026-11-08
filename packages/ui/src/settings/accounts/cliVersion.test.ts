import { expect, it } from "vitest";

import { isNewerVersion } from "./cliVersion";

it("compares the numbers of a version, not its text", () => {
  expect(isNewerVersion("2.1.320", "2.1.319")).toBe(true);
  expect(isNewerVersion("2.10.0", "2.9.9")).toBe(true);
  expect(isNewerVersion("3.0.0", "2.99.99")).toBe(true);
  expect(isNewerVersion("2.1.319", "2.1.319")).toBe(false);
  expect(isNewerVersion("2.1.318", "2.1.319")).toBe(false);
});

it("reads the version out of the name a source gives it", () => {
  expect(isNewerVersion("rust-v0.5.2", "0.5.1")).toBe(true);
  expect(isNewerVersion("v1.0.0", "1.0.0")).toBe(false);
});

it("says nothing where a version has no three numbers", () => {
  expect(isNewerVersion("nightly", "1.0.0")).toBeUndefined();
  expect(isNewerVersion("1.0.0", "1.0")).toBeUndefined();
});
