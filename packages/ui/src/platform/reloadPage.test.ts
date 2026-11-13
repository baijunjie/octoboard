import { afterEach, beforeEach, expect, it, vi } from "vitest";

// The module holds its flag for the life of the page, so each test loads a fresh copy.
async function load() {
  vi.resetModules();
  return import("./reloadPage");
}

const reload = vi.fn();

beforeEach(() => {
  vi.stubGlobal("window", { location: { reload } });
});

afterEach(() => {
  vi.unstubAllGlobals();
  reload.mockClear();
});

it("is not reloading until the page is reloaded", async () => {
  const { isReloading } = await load();
  expect(isReloading()).toBe(false);
});

it("reloads the page and from then on says it is reloading", async () => {
  const { isReloading, reloadPage } = await load();
  reloadPage();
  expect(reload).toHaveBeenCalledOnce();
  expect(isReloading()).toBe(true);
});
