import type { Agent, AgentAvailability } from "../../protocol";
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
    steps: [(ui) => ui.press(ui.t("rail.settings"))],
  },
  {
    id: "settings-general-background-run",
    group: GROUP,
    title: "General, run in the background offered (desktop app)",
    description: "The last row, which only the desktop app has: off, as it starts.",
    state,
    backgroundRun: true,
    steps: [(ui) => ui.press(ui.t("rail.settings"))],
  },
  {
    id: "settings-general-background-run-on",
    group: GROUP,
    title: "General, run in the background on",
    state,
    backgroundRun: true,
    preferences: { runInBackground: true },
    steps: [(ui) => ui.press(ui.t("rail.settings"))],
  },
  {
    id: "settings-git",
    group: GROUP,
    title: "Git",
    state,
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.git"))],
  },
  {
    id: "settings-git-auto-sync",
    group: GROUP,
    title: "Git, automatic sync on",
    state: { ...state, settings: { auto_sync_repositories: true, default_clone_dir: "/Users/dev/Projects", accounts: [] } },
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.git"))],
  },
  {
    id: "settings-accounts",
    group: GROUP,
    title: "Agent accounts",
    description:
      "Claude Code is available, Codex is not installed and Grok Build is not yet determined. In a window shorter than the section it scrolls on its own and fades at the edge it can still be scrolled towards.",
    state: {
      ...state,
      settings: {
        auto_sync_repositories: false,
        default_clone_dir: "/Users/dev/Projects",
        accounts: [
          { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" },
          {
            id: "a-long",
            agent: "claude",
            name: "Client",
            config_dir: "/Users/dev/work/clients/acme-corporation/internal-platform/.claude-config",
          },
          { id: "a-codex", agent: "codex", name: "Personal", config_dir: "/Users/dev/.codex-personal" },
        ],
      },
      agentAvailability: new Map<Agent, AgentAvailability>([
        ["claude", { agent: "claude", availability: "available", default_account_dir: "/Users/dev/.claude" }],
        ["codex", { agent: "codex", availability: "unavailable", default_account_dir: "/Users/dev/.codex" }],
        ["grok", { agent: "grok", availability: "not_determined" }],
      ]),
    },
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.accounts"))],
  },
  {
    id: "settings-trusted-folders",
    group: GROUP,
    title: "Trusted folders",
    state,
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.trustedFolders"))],
  },
  {
    id: "settings-trusted-folders-empty",
    group: GROUP,
    title: "Trusted folders, none",
    state: { ...state, trustedDirectories: [] },
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.trustedFolders"))],
  },
  {
    id: "settings-permissions",
    group: GROUP,
    title: "Permissions",
    description: "Neither Screen Recording nor Accessibility has been granted: each row offers Grant.",
    state,
    macosPermissions: { screenRecording: false, accessibility: false, screenRecordingAtLaunch: false },
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.permissions"))],
  },
  {
    id: "settings-permissions-refused",
    group: GROUP,
    title: "Permissions, the prompts answered no",
    description:
      "Each row points at System Settings, in its own words, once a request comes back without the grant: Grant was pressed on Screen Recording and then on Accessibility.",
    state,
    macosPermissions: { screenRecording: false, accessibility: false, screenRecordingAtLaunch: false },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
      // The first Grant is Screen Recording's; once it is refused that row offers Open System Settings
      // instead, so the next Grant found is Accessibility's.
      (ui) => ui.press(ui.t("settings.permissions.grant")),
      (ui) => ui.press(ui.t("settings.permissions.grant")),
    ],
  },
  {
    id: "settings-permissions-restart",
    group: GROUP,
    title: "Permissions, granted since launch",
    description: "Screen Recording is granted but was not when the process started, so a restart is still owed.",
    state,
    macosPermissions: { screenRecording: true, accessibility: true, screenRecordingAtLaunch: false },
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.permissions"))],
  },
  {
    id: "settings-permissions-unreadable",
    group: GROUP,
    title: "Permissions, the shell cannot be asked",
    description: "What a broken IPC looks like: the grants could not be read, on the platform that has them.",
    state,
    macosPermissions: "unreadable",
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.permissions"))],
  },
  {
    id: "settings-permissions-browser",
    group: GROUP,
    title: "Permissions, in a browser",
    description: "No shell to ask, so there is nothing to manage.",
    state,
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.permissions"))],
  },
  {
    id: "settings-notifications",
    group: GROUP,
    title: "Notifications",
    description: "The browser's own permission state decides the row's text.",
    state,
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.notifications"))],
  },
];
