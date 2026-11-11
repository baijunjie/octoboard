import { describe, expect, it } from "vitest";

import { booleanSetting } from "../configuration/booleanSettings";
import { DEVELOPMENT_CHOICES } from "../git/gitWorkflow";
import { translatorFor } from "../i18n/catalog";
import type { PlatformAdapter } from "../platform";
import type { Settings } from "../protocol";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { ALL_COMMANDS } from "../shortcuts/commands";
import { BROWSER_HOST, MAC_HOST } from "../test/hosts";
import { queryWords, sectionMatches, SETTINGS_SEARCH_INDEX, type SettingsSearchContext } from "./settingsSearch";
import type { SettingsSectionId } from "./sections";

const t = translatorFor("en");

const BROWSER: PlatformAdapter = { kind: "browser", host: BROWSER_HOST };
const DESKTOP: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  launchAtLogin: { status: async () => "disabled", set: async () => "disabled", openSystemSettings: async () => {} },
  appMenu: { onSettingsRequested: () => () => {}, setLabels: async () => {} },
};

const context = (overrides: Partial<SettingsSearchContext> = {}): SettingsSearchContext => ({ platform: BROWSER, keepAwake: false, settings: DEFAULT_SETTINGS, ...overrides });
const withSettings = (changes: Partial<Settings>): Settings => ({ ...DEFAULT_SETTINGS, ...changes });

const SECTION_LABELS: Record<SettingsSectionId, Parameters<typeof sectionMatches>[1]> = {
  general: "settings.section.general",
  "app-behavior": "settings.section.appBehavior",
  git: "settings.section.git",
  accounts: "settings.section.accounts",
  "agent-permissions": "settings.section.agentPermissions",
  "trusted-folders": "settings.section.trustedFolders",
  permissions: "settings.section.permissions",
  notifications: "settings.section.notifications",
  shortcuts: "settings.section.shortcuts",
  cleanup: "settings.section.cleanup",
  configuration: "settings.section.configuration",
  about: "settings.section.about",
};

/** What `query` finds in `section`, worded in English. */
function find(section: SettingsSectionId, query: string, given: SettingsSearchContext = context()) {
  return sectionMatches(section, SECTION_LABELS[section], queryWords(query, "en"), t, "en", given);
}

describe("queryWords", () => {
  it("trims the query, folds its case and splits it into words", () => {
    expect(queryWords("  Git   WORKFLOW\t", "en")).toEqual(["git", "workflow"]);
  });

  it("gives no words for an empty or blank query", () => {
    expect(queryWords("", "en")).toEqual([]);
    expect(queryWords(" \t\n ", "en")).toEqual([]);
  });

  it("folds the case by the rules of the given language", () => {
    expect(queryWords("I", "tr")).toEqual(["ı"]);
    expect(queryWords("I", "en")).toEqual(["i"]);
  });
});

describe("sectionMatches", () => {
  it("finds an entry by its name, whatever the case of either side", () => {
    // "Appearance" is worded with a capital in the catalog; the query is folded by `queryWords`.
    expect([...find("general", "appearance").ids]).toEqual(["appearance"]);
    expect([...find("general", "APPEARANCE").ids]).toEqual(["appearance"]);
  });

  it("finds an entry by a word of its description", () => {
    expect(find("general", "operating system").ids.has("appearance")).toBe(true);
  });

  it("finds every entry of a section by the section's name", () => {
    expect(find("general", "general").ids.size).toBe(SETTINGS_SEARCH_INDEX.general.length);
  });

  it("finds the entries of a group by its heading, and reports the group", () => {
    const { ids, groups } = find("general", "sorting");
    expect([...ids]).toEqual(["session-order.sessions", "session-order.activeProjects", "session-order.inactiveProjects"]);
    expect([...groups]).toEqual(["settings.sort.group"]);
  });

  it("reports no group for an entry that sits in none", () => {
    expect(find("general", "appearance").groups.size).toBe(0);
  });

  it("matches only an entry that has every word, wherever each is found in its text", () => {
    // Section name and group heading together.
    expect(find("general", "general sorting").ids.size).toBe(3);
    // A word of the name and one of the description.
    expect(find("general", "appearance light").ids.has("appearance")).toBe(true);
    // The Appearance row has no group, so it cannot have "sorting" in its text.
    expect(find("general", "appearance sorting").ids.size).toBe(0);
    expect(find("general", "appearance zzzznope").ids.size).toBe(0);
  });

  it("finds nothing for a word that is nowhere in the section", () => {
    const { ids, groups } = find("general", "zzzznope");
    expect(ids.size).toBe(0);
    expect(groups.size).toBe(0);
  });

  describe("an entry that is shown only on some platforms or conditions", () => {
    it("is counted only where the platform gives it", () => {
      expect(find("app-behavior", "launch at login", context({ platform: BROWSER })).ids.has("launch-at-login")).toBe(false);
      expect(find("app-behavior", "launch at login", context({ platform: DESKTOP })).ids.has("launch-at-login")).toBe(true);
    });

    it("does not count a section's whole set by its name where the platform has none of it", () => {
      expect(find("configuration", "configuration", context({ platform: BROWSER })).ids.size).toBe(0);
      expect(find("configuration", "configuration", context({ platform: { ...DESKTOP, configurationFile: { save: async () => undefined, open: async () => undefined } } })).ids.size).toBe(2);
    });

    it("counts the sleep switch only on a host that can be held awake, and its choice only while it is on", () => {
      expect(find("app-behavior", "prevent sleep", context({ keepAwake: false })).ids.has("keep_awake")).toBe(false);
      expect(find("app-behavior", "prevent sleep", context({ keepAwake: true })).ids.has("keep_awake")).toBe(true);
      expect(find("app-behavior", "when to prevent sleep", context({ keepAwake: true, settings: withSettings({ keep_awake: false }) })).ids.has("keep_awake_when")).toBe(false);
      expect(find("app-behavior", "when to prevent sleep", context({ keepAwake: true, settings: withSettings({ keep_awake: true }) })).ids.has("keep_awake_when")).toBe(true);
    });

    it("counts the git items that shape a branch only while sessions work on a branch of their own", () => {
      const inPlace = context({ settings: withSettings({ git_workflow: { ...DEFAULT_SETTINGS.git_workflow, development: "in_place" } }) });
      const worktree = context({ settings: withSettings({ git_workflow: { ...DEFAULT_SETTINGS.git_workflow, development: "worktree" } }) });
      for (const id of ["git_workflow.squash_before_delivery", "git_workflow.rebase_before_delivery", "git_workflow.delivery"] as const) {
        expect(find("git", "development", inPlace).ids.has(id)).toBe(false);
        expect(find("git", "development", worktree).ids.has(id)).toBe(true);
      }
      expect(find("git", "development", inPlace).ids.has("git_workflow.development")).toBe(true);
    });

    it("counts a shortcut only where the client has it", () => {
      const desktopOnly = ALL_COMMANDS.find((command) => command.availability === "app")!;
      const query = t(desktopOnly.label);
      expect([...find("shortcuts", query, context({ platform: BROWSER })).ids]).not.toContain(`shortcut.${desktopOnly.id}`);
      expect([...find("shortcuts", query, context({ platform: DESKTOP })).ids]).toContain(`shortcut.${desktopOnly.id}`);
    });
  });

  describe("an entry whose wording depends on a value", () => {
    // Changing the value while searching must not take the row out from under the user, so every wording counts.
    it("is found by the description of either position of a Git workflow switch, whichever it is in", () => {
      const gateOn = context({ settings: withSettings({ git_workflow: { ...DEFAULT_SETTINGS.git_workflow, revert_gate: true } }) });
      const gateOff = context({ settings: withSettings({ git_workflow: { ...DEFAULT_SETTINGS.git_workflow, revert_gate: false } }) });
      for (const wording of [t("settings.gitWorkflow.revertGate.description.on"), t("settings.gitWorkflow.revertGate.description.off")]) {
        expect(find("git", wording, gateOn).ids.has("git_workflow.revert_gate")).toBe(true);
        expect(find("git", wording, gateOff).ids.has("git_workflow.revert_gate")).toBe(true);
      }
    });

    it("is found by the description of either position of a daemon switch, whichever it is in", () => {
      const { description } = booleanSetting("resume_sessions_on_start");
      for (const value of [true, false]) {
        const given = context({ settings: withSettings({ resume_sessions_on_start: value }) });
        expect(find("app-behavior", t(description.on), given).ids.has("resume_sessions_on_start")).toBe(true);
        expect(find("app-behavior", t(description.off), given).ids.has("resume_sessions_on_start")).toBe(true);
      }
    });

    it("is found by the description of every sleep choice, whichever is made", () => {
      for (const when of ["always", "working"] as const) {
        const given = context({ keepAwake: true, settings: withSettings({ keep_awake: true, keep_awake_when: when }) });
        expect(find("app-behavior", t("settings.keepAwake.when.description.always"), given).ids.has("keep_awake_when")).toBe(true);
        expect(find("app-behavior", t("settings.keepAwake.when.description.working"), given).ids.has("keep_awake_when")).toBe(true);
      }
    });

    it("is found by the description of every place to develop, whichever is chosen", () => {
      for (const development of DEVELOPMENT_CHOICES.map(({ value }) => value)) {
        const given = context({ settings: withSettings({ git_workflow: { ...DEFAULT_SETTINGS.git_workflow, development } }) });
        for (const choice of DEVELOPMENT_CHOICES) expect(find("git", t(choice.description), given).ids.has("git_workflow.development")).toBe(true);
      }
    });

    it("is not found by the text of its help tip, which the row does not show", () => {
      const { help } = booleanSetting("auto_pull_repositories");
      expect(help).toBeDefined();
      expect(find("git", t(help!.text)).ids.has("auto_pull_repositories")).toBe(false);
    });
  });

  describe("lists of the user's own things", () => {
    it("finds the agent accounts as one block, by the section's name and by Agents", () => {
      for (const query of ["agent accounts", "agents", "accounts agents"]) {
        expect([...find("accounts", query).ids]).toContain("accounts.agents");
      }
    });

    it("finds the trusted folders as one block, by the section's name and by its intro", () => {
      const intro = t("settings.trusted.description");
      expect([...find("trusted-folders", "trusted folders").ids]).toEqual(["trusted-folders.list"]);
      expect([...find("trusted-folders", intro).ids]).toEqual(["trusted-folders.list"]);
    });
  });

  describe("an entry whose description is live status", () => {
    it("finds the notifications row by its name, with no group", () => {
      const { ids, groups } = find("notifications", "waiting");
      expect([...ids]).toEqual(["notifications"]);
      expect(groups.size).toBe(0);
    });

    it("finds the updates version row by its name and by its section, and not by a word that is not its name", () => {
      const updater = context({ platform: { ...DESKTOP, updater: { currentVersion: async () => "1.0.0", check: async () => undefined, install: async () => {} } } });
      expect(find("about", "updates", updater).ids.has("updates.version")).toBe(true);
      expect(find("about", "about", updater).ids.has("updates.version")).toBe(true);
      expect(find("about", "updates zzzznope", updater).ids.has("updates.version")).toBe(false);
    });
  });

  describe("the keyboard shortcuts", () => {
    it("finds a shortcut by its command's name", () => {
      const command = ALL_COMMANDS.find((candidate) => candidate.availability === "always")!;
      const found = find("shortcuts", t(command.label), context({ platform: DESKTOP }));
      expect([...found.ids]).toContain(`shortcut.${command.id}`);
      // Only commands that carry every word of the name are found, not the whole section.
      expect(found.ids.size).toBeLessThan(SETTINGS_SEARCH_INDEX.shortcuts.length);
    });

    it("finds the commands of a group by its heading", () => {
      const available = ALL_COMMANDS.filter((command) => command.group === "navigation" && (command.availability === "always" || command.availability === "app"));
      const found = find("shortcuts", t("settings.shortcuts.group.navigation"), context({ platform: DESKTOP }));
      for (const command of available) expect([...found.ids]).toContain(`shortcut.${command.id}`);
      expect([...found.groups]).toContain("settings.shortcuts.group.navigation");
    });
  });
});
