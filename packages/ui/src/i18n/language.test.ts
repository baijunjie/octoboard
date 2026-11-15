// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";

import { currentLanguage, setLanguage, subscribeLanguage, t } from "./language";

const KEY = "octoboard.language";

afterEach(() => {
  setLanguage("en");
  localStorage.clear();
});

/** What another window storing `value` at `key` sends this one. */
function storedElsewhere(key: string, value: string | null): void {
  window.dispatchEvent(new StorageEvent("storage", { key, newValue: value, storageArea: localStorage }));
}

it("keeps the language a pick makes in the client's own storage and on the document, with its reading direction", () => {
  setLanguage("ar");
  expect(currentLanguage()).toBe("ar");
  expect(localStorage.getItem(KEY)).toBe("ar");
  expect(document.documentElement.lang).toBe("ar");
  expect(document.documentElement.dir).toBe("rtl");
  setLanguage("de");
  expect(document.documentElement.lang).toBe("de");
  expect(document.documentElement.dir).toBe("ltr");
});

it("takes up the language another window of the application picks, and tells its subscribers once the document carries it", () => {
  const seen: string[] = [];
  const undo = subscribeLanguage(() => seen.push(`${currentLanguage()}/${document.documentElement.lang}`));
  storedElsewhere(KEY, "ja");
  expect(currentLanguage()).toBe("ja");
  expect(document.documentElement.lang).toBe("ja");
  expect(seen).toEqual(["ja/ja"]);
  undo();
});

it("falls back to a language the system suggests when another window clears or garbles the stored one", () => {
  setLanguage("ja");
  storedElsewhere(KEY, "tlh");
  expect(currentLanguage()).not.toBe("tlh");
  expect(document.documentElement.lang).toBe(currentLanguage());
});

it("does not take what another window stores under another key for a language", () => {
  setLanguage("ja");
  storedElsewhere("octoboard.theme", "de");
  expect(currentLanguage()).toBe("ja");
});

it("words a message for code outside React in the current language", () => {
  setLanguage("de");
  const german = t("common.cancel");
  setLanguage("en");
  expect(t("common.cancel")).toBe("Cancel");
  expect(german).not.toBe("Cancel");
});
