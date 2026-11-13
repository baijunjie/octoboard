// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { translatorFor } from "../i18n/catalog";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import { ThemeProvider } from "../preferences/theme";
import type { Event, Settings } from "../protocol";
import type { State } from "../store";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { flushPromises } from "../test/clock";
import { BROWSER_HOST, MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { matchesWidthQuery } from "../test/viewport";
import { UpdatesProvider } from "../updates/UpdatesProvider";
import { SettingsDialog } from "./SettingsDialog";
import { SettingsReadsProvider } from "./SettingsReadsProvider";
import { SETTINGS_SECTION_IDS, type SettingsSectionId } from "./sections";
import { searchReads, useSettingsReads, type SettingsReads } from "./settingsReads";
import { SETTINGS_SEARCH_INDEX, type SettingsSearchContext, type SettingsSearchEntry } from "./settingsSearch";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The list is a chunk of its own, opened only from a button.
vi.mock("../about/ThirdPartyLicensesDialog", () => ({ ThirdPartyLicensesDialog: () => null }));

const t = translatorFor("en");
const worded = (text: SettingsSearchEntry["label"]) => (typeof text === "function" ? text(t) : t(text));
/** `text` without the isolates (`isolate`) a row puts around a value it substitutes, which the index words without. */
const withoutIsolates = (text: string) => text.replace(/[\u2068\u2069]/g, "");

// A wide window, so the sections are a column beside the panel.
beforeEach(() => {
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: matchesWidthQuery(media, 1440), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--breakpoint-tablet");
    document.documentElement.style.removeProperty("--breakpoint-phone");
  });
});
const HOST = { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "settings" } as const;

/** A desktop shell with every capability a section can show a row for, a host that can be held awake and the settings
 * that show every row that depends on one; and a browser with none of them, sessions working in place. */
const CASES: { name: string; platform: PlatformAdapter; state: Partial<State> }[] = [
  {
    name: "a desktop shell with every capability",
    platform: {
      kind: "tauri",
      host: MAC_HOST,
      launchAtLogin: { status: async () => "disabled", set: async () => "disabled", openSystemSettings: async () => {} },
      backgroundRun: { set: async () => {} },
      statusItem: { setMenu: async () => {}, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} },
      macosPermissions: { status: async () => ({ screenRecording: false, accessibility: true }), request: async () => false, openSystemSettings: async () => {} },
      configurationFile: { save: async () => undefined, open: async () => undefined },
      updater: { currentVersion: async () => "1.0.0", check: async () => undefined, install: async () => {} },
      appMenu: { onSettingsRequested: () => () => {}, setLabels: async () => {} },
    },
    state: { host: { ...HOST, keep_awake: true }, settings: { ...DEFAULT_SETTINGS, keep_awake: true, git_workflow: { ...DEFAULT_SETTINGS.git_workflow, development: "worktree" } } },
  },
  {
    name: "a browser",
    platform: { kind: "browser", host: BROWSER_HOST },
    state: { settings: { ...DEFAULT_SETTINGS, git_workflow: { ...DEFAULT_SETTINGS.git_workflow, development: "in_place" } } },
  },
];

/** What the search is told of the reads, worked out by the same function the dialog uses from a read of its own on
 * the same platform, so that these tests do not state the readiness the dialog is meant to compute. */
function ReadsProbe({ onReads }: { onReads: (reads: SettingsReads) => void }): null {
  onReads(useSettingsReads());
  return null;
}

async function mountSection(section: SettingsSectionId, platform: PlatformAdapter, state: Partial<State>): Promise<{ panel: Element; reads: SettingsSearchContext["reads"] }> {
  let dialogReads!: SettingsReads;
  const daemon = testDaemon({ state: { connectionState: "open", ...state }, request: async (): Promise<Event> => ({ type: "ack", id: null }) });
  mountHost(
    <LanguageProvider>
      <ThemeProvider>
        <UpdatesProvider firstCheckDelayMs={null}>
          <SettingsDialog initialSection={section} onClose={() => {}} />
          <SettingsReadsProvider><ReadsProbe onReads={(reads) => { dialogReads = reads; }} /></SettingsReadsProvider>
        </UpdatesProvider>
      </ThemeProvider>
    </LanguageProvider>,
    { platform, daemon },
  );
  // The rows that wait on a read of the platform (launch at login, the macOS permissions, the running version).
  for (let turn = 0; turn < 3; turn += 1) await act(flushPromises);
  return { panel: document.querySelector('[role="tabpanel"]')!, reads: searchReads(dialogReads) };
}

describe.each(CASES)("in $name", ({ platform, state }) => {
  // The Permissions section is listed only where the system has privacy grants to give, the Updates one where the
  // platform updates itself (a daemon that does is the same section over another store).
  const sections = SETTINGS_SECTION_IDS.filter((id) => (id !== "permissions" || platform.host.os === "macos") && (id !== "updates" || platform.updater !== undefined));

  it.each(sections)("the %s section renders exactly the entries the search index shows for it, each in its group and worded as indexed", async (section) => {
    const { panel, reads } = await mountSection(section, platform, state);
    const context: SettingsSearchContext = { platform, keepAwake: state.host?.keep_awake === true, settings: state.settings as Settings, reads };
    const rendered = [...panel.querySelectorAll<HTMLElement>("[data-settings-search-id]")];
    const entries = SETTINGS_SEARCH_INDEX[section].filter((entry) => !entry.shown || entry.shown(context));
    expect(rendered.map((element) => element.dataset.settingsSearchId)).toEqual(entries.map((entry) => entry.id));
    for (const [index, entry] of entries.entries()) {
      const element = rendered[index]!;
      expect(element.closest<HTMLElement>("[data-settings-search-group]")?.dataset.settingsSearchGroup).toBe(entry.group);
      // A row shows its entry's name and one of its description's wordings; a block is found by a name of the
      // section's own and is left out.
      if (element.hasAttribute("data-settings-search-block")) continue;
      expect(element.textContent).toContain(worded(entry.label));
      // An entry with no description searches none: what such a row says under its name is live status.
      if (!entry.description) continue;
      // A row can follow its description with live status on a line of its own (a permission not yet granted), which
      // is not searched, so the description starts with one of the wordings.
      const description = withoutIsolates(element.querySelector("[data-setting-description]")?.textContent ?? "");
      const wordings = entry.description.map(worded);
      expect(wordings.some((wording) => description.startsWith(wording)), `"${description}" is none of ${JSON.stringify(wordings)}`).toBe(true);
    }
  });
});

/** Mounts `section` on the desktop shell with the reads of the system answering as given, and holds the section's
 * rendered entries to the index's under the readiness the dialog works out. */
async function renderedMatchesIndex(section: SettingsSectionId, overrides: Partial<PlatformAdapter>): Promise<{ rendered: (string | undefined)[]; indexed: string[] }> {
  const platform: PlatformAdapter = { ...CASES[0]!.platform, ...overrides };
  const state = CASES[0]!.state;
  const { panel, reads } = await mountSection(section, platform, state);
  const context: SettingsSearchContext = { platform, keepAwake: true, settings: state.settings as Settings, reads };
  return {
    rendered: [...panel.querySelectorAll<HTMLElement>("[data-settings-search-id]")].map((element) => element.dataset.settingsSearchId),
    indexed: SETTINGS_SEARCH_INDEX[section].filter((entry) => !entry.shown || entry.shown(context)).map((entry) => entry.id),
  };
}

describe("before the reads of the system have answered", () => {
  const pending = new Promise<never>(() => {});
  const waiting: Partial<PlatformAdapter> = {
    launchAtLogin: { status: () => pending, set: async () => "disabled", openSystemSettings: async () => {} },
    macosPermissions: { status: () => pending, request: async () => false, openSystemSettings: async () => {} },
  };

  it.each([
    ["app-behavior", "launch-at-login"],
    ["permissions", "permissions.screenRecording"],
  ] as const)("the %s section renders exactly the entries the index shows, without %s", async (section, waitingId) => {
    const { rendered, indexed } = await renderedMatchesIndex(section, waiting);
    expect(rendered).toEqual(indexed);
    expect(indexed).not.toContain(waitingId);
  });
});

describe("when the permissions cannot be read", () => {
  it("the Permissions section renders exactly the entries the index shows, the alert in place of the rows", async () => {
    const { rendered, indexed } = await renderedMatchesIndex("permissions", {
      macosPermissions: { status: () => Promise.reject(new Error("no such command")), request: async () => false, openSystemSettings: async () => {} },
    });
    expect(rendered).toEqual(indexed);
    expect(indexed).toEqual(["permissions.unreadable"]);
  });
});
