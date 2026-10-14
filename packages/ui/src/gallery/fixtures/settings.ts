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

const browserState: Scenario["state"] = {
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
    };

export const settingsScenarios: Scenario[] = [
  {
    id: "settings-general",
    group: GROUP,
    title: "Browser General settings",
    description: "Browser General settings. Other sections share the same mock data and can be explored manually.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
    ],
  },
  {
    id: "settings-general-background-run",
    group: GROUP,
    title: "General, run in the background offered (desktop app)",
    description: "The desktop app's extra row starts off. Switch it on and off to read both descriptions.",
    state,
    backgroundRun: true,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
    ],
  },
  {
    id: "settings-permissions",
    group: GROUP,
    title: "Permissions",
    description: "Neither Screen Recording nor Accessibility has been granted. Press each Grant to see the refused request point at System Settings; revisiting the section offers Grant again.",
    state,
    macosPermissions: { screenRecording: false, accessibility: false, screenRecordingAtLaunch: false },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
    ],
  },
  {
    id: "settings-permissions-restart",
    group: GROUP,
    title: "Permissions, granted since launch",
    description: "Screen Recording is granted but was not when the process started, so a restart is still owed.",
    state,
    macosPermissions: { screenRecording: true, accessibility: true, screenRecordingAtLaunch: false },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
    ],
  },
  {
    id: "settings-permissions-unreadable",
    group: GROUP,
    title: "Permissions, the shell cannot be asked",
    description: "What a broken IPC looks like: the grants could not be read, on the platform that has them.",
    state,
    macosPermissions: "unreadable",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
    ],
  },
  {
    id:"settings-git",
    group:GROUP,
    title:"Settings: git",
    state:browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.git")),
    ],
  },
  {
    id:"settings-accounts",
    group:GROUP,
    title:"Settings: accounts",
    state:browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id:"settings-trustedFolders",
    group:GROUP,
    title:"Settings: trustedFolders",
    state:browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.trustedFolders")),
    ],
  },
  {
    id:"settings-notifications",
    group:GROUP,
    title:"Settings: notifications",
    state:browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.notifications")),
    ],
  },
  {
    id:"settings-configuration",
    group:GROUP,
    title:"Settings: configuration",
    state:browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.configuration")),
    ],
  },
];
