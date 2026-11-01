// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

afterEach(() => localStorage.clear());

// The preference reads storage as its module loads, so each stored value needs a fresh load.
async function loadWith(stored: string | null) {
  if (stored !== null) localStorage.setItem(PREFERENCE_KEYS.updateChannel, stored);
  vi.resetModules();
  const { updateChannel } = await import("./updateChannel");
  return updateChannel.get();
}

it("follows Stable when nothing was chosen", async () => {
  expect(await loadWith(null)).toBe("stable");
});

it("keeps a saved choice of either channel", async () => {
  expect(await loadWith("beta")).toBe("beta");
  expect(await loadWith("stable")).toBe("stable");
});

it("reads anything else stored as Stable", async () => {
  expect(await loadWith("nightly")).toBe("stable");
});
