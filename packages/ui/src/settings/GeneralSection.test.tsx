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
const HOST = { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "settings" } as const;

const launchAtLogin = { status: async () => "disabled" as const, set: async () => "disabled" as const, openSystemSettings: async () => {} };
const backgroundRun = { set: async () => {} };
const statusItem = { setMenu: async () => {}, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };

const stubMatchMedia = () => {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => void vi.unstubAllGlobals());
};

/** What the section shows: the rows outside any group, then each group's heading and rows, in document order. */
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

it("shows the language row on its own at the top, then Startup, Window, Power, Terminals and Sorting, where the platform and the daemon's host offer everything", async () => {
  const { ungrouped, groups } = await layout(DESKTOP, { host: { ...HOST, keep_awake: true } });
  expect(ungrouped).toEqual(["language"]);
  expect(groups.map(({ key }) => key)).toEqual([
    "settings.general.group.startup",
    "settings.general.group.window",
    "settings.general.group.power",
    "settings.general.group.terminals",
    "settings.sort.group",
  ]);
  expect(groups.map(({ title }) => title)).toEqual([
    t("settings.general.group.startup"),
    t("settings.general.group.window"),
    t("settings.general.group.power"),
    t("settings.general.group.terminals"),
    t("settings.sort.group"),
  ]);
  expect(groups.map(({ ids }) => ids)).toEqual([
    ["launch-at-login", "resume_sessions_on_start"],
    ["run-in-background", "menu-bar-icon"],
    ["keep_awake"],
    ["auto_close_idle_shell_terminals"],
    ["session-order.sessions", "session-order.activeProjects", "session-order.inactiveProjects"],
  ]);
});

it("leaves out the Window and Power groups and the launch-at-login row in a browser on a host that cannot be held awake", async () => {
  const { ungrouped, groups } = await layout(BROWSER);
  expect(ungrouped).toEqual(["language"]);
  expect(groups.map(({ key, ids }) => [key, ids])).toEqual([
    ["settings.general.group.startup", ["resume_sessions_on_start"]],
    ["settings.general.group.terminals", ["auto_close_idle_shell_terminals"]],
    ["settings.sort.group", ["session-order.sessions", "session-order.activeProjects", "session-order.inactiveProjects"]],
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
