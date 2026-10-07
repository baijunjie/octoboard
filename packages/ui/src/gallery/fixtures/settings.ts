import type { Scenario } from "../scenario";
import { SAMPLE, snapshotState } from "./builders";

const GROUP = "Settings";

const { console: console_, web, api, sessions } = SAMPLE;

const state = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions,
  trustedDirectories: ["/Users/dev/code", "/Users/dev/work/clients/acme-corporation/internal-platform"],
});

export const settingsScenarios: Scenario[] = [
  {
    id: "settings-general",
    group: GROUP,
    title: "General",
    state,
    steps: [(ui) => ui.press(ui.t("titleBar.settings"))],
  },
  {
    id: "settings-git",
    group: GROUP,
    title: "Git",
    state,
    steps: [(ui) => ui.press(ui.t("titleBar.settings")), (ui) => ui.press(ui.t("settings.section.git"))],
  },
  {
    id: "settings-git-auto-sync",
    group: GROUP,
    title: "Git, automatic sync on",
    state: { ...state, settings: { auto_sync_repositories: true } },
    steps: [(ui) => ui.press(ui.t("titleBar.settings")), (ui) => ui.press(ui.t("settings.section.git"))],
  },
  {
    id: "settings-trusted-folders",
    group: GROUP,
    title: "Trusted folders",
    state,
    steps: [(ui) => ui.press(ui.t("titleBar.settings")), (ui) => ui.press(ui.t("settings.section.trustedFolders"))],
  },
  {
    id: "settings-trusted-folders-empty",
    group: GROUP,
    title: "Trusted folders, none",
    state: { ...state, trustedDirectories: [] },
    steps: [(ui) => ui.press(ui.t("titleBar.settings")), (ui) => ui.press(ui.t("settings.section.trustedFolders"))],
  },
  {
    id: "settings-notifications",
    group: GROUP,
    title: "Notifications",
    description: "The browser's own permission state decides the row's text.",
    state,
    steps: [(ui) => ui.press(ui.t("titleBar.settings")), (ui) => ui.press(ui.t("settings.section.notifications"))],
  },
];
