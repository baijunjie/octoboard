import { expect, it } from "vitest";

import { agentCliStatusOf } from "../../fixtures/builders";
import { hasCliUpdate, installMethodOf, isNewerVersion, updatesByMethod } from "./cliVersion";

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

const newer = (install_method: string) => agentCliStatusOf("claude", { installed_version: "2.1.318", latest_version: "2.1.319", install_method });

it.each(["native", "npm", "homebrew"])("offers an update for a CLI installed by %s that has a newer version", (method) => {
  expect(updatesByMethod(method)).toBe(true);
  expect(hasCliUpdate(newer(method))).toBe(true);
});

it.each(["unknown", "from_a_newer_daemon"])("offers no update, and marks none, for an install method of %s", (method) => {
  expect(updatesByMethod(method)).toBe(false);
  expect(hasCliUpdate(newer(method))).toBe(false);
});

it("reads an install method this build does not know as unknown, and none as none", () => {
  expect(installMethodOf("npm")).toBe("npm");
  expect(installMethodOf("from_a_newer_daemon")).toBe("unknown");
  expect(installMethodOf(undefined)).toBeUndefined();
  expect(installMethodOf(null)).toBeUndefined();
});
