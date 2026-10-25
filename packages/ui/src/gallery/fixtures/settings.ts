import { AGENT_LABEL } from "../../agents";
import { describing, type Ui } from "../interact";
import type { AccountLogin, AccountSharing, Agent, AgentAvailability } from "../../protocol";
import type { Scenario, Step } from "../scenario";
import { ALL_AGENTS, SAMPLE, accountSharingOf, minutesAgo, snapshotState } from "./builders";
import { noneAddedState } from "./empty";
import { agentAvailability } from "./liveChanges";
import { manyClaudeAccounts, quotaFailure, quotaOf, quotaWindow } from "./quotas";
import { DEFAULT_WORKTREE_SETTINGS } from "../../worktreeSettings";

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
    sidebar_order_by_status: true,
    added_agents: ["claude", "codex"],
    default_clone_dir: "/Users/dev/Projects",
    worktrees: DEFAULT_WORKTREE_SETTINGS,
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
      { id: "a-codex-auto", agent: "codex", name: "Automation", config_dir: "/Users/dev/.codex-automation" },
      { id: "a-codex-user", agent: "codex", name: "Production", config_dir: "/Users/dev/.codex-production" },
    ],
  },
  agentAvailability: new Map(browserState.agentAvailability).set("codex", { agent: "codex", availability: "available", default_account_dir: "/Users/dev/.codex" }),
};

/** Every agent installed and its readings in, one tab each: Claude Code's default account and Work have their plan
 * and windows (Work's spent), Client has no reading yet while its agent is available, Codex's accounts have a plan
 * and two windows each but Automation's, which has no plan and whose 5h window, spent when it was read, has
 * since reset, and Grok has one credit pool. The readings are of different ages (Work's seven minutes, Personal's
 * three, Automation's sixteen, the rest's one). Claude Code's permission approval is set to automatic and Codex's to
 * automatic review. */
const quotaState: Scenario["state"] = {
  ...approvalState,
  settings: {
    ...approvalState.settings!,
    added_agents: ALL_AGENTS,
    claude_approval_mode: "auto",
    codex_approval_reviewer: "auto_review",
  },
  agentAvailability: new Map(approvalState.agentAvailability).set("grok", { agent: "grok", availability: "available", default_account_dir: "/Users/dev/.grok" }),
  accountQuotas: [
    quotaOf("claude", null, "max", [
      quotaWindow("weekly_model", 0, null, "Fable"),
      quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60),
      quotaWindow("session", 18, 117),
    ]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 42), quotaWindow("weekly", 137, 2 * 24 * 60 + 5 * 60)], { fetched_at: minutesAgo(7) }),
    quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
    quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)], { fetched_at: minutesAgo(3) }),
    quotaOf("codex", "a-codex-auto", null, [quotaWindow("session", 100, -1), quotaWindow("weekly", 9, 6 * 24 * 60)], { fetched_at: minutesAgo(16) }),
    quotaOf("codex", "a-codex-user", "plus", [quotaWindow("session", 97, 25), quotaWindow("weekly", 91, 2 * 24 * 60 + 11 * 60)]),
    quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 23, 4 * 24 * 60 + 11 * 60)]),
  ],
};

/** Claude Code with more owned accounts than the dialog is tall: the default account, Work and Client, then eight
 * more (`manyClaudeAccounts`). */
const many = manyClaudeAccounts();

const manyAccountsState: Scenario["state"] = {
  ...quotaState,
  settings: { ...quotaState.settings!, accounts: [...quotaState.settings!.accounts, ...many.accounts] },
  accountQuotas: [...quotaState.accountQuotas!, ...many.quotas],
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

/** For the narrow layout: Claude Code's default account with a long model name, and Work, an owned account whose
 * settings dialog has a second, shorter reading to compare with. */
const narrowQuotaState: Scenario["state"] = {
  ...quotaState,
  settings: { ...approvalState.settings!, added_agents: ALL_AGENTS, accounts: approvalState.settings!.accounts.filter((account) => account.id === "a-work") },
  accountQuotas: [
    quotaOf("claude", null, "max", [
      quotaWindow("session", 18, 117),
      quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60),
      quotaWindow("weekly_model", 27, 3 * 24 * 60 + 4 * 60, NARROW_MODEL),
    ]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 62, 42), quotaWindow("weekly", 34, 2 * 24 * 60 + 5 * 60)]),
  ],
};

/** Claude Code's default account and Work turned off for new sessions; Client, with no reading yet, stays on. Work keeps
 * its reading, which its row keeps showing while it is off. */
const disabledState: Scenario["state"] = {
  ...quotaState,
  settings: {
    ...quotaState.settings!,
    disabled_default_accounts: ["claude"],
    accounts: quotaState.settings!.accounts.map((account) => account.id === "a-work" ? { ...account, disabled: true } : account),
  },
};

/** Work with a reading that has all three kinds of window, a model's own weekly one among them, for the details dialog's
 * grid: nothing used up, so its bars have something to show. */
const detailsWorkState: Scenario["state"] = {
  ...quotaState,
  accountQuotas: [
    ...quotaState.accountQuotas!.filter((quota) => quota.account !== "a-work" || quota.agent !== "claude"),
    quotaOf("claude", "a-work", "pro", [
      quotaWindow("session", 38, 150),
      quotaWindow("weekly", 66, 2 * 24 * 60 + 5 * 60),
      quotaWindow("weekly_model", 20, 2 * 24 * 60 + 5 * 60, "Fable"),
    ], { fetched_at: minutesAgo(7) }),
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

/** Settings → Agent accounts on Claude Code's tab, Work's Log in icon button in the Status column (the first signed-out account), then
 * the moment the login takes to reach waiting and end inside its modal. */
function loginFromInline(): Scenario["steps"] {
  return [
    (ui) => ui.press(ui.t("rail.settings")),
    (ui) => ui.press(ui.t("settings.section.accounts")),
    (ui) => ui.press(ui.t("settings.accounts.login.open", { name: "Work" })),
    (ui) => ui.wait(LOGIN_ENDS_WITHIN_MS),
  ];
}

const openAccounts: Step[] = [
  (ui) => ui.press(ui.t("rail.settings")),
  (ui) => ui.press(ui.t("settings.section.accounts")),
];

/** The Settings item of the actions menu of the account `name` of `agent` on the tab in view (the default account's name is the
 * language's own); a menu item, since the rail has a Settings button of its name. */
const openDetails = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step => async (ui) => {
  await ui.press(ui.t("settings.accounts.actionsLabel", { name: typeof name === "string" ? name : name(ui), agent: AGENT_LABEL[agent] }));
  await ui.pressMenuItem(ui.t("settings.accounts.settingsItem"));
};
const defaultAccount = (ui: Ui) => ui.t("settings.accounts.defaultName");

/** Settings → Agent accounts with the details of the account `name` in front. */
const accountDetails = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step[] => [...openAccounts, openDetails(name, agent)];

const openSharingHelp = (name: string, agent: Agent = "claude"): Step[] => [
  ...accountDetails(name, agent),
  (ui) => ui.touchPress(ui.t("settings.accounts.sharing.helpLabel")),
];

/** Claude Code's tab, its permission approval set to Approve everything: the confirmation dialog it opens. */
const bypassSteps: Step[] = [
  ...openAccounts,
  (ui) => ui.press(ui.t("settings.accounts.approval.label", { agent: AGENT_LABEL.claude })),
  (ui) => ui.press(ui.t("settings.accounts.approval.claudeBypass")),
];

/** The two owned Claude Code accounts the sharing scenarios share the page with the default account: Work is the first
 * row, Client the second. A third row does not fit under the explanation lines. */
function sharingAccounts(work: { share: boolean; incomplete?: boolean }, client: { share: boolean; incomplete?: boolean }): Scenario["state"] {
  const base = browserState!;
  return {
    ...base,
    settings: {
      ...base.settings!,
      added_agents: ["claude"],
      accounts: [
        { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work", share_default_config: work.share, share_default_config_incomplete: work.incomplete ?? false },
        { id: "a-client", agent: "claude", name: "Client", config_dir: "/Users/dev/.claude-client", share_default_config: client.share, share_default_config_incomplete: client.incomplete ?? false },
      ],
    },
  };
}

/** Work does not share and Client does, with no working session or sign-in in the way. */
const sharingState = sharingAccounts({ share: false }, { share: true });

/** Work has two sessions working and is being signed in as well, so its switch cannot be used and says both why. */
const sharingInUseState: Scenario["state"] = {
  ...sharingAccounts({ share: false }, { share: true }),
  accountSharing: accountSharingOf([{ account: "a-work", in_use: { sessions: 2, signing_in: true } }]),
};

/** Work is changing entries, turning sharing on, 3 of 9 done. A change under way has its choice recorded and is
 * incomplete until it ends. */
const sharingProgressState: Scenario["state"] = {
  ...sharingAccounts({ share: true, incomplete: true }, { share: false }),
  accountSharing: accountSharingOf([{ account: "a-work", progress: { share: true, phase: "changing", done: 3, total: 9, item: "plugins/" } }]),
};

/** A phase around the entries: Work is ending its idle sessions before turning sharing on. */
const sharingRestartState: Scenario["state"] = {
  ...sharingAccounts({ share: true, incomplete: true }, { share: false }),
  accountSharing: accountSharingOf([{ account: "a-work", progress: { share: true, phase: "stopping", done: 0, total: 9 } }]),
};

/** A change the daemon was restarted in the middle of: Work's choice is recorded as on and incomplete (the switch shows
 * its position before the change), with no failure kept, since that lives in the daemon's memory only. */
const sharingIncompleteState: Scenario["state"] = sharingAccounts({ share: true, incomplete: true }, { share: false });

/** One owned account that does not share, and only that agent added, so its sharing explanation
 * opens on a short page. Codex keeps the name the other Codex scenarios use. */
function sharingHelpState(agent: Agent): Scenario["state"] {
  const base = browserState!;
  return {
    ...base,
    settings: {
      ...base.settings!,
      added_agents: [agent],
      accounts: [{ id: `a-${agent}-share`, agent, name: agent === "codex" ? "Personal" : "Work", config_dir: `/Users/dev/.${agent}-work`, share_default_config: false }],
    },
    agentAvailability: new Map(base.agentAvailability).set(agent, { agent, availability: "available", default_account_dir: `/Users/dev/.${agent}` }),
    // Claude Code's account has 2 idle sessions, which a change would end first and resume after: the last line of its explanation.
    ...(agent === "claude" && { accountSharing: accountSharingOf([{ account: `a-${agent}-share`, idle_sessions: 2 }]) }),
  };
}

/** What turning sharing off for Work left unfinished: a directory entry, and a conversation two
 * sessions of this scenario record, so the alert can show their titles. */
const SHARING_FAILURE_ITEMS: NonNullable<AccountSharing["failure"]>["items"] = [
  { item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied" },
  { item: "abc", left: "unchanged", cause: "permission_denied", detail: "permission denied", sessions: ["s-web-1", "s-web-2"] },
  { item: "plugins/", left: "unchanged", cause: "busy", detail: "resource busy" },
];

const SHARING_FAILURE_DETAIL = "`skills/` left unchanged (permission denied); the conversation `abc` of session s-web-1, s-web-2 left in the default directory (permission denied); `plugins/` left unchanged (resource busy)";

/** Turning sharing off for Work failed: the choice is recorded as off and incomplete, and the daemon still holds the
 * failure. */
const sharingFailedState: Scenario["state"] = {
  ...browserState,
  settings: {
    ...browserState.settings!,
    added_agents: ["claude"],
    accounts: [
      { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work", share_default_config: false, share_default_config_incomplete: true },
    ],
  },
  accountSharing: accountSharingOf([{ account: "a-work", failure: { share: false, items: SHARING_FAILURE_ITEMS, detail: SHARING_FAILURE_DETAIL } }]),
};

/** Turning sharing on for Work failed because the default account's directory has moved since sharing was turned on:
 * no entry is named, and the raw reason is all Details holds. The failure was not rolled back, so the recorded
 * choice, on, is where the switch sits. */
const sharingDefaultMovedState: Scenario["state"] = {
  ...sharingAccounts({ share: true, incomplete: true }, { share: false }),
  accountSharing: accountSharingOf([{
    account: "a-work",
    failure: { share: true, cause: "default_moved", items: [], detail: "the default account's directory /Users/dev/.claude was not found; it was /Users/dev/.claude-old when sharing was turned on" },
  }]),
};

/** Turning sharing on for Work failed as a whole for lack of disk space: the daemon names a cause but no entry, so the
 * warning holds the reason sentence rather than a line per cause. */
const sharingNoSpaceState: Scenario["state"] = {
  ...sharingAccounts({ share: true, incomplete: true }, { share: false }),
  accountSharing: accountSharingOf([{
    account: "a-work",
    failure: { share: true, cause: "no_space", items: [], detail: "no space left on device (os error 28)" },
  }]),
};

/** Client shares and its removal turns sharing off first, which does not finish. */
const SHARING_REMOVAL_ERROR = {
  code: "account_sharing_failed",
  params: {
    account: "a-client",
    name: "Client",
    cause: "permission_denied",
    items: JSON.stringify([
      { item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied" },
      { item: "settings.json", left: "backed_up", cause: "permission_denied", detail: "permission denied" },
    ]),
    detail: "`skills/` left unchanged (permission denied); `settings.json` backed up, with nothing linked in its place (permission denied)",
  },
  message: "ending the sharing of the default configuration with account `Client` did not complete: `skills/` left unchanged (permission denied); `settings.json` backed up, with nothing linked in its place (permission denied)",
};

export const settingsScenarios: Scenario[] = [
  {
    id: "settings-general",
    group: GROUP,
    category: "normal",
    title: "Browser General settings",
    description: "The Language row follows Appearance, then Markdown style starts on Tailwind. The \"Order the sidebar by status\" switch follows it, on, with the description that says sessions and projects move as their status changes. The archive-cleanup switch follows sidebar status ordering, on, with the description that says what is deleted and when. It is the last row.",
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
    description: "The archive-cleanup switch below sidebar status ordering is off, and its description now starts with \"Once this is on\" instead of stating the behaviour.",
    state: { ...browserState, settings: { ...browserState.settings!, auto_delete_unresumable_archives: false } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
    ],
  },
  {
    id: "settings-general-sidebar-order-off",
    group: GROUP,
    category: "normal",
    title: "General, sidebar status ordering switched off",
    description: "The \"Order the sidebar by status\" switch below Markdown style is off, and its description now says sessions are listed newest first and projects by name, pinned ones first, before ending with \"Once this is on\". Switch it on and off to read both descriptions; the sidebar re-orders at once (see Console sessions, ordering by status switched off).",
    state: { ...browserState, settings: { ...browserState.settings!, sidebar_order_by_status: false } },
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
    description: "The desktop app's background row follows the archive-cleanup row and starts off. Switch it on and off to read both descriptions.",
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
    description: "Neither Screen Recording nor Accessibility has been granted, so each row ends in a Grant button and no badge. Press each Grant to see the refused request point at System Settings; revisiting the section offers Grant again.",
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
    description: "Both permissions are granted, each marked by a soft success Granted badge at the right end of its row, where the Grant button would be, and its description no longer says Granted. Screen Recording was not granted when the process started, so its row still says a restart is owed.",
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
    description: "What a broken IPC looks like: the grants could not be read, on the platform that has them. The failure is a danger Alert in place of the table of grants, titled \"Could not read the permissions\" with one line under it.",
    state,
    macosPermissions: "unreadable",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
    ],
  },
  {
    id: "settings-agent-permissions",
    group: GROUP,
    category: "normal",
    title: "Agent permissions",
    description: "The lead line sits under the section heading, above a rule that opens the four rows: the session-request switch, the resume-request switch, the account-switch switch, then the handoff switch, all off. Each description starts with \"Once this is on\"; the resume one names reopening, resuming and resuming a conversation in a new session, and the last two say that until then you are asked and can change the account (and for a handoff the agent), model and reasoning effort before confirming. Switch each on to read its other description; none moves another.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.agentPermissions")),
    ],
  },
  {
    id: "settings-agent-permissions-on",
    group: GROUP,
    category: "normal",
    title: "Agent permissions, all approved automatically",
    description: "All four switches are on, and each description now states the behaviour instead of starting with \"Once this is on\".",
    state: {
      ...browserState,
      settings: { ...browserState.settings!, auto_approve_session_requests: true, auto_approve_resume_requests: true, auto_approve_account_switches: true, auto_approve_handoffs: true },
    },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.agentPermissions")),
    ],
  },
  {
    id: "settings-git",
    group: GROUP,
    category: "normal",
    title: "Settings: git",
    description: "Default clone directory is the first row, before Automatically pull repositories; its explanation describes when the chosen directory is used, and the path appears in the input. Automatically pull repositories shows its first two sentences under the label, and a small muted \"?\" at the end of the label holds the sentence about the remote being checked. It is the only row on this page with a help tip.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.git")),
    ],
  },
  {
    id: "settings-worktrees",
    group: GROUP,
    category: "normal",
    title: "Settings: worktrees",
    description: "Worktrees follows Git in the section list. Four rows in this order: Use worktrees for development and Rebase before merging are on, each with the explanation that says what being on does; Revert gate is off, with a small muted \"?\" at the end of its label and an explanation starting \"Once this is on, in a project session\"; Delivery, last, reads Ask each time in a select at the end of its row. Use worktrees for development ends its explanation with \"Rebase, delivery and the revert gate are shown while this is on.\"",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.worktrees")),
    ],
  },
  {
    id: "settings-worktrees-changed",
    group: GROUP,
    category: "normal",
    title: "Settings: worktrees changed from the defaults",
    description: "Use worktrees for development stays on and the other rows differ from the defaults: Rebase before merging is off, its explanation saying what turning it on would do; Revert gate is on, its explanation starting \"In a project session\" and saying a change takes effect at once, running sessions included. Delivery reads Open a pull request, the longest of its three labels, which fits the select without being cut.",
    state: { ...browserState, settings: { ...browserState.settings!, worktrees: { use_worktrees: true, rebase_before_merge: false, delivery: "pull_request", revert_gate: true } } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.worktrees")),
    ],
  },
  {
    id: "settings-worktrees-off",
    group: GROUP,
    category: "normal",
    title: "Settings: worktrees, worktrees off",
    description: "Use worktrees for development is off and is the only row on the page, its explanation starting \"Once this is on\" and still ending with the line that rebase, delivery and the revert gate are shown while it is on. Rebase before merging, Revert gate and Delivery are not shown, though the stored values (rebase on, revert gate on, delivery Open a pull request) are kept and come back when the switch is turned on.",
    state: { ...browserState, settings: { ...browserState.settings!, worktrees: { use_worktrees: false, rebase_before_merge: true, delivery: "pull_request", revert_gate: true } } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.worktrees")),
    ],
  },
  {
    id: "settings-worktrees-revert-gate-help",
    group: GROUP,
    category: "normal",
    title: "Settings: worktrees, revert gate help tip open",
    description: "The mouse rests on the \"?\" after the Revert gate label, so its popover is open: it says what the gate blocks and how to undo a commit on purpose, with git revert or a \"Reverts: <sha>\" line in the undoing commit's message, both as monospace chips. No focus has moved; the text can be selected.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.worktrees")),
      (ui) => ui.hoverStatusMark(ui.t("settings.worktrees.revertGate.help")),
    ],
  },
  {
    id: "settings-accounts",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code tab",
    description: "Claude Code's tab. The agents are tabs in an underline strip, unlike the filled tabs that switch the Settings sections, each with its own small ⋮ button at its end and the strip ending in two icon buttons, Add agent and Add account. Under it, one Permission approval row for the whole agent: its label with a help button, \"Applies to every Claude Code account\" under it and the selector at the end, on Automatic. Then one table lists the agent's accounts, the default account first, under the headers Account, Status, Usage and Disable (with a small \"?\") and an unlabelled last column holding only the account's ⋮ menu. Each row has the name with its plan badge; a soft success \"Logged in\" badge; a two-layer usage bar like the one the account pickers show, the session window's over the weekly one, with no numbers and, unlike the pickers' bars, no hairline edge (Work is spent, so its bar is empty, also where use went past 100%); and the Disable switch, off. Client has no reading yet, so it has no plan or badge and its Usage cell says it is being read. Above the tab strip, the account-selection switch is followed directly by the \"Start usage windows automatically\" switch, both off, each description starting with \"Once this is on\"; the usage-window one is a single short sentence on what the switch does and its quota cost, with a small muted \"?\" at the end of its label. Remove Work or Client: the row fades out while sliding to the right while the remaining rows slide into place. \"Codex uninstalled\" among the \"Live changes\" buttons is the machine finding Codex missing while the strip is shown: its tab gains the warning mark and the tooltip that says why, widening in place while the tab after it slides along; \"Codex installed\" takes the mark away again. With reduced motion the strip changes at once.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
    liveChanges: [
      agentAvailability("uninstall-codex", "Codex uninstalled", "codex", "unavailable"),
      agentAvailability("install-codex", "Codex installed", "codex", "available"),
    ],
  },
  {
    id: "settings-accounts-disabled",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, accounts turned off",
    description: "The default account and Work are turned off: their Disable switches are on, and nothing else about their rows changes, since a turned-off account keeps its name, plan, status and usage bar as they were; Client's switch is off. The header's \"?\" next to Disable explains what disabling means. Turn Disable off for either and the switch moves back; turn it on for Client and it does.",
    state: disabledState,
    steps: openAccounts,
  },
  {
    id: "settings-account-details",
    group: GROUP,
    category: "normal",
    title: "Account details, an account the user owns",
    description: "Work's settings dialog, opened from its ⋮ menu, in a modal with no footer button. The heading is the name alone; the Pro plan badge and the \"Logged in\" badge sit beside it, and the config directory is the line under it. The Usage section has \"Updated 7 minutes ago\" at the end of its heading and the rail's meters: Session, as the two-layer bar with Weekly · all models under it, each line saying what remains and when it resets, then Fable weekly with its own bar. Below, two rows with a description under each label and the control at the end: Share default configuration, with its help button and its switch, off, whose description says turning it on links the account's configuration and history to the default account's directory, and Disable account, with its switch, off, whose description says that once on, the account is left out of the account choices.",
    state: detailsWorkState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-account-details-default",
    group: GROUP,
    category: "normal",
    title: "Account details, the default account",
    description: "The default account's settings dialog: the heading is Default, with the Max badge and \"Logged in\" beside it, and the config directory is the line under it, ending right after the path with the small \"?\" that explains the default account. Usage shows all three windows. The only row is Disable account: the default account has no Share default configuration row, and the permission approval is set on the tab, not here.",
    state: quotaState,
    steps: accountDetails(defaultAccount),
  },
  {
    id: "settings-account-details-not-logged-in",
    group: GROUP,
    category: "normal",
    title: "Account details, an account not logged in",
    description: "Work's settings dialog, which has never been logged in: beside the name the soft warning \"Not logged in\" badge and the Log in icon button, and the Usage section has no meters but says \"Not logged in yet.\" with \"Log in to use this account.\" under it. The Share default configuration and Disable account rows are as usual.",
    state: notSignedInState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-account-details-agent-not-installed",
    group: GROUP,
    category: "exceptional",
    title: "Account details, the agent is not installed",
    description: "Personal's settings dialog on the Codex tab, whose agent is not installed: beside the name the login state reads \"Unknown\", and under the line with the config directory a line says it is not usable until Codex is on PATH, both as chips. The Usage section is a dash, the account having no reading.",
    state: unavailableQuotaState,
    steps: [...openAccounts, (ui) => ui.press(ui.t("agents.notInstalled", { agent: AGENT_LABEL.codex })), openDetails("Personal", "codex")],
  },
  {
    id: "settings-account-details-narrow",
    group: GROUP,
    category: "normal",
    title: "Account details in a narrow window",
    description: "Below 640px the details stack: a model's name as long as one can be wraps beside its reset time on the same line, with its bar under it. The default account's settings dialog is shown, with its Session, Weekly · all models and long-named weekly lines, and the Disable account row.",
    width: 560,
    state: narrowQuotaState,
    steps: accountDetails(defaultAccount),
  },
  {
    id: "settings-accounts-tab-row-tooltip",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, the icon buttons at the end of the tab row",
    description: "Add account, the right-hand icon button at the end of the tab row (the other, Add agent, is beside it), has the pointer on it, so its tooltip, \"Add account\", is open above it. The two buttons are plain icons in the same style as the ⋮ buttons, with no words.",
    state: quotaState,
    steps: [...openAccounts, (ui) => ui.hoverTitledControl(ui.t("settings.accounts.add"))],
  },
  {
    id: "settings-accounts-switches-on",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, both account switches on",
    description: "Above the agent tabs, the account-selection switch and the \"Start usage windows automatically\" switch below it are both on. Each description states the behaviour and does not start with \"Once this is on\"; the usage-window one is a single short sentence with its \"?\" after the label. Switch either off and on again to read both of its descriptions; neither moves the other.",
    state: { ...quotaState, settings: { ...quotaState.settings!, auto_select_account: true, auto_start_quota_windows: true } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-start-windows-help-popover",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, usage-window help popover open on touch",
    description: "The \"?\" after the \"Start usage windows automatically\" label has been touched with a finger, so the explanation stays open in a popover above it, covering part of the row above: why a window that has reset shows no countdown until a request starts it, and what the switch sends. The row's own description stays one short sentence beneath the label.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.touchPress(ui.t("settings.startQuotaWindows.help")),
    ],
  },
  {
    id: "settings-accounts-default-help",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, default account help tip open",
    description: "The mouse rests on the \"?\" at the end of the config directory line in the default account's settings dialog, so its popover is open, saying this is the agent's own default configuration directory, which new sessions use when no account is chosen and other accounts can share; no focus has moved, the popover stays while the pointer is on the \"?\" or the popover, and its text can be selected. The table behind the dialog carries no such \"?\".",
    state: quotaState,
    steps: [...accountDetails(defaultAccount), (ui) => ui.hoverStatusMark(ui.t("settings.accounts.defaultHelp"))],
  },
  {
    id: "settings-accounts-many",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code tab scrolled with many accounts",
    description: "The section is scrolled down, so the agent tab strip with its ⋮ buttons and the two icon buttons is pinned at the top of it with its bottom rule and the Claude Code indicator showing, on the dialog's own surface; the table's header and the first rows have scrolled away underneath it and are cut off cleanly against its rule.",
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
    description: "Codex's tab has its own Permission approval row, \"Applies to every Codex account\", on Automatic review, the reviewers' wording. Codex has no model-specific window: each Usage cell is the two-layer bar, the 5h window over the weekly one. Automation has no plan, so no plan badge, and its 5h window, read as spent 16 minutes ago, reset a minute ago: its top layer is full; Production, further down, is nearly spent: both layers are almost empty.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.codex),
    ],
  },
  ...(["claude", "codex"] as const).map((agent): Scenario => ({
    id: `settings-accounts-approval-help-${agent}`,
    group: GROUP,
    category: "normal",
    title: `Settings: accounts, ${AGENT_LABEL[agent]} permission approval explanation`,
    description: "The help button beside the Permission approval label above the table is touched with a finger. Its popover keeps the agent-specific explanation next to the selector it describes.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      ...(agent === "codex" ? [((ui) => ui.press(AGENT_LABEL.codex)) satisfies Step] : []),
      (ui) => ui.touchPress(ui.t("settings.accounts.approval.helpLabel")),
    ],
  })),
  {
    id: "settings-accounts-grok",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Grok Build tab",
    description: "Grok Build has no permission approval choice, so its tab has no Permission approval row above the table: the tab strip is followed directly by the table, with its one default account, a single weekly layer in its Usage cell and the columns Account, Status, Usage and Disable.",
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
    title: "Settings: accounts, Claude Code table in a narrow window",
    description: "Below about 640px the Permission approval row above the table stays within the section, and the table is wider than the section, so it scrolls sideways: its columns keep their widths, the later columns are cut at the edge and the ⋮ column stays pinned at the end, with a hairline at its start while there is more beside it. The tab strip, the two switches above it and the tab buttons fit the width. The scroller is a focusable region while it overflows.",
    width: 560,
    state: narrowQuotaState,
    steps: openAccounts,
  },
  {
    id: "settings-accounts-many-narrow",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code tab scrolled with many accounts in a narrow window",
    description: "Below 640px the sections scroll too: with the section scrolled down, the agent tab strip is pinned at the top of it, the table rows pass beneath it, and the table itself still scrolls sideways with its actions column pinned. The Settings sections are tabs in a row above, which stays put.",
    width: 560,
    state: manyAccountsState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.scrollPanel(ui.t("settings.section.accounts"), SCROLLED_UNDER_STRIP),
    ],
  },
  {
    id: "settings-accounts-agents-undecided",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, list of added agents not yet decided",
    description: "The daemon has not yet found out which agents are installed, so every agent has a tab, in the standard order. The tabs carry no warning icon and no ⋮ button, cannot be dragged and open no menu on a right-click; the Add agent and Add account icon buttons are offered at the end of the strip.",
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
    description: "Every agent has been removed: no tab strip or Add account button, a hint to add an installed agent, and Add agent is offered. The account-selection and \"Start usage windows automatically\" switches stay above Add agent, both off, their descriptions starting with \"Once this is on\". The accounts set up earlier are kept, only unlisted.",
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
    description: "The menu a right-click on the Codex tab opens (the Menu key or Shift+F10 on a focused tab does the same): Move earlier and Move later, both available for the middle tab, and Remove agent in the danger tone after a separator. The same entries are behind the tab's own ⋮ button, in the scenario after this one.",
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
    title: "Settings: accounts, tab actions button menu open",
    description: "The ⋮ button at the end of Grok Build's tab, named \"Actions for Grok Build\", is open: Move earlier, Move later disabled because Grok Build is the last tab, and Remove agent in the danger tone. Every tab has its own button, so no tab needs to be selected first.",
    state: quotaState,
    steps: [
      ...openAccounts,
      (ui) => ui.press(ui.t("settings.accounts.agentActions", { agent: AGENT_LABEL.grok })),
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
    description: "Removing Claude Code, which a console and both projects have as their default agent and four sessions run on: after the lead sentence, a bulleted list says what still uses it. Each line says how its kind uses the agent, then the names: the console and projects have it as their default agent, the sessions use it. Every name is a chip with no parentheses (more than five of a kind are only counted).",
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
    id: "settings-add-agent-from-empty-state",
    group: GROUP,
    category: "normal",
    title: "Add agent, reached from the terminal's empty state",
    description: "The Add agent… button under \"No agent added\" opens Settings on Agent accounts with the Add agent dialog already in front: all three agents are Installed with their Add buttons, none is Added, and the section behind shows no tabs.",
    state: noneAddedState,
    steps: [
      (ui) => ui.press("Landing page"),
      (ui) => ui.press(ui.t("agents.addAction")),
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
    description: "Work's ⋮ button, the only thing in the last column, is open: Settings first, then Edit and Log in, then Remove in the danger tone after a separator.",
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
    description: "The default account's menu holds only what applies to it: Settings, Rename and Log in, with no Edit and no Remove.",
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
    description: "Claude Code's default account is called \"Studio laptop\" in place of Default; it keeps its plan badge, its \"Logged in\" badge and its usage bar, and its ⋮ menu and settings dialog open under that name. On the Codex tab the default account is still Default.",
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
    description: "Work (never logged in) and Client (whose config directory does not exist yet) both show the soft warning \"Not logged in\" badge in the Status column with a small Log in icon button beside it, and a dash in the Usage cell, which assistive technology reads as no usage reading; they have no plan badge, and their Disable switches are off as usual. The default account above them is logged in: its plan, its \"Logged in\" badge and its usage bar.",
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
    description: "Log in was chosen for Work and the CLI gave up: the modal stays open, now saying \"Could not log in to\" the name as a chip, with the named agent in the short explanation and the CLI's multiline reason in an external selectable code block, and Log in again as its main button beside Close.",
    state: notSignedInState,
    loginOutcome: "failed",
    steps: loginFromInline(),
  },
  {
    id: "settings-account-login-timed-out",
    group: GROUP,
    category: "exceptional",
    title: "Account login timed out",
    description: "Log in was chosen for Work and nobody authorized in time: the modal stays open, saying the login to the name as a chip timed out, with no reason beneath, and Log in again as its main button beside Close.",
    state: notSignedInState,
    loginOutcome: "timed_out",
    steps: loginFromInline(),
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
    description: "Choosing Approve everything in the Permission approval row opens a destructive confirmation instead of saving: its title asks whether to approve everything in every Claude Code account, with the agent's name as a chip, the message says every tool will run without asking in every account's sessions, and the danger-coloured button repeats the choice. Cancelling leaves the row on Automatic.",
    state: quotaState,
    steps: bypassSteps,
  },
  {
    id: "settings-accounts-sharing",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, sharing the default configuration",
    description: "Work's settings dialog, with Share default configuration as the row above Disable account: the label with its help button, the description under it, which says turning it on links the account's configuration and history to the default account's directory (it reads as already linked once the switch is on), and the switch at the end, off and usable, with nothing else under it. The help button holds what is shared and never shared, what turning sharing off does and what changing it does to idle sessions. The default account's settings dialog has no such row.",
    state: sharingState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-accounts-sharing-in-use",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, sharing cannot be changed while the account is in use",
    description: "Work's sharing switch is faded but still focusable, and the lines under the row's description, which say why, read that sharing cannot be changed while 2 of the account's sessions are working and to wait for them to finish, and that it cannot be changed while the account is being signed in.",
    state: sharingInUseState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-accounts-sharing-progress",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, sharing change running",
    description: "Work is turning sharing on and changing entries, so its switch is faded but still focusable, with no lock line and no state description under the label. A determinate progress bar under the row reads 3 of 9, with the entry being processed, plugins/, as a chip under it.",
    state: sharingProgressState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-accounts-sharing-progress-restart",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, sharing change ending and resuming sessions",
    description: "Work is turning sharing on and is ending the account's idle sessions first: its switch is faded but still focusable, with no lock line and no state description under the label, and an indeterminate bar under the row reads \"Ending idle sessions…\" in place of the count, with no entry.",
    state: sharingRestartState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-accounts-sharing-help-claude",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code sharing explanation",
    description: "In Work's settings dialog the help button beside Share default configuration is touched with a finger, so the explanation stays open in a popover. It says what is shared, configuration and conversation history, then what is never shared: login credentials, .claude.json (noting it holds user-scope MCP servers) and plans/, the two paths as chips. The popover keeps the same narrow width whatever the window, so a chip that does not fit at the end of a line moves to the next one whole, and no empty chip fragment is left behind at the end of the line above. The next paragraph is what turning sharing off does, and the last that changing sharing ends the account's 2 idle sessions and resumes them afterwards; what turning sharing on does is the row's own description, not in the tip.",
    state: sharingHelpState("claude"),
    steps: openSharingHelp("Work"),
  },
  {
    id: "settings-accounts-sharing-help-grok",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Grok Build sharing explanation",
    description: "The same explanation for a Grok Build account. What is never shared is login credentials, trusted_folders.toml and hooks-paths, the two files as chips. The popover keeps a narrow width, so a chip that does not fit at the end of a line moves to the next one whole, and no empty chip fragment is left at the end of the line above. What is shared, and what turning sharing off does, reads the same as for Claude Code.",
    state: sharingHelpState("grok"),
    steps: openSharingHelp("Work", "grok"),
  },
  {
    id: "settings-accounts-sharing-help-codex",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Codex sharing explanation",
    description: "The same explanation for a Codex account. What is never shared is one sentence, that login credentials are never shared, and no file is named. Personal's switch is off, with nothing under it.",
    state: sharingHelpState("codex"),
    steps: openSharingHelp("Personal", "codex"),
  },
  {
    id: "settings-account-approval-error",
    group: GROUP,
    category: "exceptional",
    title: "Permission approval could not be saved",
    description: "The error alert separates the failed-save summary from the selectable Permission denied diagnostic beneath it. Codex's Permission approval row keeps Follow account configuration behind the alert; acknowledging the error leaves that saved selection intact.",
    state: approvalState,
    accountApprovalError: { code: "store_io_failed", params: { detail: "Permission denied (os error 13)" }, message: "The account settings could not be saved: Permission denied (os error 13)" },
    steps: [
      ...openAccounts,
      (ui) => ui.press(AGENT_LABEL.codex),
      (ui) => ui.press(ui.t("settings.accounts.approval.label", { agent: AGENT_LABEL.codex })),
      (ui) => ui.press(ui.t("settings.accounts.approval.auto")),
    ],
  },
  {
    id: "settings-account-approval-bypass-error",
    group: GROUP,
    category: "exceptional",
    title: "Approve everything could not be saved",
    description: "Confirming Approve everything was refused. The error alert shows the failed-save summary above a selectable Permission denied diagnostic, with the confirmation dialog still open beneath it; the Permission approval row still reads Automatic.",
    state: quotaState,
    accountApprovalError: { code: "store_io_failed", params: { detail: "Permission denied (os error 13)" }, message: "The account settings could not be saved: Permission denied (os error 13)" },
    steps: [
      ...bypassSteps,
      (ui) => ui.confirm(),
    ],
  },
  {
    id: "settings-accounts-sharing-failed",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change could not be finished",
    description: "In Work's settings dialog, turning sharing off did not finish, so the switch sits at its position before the change, on, and the row shows no state description. Under the row one warning alert (not danger-coloured) holds everything about the failure; nothing of it sits outside the warning box. The visible copy is at most two sentences: the title, \"Could not finish turning sharing off\", then \"Some files or folders could not be changed.\", and the one main thing to do, to turn the switch off again to try once more. At the end of the box is a Details button for the rest; there is no Try again button and no separate error alert.",
    state: sharingFailedState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-accounts-sharing-failed-details",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change could not be finished, details open",
    description: "The same failure as above with the Details popover open, holding first the further explanation, the reasons grouped by cause (no permission to change skills/ and 1 more, followed by \"Check that the files and folders involved can be changed and are not locked.\", and plugins/ in use, followed by \"Close other programs using the files involved, such as an agent in a terminal, an editor or a sync tool.\", each one compact line with its fix run on after it), then the raw side: each unfinished entry is one tight row, its chip and what it was left as in words (skills/ and plugins/ left unchanged; the conversation abc, with the titles of the sessions that recorded it, Fix the summary layout and Update the dependencies, joined as a list, left in the default directory) followed by the system's original reason (permission denied, resource busy) as small inline code on the same row, not a separate diagnostic block.",
    state: sharingFailedState,
    steps: [...accountDetails("Work"), (ui) => ui.press(ui.t("error.details"))],
  },
  {
    id: "settings-accounts-sharing-default-moved",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change failed, the default directory has moved",
    description: "In Work's settings dialog the switch sits at its recorded position, on, and the row shows no state description. Under the row one warning alert holds the title \"Could not finish turning sharing on\", the single reason that the default account's directory has moved since sharing was turned on (no entry is named) and the advice to turn the switch off, then on again, and a Details button. Opened, it holds only the raw reason, alone in a block on its own tinted surface, not as inline code.",
    state: sharingDefaultMovedState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-accounts-sharing-no-space",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change failed, not enough disk space",
    description: "In Work's settings dialog, turning sharing on did not finish, so the switch sits at its position before the change, off, and the row shows no state description. Under the row the warning alert holds the title \"Could not finish turning sharing on\", the single cause \"There is not enough disk space.\" and its fix \"Free up disk space.\" as its two visible sentences (no entry is named), and a Details button. Opened, it holds the advice to turn the switch on again to try once more, then the raw reason alone in a block on its own tinted surface.",
    state: sharingNoSpaceState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-accounts-sharing-incomplete",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change left unfinished by a restart",
    description: "In Work's settings dialog the choice is recorded as on, but the daemon was restarted in the middle of the change, so the switch sits at its position before the change, off, and the row shows no state description. Under the row a warning alert holds the title, \"The last attempt to turn sharing on did not finish\", with the advice to turn the switch on again on a line below it, inside the box. There is no Try again button and no Details button, since no entries are known after a restart.",
    state: sharingIncompleteState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-accounts-sharing-refused",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change refused, the account is in use",
    description: "In Work's settings dialog the switch looked free, but pressing it was refused because a session on the account has started working since: an error alert says sharing for Work cannot be changed while 1 of its sessions is working, and advises waiting for the working sessions to finish, then trying again. The switch is still off.",
    state: sharingState,
    accountSharingError: {
      code: "account_in_use",
      params: { account: "a-work", name: "Work", sessions: "1", signing_in: "false" },
      message: "account `Work` is in use: 1 of its sessions is working",
    },
    steps: [...accountDetails("Work"), (ui) => ui.press(ui.t("settings.accounts.sharing.switchLabel", { name: "Work" }))],
  },
  {
    id: "settings-accounts-quota-unavailable",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, Claude Code quota unavailable or out of date",
    description: "Claude Code's default account and Work have nothing to show: their Usage cells are a dash followed by a danger-coloured circled \"!\" (\"Quota unavailable\"), one a timeout and one a failed command; neither has a plan or a status, and their names carry no mark of their own. Client has a plan and reads \"Logged in\", but its Usage cell is a dash followed by a warning-coloured triangle, \"Out of date\", its last reading being too old to show (the scenario after this one opens why). No sentence of error text sits in the table; hovering a mark, or tapping it with a finger, opens one small popover with its name and its reason.",
    state: unavailableQuotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-quota-failure-reason",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, the reason an out-of-date quota carries",
    description: "The warning-coloured triangle after the dash in Client's Usage cell is tapped with a finger: its popover holds why the newer reading failed. Client's settings dialog shows the last windows with the same triangle after \"Updated 12 minutes ago\".",
    state: unavailableQuotaState,
    steps: [...openAccounts, (ui) => ui.touchPressStatusMark(describing("the out-of-date mark", (name) => name.startsWith(ui.t("quota.stale", { age: "\u0000" }).split("\u0000")[0])))],
  },
  {
    id: "settings-accounts-grok-quota-unavailable",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, Grok Build binary not found",
    description: "Grok Build's default account has a dash in its Usage cell followed by a danger-coloured circled \"!\", and no plan or status; the mark's reason names the missing binary and PATH as code.",
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
    description: "The Codex tab is selected and carries a warning icon after its name; its Permission approval row is there as usual. Neither the default account nor Personal has a plan, a status or a usage reading: their Status cells are empty and their Usage cells dashes.",
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
    description: "Claude Code and Codex are listed, Codex followed by a grey Not installed badge; Grok Build, installed but not added, is not. Codex is badged but still selectable, unlike the agent pickers elsewhere, because an account may be set up before its agent is installed.",
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
  {
    id: "settings-account-delete-sharing-failed",
    group: GROUP,
    category: "exceptional",
    title: "Remove account refused, its sharing could not be turned off",
    description: "Confirming the removal of Client, which shares the default configuration, was refused because turning its sharing off did not finish. The confirmation dialog stays open and the refusal is reported: it says what could not be finished for Client and gives the reason by cause: no permission to change skills/ and 1 more, followed by \"Check that the files and folders involved can be changed and are not locked.\" on the same line, with the raw entries behind a collapsed Details disclosure; opened, each entry is one tight row, its sentence followed by the system's reason as small inline code. Behind the dialog Client is still listed.",
    state: sharingState,
    accountSharingError: SHARING_REMOVAL_ERROR,
    steps: [
      ...openAccounts,
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Client", agent: "Claude Code" })),
      (ui) => ui.press(ui.t("common.remove")),
      (ui) => ui.confirm(),
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
    description: "Once Add is pressed, the daemon's refusal, naming the account that already uses the directory, is the error alert dialog's, raised over the dialog; the config directory field and the dialog itself show no error.",
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
