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
    title: "Browser settings and accounts",
    description: "General, Git, Notifications, Permissions (macOS-only here), Trusted folders and Configuration share these browser settings. Git sync switches on and off. Claude Code accounts are available, Codex is absent and Grok Build undetermined; one account path wraps, and Short makes its section scroll. Remove the two trusted paths to reach the empty section.",
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
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.git")), (ui) => ui.press(ui.t("settings.git.autoSync.label")), (ui) => ui.press(ui.t("settings.git.autoSync.label")), (ui) => ui.press(ui.t("settings.section.general"))],
  },
  {
    id: "settings-general-background-run",
    group: GROUP,
    title: "General, run in the background offered (desktop app)",
    description: "The desktop app's extra row starts off. Switch it on and off to read both descriptions.",
    state,
    backgroundRun: true,
    steps: [(ui) => ui.press(ui.t("rail.settings"))],
  },
  {
    id: "settings-permissions",
    group: GROUP,
    title: "Permissions",
    description: "Neither Screen Recording nor Accessibility has been granted. Press each Grant to see the refused request point at System Settings; revisiting the section offers Grant again.",
    state,
    macosPermissions: { screenRecording: false, accessibility: false, screenRecordingAtLaunch: false },
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.permissions"))],
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
];
