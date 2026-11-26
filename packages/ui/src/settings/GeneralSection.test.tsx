// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { translatorFor } from "../i18n/catalog";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import { ThemeProvider } from "../preferences/theme";
import type { State } from "../store";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { flushPromises } from "../test/clock";
import { BROWSER_HOST, MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { GeneralSection } from "./GeneralSection";
import { SettingsReadsProvider } from "./SettingsReadsProvider";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const t = translatorFor("en");
const HOST = { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "settings", application: true } as const;

const launchAtLogin = { status: async () => "disabled" as const, set: async () => "disabled" as const, openSystemSettings: async () => {} };
const backgroundRun = { set: async () => {} };
const statusItem = { setMenu: async () => {}, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };

const stubMatchMedia = () => {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => void vi.unstubAllGlobals());
};

/** What the section shows: the rows outside any group, which come first, then each group's heading and rows. */
async function layout(platform: PlatformAdapter, state: Partial<State> = {}) {
  stubMatchMedia();
  const daemon = testDaemon({ state: { connectionState: "open", settings: DEFAULT_SETTINGS, ...state } });
  const container = mountHost(
    <LanguageProvider>
      <ThemeProvider>
        <SettingsReadsProvider>
          <GeneralSection />
        </SettingsReadsProvider>
      </ThemeProvider>
    </LanguageProvider>,
    { platform, daemon },
  );
  // The launch-at-login row waits on a read of the system.
  for (let turn = 0; turn < 3; turn += 1) await act(flushPromises);
  const ids = (root: Element) => [...root.querySelectorAll<HTMLElement>("[data-settings-search-id]")].map((element) => element.dataset.settingsSearchId);
  const groups = [...container.querySelectorAll<HTMLElement>("[data-settings-search-group]")].map((group) => ({
    title: group.querySelector("h3, h2, [id]")?.textContent,
    key: group.dataset.settingsSearchGroup,
    ids: ids(group),
  }));
  const ungrouped = ids(container).filter((id) => !groups.some((group) => group.ids.includes(id)));
  return { ungrouped, groups };
}

const DESKTOP: PlatformAdapter = { kind: "tauri", host: MAC_HOST, launchAtLogin, backgroundRun, statusItem };
const BROWSER: PlatformAdapter = { kind: "browser", host: BROWSER_HOST };

it("shows the language row on its own, then Sorting, Terminals, Power, Window and Startup, where the platform and the daemon's host offer everything", async () => {
  const { ungrouped, groups } = await layout(DESKTOP, { host: { ...HOST, keep_awake: true } });
  expect(ungrouped).toEqual(["language"]);
  expect(groups.map(({ key }) => key)).toEqual([
    "settings.sort.group",
    "settings.general.group.terminals",
    "settings.general.group.power",
    "settings.general.group.window",
    "settings.general.group.startup",
  ]);
  expect(groups.map(({ title }) => title)).toEqual([
    t("settings.sort.group"),
    t("settings.general.group.terminals"),
    t("settings.general.group.power"),
    t("settings.general.group.window"),
    t("settings.general.group.startup"),
  ]);
  expect(groups.map(({ ids }) => ids)).toEqual([
    ["session-order.sessions", "session-order.activeProjects", "session-order.inactiveProjects"],
    ["session-composer", "auto_close_idle_shell_terminals"],
    ["keep_awake"],
    ["run-in-background", "menu-bar-icon"],
    ["launch-at-login", "resume_sessions_on_start"],
  ]);
});

it("leaves out the Window and Power groups and the launch-at-login row in a browser on a host that cannot be held awake", async () => {
  const { ungrouped, groups } = await layout(BROWSER);
  expect(ungrouped).toEqual(["language"]);
  expect(groups.map(({ key, ids }) => [key, ids])).toEqual([
    ["settings.sort.group", ["session-order.sessions", "session-order.activeProjects", "session-order.inactiveProjects"]],
    ["settings.general.group.terminals", ["session-composer", "auto_close_idle_shell_terminals"]],
    ["settings.general.group.startup", ["resume_sessions_on_start"]],
  ]);
});

it("shows only the background run row in the Window group where the platform has no menu bar icon", async () => {
  const { groups } = await layout({ kind: "tauri", host: MAC_HOST, backgroundRun });
  expect(groups.find(({ key }) => key === "settings.general.group.window")?.ids).toEqual(["run-in-background"]);
});

it("shows only the menu bar icon row in the Window group where the platform cannot keep the app running", async () => {
  const { groups } = await layout({ kind: "tauri", host: MAC_HOST, statusItem });
  expect(groups.find(({ key }) => key === "settings.general.group.window")?.ids).toEqual(["menu-bar-icon"]);
});

it("leaves out the Window group in another daemon's window, whose icon has no switch of its own", async () => {
  const { setVisible: _, ...iconOnly } = statusItem;
  const { groups } = await layout({ kind: "tauri", host: MAC_HOST, statusItem: iconOnly });
  expect(groups.map(({ key }) => key)).not.toContain("settings.general.group.window");
});

it("shows the Power group only when the daemon's own host can be held awake, whatever this window's platform", async () => {
  const awake = await layout(BROWSER, { host: { ...HOST, keep_awake: true } });
  expect(awake.groups.find(({ key }) => key === "settings.general.group.power")?.ids).toEqual(["keep_awake"]);
  const notAwake = await layout(DESKTOP, { host: { ...HOST, keep_awake: false } });
  expect(notAwake.groups.map(({ key }) => key)).not.toContain("settings.general.group.power");
});

it("offers the choice of when to keep awake in the Power group once keeping awake is on", async () => {
  const { groups } = await layout(BROWSER, { host: { ...HOST, keep_awake: true }, settings: { ...DEFAULT_SETTINGS, keep_awake: true } });
  expect(groups.find(({ key }) => key === "settings.general.group.power")?.ids).toEqual(["keep_awake", "keep_awake_when"]);
});

const commandLine = { status: async () => ({ state: "notInstalled" as const }), install: async () => ({ state: "notInstalled" as const }), remove: async () => ({ state: "notInstalled" as const }) };

it("shows the Command line group, after Startup, only where the platform has the command-line tool", async () => {
  const offered = await layout({ ...DESKTOP, commandLine }, { host: { ...HOST, keep_awake: true } });
  expect(offered.groups.map(({ key }) => key)).toEqual([
    "settings.sort.group",
    "settings.general.group.terminals",
    "settings.general.group.power",
    "settings.general.group.window",
    "settings.general.group.startup",
    "settings.general.group.commandLine",
  ]);
  expect(offered.ungrouped).toEqual(["language"]);
  expect(offered.groups.find(({ key }) => key === "settings.general.group.commandLine")).toMatchObject({ title: t("settings.general.group.commandLine"), ids: ["command-line"] });
  const without = await layout(DESKTOP, { host: { ...HOST, keep_awake: true } });
  expect(without.groups.map(({ key }) => key)).not.toContain("settings.general.group.commandLine");
  expect(without.groups.flatMap(({ ids }) => ids)).not.toContain("command-line");
});

it("puts the language row before the first group, in the page's order", async () => {
  stubMatchMedia();
  const daemon = testDaemon({ state: { connectionState: "open", settings: DEFAULT_SETTINGS } });
  const container = mountHost(
    <LanguageProvider>
      <ThemeProvider>
        <SettingsReadsProvider>
          <GeneralSection />
        </SettingsReadsProvider>
      </ThemeProvider>
    </LanguageProvider>,
    { platform: BROWSER, daemon },
  );
  await act(flushPromises);
  const language = container.querySelector('[data-settings-search-id="language"]')!;
  const firstGroup = container.querySelector("[data-settings-search-group]")!;
  expect(language.compareDocumentPosition(firstGroup) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});
