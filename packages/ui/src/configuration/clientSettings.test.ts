// @vitest-environment jsdom
import { expect, it, onTestFinished, vi } from "vitest";

import { currentLanguage, setLanguage } from "../i18n/language";
import { runInBackground } from "../lifecycle/runInBackground";
import { markdownStyle } from "../viewer/markdown/markdownStyle";
import { applyClientSettings, clientSettingsOf, clientSettingValues, documentClientSettings, heldClientSettings } from "./clientSettings";

const DESKTOP = { backgroundRun: true };
const BROWSER = { backgroundRun: false };
const FILE = { appearance: "dark", markdown_style: "github", run_in_background: true, interface_language: "ja" };

it("holds the appearance, Markdown style and language a client has, and the background running only where its platform has it", () => {
  onTestFinished(() => {
    markdownStyle.set("tailwind");
    runInBackground.set(false);
  });
  markdownStyle.set("latex");
  runInBackground.set(true);
  expect(heldClientSettings({ appearance: "light", language: "de" }, DESKTOP)).toEqual({ appearance: "light", markdown_style: "latex", run_in_background: true, interface_language: "de" });
  expect(heldClientSettings({ appearance: "light", language: "de" }, BROWSER)).toEqual({ appearance: "light", markdown_style: "latex", interface_language: "de" });
});

it("takes from a document the values this client can stand behind and nothing else", () => {
  expect(clientSettingsOf(FILE, DESKTOP)).toEqual(FILE);
  // A browser has no background running.
  expect(clientSettingsOf(FILE, BROWSER)).toEqual({ appearance: "dark", markdown_style: "github", interface_language: "ja" });
  expect(clientSettingsOf({ appearance: "sepia", markdown_style: "word", run_in_background: "yes", interface_language: "tlh" }, DESKTOP)).toEqual({});
  for (const settings of [undefined, null, 7, "dark", {}]) expect(clientSettingsOf(settings, DESKTOP), String(settings)).toEqual({});
  expect(documentClientSettings({ settings: FILE }, BROWSER)).toEqual({ appearance: "dark", markdown_style: "github", interface_language: "ja" });
  expect(documentClientSettings({ settings: null }, BROWSER)).toEqual({});
  expect(documentClientSettings(undefined, BROWSER)).toEqual({});
});

it("applies what it is given and leaves the rest as it is", () => {
  onTestFinished(() => {
    markdownStyle.set("tailwind");
    runInBackground.set(false);
    setLanguage("en");
  });
  const setAppearance = vi.fn();
  applyClientSettings({ markdown_style: "newsprint" }, setAppearance);
  expect(markdownStyle.get()).toBe("newsprint");
  expect(runInBackground.get()).toBe(false);
  expect(currentLanguage()).toBe("en");
  expect(setAppearance).not.toHaveBeenCalled();
  applyClientSettings({ appearance: "dark", run_in_background: true, interface_language: "ja" }, setAppearance);
  expect(setAppearance).toHaveBeenCalledExactlyOnceWith("dark");
  expect(runInBackground.get()).toBe(true);
  expect(currentLanguage()).toBe("ja");
  expect(markdownStyle.get()).toBe("newsprint");
});

it("words each value as its settings page does, with the one this client has where given", () => {
  const t = (key: string) => key;
  expect(clientSettingValues(t, { appearance: "dark", markdown_style: "github", interface_language: "ja", run_in_background: false })).toEqual([
    { field: "appearance", label: "settings.appearance.label", value: "settings.appearance.dark" },
    { field: "interface_language", label: "settings.language.label", value: "日本語", lang: "ja" },
    { field: "markdown_style", label: "settings.markdownStyle.label", value: "GitHub", lang: "en" },
    { field: "run_in_background", label: "settings.background.label", value: "configuration.import.value.off" },
  ]);
  expect(clientSettingValues(t, { markdown_style: "github" }, { markdown_style: "tailwind", appearance: "light" })).toEqual([
    { field: "markdown_style", label: "settings.markdownStyle.label", value: "GitHub", lang: "en", current: "Tailwind", currentLang: "en" },
  ]);
  expect(clientSettingValues(t, { interface_language: "de" }, { interface_language: "ar" })).toEqual([
    { field: "interface_language", label: "settings.language.label", value: "Deutsch", lang: "de", current: "العربية", currentLang: "ar" },
  ]);
  expect(clientSettingValues(t, {})).toEqual([]);
});
