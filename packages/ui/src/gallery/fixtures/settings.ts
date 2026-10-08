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
    state: { ...state, settings: { auto_sync_repositories: true, accounts: [] } },
    steps: [(ui) => ui.press(ui.t("titleBar.settings")), (ui) => ui.press(ui.t("settings.section.git"))],
  },
  {
    id: "settings-accounts",
    group: GROUP,
    title: "Agent accounts",
    description: "Claude Code is available, Codex is not installed and Grok Build is not yet determined.",
    state: {
      ...state,
      settings: {
        auto_sync_repositories: false,
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
    steps: [(ui) => ui.press(ui.t("titleBar.settings")), (ui) => ui.press(ui.t("settings.section.accounts"))],
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
