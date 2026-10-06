import { expect, it } from "vitest";

import { CATALOGS, format, splitPlaceholders, type Message, type MessageKey } from "./catalog";
import type { Language } from "./languages";
import { en } from "./messages/en";

it("fills placeholders, selects the plural form and falls back to English", () => {
  expect(format("en", "titleBar.waiting", { count: 1 })).toBe("1 session is waiting for you. Go to the next one");
  expect(format("de", "titleBar.waiting", { count: 2 })).toBe("2 sessions are waiting for you. Go to the next one");
});

const translated = Object.entries(CATALOGS).filter(([language]) => language !== "en") as [
  Language,
  Record<string, Message>,
][];

function formsOf(message: Message): string[] {
  return typeof message === "string" ? [message] : Object.values(message);
}

function placeholdersOf(text: string): Set<string> {
  return new Set(splitPlaceholders(text).filter((_, index) => index % 2 === 1));
}

it("gives each plural message exactly the categories its language needs", () => {
  for (const [language, catalog] of translated) {
    const categories = new Intl.PluralRules(language).resolvedOptions().pluralCategories.sort();
    for (const [key, message] of Object.entries(catalog)) {
      if (typeof message === "string") continue;
      expect(Object.keys(message).sort(), `${language} ${key}`).toEqual(categories);
    }
  }
});

it("keeps every translated message's placeholders those of the English one", () => {
  for (const [language, catalog] of translated) {
    for (const [key, message] of Object.entries(catalog)) {
      const english = en[key as MessageKey] as Message;
      const expected = new Set(formsOf(english).flatMap((form) => [...placeholdersOf(form)]));
      for (const form of formsOf(message)) {
        const actual = placeholdersOf(form);
        // A plural form may leave out `{count}` (a word for "one" needs no number), but never add one.
        const required = typeof message === "string" ? expected : new Set([...expected].filter((name) => name !== "count"));
        const missing = [...required].filter((name) => !actual.has(name));
        const extra = [...actual].filter((name) => !expected.has(name));
        expect({ missing, extra }, `${language} ${key}`).toEqual({
          missing: [],
          extra: [],
        });
      }
    }
  }
});
