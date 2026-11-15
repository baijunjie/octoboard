import { expect, it } from "vitest";

import { agentLanguageSettingValues, documentAgentLanguage } from "./agentLanguageSettingValues";

it("takes a document's agent language", () => {
  expect(documentAgentLanguage({ settings: { agent_language: "de" } })).toBe("de");
});

it("takes the interface language of a document from a build that held one language for both, as its agent language", () => {
  expect(documentAgentLanguage({ settings: { interface_language: "ja" } })).toBe("ja");
});

it("takes a document's agent language over its interface language", () => {
  expect(documentAgentLanguage({ settings: { agent_language: "de", interface_language: "ja" } })).toBe("de");
});

it("finds none in a document that carries neither, or no settings, or is not an object", () => {
  expect(documentAgentLanguage({ settings: { appearance: "dark" } })).toBeUndefined();
  expect(documentAgentLanguage({ settings: null })).toBeUndefined();
  expect(documentAgentLanguage({})).toBeUndefined();
  expect(documentAgentLanguage(null)).toBeUndefined();
  expect(documentAgentLanguage(undefined)).toBeUndefined();
  expect(documentAgentLanguage("agent_language")).toBeUndefined();
});

it("words a language this build offers as a line of the agent language, named in itself and marked with its own language", () => {
  expect(agentLanguageSettingValues("de")).toEqual([
    { field: "agent_language", label: "settings.agentLanguage.label", value: "Deutsch", lang: "de" },
  ]);
});

it("adds the language the daemon holds now, to say what would change", () => {
  expect(agentLanguageSettingValues("de", "ja")).toEqual([
    { field: "agent_language", label: "settings.agentLanguage.label", value: "Deutsch", lang: "de", current: "日本語", currentLang: "ja" },
  ]);
});

it("compares with nothing when the daemon's language is none this build offers", () => {
  const [line] = agentLanguageSettingValues("de", "tlh");
  expect(line).not.toHaveProperty("current");
  expect(line).not.toHaveProperty("currentLang");
});

it("gives no line for a language this build does not offer, or none", () => {
  for (const wanted of ["tlh", "", 3, null, undefined]) expect(agentLanguageSettingValues(wanted, "de"), String(wanted)).toEqual([]);
});
