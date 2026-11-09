// @vitest-environment jsdom
import { expect, it, onTestFinished, vi } from "vitest";

import { keepAwake, keepAwakeWhen } from "../lifecycle/keepAwake";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import type { SettingsSection } from "../protocol";
import { sessionOrderPreference } from "../store";
import { markdownStyle } from "../viewer/markdown/markdownStyle";
import { applyClientSettings, clientSettingsOf, clientSettingValues, documentClientSettings, heldClientSettings } from "./clientSettings";

const DESKTOP = { backgroundRun: true, menuBarIcon: true, keepAwake: true };
const BROWSER = { backgroundRun: false, menuBarIcon: false, keepAwake: false };
const ALL: SettingsSection[] = ["general", "app_behavior", "git", "agent_behavior", "keyboard_shortcuts", "cleanup"];
const FILE = { appearance: "dark", markdown_style: "github", run_in_background: true, show_menu_bar_icon: false, keep_awake: true, keep_awake_when: "always" };

it("holds the appearance, Markdown style, and session order of the General section, and the background running, menu bar icon and keeping awake of App behavior only where its platform has them", () => {
  onTestFinished(() => {
    markdownStyle.set("tailwind");
    runInBackground.set(false);
    showMenuBarIcon.set(true);
    keepAwake.set(false);
    keepAwakeWhen.set("working");
    sessionOrderPreference.set({ sessions: "status", activeProjects: "status", inactiveProjects: "name" });
  });
  markdownStyle.set("latex");
  runInBackground.set(true);
  showMenuBarIcon.set(false);
  keepAwake.set(true);
  keepAwakeWhen.set("always");
  sessionOrderPreference.set({ sessions: "started", activeProjects: "used", inactiveProjects: "used" });
  const general = { appearance: "light", markdown_style: "latex", session_order: { sessions: "started", active_projects: "used", inactive_projects: "used" } };
  const appBehavior = { run_in_background: true, show_menu_bar_icon: false, keep_awake: true, keep_awake_when: "always" };
  expect(heldClientSettings({ appearance: "light" }, DESKTOP, ALL)).toEqual({ ...general, ...appBehavior });
  expect(heldClientSettings({ appearance: "light" }, BROWSER, ALL)).toEqual(general);
  expect(heldClientSettings({ appearance: "light" }, DESKTOP, ["general"])).toEqual(general);
  expect(heldClientSettings({ appearance: "light" }, DESKTOP, ["app_behavior"])).toEqual(appBehavior);
  expect(heldClientSettings({ appearance: "light" }, DESKTOP, ["git", "keyboard_shortcuts"])).toEqual({});
});

it("takes from a document the values this client can stand behind and nothing else", () => {
  expect(clientSettingsOf(FILE, DESKTOP)).toEqual(FILE);
  // A browser has no background running, menu bar icon or keeping awake.
  expect(clientSettingsOf(FILE, BROWSER)).toEqual({ appearance: "dark", markdown_style: "github" });
  expect(clientSettingsOf({ appearance: "sepia", markdown_style: "word", run_in_background: "yes", show_menu_bar_icon: "no", keep_awake: 1, keep_awake_when: "sometimes" }, DESKTOP)).toEqual({});
  for (const settings of [undefined, null, 7, "dark", {}]) expect(clientSettingsOf(settings, DESKTOP), String(settings)).toEqual({});
  expect(documentClientSettings({ settings: FILE }, BROWSER)).toEqual({ appearance: "dark", markdown_style: "github" });
  expect(documentClientSettings({ settings: null }, BROWSER)).toEqual({});
  expect(documentClientSettings(undefined, BROWSER)).toEqual({});
});

it("takes the parts of a session order this client knows, and none where it knows no part", () => {
  expect(clientSettingsOf({ session_order: { sessions: "started", active_projects: "used", inactive_projects: "name" } }, BROWSER)).toEqual({
    session_order: { sessions: "started", activeProjects: "used", inactiveProjects: "name" },
  });
  expect(clientSettingsOf({ session_order: { sessions: "oldest", active_projects: "name", inactive_projects: "status" } }, BROWSER)).toEqual({ session_order: { activeProjects: "name" } });
  for (const session_order of [{ sessions: "oldest" }, {}, null, 7]) expect(clientSettingsOf({ session_order }, BROWSER), String(session_order)).toEqual({});
});

it("applies what it is given and leaves the rest as it is", () => {
  onTestFinished(() => {
    markdownStyle.set("tailwind");
    runInBackground.set(false);
    showMenuBarIcon.set(true);
    keepAwake.set(false);
    keepAwakeWhen.set("working");
  });
  const setAppearance = vi.fn();
  applyClientSettings({ markdown_style: "newsprint" }, setAppearance);
  expect(markdownStyle.get()).toBe("newsprint");
  expect(runInBackground.get()).toBe(false);
  expect(showMenuBarIcon.get()).toBe(true);
  expect(keepAwake.get()).toBe(false);
  expect(keepAwakeWhen.get()).toBe("working");
  expect(setAppearance).not.toHaveBeenCalled();
  applyClientSettings({ appearance: "dark", run_in_background: true, show_menu_bar_icon: false, keep_awake: true, keep_awake_when: "always" }, setAppearance);
  expect(setAppearance).toHaveBeenCalledExactlyOnceWith("dark");
  expect(runInBackground.get()).toBe(true);
  expect(showMenuBarIcon.get()).toBe(false);
  expect(keepAwake.get()).toBe(true);
  expect(keepAwakeWhen.get()).toBe("always");
  expect(markdownStyle.get()).toBe("newsprint");
});

it("applies the parts of a session order it is given and leaves the others as they are", () => {
  onTestFinished(() => sessionOrderPreference.set({ sessions: "status", activeProjects: "status", inactiveProjects: "name" }));
  sessionOrderPreference.set({ sessions: "status", activeProjects: "status", inactiveProjects: "name" });
  applyClientSettings({ session_order: { activeProjects: "used" } }, vi.fn());
  expect(sessionOrderPreference.get()).toEqual({ sessions: "status", activeProjects: "used", inactiveProjects: "name" });
});

it("words each value as its settings page does, with the one this client has where given", () => {
  const t = (key: string) => key;
  expect(clientSettingValues(t, { appearance: "dark", markdown_style: "github", run_in_background: false, show_menu_bar_icon: true, keep_awake: true, keep_awake_when: "working" })).toEqual([
    { field: "appearance", label: "settings.appearance.label", value: "settings.appearance.dark" },
    { field: "markdown_style", label: "settings.markdownStyle.label", value: "GitHub", lang: "en" },
    { field: "run_in_background", label: "settings.background.label", value: "configuration.import.value.off" },
    { field: "show_menu_bar_icon", label: "settings.menuBarIcon.label", value: "configuration.import.value.on" },
    { field: "keep_awake", label: "settings.keepAwake.label", value: "configuration.import.value.on" },
    { field: "keep_awake_when", label: "settings.keepAwake.when.label", value: "settings.keepAwake.when.working" },
  ]);
  expect(clientSettingValues(t, { markdown_style: "github" }, { markdown_style: "tailwind", appearance: "light" })).toEqual([
    { field: "markdown_style", label: "settings.markdownStyle.label", value: "GitHub", lang: "en", current: "Tailwind", currentLang: "en" },
  ]);
  expect(clientSettingValues(t, {})).toEqual([]);
});

it("leaves the language to the daemon: a document's is no client setting, and nothing here applies one", () => {
  const settings = { interface_language: "ja" };
  expect(clientSettingsOf(settings, DESKTOP)).toEqual({});
  expect(heldClientSettings({ appearance: "light" }, DESKTOP, ALL)).not.toHaveProperty("interface_language");
});
