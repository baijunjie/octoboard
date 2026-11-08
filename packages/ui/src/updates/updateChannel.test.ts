// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { effectiveUpdateChannel, isPrerelease } from "./updateChannel";

afterEach(() => localStorage.clear());

// The preference reads storage as its module loads, so each stored value needs a fresh load.
async function loadWith(stored: string | null) {
  if (stored !== null) localStorage.setItem(PREFERENCE_KEYS.updateChannel, stored);
  vi.resetModules();
  const { updateChannel } = await import("./updateChannel");
  return updateChannel.get();
}

it("holds no choice when nothing was chosen", async () => {
  expect(await loadWith(null)).toBeUndefined();
});

it("keeps a saved choice of either channel", async () => {
  expect(await loadWith("beta")).toBe("beta");
  expect(await loadWith("stable")).toBe("stable");
});

it("reads anything else stored as no choice", async () => {
  expect(await loadWith("nightly")).toBeUndefined();
});

it("tells a prerelease by its suffix, build metadata aside", () => {
  expect(isPrerelease("0.1.0-beta.3")).toBe(true);
  expect(isPrerelease("1.0.0-rc.1+build.5")).toBe(true);
  expect(isPrerelease("1.0.0")).toBe(false);
  expect(isPrerelease("1.0.0+build-5")).toBe(false);
});

it("defaults to Beta on a prerelease build and Stable on a stable one", () => {
  expect(effectiveUpdateChannel(undefined, "0.1.0-beta.3")).toBe("beta");
  expect(effectiveUpdateChannel(undefined, "1.0.0")).toBe("stable");
});

it("defaults to Stable while the running version is not known", () => {
  expect(effectiveUpdateChannel(undefined, undefined)).toBe("stable");
});

it("keeps a saved choice whichever build runs", () => {
  expect(effectiveUpdateChannel("stable", "0.1.0-beta.3")).toBe("stable");
  expect(effectiveUpdateChannel("beta", "1.0.0")).toBe("beta");
});
