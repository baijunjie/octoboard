// @vitest-environment jsdom
import { expect, it, onTestFinished, vi } from "vitest";

import { format, type Translate } from "../i18n/catalog";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import type { SettingsSection } from "../protocol";
import { sessionOrderPreference } from "../store";
import { terminalFontSize } from "../terminal/terminalFontSize";
import { markdownStyle } from "../viewer/markdown/markdownStyle";
import { applyClientSettings, clientSettingsOf, clientSettingValues, documentClientSettings, heldClientSettings } from "./clientSettings";

const DESKTOP = { backgroundRun: true, menuBarIcon: true };
const BROWSER = { backgroundRun: false, menuBarIcon: false };
const ALL: SettingsSection[] = ["general", "appearance", "agent_behavior", "git", "keyboard_shortcuts", "cleanup"];
const FILE = { appearance: "dark", markdown_style: "github", terminal_font_size: 18, run_in_background: true, show_menu_bar_icon: false };

it("holds the session order of the General section and, only where its platform has them, the background running and menu bar icon, and the appearance, Markdown style and terminal font size of the Appearance section", () => {
  onTestFinished(() => {
    markdownStyle.set("tailwind");
    terminalFontSize.set(15);
    runInBackground.set(false);
    showMenuBarIcon.set(true);
    sessionOrderPreference.set({ sessions: "status", activeProjects: "status", inactiveProjects: "name" });
  });
  markdownStyle.set("latex");
  terminalFontSize.set(20);
  runInBackground.set(true);
  showMenuBarIcon.set(false);
  sessionOrderPreference.set({ sessions: "started", activeProjects: "used", inactiveProjects: "used" });
  const appearance = { appearance: "light", markdown_style: "latex", terminal_font_size: 20 };
  const order = { session_order: { sessions: "started", active_projects: "used", inactive_projects: "used" } };
  const window = { run_in_background: true, show_menu_bar_icon: false };
  expect(heldClientSettings({ appearance: "light" }, DESKTOP, ALL)).toEqual({ ...appearance, ...order, ...window });
  expect(heldClientSettings({ appearance: "light" }, BROWSER, ALL)).toEqual({ ...appearance, ...order });
  expect(heldClientSettings({ appearance: "light" }, DESKTOP, ["general"])).toEqual({ ...order, ...window });
  expect(heldClientSettings({ appearance: "light" }, DESKTOP, ["appearance"])).toEqual(appearance);
  expect(heldClientSettings({ appearance: "light" }, DESKTOP, ["git", "keyboard_shortcuts"])).toEqual({});
});

it("takes from a document the values this client can stand behind and nothing else", () => {
  expect(clientSettingsOf(FILE, DESKTOP)).toEqual(FILE);
  // A browser has no background running, or menu bar icon.
  expect(clientSettingsOf(FILE, BROWSER)).toEqual({ appearance: "dark", markdown_style: "github", terminal_font_size: 18 });
  expect(clientSettingsOf({ appearance: "sepia", markdown_style: "word", terminal_font_size: "big", run_in_background: "yes", show_menu_bar_icon: "no" }, DESKTOP)).toEqual({});
  for (const settings of [undefined, null, 7, "dark", {}]) expect(clientSettingsOf(settings, DESKTOP), String(settings)).toEqual({});
  expect(documentClientSettings({ settings: FILE }, BROWSER)).toEqual({ appearance: "dark", markdown_style: "github", terminal_font_size: 18 });
  expect(documentClientSettings({ settings: null }, BROWSER)).toEqual({});
  expect(documentClientSettings(undefined, BROWSER)).toEqual({});
});

it("holds a document's terminal font size within the range this client offers", () => {
  expect(clientSettingsOf({ terminal_font_size: 4 }, BROWSER)).toEqual({ terminal_font_size: 10 });
  expect(clientSettingsOf({ terminal_font_size: 99 }, BROWSER)).toEqual({ terminal_font_size: 24 });
  expect(clientSettingsOf({ terminal_font_size: 15.6 }, BROWSER)).toEqual({ terminal_font_size: 16 });
  for (const terminal_font_size of [null, "15", NaN, {}]) expect(clientSettingsOf({ terminal_font_size }, BROWSER), String(terminal_font_size)).toEqual({});
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
    terminalFontSize.set(15);
    runInBackground.set(false);
    showMenuBarIcon.set(true);
  });
  const setAppearance = vi.fn();
  applyClientSettings({ markdown_style: "newsprint", terminal_font_size: 12 }, setAppearance);
  expect(markdownStyle.get()).toBe("newsprint");
  expect(terminalFontSize.get()).toBe(12);
  expect(runInBackground.get()).toBe(false);
  expect(showMenuBarIcon.get()).toBe(true);
  expect(setAppearance).not.toHaveBeenCalled();
  applyClientSettings({ appearance: "dark", run_in_background: true, show_menu_bar_icon: false }, setAppearance);
  expect(setAppearance).toHaveBeenCalledExactlyOnceWith("dark");
  expect(runInBackground.get()).toBe(true);
  expect(showMenuBarIcon.get()).toBe(false);
  expect(markdownStyle.get()).toBe("newsprint");
  expect(terminalFontSize.get()).toBe(12);
});

it("applies the parts of a session order it is given and leaves the others as they are", () => {
  onTestFinished(() => sessionOrderPreference.set({ sessions: "status", activeProjects: "status", inactiveProjects: "name" }));
  sessionOrderPreference.set({ sessions: "status", activeProjects: "status", inactiveProjects: "name" });
  applyClientSettings({ session_order: { activeProjects: "used" } }, vi.fn());
  expect(sessionOrderPreference.get()).toEqual({ sessions: "status", activeProjects: "used", inactiveProjects: "name" });
});

it("words the terminal font size by the catalog message, with the number in the language's own digits", () => {
  const worded = (language: "en" | "ar") => clientSettingValues(((key, params) => format(language, key, params)) as Translate, { terminal_font_size: 18 })[0].value;
  expect(worded("en")).toBe("18 px");
  expect(worded("ar")).toBe(`${new Intl.NumberFormat("ar").format(18)} px`);
});

it("words each value as its settings page does, with the one this client has where given", () => {
  const t = ((key: string, params?: { size: number }) => (params ? `${key}:${params.size}` : key)) as Translate;
  expect(clientSettingValues(t, { appearance: "dark", markdown_style: "github", terminal_font_size: 18, run_in_background: false, show_menu_bar_icon: true })).toEqual([
    { field: "appearance", label: "settings.theme.label", value: "settings.theme.dark" },
    { field: "markdown_style", label: "settings.markdownStyle.label", value: "GitHub", lang: "en" },
    { field: "terminal_font_size", label: "settings.terminalFontSize.label", value: "configuration.import.value.pixels:18" },
    { field: "run_in_background", label: "settings.background.label", value: "configuration.import.value.off" },
    { field: "show_menu_bar_icon", label: "settings.menuBarIcon.label", value: "configuration.import.value.on" },
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
