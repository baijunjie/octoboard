import { AGENT_LABEL } from "../../agents";
import type { AccountLogin, Agent, AgentAvailability } from "../../protocol";
import type { Scenario } from "../scenario";
import { SAMPLE, minutesAgo, snapshotState } from "./builders";
import { quotaFailure, quotaOf, quotaWindow } from "./quotas";

const GROUP = "Settings";

const { console: console_, web, api, sessions } = SAMPLE;

/** Every agent, as the list of added agents of a user who has added them all. */
const ALL_AGENTS: Agent[] = ["claude", "codex", "grok"];

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
    added_agents: ["claude", "codex"],
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

/** The list of added agents still undecided: the daemon has not yet determined which agents are installed, so it has
 * no availability to show either. Settings lists every agent, none of them removable or movable. */
const undecidedState: Scenario["state"] = {
  ...browserState,
  settings: { ...browserState.settings!, added_agents: null },
  agentAvailability: new Map(),
};

/** The user has removed every agent: the accounts they set up are kept, but none is listed. */
const noAgentsState: Scenario["state"] = {
  ...browserState,
  settings: { ...browserState.settings!, added_agents: [] },
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
    added_agents: ALL_AGENTS,
    claude_default_approval_mode: "auto",
    accounts: approvalState.settings!.accounts.map((account) => account.id === "a-work" ? { ...account, approval_mode: "bypass" as const } : account),
  },
  agentAvailability: new Map(approvalState.agentAvailability).set("grok", { agent: "grok", availability: "available", default_account_dir: "/Users/dev/.grok" }),
  accountQuotas: [
    quotaOf("claude", null, "max", [
      quotaWindow("weekly_model", 0, null, "Fable"),
      quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60),
      quotaWindow("session", 18, 117),
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
  settings: { ...browserState.settings!, added_agents: ALL_AGENTS },
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
  settings: { ...approvalState.settings!, added_agents: ALL_AGENTS, accounts: approvalState.settings!.accounts.filter((account) => account.id === "a-codex") },
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

/** Claude Code's default account under a name the user gave it, beside Codex's and Grok Build's, which keep the
 * localized "Default". */
const RENAMED_DEFAULT = "Studio laptop";

const renamedDefaultState: Scenario["state"] = {
  ...quotaState,
  settings: { ...quotaState.settings!, default_account_names: { claude: RENAMED_DEFAULT } },
};

/** The accounts that cannot be read yet because they were never signed in: Work has a configuration directory that is
 * empty of credentials, Client's directory does not exist at all (an account added moments ago, before any sign-in). */
const notSignedInState: Scenario["state"] = {
  ...quotaState,
  accountQuotas: [
    ...quotaState.accountQuotas!.filter((quota) => quota.account !== "a-work"),
    quotaOf("claude", "a-work", null, [], {
      unavailable: quotaFailure("quota_not_signed_in", { agent: "Claude Code" }, "Claude Code is not signed in: Error: not logged in"),
    }),
    quotaOf("claude", "a-long", null, [], {
      unavailable: quotaFailure("quota_config_dir_missing", { agent: "Claude Code", dir: "/Users/dev/work/clients/acme-corporation/internal-platform/.claude-config" }, "The config directory does not exist"),
    }),
  ],
};

/** Only Claude Code is added; Codex is installed, Grok Build is not yet known to be (its check is still running). */
const addAgentCheckingState: Scenario["state"] = {
  ...approvalState,
  settings: { ...approvalState.settings!, added_agents: ["claude"] },
};

/** Only Claude Code is added; Codex is not installed and Grok Build is. */
const addAgentState: Scenario["state"] = {
  ...browserState,
  settings: { ...browserState.settings!, added_agents: ["claude"] },
  agentAvailability: new Map(browserState.agentAvailability).set("grok", { agent: "grok", availability: "available", default_account_dir: "/Users/dev/.grok" }),
};

/** Everything that still uses Claude Code: a console, both projects and four sessions of it. */
const inUseState: Scenario["state"] = {
  ...quotaState,
  projects: new Map([web, api].map((project) => [project.id, { ...project, default_agent: "claude" as const }])),
};

/** Every account of Claude Code read, Client included, so that behind a login's modal no row is still waiting for a
 * quota. */
const signedInState: Scenario["state"] = {
  ...quotaState,
  accountQuotas: [
    ...quotaState.accountQuotas!.filter((quota) => quota.account !== "a-work"),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 34, 150), quotaWindow("weekly", 21, 2 * 24 * 60 + 5 * 60)]),
    quotaOf("claude", "a-long", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)]),
  ],
};

/** A login as the daemon last reported it; `patch` is what differs from one just begun. */
function loginOf(agent: Agent, account: string | null, patch: Partial<AccountLogin>): AccountLogin {
  return { agent, account, status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null, ...patch };
}

const CLAUDE_PAGE = "https://claude.ai/oauth/authorize?client_id=octoboard&response_type=code";

/** The state with these logins held by the daemon, as a window opened mid-sign-in finds them. */
function withLogins(base: Scenario["state"], ...logins: AccountLogin[]): Scenario["state"] {
  return { ...base, accountLogins: logins };
}

/** How far the many-accounts section is scrolled: the default account, Work and a few rows more have gone under the strip. */
const SCROLLED_UNDER_STRIP = 420;

/** Longer than the fixture daemon takes to walk a login from starting through waiting to its outcome. */
const LOGIN_ENDS_WITHIN_MS = 700;

/** Past any section's end, which the scroller clamps to. */
const SCROLLED_TO_END = 10_000;

/** Settings → Agent accounts on Claude Code's tab, Log in chosen from the menu of the account named `name`, which opens
 * the login's modal, then the moment the login takes to reach waiting and end inside it. */
function loginFromMenu(name: string): Scenario["steps"] {
  return [
    (ui) => ui.press(ui.t("rail.settings")),
    (ui) => ui.press(ui.t("settings.section.accounts")),
    (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name, agent: AGENT_LABEL.claude })),
    (ui) => ui.press(ui.t("settings.accounts.login")),
    (ui) => ui.wait(LOGIN_ENDS_WITHIN_MS),
  ];
}

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
    description: "The Language row follows Appearance, then Markdown style starts on Tailwind. The clone-directory explanation describes when the chosen directory is used; the path appears in the input. The archive-cleanup switch sits right below it, on, with the description that says what is deleted and when.",
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
    description: "The agents are tabs in an underline strip, unlike the filled tabs that switch the Settings sections, and Claude Code is open. The default account's name line ends with a small muted \"?\" after Default and its plan chip, with only its config directory beneath. Each account's plan sits beside its name, with its quota windows beneath: a headline with what remains, the countdown at the end of its line and a bar of the same remainder in the agent's colour; the default account lists five-hour quota first, weekly next and Fable last, with no countdown on the model window. Work is spent: 0% remaining, also where use went past 100%. Client has no quota yet and says it is being read. A line under the tab strip describes the approval selector each account has: the default account is set to Automatic, Work to Approve everything and Client follows its account configuration. Remove Work or Client: the account fades out while sliding to the right while the remaining accounts slide into place.",
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
    description: "Below 640px Codex's approval selector and the account's actions button drop beneath the name. The section is scrolled to the end, so Personal's selector and actions button are in view under the pinned tab strip.",
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
    id: "settings-accounts-agents-undecided",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, list of added agents not yet decided",
    description: "The daemon has not yet found out which agents are installed, so every agent has a tab, in the standard order. The tabs carry no warning icon, cannot be dragged and open no menu on a right-click; Add account and Add agent are offered.",
    state: undecidedState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-agents-none",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, no agent added",
    description: "Every agent has been removed: no tab strip, a hint to add an installed agent, and Add account is unavailable while Add agent is not. The accounts set up earlier are kept, only unlisted.",
    state: noAgentsState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-agents-reordered",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, a subset of agents in the user's order",
    description: "Only Grok Build and Claude Code are added, with Grok Build first, so its tab is the one open. Codex has no tab although it has an account (Personal), which is kept for when the agent is added again.",
    state: { ...quotaState, settings: { ...quotaState.settings!, added_agents: ["grok", "claude"] } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-agent-menu",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, agent tab menu open",
    description: "The menu a right-click on the Codex tab opens (the Menu key or Shift+F10 on a focused tab does the same): Move earlier and Move later, both available for the middle tab, and Remove agent in the danger tone after a separator. The same entries are behind the \"More actions\" button at the end of the tab strip, in the scenario after this one.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.contextMenu(AGENT_LABEL.codex),
    ],
  },
  {
    id: "settings-accounts-agent-more-menu",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, tab strip More actions menu open",
    description: "Grok Build's tab is selected and the \"More actions\" button at the end of the strip, which acts on the selected agent, is open: Move earlier, Move later disabled because Grok Build is last, and Remove agent in the danger tone. The tabs keep clear of the button.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.grok),
      (ui) => ui.press(ui.t("settings.accounts.agentMoreActions", { agent: AGENT_LABEL.grok })),
    ],
  },
  {
    id: "settings-accounts-agent-remove",
    group: GROUP,
    category: "normal",
    title: "Remove agent confirmation",
    description: "Removing Grok Build, which nothing uses: the message says its accounts are kept and come back if it is added again, with no line about what still uses it. The confirming button is destructive.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.contextMenu(AGENT_LABEL.grok),
      (ui) => ui.press(ui.t("settings.accounts.removeAgent")),
    ],
  },
  {
    id: "settings-accounts-agent-remove-in-use",
    group: GROUP,
    category: "normal",
    title: "Remove agent confirmation, agent still in use",
    description: "Removing Claude Code, which a console and both projects have as their agent and four sessions run on: after the lead sentence, a bulleted list says what still uses it, each line a count and then the names, every name a chip with no parentheses (more than five of a kind are only counted).",
    state: inUseState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.contextMenu(AGENT_LABEL.claude),
      (ui) => ui.press(ui.t("settings.accounts.removeAgent")),
    ],
  },
  {
    id: "settings-add-agent",
    group: GROUP,
    category: "normal",
    title: "Add agent, one agent not installed",
    description: "Every agent is listed with its state: Claude Code is Added, Codex is Not installed and offers \"How to install\" (an external link) in place of a button, and Grok Build is Installed with its Add button.",
    state: addAgentState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.addAgent")),
    ],
  },
  {
    id: "settings-add-agent-checking",
    group: GROUP,
    category: "normal",
    title: "Add agent, one agent still being checked",
    description: "Grok Build reads \"Checking…\" while Octoboard finds out whether it is installed, with its Add button disabled; Codex, installed, can be added, and Claude Code is Added.",
    state: addAgentCheckingState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.addAgent")),
    ],
  },
  {
    id: "settings-account-menu-owned",
    group: GROUP,
    category: "normal",
    title: "Account actions menu, owned account",
    description: "Work's actions button is open: Edit and Log in, then Remove in the danger tone after a separator.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Work", agent: AGENT_LABEL.claude })),
    ],
  },
  {
    id: "settings-account-menu-default",
    group: GROUP,
    category: "normal",
    title: "Account actions menu, default account",
    description: "The default account's menu holds only what applies to it: Rename and Log in, with no Edit and no Remove.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: ui.t("settings.accounts.defaultName"), agent: AGENT_LABEL.claude })),
    ],
  },
  {
    id: "settings-accounts-default-renamed",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, default account under a custom name",
    description: "Claude Code's default account is called \"Studio laptop\" in place of Default; it keeps its help tip, its config directory and its plan. On the Codex tab the default account is still Default.",
    state: renamedDefaultState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-account-rename-default",
    group: GROUP,
    category: "normal",
    title: "Rename the default account",
    description: "The dialog holds only the Name field, filled with the current custom name (no agent picker, no config directory); saving it empty goes back to Default, as the hint says.",
    state: renamedDefaultState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: RENAMED_DEFAULT, agent: AGENT_LABEL.claude })),
      (ui) => ui.press(ui.t("settings.accounts.renameDefault")),
    ],
  },
  {
    id: "settings-account-login-waiting",
    group: GROUP,
    category: "normal",
    title: "Account login, waiting for the browser",
    description: "Settings finds Work's login already running, so its modal is open: titled \"Log in to\" and the name as a chip, a spinner with \"Waiting for you to authorize in your browser…\", and in the footer \"Open the page again\" beside Cancel login. The same modal serves Claude Code and Codex.",
    state: withLogins(signedInState, loginOf("claude", "a-work", { authorization_url: CLAUDE_PAGE })),
    externalLinks: "available",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-account-login-device-code",
    group: GROUP,
    category: "normal",
    title: "Account login, device code (Grok Build)",
    description: "Settings finds the default Grok Build account's login already running, so its modal is open. Grok Build opens its own page, so the modal shows the code to confirm there, set as code, with \"Open the page again\" and Cancel login in the footer; Octoboard does not open the page itself.",
    state: withLogins(quotaState, loginOf("grok", null, {
      authorization_url: "https://accounts.x.ai/oauth2/device?user_code=ABCD-1234",
      device_code: "ABCD-1234",
      opened_by_agent: true,
    })),
    externalLinks: "available",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-not-signed-in",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, accounts not logged in yet",
    description: "Work (never logged in) and Client (whose config directory does not exist yet) both carry the amber \"Not logged in\" badge after the name, with no quota line and no raw CLI error beneath; the default account above them carries the green \"Logged in\" badge beside its plan and reads its quota as usual, so the two badges sit side by side.",
    state: notSignedInState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-account-login-failed",
    group: GROUP,
    category: "exceptional",
    title: "Account login failed",
    description: "Log in was chosen for Work and the CLI gave up: the modal stays open, now saying \"Could not log in to\" the name as a chip, with the CLI's reason on its own line, and Log in again as its main button beside Close.",
    state: notSignedInState,
    loginOutcome: "failed",
    steps: loginFromMenu("Work"),
  },
  {
    id: "settings-account-login-timed-out",
    group: GROUP,
    category: "exceptional",
    title: "Account login timed out",
    description: "Log in was chosen for Work and nobody authorized in time: the modal stays open, saying the login to the name as a chip timed out, with no reason beneath, and Log in again as its main button beside Close.",
    state: notSignedInState,
    loginOutcome: "timed_out",
    steps: loginFromMenu("Work"),
  },
  {
    id: "settings-account-login-codex-busy",
    group: GROUP,
    category: "exceptional",
    title: "Account login refused, another Codex account is signing in",
    description: "Choosing Log in on Personal is refused: the modal opens straight into its ended state, saying it could not log in to the name as a chip and that another Codex account is being logged in and to wait, with Log in again and Close.",
    state: quotaState,
    loginStartError: { code: "login_codex_busy", params: {}, message: "Another Codex account is being signed in. Wait for it to finish, then try again." },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.codex),
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Personal", agent: AGENT_LABEL.codex })),
      (ui) => ui.press(ui.t("settings.accounts.login")),
    ],
  },
  {
    id: "settings-add-agent-refused",
    group: GROUP,
    category: "exceptional",
    title: "Add agent refused, agent no longer installed",
    description: "Codex was listed as installed but the daemon finds it gone when Add is pressed: the error alert says Codex is not installed, and the list behind it still shows Codex as installable.",
    state: addAgentCheckingState,
    agentListError: { code: "agent_not_installed", params: { agent: "Codex" }, message: "Codex is not installed on this machine." },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.addAgent")),
      (ui) => ui.press(ui.t("settings.addAgent.addLabel", { agent: AGENT_LABEL.codex })),
    ],
  },
  {
    id: "settings-accounts-agent-reorder-refused",
    group: GROUP,
    category: "exceptional",
    title: "Agent order refused, the list changed meanwhile",
    description: "Move later on the Claude Code tab was refused because the added agents changed elsewhere: the error alert says the order could not be saved, and the tabs stay in their old order.",
    state: quotaState,
    agentListError: { code: "agent_order_invalid", params: {}, message: "The added agents changed meanwhile, so this order could not be saved." },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.contextMenu(AGENT_LABEL.claude),
      (ui) => ui.press(ui.t("settings.accounts.moveLater")),
    ],
  },
  {
    id: "settings-account-rename-default-taken",
    group: GROUP,
    category: "exceptional",
    title: "Rename the default account, name already in use",
    description: "Saving \"Work\", an account of the same agent, is refused with the conflicting name as a chip in the field's validation message.",
    state: renamedDefaultState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: RENAMED_DEFAULT, agent: AGENT_LABEL.claude })),
      (ui) => ui.press(ui.t("settings.accounts.renameDefault")),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.type("Work"),
      (ui) => ui.confirm(),
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
    description: "The Codex tab is selected and carries a warning icon after its name. Neither the default account nor Personal has a quota or a plan, and Personal says it is not usable.",
    state: unavailableQuotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("agents.notInstalled", { agent: AGENT_LABEL.codex })),
    ],
  },
  {
    id: "settings-accounts-codex-not-installed-tooltip",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, Codex not-installed tooltip",
    description: "The pointer is on the Codex tab. The tooltip says Codex is not installed and its accounts are not usable until it is on PATH, with PATH as a chip.",
    state: unavailableQuotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.hoverAnchored(
        ui.t("agents.notInstalled", { agent: AGENT_LABEL.codex }),
        ui.t("settings.accounts.notInstalledTip", { agent: AGENT_LABEL.codex, pathVar: "PATH" }),
      ),
    ],
  },
  {
    id: "settings-account-add-picker",
    group: GROUP,
    category: "normal",
    title: "Add account, agent picker lists only added agents",
    description: "Claude Code and \"Codex (not installed)\" are listed; Grok Build, installed but not added, is not. Codex is marked but still selectable, unlike the agent pickers elsewhere, because an account may be set up before its agent is installed.",
    state: { ...unavailableQuotaState, settings: { ...unavailableQuotaState.settings!, added_agents: ["claude", "codex"] } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.add")),
      // The picker's trigger is named by the agent it holds, as is the selected agent tab behind the dialog; pressing
      // that tab again changes nothing.
      (ui) => ui.pressEach(AGENT_LABEL.claude),
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
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Work", agent: "Claude Code" })),
      (ui) => ui.press(ui.t("common.remove")),
    ],
  },
  ...(["existing", "default", "reserved"] as const).map((kind): Scenario => ({
    id: `settings-account-name-taken-${kind}`,
    group: GROUP,
    category: "exceptional",
    title: kind === "existing" ? "Add account, name already in use" : kind === "default" ? "Add account, default account name reserved" : "Add account, \"Default\" reserved while the default account is renamed",
    description: kind === "reserved"
      ? "Claude Code's default account has been renamed, yet typing \"Default\" is still refused: the message says the word is reserved for the default account, with the word typed as a chip."
      : "The conflicting account name is a chip in the field's validation message.",
    state: kind === "reserved" ? renamedDefaultState : browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.add")),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.type(kind === "existing" ? "Work" : kind === "default" ? ui.t("settings.accounts.defaultName") : "Default"),
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
