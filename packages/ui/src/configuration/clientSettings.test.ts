// @vitest-environment jsdom
import { expect, it, onTestFinished, vi } from "vitest";

import { currentLanguage, setLanguage } from "../i18n/language";
import { keepAwake, keepAwakeWhen } from "../lifecycle/keepAwake";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import { markdownStyle } from "../viewer/markdown/markdownStyle";
import { applyClientSettings, clientSettingsOf, clientSettingValues, documentClientSettings, heldClientSettings } from "./clientSettings";

const DESKTOP = { backgroundRun: true, menuBarIcon: true, keepAwake: true };
const BROWSER = { backgroundRun: false, menuBarIcon: false, keepAwake: false };
const FILE = { appearance: "dark", markdown_style: "github", run_in_background: true, show_menu_bar_icon: false, keep_awake: true, keep_awake_when: "always", interface_language: "ja" };

it("holds the appearance, Markdown style and language a client has, and the background running, menu bar icon and keeping awake only where its platform has them", () => {
  onTestFinished(() => {
    markdownStyle.set("tailwind");
    runInBackground.set(false);
    showMenuBarIcon.set(true);
    keepAwake.set(false);
    keepAwakeWhen.set("working");
  });
  markdownStyle.set("latex");
  runInBackground.set(true);
  showMenuBarIcon.set(false);
  keepAwake.set(true);
  keepAwakeWhen.set("always");
  expect(heldClientSettings({ appearance: "light", language: "de" }, DESKTOP)).toEqual({ appearance: "light", markdown_style: "latex", run_in_background: true, show_menu_bar_icon: false, keep_awake: true, keep_awake_when: "always", interface_language: "de" });
  expect(heldClientSettings({ appearance: "light", language: "de" }, BROWSER)).toEqual({ appearance: "light", markdown_style: "latex", interface_language: "de" });
});

it("takes from a document the values this client can stand behind and nothing else", () => {
  expect(clientSettingsOf(FILE, DESKTOP)).toEqual(FILE);
  // A browser has no background running, menu bar icon or keeping awake.
  expect(clientSettingsOf(FILE, BROWSER)).toEqual({ appearance: "dark", markdown_style: "github", interface_language: "ja" });
  expect(clientSettingsOf({ appearance: "sepia", markdown_style: "word", run_in_background: "yes", show_menu_bar_icon: "no", keep_awake: 1, keep_awake_when: "sometimes", interface_language: "tlh" }, DESKTOP)).toEqual({});
  for (const settings of [undefined, null, 7, "dark", {}]) expect(clientSettingsOf(settings, DESKTOP), String(settings)).toEqual({});
  expect(documentClientSettings({ settings: FILE }, BROWSER)).toEqual({ appearance: "dark", markdown_style: "github", interface_language: "ja" });
  expect(documentClientSettings({ settings: null }, BROWSER)).toEqual({});
  expect(documentClientSettings(undefined, BROWSER)).toEqual({});
});

it("applies what it is given and leaves the rest as it is", () => {
  onTestFinished(() => {
    markdownStyle.set("tailwind");
    runInBackground.set(false);
    showMenuBarIcon.set(true);
    keepAwake.set(false);
    keepAwakeWhen.set("working");
    setLanguage("en");
  });
  const setAppearance = vi.fn();
  applyClientSettings({ markdown_style: "newsprint" }, setAppearance);
  expect(markdownStyle.get()).toBe("newsprint");
  expect(runInBackground.get()).toBe(false);
  expect(showMenuBarIcon.get()).toBe(true);
  expect(keepAwake.get()).toBe(false);
  expect(keepAwakeWhen.get()).toBe("working");
  expect(currentLanguage()).toBe("en");
  expect(setAppearance).not.toHaveBeenCalled();
  applyClientSettings({ appearance: "dark", run_in_background: true, show_menu_bar_icon: false, keep_awake: true, keep_awake_when: "always", interface_language: "ja" }, setAppearance);
  expect(setAppearance).toHaveBeenCalledExactlyOnceWith("dark");
  expect(runInBackground.get()).toBe(true);
  expect(showMenuBarIcon.get()).toBe(false);
  expect(keepAwake.get()).toBe(true);
  expect(keepAwakeWhen.get()).toBe("always");
  expect(currentLanguage()).toBe("ja");
  expect(markdownStyle.get()).toBe("newsprint");
});

it("words each value as its settings page does, with the one this client has where given", () => {
  const t = (key: string) => key;
  expect(clientSettingValues(t, { appearance: "dark", markdown_style: "github", interface_language: "ja", run_in_background: false, show_menu_bar_icon: true, keep_awake: true, keep_awake_when: "working" })).toEqual([
    { field: "appearance", label: "settings.appearance.label", value: "settings.appearance.dark" },
    { field: "interface_language", label: "settings.language.label", value: "日本語", lang: "ja" },
    { field: "markdown_style", label: "settings.markdownStyle.label", value: "GitHub", lang: "en" },
    { field: "run_in_background", label: "settings.background.label", value: "configuration.import.value.off" },
    { field: "show_menu_bar_icon", label: "settings.menuBarIcon.label", value: "configuration.import.value.on" },
    { field: "keep_awake", label: "settings.keepAwake.label", value: "configuration.import.value.on" },
    { field: "keep_awake_when", label: "settings.keepAwake.when.label", value: "settings.keepAwake.when.working" },
  ]);
  expect(clientSettingValues(t, { markdown_style: "github" }, { markdown_style: "tailwind", appearance: "light" })).toEqual([
    { field: "markdown_style", label: "settings.markdownStyle.label", value: "GitHub", lang: "en", current: "Tailwind", currentLang: "en" },
  ]);
  expect(clientSettingValues(t, { interface_language: "de" }, { interface_language: "ar" })).toEqual([
    { field: "interface_language", label: "settings.language.label", value: "Deutsch", lang: "de", current: "العربية", currentLang: "ar" },
  ]);
  expect(clientSettingValues(t, {})).toEqual([]);
});
