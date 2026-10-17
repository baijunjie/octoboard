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
    auto_pull_repositories: false,
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

const approvalState: Scenario["state"] = {
  ...browserState,
  settings: {
    ...browserState.settings!,
    accounts: [
      ...browserState.settings!.accounts,
      { id: "a-codex-auto", agent: "codex", name: "Automation", config_dir: "/Users/dev/.codex-automation", approval_reviewer: "auto_review" },
      { id: "a-codex-user", agent: "codex", name: "Production", config_dir: "/Users/dev/.codex-production", approval_reviewer: "user" },
    ],
  },
  agentAvailability: new Map(browserState.agentAvailability).set("codex", { agent: "codex", availability: "available", default_account_dir: "/Users/dev/.codex" }),
};

export const settingsScenarios: Scenario[] = [
  {
    id: "settings-general",
    group: GROUP,
    category: "normal",
    title: "Browser General settings",
    description: "The Markdown style row sits between Appearance and Language and starts on Tailwind. The clone-directory explanation describes when the chosen directory is used; the path appears in the input.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
    ],
  },
  {
    id: "settings-general-markdown-style-open",
    group: GROUP,
    category: "normal",
    title: "General, Markdown style drop-down open",
    description: "The four styles are listed by name, in every language, with Tailwind marked as the current one.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.markdownStyle.label")),
    ],
  },
  {
    id: "settings-general-background-run",
    group: GROUP,
    category: "normal",
    title: "General, run in the background offered (desktop app)",
    description: "The desktop app's background row starts off; switch it on and off to read both descriptions.",
    state,
    backgroundRun: true,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
    ],
  },
  {
    id: "settings-permissions",
    group: GROUP,
    category: "normal",
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
    category: "normal",
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
    category: "exceptional",
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
    id: "settings-git",
    group: GROUP,
    category: "normal",
    title: "Settings: git",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.git")),
    ],
  },
  {
    id: "settings-accounts",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts",
    description: "Codex's default account and Personal follow their account configuration; Automation uses automatic review and Production asks you. Change any approval mode to save it independently. Claude Code and Grok have no approval selector. Remove Work or Client: the account fades out while sliding to the right while the remaining accounts slide into place.",
    state: approvalState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-account-approval-error",
    group: GROUP,
    category: "exceptional",
    title: "Account approval mode could not be saved",
    description: "The error toast explains the failed save. Codex's default account keeps Follow account configuration; retrying leaves the saved selection intact.",
    state: approvalState,
    accountApprovalError: { code: "store_io_failed", params: { detail: "Permission denied" }, message: "The account settings could not be saved: Permission denied" },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.approval.label", { name: ui.t("settings.accounts.defaultName") })),
      (ui) => ui.press(ui.t("settings.accounts.approval.auto")),
    ],
  },
  {
    id: "settings-account-delete",
    group: GROUP,
    category: "normal",
    title: "Remove account confirmation",
    description: "The account name appears as a chip in the confirmation title.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.removeLabel", { name: "Work", agent: "Claude Code" })),
    ],
  },
  ...(["existing", "default"] as const).map((kind): Scenario => ({
    id: `settings-account-name-taken-${kind}`,
    group: GROUP,
    category: "exceptional",
    title: kind === "existing" ? "Add account, name already in use" : "Add account, default account name reserved",
    description: "The conflicting account name is a chip in the field's validation message.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.add")),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.type(kind === "existing" ? "Work" : ui.t("settings.accounts.defaultName")),
      (ui) => ui.focus(ui.t("settings.accounts.configDir")),
      (ui) => ui.type("/Users/dev/.claude-studio"),
      (ui) => ui.confirm(),
    ],
  })),
  {
    id: "settings-account-directory-relative",
    group: GROUP,
    category: "exceptional",
    title: "Add account, config directory not absolute",
    description: "Once Add is pressed, the relative config directory is marked invalid under its field, with the home prefix an absolute path may start with set as code in the danger tone.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.add")),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.type("Studio"),
      (ui) => ui.focus(ui.t("settings.accounts.configDir")),
      (ui) => ui.type("claude-studio"),
      (ui) => ui.confirm(),
    ],
  },
  {
    id: "settings-trustedFolders",
    group: GROUP,
    category: "normal",
    title: "Settings: trustedFolders",
    description: "Remove the first trusted folder: its row fades out while sliding to the right while the remaining path slides into place.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.trustedFolders")),
    ],
  },
  {
    id: "settings-notifications",
    group: GROUP,
    category: "normal",
    title: "Settings: notifications",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.notifications")),
    ],
  },
  {
    id: "settings-configuration",
    group: GROUP,
    category: "normal",
    title: "Settings: configuration",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.configuration")),
    ],
  },
];
