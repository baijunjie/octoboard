// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { currentLanguage, followLocalLanguage, setLanguage, t } from "./language";

const KEY = "octoboard.language";

afterEach(() => {
  setLanguage("en");
  localStorage.clear();
});

it("keeps the language a pick makes in this window's storage, as the cache the next start renders from, and on the document, with its reading direction", () => {
  setLanguage("ar");
  expect(currentLanguage()).toBe("ar");
  expect(localStorage.getItem(KEY)).toBe("ar");
  expect(document.documentElement.lang).toBe("ar");
  expect(document.documentElement.dir).toBe("rtl");
  setLanguage("de");
  expect(document.documentElement.lang).toBe("de");
  expect(document.documentElement.dir).toBe("ltr");
});

it("words a message for code outside React in the current language", () => {
  setLanguage("de");
  const german = t("common.cancel");
  setLanguage("en");
  expect(t("common.cancel")).toBe("Cancel");
  expect(german).not.toBe("Cancel");
});

/** The language module as a page loads it: in the local daemon's window, or with `identity` in the window of the daemon
 * that names, which the entry hands in before anything reads a preference. */
async function loadedIn(identity?: string): Promise<typeof import("./language")> {
  vi.resetModules();
  (await import("../preferences/persistedPreference")).setPreferenceDaemonIdentity(identity);
  return import("./language");
}

afterEach(() => vi.resetModules());

it("starts a remote daemon's window from the local window's cached language while it has none of its own, and keeps its own picks apart from it", async () => {
  localStorage.setItem(KEY, "de");
  const remote = await loadedIn("me@host");
  expect(remote.currentLanguage()).toBe("de");
  remote.setLanguage("ja");
  expect(localStorage.getItem(`${KEY}@me@host`)).toBe("ja");
  expect(localStorage.getItem(KEY)).toBe("de");
  expect((await loadedIn()).currentLanguage()).toBe("de");
  expect((await loadedIn("me@host")).currentLanguage()).toBe("ja");
});

it("keeps the language of the local window's daemon apart from a remote window's change", async () => {
  const local = await loadedIn();
  local.setLanguage("fr");
  const remote = await loadedIn("me@host");
  remote.setLanguage("ko");
  expect(localStorage.getItem(KEY)).toBe("fr");
  expect((await loadedIn()).currentLanguage()).toBe("fr");
});

it("renders a remote daemon's window in the cache of its own daemon rather than the local one's", async () => {
  localStorage.setItem(KEY, "de");
  localStorage.setItem(`${KEY}@me@host`, "es");
  expect((await loadedIn("me@host")).currentLanguage()).toBe("es");
  expect((await loadedIn("other@host")).currentLanguage()).toBe("de");
});

it("takes up the language another window of the local daemon picks once it follows, and keeps the document in step", () => {
  followLocalLanguage();
  window.dispatchEvent(new StorageEvent("storage", { key: KEY, newValue: "ja", storageArea: localStorage }));
  expect(currentLanguage()).toBe("ja");
  expect(document.documentElement.lang).toBe("ja");
});
