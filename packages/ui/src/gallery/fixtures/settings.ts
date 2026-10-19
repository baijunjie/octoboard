import { AGENT_LABEL } from "../../agents";
import type { Agent, AgentAvailability } from "../../protocol";
import type { Scenario } from "../scenario";
import { SAMPLE, minutesAgo, snapshotState } from "./builders";
import { quotaFailure, quotaOf, quotaWindow } from "./quotas";

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
    auto_delete_unresumable_archives: true,
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

/** Every agent installed and its readings in, one tab each: Claude Code's default account and Work have their plan
 * and windows (Work's spent), Client has no reading yet while its agent is available, Codex's accounts have a plan
 * and two windows each but Automation's, which has no plan, and Grok has one credit pool. Claude Code's default
 * account is set to automatic, Work to approve everything and Client follows its configuration. */
const quotaState: Scenario["state"] = {
  ...approvalState,
  settings: {
    ...approvalState.settings!,
    claude_default_approval_mode: "auto",
    accounts: approvalState.settings!.accounts.map((account) => account.id === "a-work" ? { ...account, approval_mode: "bypass" as const } : account),
  },
  agentAvailability: new Map(approvalState.agentAvailability).set("grok", { agent: "grok", availability: "available", default_account_dir: "/Users/dev/.grok" }),
  accountQuotas: [
    quotaOf("claude", null, "max", [
      quotaWindow("session", 18, 117),
      quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60),
      quotaWindow("weekly_model", 0, null, "Fable"),
    ]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 42), quotaWindow("weekly", 137, 2 * 24 * 60 + 5 * 60)]),
    quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
    quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)]),
    quotaOf("codex", "a-codex-auto", null, [quotaWindow("session", 3, 295), quotaWindow("weekly", 9, 6 * 24 * 60)]),
    quotaOf("codex", "a-codex-user", "plus", [quotaWindow("session", 91, 25), quotaWindow("weekly", 78, 2 * 24 * 60 + 11 * 60)]),
    quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 23, 4 * 24 * 60 + 11 * 60)]),
  ],
};

/** Claude Code with more owned accounts than the dialog is tall: the default account, Work and Client, then eight
 * more, each with a plan and two windows so every row is as tall as a real one. */
const MANY_CLAUDE_ACCOUNTS = ["Research", "Billing", "Support", "Docs", "Staging", "Mobile", "Data", "Infra"] as const;

const manyAccountsState: Scenario["state"] = {
  ...quotaState,
  settings: {
    ...quotaState.settings!,
    accounts: [
      ...quotaState.settings!.accounts,
      ...MANY_CLAUDE_ACCOUNTS.map((name) => ({ id: `a-${name.toLowerCase()}`, agent: "claude" as const, name, config_dir: `/Users/dev/.claude-${name.toLowerCase()}` })),
    ],
  },
  accountQuotas: [
    ...quotaState.accountQuotas!,
    ...MANY_CLAUDE_ACCOUNTS.map((name, index) =>
      quotaOf("claude", `a-${name.toLowerCase()}`, index % 3 === 0 ? "max" : "pro", [
        quotaWindow("session", (index * 23 + 11) % 100, 30 + index * 31),
        quotaWindow("weekly", (index * 37 + 8) % 100, (index + 1) * 24 * 60 + 90),
      ])),
  ],
};

/** The readings that did not come out whole, on the tabs of Claude Code and Grok. Codex is not installed, so its
 * accounts have no entry and Personal says it is not usable; Grok's binary went missing from the login shell's PATH. */
const unavailableQuotaState: Scenario["state"] = {
  ...browserState,
  agentAvailability: new Map(browserState.agentAvailability).set("grok", { agent: "grok", availability: "available", default_account_dir: "/Users/dev/.grok" }),
  accountQuotas: [
    quotaOf("claude", null, null, [], { unavailable: quotaFailure("quota_timed_out", { agent: "Claude Code" }, "Claude Code took too long to report its quota.") }),
    quotaOf("claude", "a-work", null, [], {
      unavailable: quotaFailure("quota_command_failed", { agent: "Claude Code", detail: "exit status 1: Error: not logged in" }, "Claude Code could not report its quota: exit status 1: Error: not logged in"),
    }),
    quotaOf("claude", "a-long", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)], {
      fetched_at: minutesAgo(12),
      unavailable: quotaFailure("quota_timed_out", { agent: "Claude Code" }, "Claude Code took too long to report its quota."),
    }),
    quotaOf("grok", null, null, [], {
      unavailable: quotaFailure("quota_binary_not_found", { binary: "grok" }, "grok was not found, so the quota cannot be read."),
    }),
  ],
};

/** One window wide enough to wrap: a model's display name as long as one can be. */
const NARROW_MODEL = "Claude Opus Extended Context Research Preview";

/** For the narrow layout: Claude Code's default account with a long model name, and Codex's default account and
 * Personal. */
const narrowQuotaState: Scenario["state"] = {
  ...quotaState,
  settings: { ...approvalState.settings!, accounts: approvalState.settings!.accounts.filter((account) => account.id === "a-codex") },
  accountQuotas: [
    quotaOf("claude", null, "max", [
      quotaWindow("session", 18, 117),
      quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60),
      quotaWindow("weekly_model", 27, 3 * 24 * 60 + 4 * 60, NARROW_MODEL),
    ]),
    quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
    quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)]),
  ],
};

/** How far the many-accounts section is scrolled: the default account, Work and a few rows more have gone under the strip. */
const SCROLLED_UNDER_STRIP = 420;

/** Past any section's end, which the scroller clamps to. */
const SCROLLED_TO_END = 10_000;

/** Claude Code's tab, Client's approval selector set to Approve everything: the confirmation dialog it opens. */
const bypassSteps: Scenario["steps"] = [
  (ui) => ui.press(ui.t("rail.settings")),
  (ui) => ui.press(ui.t("settings.section.accounts")),
  (ui) => ui.press(ui.t("settings.accounts.approval.label", { name: "Client", agent: AGENT_LABEL.claude })),
  (ui) => ui.press(ui.t("settings.accounts.approval.claudeBypass")),
];

export const settingsScenarios: Scenario[] = [
  {
    id: "settings-general",
    group: GROUP,
    category: "normal",
    title: "Browser General settings",
    description: "The Markdown style row sits between Appearance and Language and starts on Tailwind. The clone-directory explanation describes when the chosen directory is used; the path appears in the input. The archive-cleanup switch sits right below it, on, with the description that says what is deleted and when.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
    ],
  },
  {
    id: "settings-general-archive-cleanup-off",
    group: GROUP,
    category: "normal",
    title: "General, archive cleanup switched off",
    description: "The archive-cleanup switch below the clone-directory row is off, and its description now starts with \"Once this is on\" instead of stating the behaviour.",
    state: { ...browserState, settings: { ...browserState.settings!, auto_delete_unresumable_archives: false } },
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
    description: "Automatically pull repositories shows its first two sentences under the label, and a small muted \"?\" at the end of the label holds the sentence about the remote being checked. It is the only row on this page with a help tip.",
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
    title: "Settings: accounts, Claude Code tab",
    description: "The agents are tabs in an underline strip, unlike the filled tabs that switch the Settings sections, and Claude Code is open. The default account's name line ends with a small muted \"?\" after Default and its plan chip, with only its config directory beneath. Each account's plan sits beside its name, with its quota windows beneath: a headline with what remains, the countdown at the end of its line and a bar of the same remainder in the agent's colour; the model window has no countdown. Work is spent: 0% remaining, also where use went past 100%. Client has no quota yet and says it is being read. A line under the tab strip describes the approval selector each account has: the default account is set to Automatic, Work to Approve everything and Client follows its account configuration. Remove Work or Client: the account fades out while sliding to the right while the remaining accounts slide into place.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-default-help",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, default account help tip open",
    description: "Keyboard focus is on the \"?\" at the end of the default account's name line, so its tooltip is open above it, explaining what the default account is. Without the tip open, the \"?\" sits at rest in the Claude Code scenario.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.focusControl(ui.t("settings.accounts.defaultHelp")),
    ],
  },
  {
    id: "settings-accounts-default-help-popover",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, default account help popover open",
    description: "The \"?\" at the end of the default account's name line has been pressed, so the explanation stays open in a popover above it, with no tooltip on top of it; unlike the tooltip, it stays until Escape or an outside press.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.defaultHelp")),
    ],
  },
  {
    id: "settings-accounts-many",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code tab scrolled with many accounts",
    description: "The section is scrolled down, so the agent tab strip is pinned at the top of it with its bottom rule and the Claude Code indicator showing, on the dialog's own surface and not dimmed by the fade below it; account rows scroll away underneath it and are cut off cleanly against its rule.",
    state: manyAccountsState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.scrollPanel(ui.t("settings.section.accounts"), SCROLLED_UNDER_STRIP),
    ],
  },
  {
    id: "settings-accounts-codex",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Codex tab",
    description: "Each account has a plan, two quota windows and an approval selector; Automation has no plan, so no plan label. The default account and Personal follow their account configuration, Automation uses automatic review and Production asks you; change any approval mode to save it independently.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.codex),
    ],
  },
  {
    id: "settings-accounts-grok",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Grok Build tab",
    description: "Grok's one credit pool is a weekly window, with no approval selector.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.grok),
    ],
  },
  {
    id: "settings-accounts-quota-narrow",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code tab in a narrow window",
    description: "Below 640px the windows stack in one column and a long model name wraps under its own headline, the countdown staying at the end of the first line. The default account's approval selector drops beneath its name. The tab strip fits the width.",
    width: 560,
    state: narrowQuotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-many-narrow",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code tab scrolled with many accounts in a narrow window",
    description: "Below 640px the sections scroll too: with the section scrolled down, the agent tab strip is pinned at the top of it and account rows pass beneath, as in the wide window. The Settings sections are tabs in a row above, which stays put.",
    width: 560,
    state: manyAccountsState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.scrollPanel(ui.t("settings.section.accounts"), SCROLLED_UNDER_STRIP),
    ],
  },
  {
    id: "settings-accounts-codex-narrow",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Codex tab in a narrow window",
    description: "Below 640px Codex's approval selector and the account buttons drop beneath the name. The section is scrolled to the end, so Personal's selector and its Edit and Remove buttons are in view under the pinned tab strip.",
    width: 560,
    state: narrowQuotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.codex),
      (ui) => ui.scrollPanel(ui.t("settings.section.accounts"), SCROLLED_TO_END),
    ],
  },
  {
    id: "settings-account-approval-bypass",
    group: GROUP,
    category: "normal",
    title: "Approve everything confirmation",
    description: "Choosing Approve everything for Client opens a destructive confirmation instead of saving: the account name is a chip in the title, and the red button repeats the choice. Cancelling leaves Client on Follow account configuration.",
    state: quotaState,
    steps: bypassSteps,
  },
  {
    id: "settings-account-approval-error",
    group: GROUP,
    category: "exceptional",
    title: "Account approval mode could not be saved",
    description: "The error alert requires acknowledgement of the failed save. Codex's default account keeps Follow account configuration; retrying leaves the saved selection intact.",
    state: approvalState,
    accountApprovalError: { code: "store_io_failed", params: { detail: "Permission denied (os error 13)" }, message: "The account settings could not be saved: Permission denied (os error 13)" },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.codex),
      (ui) => ui.press(ui.t("settings.accounts.approval.label", { name: ui.t("settings.accounts.defaultName"), agent: AGENT_LABEL.codex })),
      (ui) => ui.press(ui.t("settings.accounts.approval.auto")),
    ],
  },
  {
    id: "settings-account-approval-bypass-error",
    group: GROUP,
    category: "exceptional",
    title: "Approve everything could not be saved",
    description: "Confirming Approve everything for Client was refused, so the error alert is open, with the confirmation dialog still open beneath it; Client still follows its account configuration.",
    state: quotaState,
    accountApprovalError: { code: "store_io_failed", params: { detail: "Permission denied (os error 13)" }, message: "The account settings could not be saved: Permission denied (os error 13)" },
    steps: [
      ...bypassSteps,
      (ui) => ui.confirm(),
    ],
  },
  {
    id: "settings-accounts-quota-unavailable",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, Claude Code quota unavailable or out of date",
    description: "Claude Code's default account and Work have nothing to show: \"Quota unavailable\" with the reason, one a timeout and one a failed command with its detail. Client keeps its last windows under \"Out of date\", with when they were read and why the newer attempt failed above them.",
    state: unavailableQuotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-grok-quota-unavailable",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, Grok Build binary not found",
    description: "Grok's reason names the missing binary and PATH as code.",
    state: unavailableQuotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.grok),
    ],
  },
  {
    id: "settings-accounts-codex-not-installed",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, Codex not installed",
    description: "The panel opens with \"Codex (not installed)\" above its accounts. Neither the default account nor Personal has a quota or a plan, and Personal says it is not usable.",
    state: unavailableQuotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.codex),
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
  ...(["owned", "default"] as const).map((kind): Scenario => ({
    id: `settings-account-directory-taken-${kind}`,
    group: GROUP,
    category: "exceptional",
    title: kind === "owned" ? "Add account, config directory used by another account" : "Add account, config directory used by the default account",
    description: "Once Add is pressed, the daemon's refusal is worded under the config directory field, naming the account that already uses it; the dialog's bottom error stays hidden.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.add")),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.type("Studio"),
      (ui) => ui.focus(ui.t("settings.accounts.configDir")),
      (ui) => ui.type(kind === "owned" ? "~/.claude-work/" : "/Users/dev/.claude"),
      (ui) => ui.confirm(),
    ],
  })),
  {
    id: "settings-trustedFolders",
    group: GROUP,
    category: "normal",
    title: "Settings: trustedFolders",
    description: "Each folder row is only its path and Remove, with no second line. Remove the first trusted folder and confirm: its row fades out while sliding to the right while the remaining path slides into place.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.trustedFolders")),
    ],
  },
  {
    id: "settings-trustedFolders-remove-confirm",
    group: GROUP,
    category: "normal",
    title: "Remove a trusted folder confirmation",
    description: "Remove on the first folder asks first: the title is \"Remove trusted folder\" followed by the home-abbreviated path (~/code) as a chip, the message says trust confirmations under it are asked about again unless the project's own permission or another trusted folder covers them and then what is kept, and the confirming button is destructive.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.trustedFolders")),
      (ui) => ui.press(ui.t("settings.trusted.removeLabel", { path: "/Users/dev/code" })),
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
