// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { browserPlatform } from "./browser";

afterEach(() => vi.restoreAllMocks());

it("opens a link in a new tab with no access to the page and no referrer", async () => {
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  await browserPlatform().links!.open("https://example.com/");
  expect(open).toHaveBeenCalledWith("https://example.com/", "_blank", "noopener,noreferrer");
});

// `noopener` makes `window.open` return null whether or not a tab opened, so a blocked popup cannot be
// told apart, and the capability resolves.
it("resolves even when the browser gives no window back", async () => {
  vi.spyOn(window, "open").mockReturnValue(null);
  await expect(browserPlatform().links!.open("https://example.com/")).resolves.toBeUndefined();
});
