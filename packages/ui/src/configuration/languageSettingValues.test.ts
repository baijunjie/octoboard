import { expect, it } from "vitest";

import { documentLanguage, languageSettingValues } from "./languageSettingValues";

it("takes a document's language", () => {
  expect(documentLanguage({ settings: { interface_language: "de" } })).toBe("de");
});

it("takes the agent language of a file of version 4 or older over the interface language beside it, as the daemon's upgrade does", () => {
  expect(documentLanguage({ version: 3, settings: { interface_language: "fr", agent_language: "de" } })).toBe("de");
  expect(documentLanguage({ version: 4, settings: { interface_language: "fr", agent_language: "de" } })).toBe("de");
  expect(documentLanguage({ version: 4, settings: { interface_language: "fr", agent_language: null } })).toBe("fr");
});

it("ignores an agent language in a file of a later version or with none, which the daemon does not read there", () => {
  expect(documentLanguage({ version: 5, settings: { interface_language: "fr", agent_language: "de" } })).toBe("fr");
  expect(documentLanguage({ settings: { interface_language: "fr", agent_language: "de" } })).toBe("fr");
});

it("finds none in a document that carries none, or no settings, or is not an object", () => {
  expect(documentLanguage({ settings: { appearance: "dark" } })).toBeUndefined();
  expect(documentLanguage({ settings: null })).toBeUndefined();
  expect(documentLanguage({})).toBeUndefined();
  expect(documentLanguage(null)).toBeUndefined();
  expect(documentLanguage(undefined)).toBeUndefined();
  expect(documentLanguage("interface_language")).toBeUndefined();
});

it("words a language this build offers as a line of the language, named in itself and marked with its own language", () => {
  expect(languageSettingValues("de")).toEqual([
    { field: "interface_language", label: "settings.language.label", value: "Deutsch", lang: "de" },
  ]);
});

it("adds the language the daemon holds now, to say what would change", () => {
  expect(languageSettingValues("de", "ja")).toEqual([
    { field: "interface_language", label: "settings.language.label", value: "Deutsch", lang: "de", current: "日本語", currentLang: "ja" },
  ]);
});

it("compares with nothing when the daemon's language is none this build offers", () => {
  const [line] = languageSettingValues("de", "tlh");
  expect(line).not.toHaveProperty("current");
  expect(line).not.toHaveProperty("currentLang");
});

it("gives no line for a language this build does not offer, or none", () => {
  for (const wanted of ["tlh", "", 3, null, undefined]) expect(languageSettingValues(wanted, "de"), String(wanted)).toEqual([]);
});
