import { DEFAULT_SETTINGS } from "../../defaultSettings";
import { AGENT_LABEL } from "../../agents";
import { describing, type Ui } from "../interact";
import type { AccountLogin, AccountSharing, AccountTrashOutcome, Agent, AgentAvailability } from "../../protocol";
import type { AccountTrashAnswer, Scenario, Step } from "../scenario";
import { accountOf, ALL_AGENTS, SAMPLE, accountSharingOf, minutesAgo, sessionOf, snapshotState } from "./builders";
import { noneAddedState } from "./empty";
import { agentAvailability } from "./liveChanges";
import type { FixtureError } from "./projectFiles";
import { USED_UP_MIX, manyClaudeAccounts, quotaFailure, quotaOf, quotaWindow } from "./quotas";

const GROUP = "Settings";

const { console: console_, web, api, sessions } = SAMPLE;

/** Sign-in emails of the accounts that have one; LONG_EMAIL is longer than the name column of the accounts table. */
const DEFAULT_EMAIL = "dev@octoboard.example";
const WORK_EMAIL = "dev@work.octoboard.example";
const LONG_EMAIL = "platform-engineering.release-automation@internal-platform.acme-corporation.example";

const state = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions,
  trustedDirectories: ["/Users/dev/code", "/Users/dev/work/clients/acme-corporation/internal-platform"],
});

const browserState: Scenario["state"] = {
  ...state,
  settings: {
    ...DEFAULT_SETTINGS,
    sidebar_order_by_status: true,
    ask_questions_as_choices: true,
    added_agents: ["claude", "codex"],
    default_clone_dir: "/Users/dev/Projects",
    accounts: [
      accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
      accountOf("a-long", "claude", "Client", "/Users/dev/work/clients/acme-corporation/internal-platform/.claude-config"),
      accountOf("a-codex", "codex", "Personal", "/Users/dev/.codex-personal"),
    ],
  },
  agentAvailability: new Map<Agent, AgentAvailability>([
    ["claude", { agent: "claude", availability: "available", default_account_dir: "/Users/dev/.claude" }],
    ["codex", { agent: "codex", availability: "unavailable", default_account_dir: "/Users/dev/.codex" }],
    ["grok", { agent: "grok", availability: "not_determined", default_account_dir: null }],
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
      accountOf("a-codex-auto", "codex", "Automation", "/Users/dev/.codex-automation"),
      accountOf("a-codex-user", "codex", "Production", "/Users/dev/.codex-production"),
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

/** `quotaState` with Claude Code not among the added agents, and Claude compatibility never chosen for any agent. */
const noClaudeCodeState: Scenario["state"] = {
  ...quotaState,
  settings: { ...quotaState.settings!, added_agents: ["codex", "grok"] },
};

/** Claude Code's accounts in every state a person meets (`USED_UP_MIX`), the default account being the usable one.
 * Client is left out; Codex and Grok stay as they were. */
const usedUpMixState: Scenario["state"] = {
  ...quotaState,
  settings: {
    ...quotaState.settings!,
    accounts: [...USED_UP_MIX.accounts, ...quotaState.settings!.accounts.filter((account) => account.agent !== "claude")],
  },
  accountQuotas: [...quotaState.accountQuotas!.filter((quota) => quota.agent !== "claude"), ...USED_UP_MIX.quotas],
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
    quotaOf("claude", null, null, [], { unavailable: quotaFailure("quota_timed_out", { agent: "Claude Code" }, "Claude Code took too long to report its usage.") }),
    quotaOf("claude", "a-work", null, [], {
      unavailable: quotaFailure("quota_command_failed", { agent: "Claude Code", detail: "exit status 1: Error: not logged in" }, "Claude Code could not report its usage: exit status 1: Error: not logged in"),
    }),
    quotaOf("claude", "a-long", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)], {
      fetched_at: minutesAgo(12),
      unavailable: quotaFailure("quota_timed_out", { agent: "Claude Code" }, "Claude Code took too long to report its usage."),
    }),
    quotaOf("grok", null, null, [], {
      unavailable: quotaFailure("quota_binary_not_found", { binary: "grok" }, "grok was not found, so usage cannot be read."),
    }),
  ],
};

/** A model's display name as long as one can be, long enough to wrap in a narrow window. */
const LONG_MODEL_NAME = "Claude Opus Extended Context Research Preview";

/** Claude Code's default account and Work turned off for new sessions, and Trial, signed out and turned off too;
 * Client, with no reading yet, stays on. Work keeps its reading, which its row keeps showing while it is off. */
const disabledState: Scenario["state"] = {
  ...quotaState,
  settings: {
    ...quotaState.settings!,
    disabled_default_accounts: ["claude"],
    accounts: [
      ...quotaState.settings!.accounts.map((account) => account.id === "a-work" ? { ...account, disabled: true } : account),
      accountOf("a-trial", "claude", "Trial", "/Users/dev/.claude-trial", { disabled: true }),
    ],
  },
  accountQuotas: [
    ...quotaState.accountQuotas!,
    quotaOf("claude", "a-trial", null, [], {
      unavailable: quotaFailure("quota_not_signed_in", { agent: "Claude Code" }, "Claude Code is not signed in: Error: not logged in"),
    }),
  ],
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
    ], { fetched_at: minutesAgo(7), email: WORK_EMAIL }),
  ],
};

/** `detailsWorkState` with Work signed in under an address far longer than the dialog is wide, to see what the heading
 * does with it. */
const longEmailDetailsState: Scenario["state"] = {
  ...detailsWorkState,
  accountQuotas: detailsWorkState.accountQuotas!.map((quota) => quota.agent === "claude" && quota.account === "a-work" ? { ...quota, email: LONG_EMAIL } : quota),
};

/** Work whose latest read failed for a reason other than being signed out, so it has no plan, windows or login state to
 * show, though the daemon still knows the email its credentials name. */
const unknownLoginEmailState: Scenario["state"] = {
  ...quotaState,
  accountQuotas: [
    ...quotaState.accountQuotas!.filter((quota) => quota.account !== "a-work" || quota.agent !== "claude"),
    quotaOf("claude", "a-work", null, [], {
      email: WORK_EMAIL,
      unavailable: quotaFailure("quota_timed_out", { agent: "Claude Code" }, "Claude Code took too long to report its usage."),
    }),
  ],
};

/** Claude Code's default account signed in with an email, for the dialogs that name it. */
const defaultEmailState: Scenario["state"] = {
  ...quotaState,
  accountQuotas: quotaState.accountQuotas!.map((quota) => quota.agent === "claude" && quota.account === null ? { ...quota, email: DEFAULT_EMAIL } : quota),
};

/** `defaultEmailState` with the name of Claude Code's default account's model-specific window as long as a model's can
 * be, for the account details dialog, whose lines wrap in a narrow window. */
const longModelNameState: Scenario["state"] = {
  ...defaultEmailState,
  accountQuotas: defaultEmailState.accountQuotas!.map((quota) =>
    quota.agent === "claude" && quota.account === null
      ? { ...quota, windows: quota.windows.map((window) => (window.kind === "weekly_model" ? { ...window, label: LONG_MODEL_NAME } : window)) }
      : quota,
  ),
};

/** Claude Code's accounts with their sign-in emails: the default account's, Work's, Client's (an address longer than
 * the name column), and Trial, signed out and so with none. Codex and Grok keep their readings as they were. */
const emailsState: Scenario["state"] = {
  ...quotaState,
  settings: {
    ...quotaState.settings!,
    accounts: [...quotaState.settings!.accounts, accountOf("a-trial", "claude", "Trial", "/Users/dev/.claude-trial")],
  },
  accountQuotas: [
    ...defaultEmailState.accountQuotas!.filter((quota) => quota.agent !== "claude" || quota.account === null),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 34, 150), quotaWindow("weekly", 21, 2 * 24 * 60 + 5 * 60)], { fetched_at: minutesAgo(7), email: WORK_EMAIL }),
    quotaOf("claude", "a-long", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)], { email: LONG_EMAIL }),
    quotaOf("claude", "a-trial", null, [], {
      unavailable: quotaFailure("quota_not_signed_in", { agent: "Claude Code" }, "Claude Code is not signed in: Error: not logged in"),
    }),
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
function loginFromInline(): Step[] {
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

/** The `item` (Details or Settings) of the actions menu of the account `name` of `agent` on the tab in view (the
 * default account's name is the language's own); a menu item, since the rail has a Settings button of its name. */
const openAccountDialog = (item: "settings.accounts.detailsItem" | "settings.accounts.settingsItem", name: string | ((ui: Ui) => string), agent: Agent): Step => async (ui) => {
  await ui.press(ui.t("settings.accounts.actionsLabel", { name: typeof name === "string" ? name : name(ui), agent: AGENT_LABEL[agent] }));
  await ui.pressMenuItem(ui.t(item));
};
const openDetails = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step => openAccountDialog("settings.accounts.detailsItem", name, agent);
const openSettings = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step => openAccountDialog("settings.accounts.settingsItem", name, agent);
const defaultAccount = (ui: Ui) => ui.t("settings.accounts.defaultName");

/** Settings → Agent accounts with the read-only details of the account `name` in front. */
const accountDetails = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step[] => [...openAccounts, openDetails(name, agent)];

/** Settings → Agent accounts with the settings (the switches) of the account `name` in front. */
const accountSettings = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step[] => [...openAccounts, openSettings(name, agent)];

const openSharingHelp = (name: string, agent: Agent = "claude"): Step[] => [
  ...accountSettings(name, agent),
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
function sharingAccounts(work: { share: boolean; incomplete?: boolean }, client: { share: boolean; incomplete?: boolean }): NonNullable<Scenario["state"]> {
  const base = browserState!;
  return {
    ...base,
    settings: {
      ...base.settings!,
      added_agents: ["claude"],
      accounts: [
        accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work", { share_default_config: work.share, share_default_config_incomplete: work.incomplete ?? false }),
        accountOf("a-client", "claude", "Client", "/Users/dev/.claude-client", { share_default_config: client.share, share_default_config_incomplete: client.incomplete ?? false }),
      ],
    },
  };
}

/** Work does not share and Client does, with no working session or sign-in in the way. */
const sharingState = sharingAccounts({ share: false }, { share: true });

/** `sharingState` with a renamed default account and Work signed in under an email, for its settings heading. */
const sharingEmailState: Scenario["state"] = {
  ...sharingState,
  settings: { ...sharingState.settings!, default_account_names: { claude: RENAMED_DEFAULT } },
  accountQuotas: [quotaOf("claude", "a-work", "pro", [quotaWindow("session", 38, 150), quotaWindow("weekly", 66, 2 * 24 * 60 + 5 * 60)], { email: WORK_EMAIL })],
};

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
      accounts: [accountOf(`a-${agent}-share`, agent, agent === "codex" ? "Personal" : "Work", `/Users/dev/.${agent}-work`)],
    },
    agentAvailability: new Map(base.agentAvailability).set(agent, { agent, availability: "available", default_account_dir: `/Users/dev/.${agent}` }),
    // Claude Code's account has 2 idle sessions, which a change would end first and resume after: the last line of its explanation.
    ...(agent === "claude" && { accountSharing: accountSharingOf([{ account: `a-${agent}-share`, idle_sessions: 2 }]) }),
  };
}

/** What turning sharing off for Work left unfinished: a directory entry, a conversation two sessions of this
 * scenario record (so the popover can show their titles), a busy entry, and one the system gave no known cause for. */
const SHARING_FAILURE_ITEMS: NonNullable<AccountSharing["failure"]>["items"] = [
  { item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied" },
  { item: "abc", left: "unchanged", cause: "permission_denied", detail: "permission denied", sessions: ["s-web-1", "s-web-2"] },
  { item: "plugins/", left: "unchanged", cause: "busy", detail: "resource busy" },
  { item: "settings.json", left: "backed_up", cause: "other", detail: "input/output error" },
];

const SHARING_FAILURE_DETAIL = "`skills/` left unchanged (permission denied); the conversation `abc` of session s-web-1, s-web-2 left in the default directory (permission denied); `plugins/` left unchanged (resource busy); `settings.json` backed up, with nothing linked in its place (input/output error)";

/** Turning sharing off for Work failed: the choice is recorded as off and incomplete, and the daemon still holds the
 * failure; the account has 2 idle sessions, which the popover says a change ends. */
const sharingFailedState: Scenario["state"] = {
  ...browserState,
  settings: {
    ...browserState.settings!,
    added_agents: ["claude"],
    accounts: [
      accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work", { share_default_config_incomplete: true }),
    ],
  },
  accountSharing: accountSharingOf([{ account: "a-work", idle_sessions: 2, failure: { share: false, cause: "other", items: SHARING_FAILURE_ITEMS, detail: SHARING_FAILURE_DETAIL } }]),
};

/** Turning sharing on for Work failed because the default account's directory has moved since sharing was
 * turned on: no entry is named, and the raw reason is all the popover holds below the advice. The failure was
 * not rolled back, so the recorded choice, on, is where the switch sits. */
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

/** Work turns sharing on and the change does not finish. */
const SHARING_TURN_ON_ERROR = {
  code: "account_sharing_failed",
  params: {
    account: "a-work",
    name: "Work",
    cause: "permission_denied",
    items: JSON.stringify([
      { item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied" },
    ]),
    detail: "`skills/` left unchanged (permission denied)",
  },
  message: "sharing the default configuration with account `Work` did not complete: `skills/` left unchanged (permission denied)",
};

/** Work's removal confirmation, reached from its ⋮ menu on Claude Code's tab. */
const removeWork: Step[] = [
  (ui) => ui.press(ui.t("rail.settings")),
  (ui) => ui.press(ui.t("settings.section.accounts")),
  (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Work", agent: AGENT_LABEL.claude })),
  (ui) => ui.press(ui.t("common.remove")),
];

/** The removal confirmation with the Trash option ticked. */
const removeWorkToTrash: Step[] = [...removeWork, (ui) => ui.press(ui.t("settings.accounts.removeTrash"))];

/** Work's config directory. */
const WORK_DIR = browserState.settings!.accounts[0].config_dir;

/** Work with live sessions on its directory: three recorded under its id (working, waiting for the user, starting) and one
 * that only has its directory (idle). An archived session of Work and a session of Client are not among those that block. */
const workRunningState: Scenario["state"] = (() => {
  const [consoleSession, web1, web2, api1] = SAMPLE.sessions;
  return {
    ...browserState,
    sessions: new Map([
      { ...consoleSession, account_id: "a-long" },
      { ...web1, account_id: "a-work" },
      { ...web2, config_dir: WORK_DIR },
      { ...api1, account_id: "a-work" },
      sessionOf("s-web-3", SAMPLE.console.id, SAMPLE.web.id, "Draft the release notes", "starting", { account_id: "a-work" }),
      sessionOf("s-web-old", SAMPLE.console.id, SAMPLE.web.id, "Earlier layout work", "archived", { account_id: "a-work" }),
    ].map((session) => [session.id, session])),
  };
})();

/** What the daemon's quick look at Work's directory answers once the Trash option is ticked, for each refusal the dialog
 * prevents: the removal is disabled with the cause stated under the option, one scenario each. */
const TRASH_PREVENTED = [
  {
    key: "protected",
    outcome: { status: "protected" },
    title: "Remove account to the Trash prevented, directory protected",
    description: "A warning alert under the ticked option says the directory is your home folder, the root of a disk or holds Octoboard's own data, so it cannot be moved to the Trash; the sentence stops at the cause. Remove is disabled.",
  },
  {
    key: "shared-account",
    outcome: { status: "shared", kind: "account", name: "Client", agent: "Claude Code" },
    title: "Remove account to the Trash prevented, directory shared with an account",
    description: "A warning alert under the ticked option says the directory overlaps the directory of the Claude Code account Client, the agent and the account each a chip, so it cannot be moved to the Trash. Remove is disabled.",
  },
  {
    key: "shared-default",
    outcome: { status: "shared", kind: "default_account", agent: "Claude Code" },
    title: "Remove account to the Trash prevented, directory is the default account's",
    description: "A warning alert under the ticked option says the directory is, holds or lies inside the default Claude Code account's directory (the agent a chip), so it cannot be moved to the Trash. No account is named. Remove is disabled.",
  },
  {
    key: "shared-project",
    outcome: { status: "shared", kind: "project", name: "Website" },
    title: "Remove account to the Trash prevented, directory holds a project",
    description: "A warning alert under the ticked option says the directory holds the folder of the project Website (a chip), so it cannot be moved to the Trash. Remove is disabled.",
  },
  {
    key: "unlookable",
    outcome: { status: "unlookable" },
    title: "Remove account to the Trash prevented, directory could not be looked at",
    description: "A warning alert under the ticked option says the directory could not be accessed, so it cannot be moved to the Trash. Remove is disabled.",
  },
  {
    key: "unreadable",
    outcome: { status: "unlookable", cause: "unreadable" },
    title: "Remove account to the Trash prevented, directory could not be read",
    description: "A warning alert under the ticked option says the directory could not be read, so it cannot be checked for files of other accounts or projects and cannot be moved to the Trash. Remove is disabled.",
  },
  {
    key: "other-unlooked",
    outcome: { status: "unlookable", cause: "other_unlooked" },
    title: "Remove account to the Trash prevented, other accounts' and projects' directories not checked",
    description: "A warning alert under the ticked option says it could not be checked whether the directory holds or overlaps another account's or a project's directory, so it cannot be moved to the Trash. Remove is disabled.",
  },
] as const satisfies readonly { key: string; outcome: AccountTrashOutcome; title: string; description: string }[];

/** What the daemon refuses, once Remove is pressed, a removal that asks for Work's directory to be moved to the Trash
 * with: only what the quick look before could not foresee (a state that changed since, a look that outran its time, the
 * move itself failing), one scenario each. */
const TRASH_REFUSALS = [
  {
    key: "sessions-started",
    error: { code: "account_has_running_sessions", params: { account: "a-work", name: "Work", sessions: "2" }, message: "account `Work` still has 2 sessions with a process" },
    inspection: undefined,
    title: "Remove account to the Trash refused, sessions started since",
    description: "Work's confirmation was opened with no live sessions, so the option was ticked and Remove was available; two sessions started on Work meanwhile, and the daemon refused. An error dialog titled Could not remove the account, over the confirmation, says only that Account Work still has 2 sessions that are running or starting, so its directory cannot be moved to the Trash. Behind it the dialog is unchanged and Work is still listed.",
  },
  {
    key: "shared-account",
    error: { code: "account_directory_shared", params: { kind: "account", name: "Client", agent: "Claude Code" }, message: "the directory is shared with another account" },
    inspection: [{ status: "ok" }, { status: "shared", kind: "account", name: "Client", agent: "Claude Code" }],
    title: "Remove account to the Trash refused, directory shared since",
    description: "The directory was not shared when the option was ticked and Remove was available; Client began using it meanwhile. An error dialog titled Could not remove the account, over the confirmation, says only that the directory overlaps the directory of the Claude Code account Client (the agent and the account each a chip), so it cannot be moved to the Trash. The confirmation asks again once the error is closed, so then the same reason stands under the option, announced, and Remove is disabled.",
  },
  {
    key: "failed",
    error: { code: "account_directory_trash_failed", params: { path: WORK_DIR, detail: "Operation not permitted (os error 1)" }, message: "could not move the directory to the Trash: Operation not permitted (os error 1)" },
    inspection: undefined,
    title: "Account directory could not be moved to the Trash",
    description: "An error dialog titled Could not remove the account, over the confirmation, says that the directory could not be moved to the Trash and, as the system's text names it (Operation not permitted, os error 1), that access was denied, with no remedy; the system's own text is in its Error details disclosure, closed here.",
  },
  {
    key: "unanswered",
    error: { code: "account_directory_trash_failed", params: { path: WORK_DIR, detail: "it did not respond in time", cause: "unanswered" }, message: "could not move the directory to the Trash: it did not respond in time" },
    inspection: { status: "undecided" },
    title: "Account directory did not respond in time",
    description: "The quick look outran its time, so the option left Remove available; the full check on Remove did not get an answer either. An error dialog titled Could not remove the account, over the confirmation, says the directory, your home folder or Octoboard's data folder did not respond in time, so the directory could not be checked. The daemon's own text is in the closed Error details disclosure. The check asked again once the error is closed does not answer in time either, so the dialog stays as it was, Remove available to try again.",
  },
  {
    key: "unreadable",
    error: { code: "account_directory_trash_failed", params: { path: WORK_DIR, detail: "it could not be read", cause: "unreadable" }, message: "could not move the directory to the Trash: it could not be read" },
    inspection: [{ status: "undecided" }, { status: "unlookable", cause: "unreadable" }],
    title: "Account directory could not be read when removing",
    description: "The quick look outran its time, so the option left Remove available; the full check on Remove could not read the directory. An error dialog titled Could not remove the account, over the confirmation, says the directory could not be read, so it could not be checked for files of other accounts or projects. The daemon's own text is in the closed Error details disclosure. The confirmation asks again once the error is closed, so then the same reason stands in a warning alert under the option, announced, and Remove is disabled.",
  },
  {
    key: "other-unlooked",
    error: { code: "account_directory_trash_failed", params: { path: WORK_DIR, detail: "it could not be checked", cause: "other_unlooked" }, message: "could not move the directory to the Trash: it could not be checked" },
    inspection: [{ status: "undecided" }, { status: "unlookable", cause: "other_unlooked" }],
    title: "Account directory could not be checked against other directories when removing",
    description: "The quick look outran its time, so the option left Remove available; the full check on Remove could not tell whether the directory holds or overlaps another account's, the default account's or a project's directory. An error dialog titled Could not remove the account, over the confirmation, says only that, and that the directory cannot be moved to the Trash. The daemon's own text is in the closed Error details disclosure. The confirmation asks again once the error is closed, so then the same reason stands in a warning alert under the option, announced, and Remove is disabled.",
  },
  {
    key: "signing-in",
    error: { code: "account_directory_trash_failed", params: { path: WORK_DIR, detail: "its sign-in did not end in time", cause: "signing_in" }, message: "could not move the directory to the Trash: its sign-in did not end in time" },
    inspection: undefined,
    title: "Account directory could not be moved to the Trash, login did not end",
    description: "Remove was pressed while Work's login was running and it did not end in time. An error dialog titled Could not remove the account, over the confirmation, says that the account's login did not end in time, so the directory could not be moved to the Trash. The daemon's own text is in the closed Error details disclosure. Closing the error leaves the dialog as it was, with no new check of the directory, Remove available to try again.",
  },

] as const satisfies readonly { key: string; error: FixtureError; inspection: AccountTrashAnswer | readonly AccountTrashAnswer[] | undefined; title: string; description: string }[];

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
    description: "The \"Order the sidebar by status\" switch below Markdown style is off, and its description now starts with \"Once this is on\" and says only what the switch does when on (sessions and projects move as their status changes, the ones waiting for you first after the pinned ones), without describing the order it keeps now. Switch it on and off to read both descriptions; the sidebar re-orders at once (see Console sessions, ordering by status switched off).",
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
    id: "settings-permissions-request-failed",
    group: GROUP,
    category: "exceptional",
    title: "Permissions, the request did not go through",
    description: "Grant on Screen Recording cannot make its request, which is an unexpected failure: an error alert titled \"Could not request the permission\" says the request for Screen Recording did not go through and to grant it yourself in System Settings, choose Privacy & Security, then choose Screen & System Audio Recording, adding Octoboard with the + button if it is not listed. The title is plain text; Screen Recording, System Settings, Privacy & Security, Screen & System Audio Recording, Octoboard and the + in the body are each a chip. Beneath the body sit the report guidance (a bug that cannot be resolved here, with Report issue to file it) and a closed Error details disclosure holding the cause; the buttons are Report issue and OK. After OK the row still offers Grant, not pending and without System Settings guidance.",
    state,
    macosPermissions: { screenRecording: false, accessibility: false, screenRecordingAtLaunch: false },
    macosRequestError: "the shell has no such command",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
      (ui) => ui.press(ui.t("settings.permissions.grant")),
    ],
  },
  {
    id: "settings-permissions-system-open-failed",
    group: GROUP,
    category: "exceptional",
    title: "Permissions, System Settings did not open",
    description: "After Grant is refused on Screen Recording and its Open System Settings is pressed, the system does not open the pane: an error alert titled \"Could not open System Settings\" says it did not open and to open it yourself, choose Privacy & Security, then choose Screen & System Audio Recording. The title is plain text; System Settings, Privacy & Security and Screen & System Audio Recording in the body are each a chip. OK is its only button: no recovery action and no Error details.",
    state,
    macosPermissions: { screenRecording: false, accessibility: false, screenRecordingAtLaunch: false },
    macosSettingsOpenError: "the system did not open x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
      (ui) => ui.press(ui.t("settings.permissions.grant")),
      (ui) => ui.press(ui.t("settings.permissions.openSettings")),
    ],
  },
  {
    id: "settings-agent-permissions",
    group: GROUP,
    category: "normal",
    title: "Agent behavior",
    description: "The lead line sits under the section heading, above a rule that opens the five rows: the session-request, resume-request, account-switch and handoff switches, all off, then Ask questions as choices, on, which is its default. Each of the four approval descriptions starts with \"Once this is on\" and says only what being on allows; the resume one names reopening, resuming and resuming a conversation in a new session, and none says what happens until then. The last row's description states what being on does, ending with the sentence that a change reaches sessions started afterwards. Switch a row to read its other description; none moves another.",
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
    title: "Agent behavior, approvals automatic, questions in text",
    description: "The four approval switches are on, each description now stating the behaviour instead of starting with \"Once this is on\", each saying only what being on allows, with no advice to turn it off, while Ask questions as choices is off, its description starting with \"Once this is on\" and ending with the sentence that a change reaches sessions started afterwards.",
    state: {
      ...browserState,
      settings: { ...browserState.settings!, auto_approve_session_requests: true, auto_approve_resume_requests: true, auto_approve_account_switches: true, auto_approve_handoffs: true, ask_questions_as_choices: false },
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
    description: "The section list reads General, Git, Agent accounts, Agent behavior, Trusted folders, Permissions, Notifications, Keyboard shortcuts, Configuration, About. Git has three headed groups holding seven rows. Repository: Default clone directory (its explanation describes when the chosen directory is used, and the path appears in the input), then Automatically pull repositories, off, whose description starts with \"Once this is on\" and says a project's branch is pulled when it is behind its upstream and can be fast-forwarded, with a small muted \"?\" after the label holding the sentence about the remote being checked. Development: Where sessions develop reads In a worktree of its own, with the explanation for that choice; Squash commits before delivery is off, its explanation saying what turning it on would do; Rebase before delivery is on, with the explanation that says what being on does; Delivery, last, reads Ask each time in a select at the end of its row. Protection: Revert gate is off, with a small muted \"?\" at the end of its label and an explanation starting \"Once this is on, in a project session\".",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.git")),
    ],
  },
  {
    id: "settings-git-changed",
    group: GROUP,
    category: "normal",
    title: "Settings: git, development on a branch and the other items changed from the defaults",
    description: "Automatically pull repositories is on, its description now stating the behaviour (\"A project's branch is pulled when...\") instead of starting with \"Once this is on\". Where sessions develop reads On a branch of its own, with the explanation that the branch shares the project directory, so it suits one session at a time. Squash commits before delivery is on, its explanation saying the commits are consolidated before a merge, push or pull request; Rebase before delivery is off, its explanation saying what turning it on would do; Delivery reads Open a pull request, the longest of its three labels, which fits the select without being cut. Revert gate, in its own group, is on, its explanation starting \"In a project session\" and saying a change takes effect at once, running sessions included.",
    state: { ...browserState, settings: { ...browserState.settings!, auto_pull_repositories: true, git_workflow: { development: "branch", squash_before_delivery: true, rebase_before_merge: false, delivery: "pull_request", revert_gate: true } } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.git")),
    ],
  },
  {
    id: "settings-git-in-place",
    group: GROUP,
    category: "normal",
    title: "Settings: git, development on the current branch",
    description: "Where sessions develop reads On the current branch, its explanation saying a session is given no rule for branches or worktrees and that squashing, rebase and delivery are shown once sessions work on a branch of their own. Squash commits before delivery, Rebase before delivery and Delivery are not shown, though the stored values (squashing on, rebase on, delivery Open a pull request) are kept and come back with another choice. Revert gate is still shown, on, in the Protection group.",
    state: { ...browserState, settings: { ...browserState.settings!, git_workflow: { development: "in_place", squash_before_delivery: true, rebase_before_merge: true, delivery: "pull_request", revert_gate: true } } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.git")),
    ],
  },
  {
    id: "settings-git-revert-gate-help",
    group: GROUP,
    category: "normal",
    title: "Settings: git, revert gate help tip open",
    description: "The mouse rests on the \"?\" after the Revert gate label, so its popover is open: it says what the gate blocks and how to undo a commit on purpose, with git revert or a \"Reverts: <sha>\" line in the undoing commit's message, both as monospace chips. No focus has moved; the text can be selected.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.git")),
      (ui) => ui.hoverStatusMark(ui.t("settings.gitWorkflow.revertGate.help")),
    ],
  },
  {
    id: "settings-git-clone-dir-tooltip",
    group: GROUP,
    category: "normal",
    title: "Settings: git, the clone directory's full path in a tooltip",
    description: "The mouse rests on the Default clone directory field, whose path is written from the home directory (~/Projects/clients/...), so a tooltip above it gives the full path from the root (/Users/dev/Projects/clients/...), long enough to wrap onto a second line. The path reads left to right, starting with its leading \"/\" at the left: under a right-to-left language the tooltip does not move it to the far end, and both lines start at the tooltip's left edge, the second not sitting at the right.",
    state: { ...browserState, settings: { ...browserState.settings!, default_clone_dir: "/Users/dev/Projects/clients/acme-corporation/internal-platform/services/billing" } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.git")),
      (ui) => ui.hoverField(ui.t("settings.cloneDir.label"), "/Users/dev/Projects/clients/acme-corporation/internal-platform/services/billing"),
    ],
  },
  {
    id: "settings-accounts",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code tab",
    description: "Claude Code's tab. Account rows have drag grips in a separate unlabelled first column, including Default; the Account header aligns with the names, and their menus offer Move earlier and Move later. Drag a grip to save a new order, or use the menu actions with a keyboard; Escape cancels a drag. The agents are tabs in an underline strip, unlike the filled tabs that switch the Settings sections, each with its own small ⋮ button at its end and the strip ending in two icon buttons, Add agent and Add account. Under it, one Permission approval row for the whole agent: its label with a help button, \"Applies to every Claude Code account\" under it and the selector at the end, on Automatic; there is no Claude compatibility row, which only the other agents' tabs have. Then one table lists the agent's accounts, Work, Default and Client in the saved order, under the headers Account, Usage, Status and Disable (with a small \"?\") and an unlabelled last column holding only the account's ⋮ menu. Each row has the name with its plan badge; a two-layer usage bar like the one the account pickers show, the session window's over the weekly one, with no numbers and, unlike the pickers' bars, no hairline edge (Work has reached its limit, so its bar is empty and grey, also where use went past 100%, followed in the same cell by a warning-coloured \"Limit reached\" badge, while its name stays muted with no badge beside it, unlike the rail's usage panel; the default account and Client have neither); a soft success \"Logged in\" badge in the Status cell after it; and the Disable switch, off. Client has no reading yet, so it has no plan or badge and its Usage cell says it is being read. Above the tab strip, the account-selection switch is followed directly by the \"Start usage windows automatically\" switch, both off, each description starting with \"Once this is on\"; the usage-window one is a single short sentence on what the switch does and its usage cost, with a small muted \"?\" at the end of its label. Remove Work or Client: the row fades out while sliding to the right while the remaining rows slide into place. \"Codex uninstalled\" among the \"Live changes\" buttons is the machine finding Codex missing while the strip is shown: its tab gains the warning mark and the tooltip that says why, widening in place while the tab after it slides along; \"Codex installed\" takes the mark away again. With reduced motion the strip changes at once. Choose 560px to see the narrow layout: below about 640px the Permission approval row stays within the section, and the table is wider than the section, so it scrolls sideways, its columns keeping their widths, the later columns cut at the edge and the ⋮ column pinned at the end with a hairline at its start while there is more beside it; the tab strip, the two switches above it and the tab buttons fit the width, and the scroller is a focusable region while it overflows.",
    state: { ...quotaState, settings: { ...quotaState.settings!, account_order: { claude: ["a-work", null, "a-long"], codex: ["a-codex-auto", "a-codex", "a-codex-user", null] } } },
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
    id: "settings-accounts-used-up",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code tab with accounts at their limit in different windows",
    description: "The table lists the default account usable (Max, with most of both windows left), then four accounts that have reached their limit and recede: each has a muted name with only its plan badge beside it, and a Usage cell holding a grey usage bar in place of Claude Code's colour, followed by a warning-coloured \"Limit reached\" badge. Work (Team) is at its limit in its session window, but its latest read failed, so after the badge its cell carries the warning-coloured \"Out of date\" triangle, the grey bar being the last windows it read 38 minutes ago; Second is at its limit in its weekly window (a full top layer over an empty one); Third and Fourth by their session windows (an empty top layer over a nearly empty and a nearly full one).",
    state: usedUpMixState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-accounts-disabled",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, accounts turned off",
    description: "The default account, Work and Trial are turned off: their Disable switches are on and their Status cells read a neutral \"Disabled\" badge in place of the login state, while the rest of each row is unchanged: the default account keeps its name, plan and usage bar, and Work its muted name and grey bar with \"Limit reached\". Trial is signed out, so it has no Log in icon beside \"Disabled\", where a signed-out account that is on has one; its menu still offers Log in. Client's switch is off, and its Status cell has no badge as it has no reading yet. The header's \"?\" next to Disable explains that a disabled account no longer appears in account choices or usage displays, except while a running session still uses it, new sessions do not use it, and running sessions keep running. Turn Disable off for any of them and the switch moves back; turn it on for Client and it does.",
    // Four rows under the three switches and the tab strip: taller than the default window, whose body would scroll.
    height: 1000,
    state: disabledState,
    steps: openAccounts,
  },
  {
    id: "settings-account-details",
    group: GROUP,
    category: "normal",
    title: "Account details, an account the user owns",
    description: "Work's details, opened from the first entry of its ⋮ menu, in a read-only modal titled Account details, with no footer button and no switch or Log in button. The body starts with the bold name Work followed on the same line by the Pro plan badge and the \"Logged in\" badge, the muted email dev@work.octoboard.example on the line under them, and the config directory is the line under that. The Usage section has \"Updated 7 minutes ago\" at the end of its heading and the rail's meters: Session, as the two-layer bar with Weekly · all models under it, each line saying what remains and when it resets, then Fable weekly with its own bar.",
    state: detailsWorkState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-account-details-default",
    group: GROUP,
    category: "normal",
    title: "Account details, the default account",
    description: "The default account's details, in a modal titled Account details: the body starts with the bold name Default followed on the same line by the Max badge and \"Logged in\", its muted email dev@octoboard.example on the line under them, and the config directory is the line under that, ending right after the path with the small \"?\" that explains the default account. Usage shows all three windows, the model-specific one under a model name as long as one can be. Choose 560px to see the details stack below 640px: that name wraps beside its reset time on the same line, with its bar under it. Nothing in the dialog can be changed: the Disable and sharing switches are in Account settings.",
    state: longModelNameState,
    steps: accountDetails(defaultAccount),
  },
  {
    id: "settings-account-details-long-email",
    group: GROUP,
    category: "normal",
    title: "Account details, an email longer than the dialog",
    description: "Work's details with a sign-in email far wider than the dialog: the first line keeps the name Work with the Pro and \"Logged in\" badges beside it, and the email is on the line under it, muted, wrapping onto further lines inside the dialog's width instead of pushing the badges down or out of it. The config directory is the line under the email.",
    state: longEmailDetailsState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-account-details-not-logged-in",
    group: GROUP,
    category: "normal",
    title: "Account details, an account not logged in",
    description: "Work's details, which has never been logged in: after the name, in the body's first line, with no email, the soft warning \"Not logged in\" badge, with no Log in button in the dialog (logging in is the row's and the menu's), and the Usage section has no meters but says \"Not logged in yet.\" with \"Log in to use this account.\" under it.",
    state: notSignedInState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-account-details-agent-not-installed",
    group: GROUP,
    category: "exceptional",
    title: "Account details, the agent is not installed",
    description: "Personal's details on the Codex tab, whose agent is not installed: after the name, in the body's first line, the login state reads \"Unknown\", and under the line with the config directory a line says it is not usable until Codex is on PATH, both as chips. The Usage section is a dash, the account having no reading.",
    state: unavailableQuotaState,
    steps: [...openAccounts, (ui) => ui.press(ui.t("agents.notInstalled", { agent: AGENT_LABEL.codex })), openDetails("Personal", "codex")],
  },
  {
    id: "settings-account-details-unknown-with-email",
    group: GROUP,
    category: "exceptional",
    title: "Account details, login state unknown with an email",
    description: "Work's details after a usage read that failed for a reason other than being signed out: the first line is the name Work followed by the plain muted \"Unknown\" in place of a login badge, with no plan, then the danger-coloured circled \"!\" for the failed read; the email dev@work.octoboard.example is on the line under it, not beside the name. The config directory is the line under the email, and the Usage section is a dash, with no windows to show.",
    state: unknownLoginEmailState,
    steps: accountDetails("Work"),
  },
  {
    id: "settings-account-settings-default",
    group: GROUP,
    category: "normal",
    title: "Account settings, the default account",
    description: "The default account's Account settings, from the second entry of its ⋮ menu: the body starts with the bold name Default, its muted email dev@octoboard.example on the line under it, and its only row is Disable account, with its switch off: the default account has no Share default configuration row, and the permission approval is set on the tab, not here. The plan, login state, config directory and usage are in the details, not here.",
    state: defaultEmailState,
    steps: accountSettings(defaultAccount),
  },
  {
    id: "settings-accounts-emails",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, sign-in emails under the names",
    description: "Claude Code's table with each account's sign-in email under its name, muted and smaller, on a single line: Default (dev@octoboard.example) and Work (dev@work.octoboard.example) show theirs whole; Client's address is longer than the name column, so it is cut on one line and fades at the end, with the whole address in a tooltip while the pointer rests on it. Trial is signed out, so its row has no email line and no gap where it would be, and its name sits as it does in the rows without a reading.",
    // Four rows under the three switches and the tab strip: taller than the default window, whose body would scroll.
    height: 1000,
    state: emailsState,
    steps: openAccounts,
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
    title: "Settings: accounts, all three account switches on",
    description: "Above the agent tabs, the account-selection switch, the \"Automatically switch accounts at the usage limit\" switch and the \"Start usage windows automatically\" switch are all on, in that order. Each description states the behaviour and does not start with \"Once this is on\"; the automatic-switch one says a session that stops at its account's usage limit is switched to another account with usage left and carries on; the usage-window one is a single short sentence with its \"?\" after the label. Switch any off and on again to read both of its descriptions (off, the automatic-switch one starts with \"Once this is on\"); none moves another.",
    state: { ...quotaState, settings: { ...quotaState.settings!, auto_select_account: true, auto_switch_on_usage_limit: true, auto_start_quota_windows: true } },
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
    description: "The mouse rests on the \"?\" at the end of the config directory line in the default account's details, so its popover is open, saying this is the agent's own default configuration directory, which new sessions use when no account is chosen and other accounts can share; no focus has moved, the popover stays while the pointer is on the \"?\" or the popover, and its text can be selected. The table behind the dialog carries no such \"?\".",
    state: quotaState,
    steps: [...accountDetails(defaultAccount), (ui) => ui.hoverStatusMark(ui.t("settings.accounts.defaultHelp"))],
  },
  {
    id: "settings-accounts-many",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code tab scrolled with many accounts",
    description: "The section is scrolled down, so the agent tab strip with its ⋮ buttons and the two icon buttons is pinned at the top of it with its bottom rule and the Claude Code indicator showing, on the dialog's own surface; the table's header and the first rows have scrolled away underneath it and are cut off cleanly against its rule. Choose 560px to see the narrow layout, where the sections scroll too: the Settings sections are tabs in a row above, which stays put, and the table itself still scrolls sideways with its actions column pinned.",
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
    description: "Codex's tab has its own Permission approval row, \"Applies to every Codex account\", on Automatic review, the reviewers' wording. Right under it comes the Claude compatibility row, with its \"?\" after the name, the same \"Applies to every Codex account\" line and its switch at the end, on (its default while Claude Code is an added agent), before the accounts table. Codex has no model-specific window: each Usage cell is the two-layer bar, the 5h window over the weekly one. Automation has no plan, so no plan badge, and its 5h window, read as spent 16 minutes ago, reset a minute ago: its top layer is full; Production, further down, is nearly spent: both layers are almost empty.",
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
    id: "settings-accounts-codex-no-claude-code",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Codex tab with Claude Code not added",
    description: "Claude Code is not among the added agents (the tab strip has Codex and Grok Build only) and Claude compatibility was never chosen, so the Claude compatibility switch on Codex's tab is off, where it is on when Claude Code is added.",
    state: noClaudeCodeState,
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
    description: "Grok Build has no permission approval choice, so its tab has no Permission approval row: the tab strip is followed by the Claude compatibility row alone, \"Applies to every Grok Build account\" under its name and its switch at the end, on (its default), and directly under it the table, with its one default account, a single weekly layer in its Usage cell and the columns Account, Usage, Status and Disable.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL.grok),
    ],
  },
  ...(["codex", "grok"] as const).map((agent): Scenario => ({
    id: `settings-accounts-claude-compatibility-help-${agent}`,
    group: GROUP,
    category: "normal",
    title: `Settings: accounts, ${AGENT_LABEL[agent]} Claude compatibility explanation`,
    description: "The help button beside the Claude compatibility label is touched with a finger. Its popover says what the switch does for this agent and that a change takes effect when a session next starts or resumes; the agent's name, Claude Code, CLAUDE.md and Octoboard are chips, and a chip may wrap across lines.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(AGENT_LABEL[agent]),
      (ui) => ui.touchPress(ui.t("settings.accounts.claudeCompat.helpLabel")),
    ],
  })),
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
    description: "Every agent has been removed: no tab strip or Add account button, a hint to add an installed agent, and Add agent is offered. The account-selection, \"Automatically switch accounts at the usage limit\" and \"Start usage windows automatically\" switches stay above Add agent, all off, their descriptions starting with \"Once this is on\". The accounts set up earlier are kept, only unlisted.",
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
    description: "Work's ⋮ button, the only thing in the last column, is open: Details first, then Settings, Edit and Log in; after a separator, Move earlier and Move later; then Remove in the danger tone after another separator. Work is between Default and Client, so both move actions are available.",
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
    description: "The default account's menu holds Details, Settings, Rename and Log in, followed by Move earlier and Move later after a separator. Default is first in the saved order, so Move earlier is disabled and Move later is available.",
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
    description: "Claude Code's default account is called \"Studio laptop\" in place of Default; it keeps its plan badge, its \"Logged in\" badge and its usage bar, and its ⋮ menu and its details and settings dialogs open under that name. On the Codex tab the default account is still Default.",
    state: renamedDefaultState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-account-edit",
    group: GROUP,
    category: "normal",
    title: "Edit an account",
    description: "Edit on Work opens the form under the plain title Edit account. Its first sentence says to edit the name and config directory of the account, with the agent and the account's name as chips, and the Name and Config directory fields below hold Work's values.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Work", agent: AGENT_LABEL.claude })),
      (ui) => ui.press(ui.t("common.edit")),
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
    description: "Settings finds Work's login already running, so its modal is open: titled with the plain name \"Log in\", the first sentence of the body saying it is logging in to Claude Code account Work… (agent and account as chips, the sentence ending in an ellipsis), then a spinner with \"Waiting for you to authorize in your browser…\", and in the footer \"Open the page again\" beside Cancel login. The same modal serves Claude Code and Codex.",
    state: withLogins(signedInState, loginOf("claude", "a-work", { authorization_url: CLAUDE_PAGE })),
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "settings-account-login-starting",
    group: GROUP,
    category: "normal",
    title: "Account login, starting",
    description: "The login's modal while the daemon is still starting it, before any page or code exists: the same \"Log in\" modal over Claude Code account Work, with a spinner row that has no visible text beside it (the status is spoken, not shown). Cancel login alone is in the footer and has the focus; there is no Open the page again, since there is no page yet.",
    state: withLogins(signedInState, loginOf("claude", "a-work", { status: "starting" })),
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
    description: "Settings finds the default Grok Build account's login already running, so its modal is open, titled \"Log in\" with the first body sentence naming Grok Build and the account. Grok Build opens its own page, so the modal shows the code to confirm there, set as code, with \"Open the page again\" and Cancel login in the footer; Octoboard does not open the page itself.",
    state: withLogins(quotaState, loginOf("grok", null, {
      authorization_url: "https://accounts.x.ai/oauth2/device?user_code=ABCD-1234",
      device_code: "ABCD-1234",
      opened_by_agent: true,
    })),
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
    description: "Work (never logged in) and Client (whose config directory does not exist yet) both have a dash in the Usage cell, which assistive technology reads as no usage reading, and after it the soft warning \"Not logged in\" badge in the Status column with a small Log in icon button beside it; they have no plan badge, and their Disable switches are off as usual. The default account above them is logged in: its plan, its \"Logged in\" badge and its usage bar.",
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
    description: "Log in was chosen for Work and the CLI gave up: the dialog becomes a fixed failure alert, with the danger icon in its header beside the title \"Could not log in\", no close button, and Escape does nothing. The body is only \"Logging in to Claude Code account Work failed.\" (agent and account as chips): the failure carries no specific cause of its own, so none follows in the sentence. \"Error details\" is open here, holding the CLI's multiline text, and nothing else, in a selectable block of its own. The footer reads Close, then Log in again as the main button, which has the focus.",
    state: notSignedInState,
    loginOutcome: "failed",
    steps: [...loginFromInline(), (ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "settings-account-login-timed-out",
    group: GROUP,
    category: "exceptional",
    title: "Account login timed out",
    description: "Log in was chosen for Work and nobody authorized in time: the dialog becomes a fixed failure alert (no close button) with the danger icon in its header and the title \"Login timed out\". The body is the one sentence \"The login to Claude Code account Work timed out.\" (agent and account as chips); it has nothing after it and no Error details, since there is nothing more to say; the footer reads Close, then Log in again as the main button, which has the focus.",
    state: notSignedInState,
    loginOutcome: "timed_out",
    steps: loginFromInline(),
  },
  {
    id: "settings-account-login-codex-busy",
    group: GROUP,
    category: "exceptional",
    title: "Account login refused, another Codex account is signing in",
    description: "Choosing Log in on Personal is refused: the dialog opens straight as a fixed failure alert (no close button), with the danger icon in its header and the title \"Could not log in\". The body opens with \"Logging in to Codex account Personal failed.\" (agent and account as chips), followed in the same sentence by the cause that another Codex account is logging in, then the remedy to wait as a paragraph of its own. It has no Error details, since there is no raw text; the footer reads Close, then Log in again.",
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
    id: "settings-account-login-being-removed",
    group: GROUP,
    category: "exceptional",
    title: "Account login refused, the account is being removed",
    description: "Choosing Log in on Work is refused while the account is being removed with its directory: the dialog opens straight as a fixed failure alert (no close button), with the danger icon in its header and the title \"Could not log in\". The body is one sentence: the login to Claude Code account Work failed (agent and account as chips), followed by the cause \"The account is being removed.\" It has no remedy line and no Error details; the footer reads Close, then Log in again, which has the focus.",
    state: notSignedInState,
    loginStartError: { code: "account_being_removed", params: { account: "a-work", name: "Work" }, message: "account `Work` is being removed" },
    steps: loginFromInline(),
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
    description: "Choosing Approve everything in the Permission approval row opens a warning modal instead of saving: a warning icon and the plain title Approve everything, a body that says every tool will run without asking you in every Claude Code account's sessions (the agent's name as a chip), including commands that change or delete files, and the danger-coloured button repeating the choice. Cancelling leaves the row on Automatic.",
    state: quotaState,
    steps: bypassSteps,
  },
  {
    id: "settings-accounts-sharing",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, sharing the default configuration",
    description: "Work's Account settings, opened from the second entry of its ⋮ menu, in a modal with no footer button: the body starts with the bold name Work, its muted email dev@work.octoboard.example on the line under it, with nothing after the name (plan, login state, config directory and usage are in the details), and under it Share default configuration as the row above Disable account: the label with its help button, the description under it, which says turning it on links the account's configuration and history to Studio laptop's directory, with the renamed default account shown as a reference chip, and the switch at the end, off and usable, with nothing else under it. The help button holds what is shared and never shared, what turning sharing off does and what changing it does to idle sessions. The default account's settings dialog has no such row.",
    state: sharingEmailState,
    steps: accountSettings("Work"),
  },
  {
    id: "settings-accounts-sharing-enabled",
    group: GROUP,
    category: "normal",
    title: "Settings: account shares the renamed default account's configuration",
    description: "Client's sharing switch is on. Its description says the configuration and history are already linked to Studio laptop's directory, using the renamed default account's actual name as a reference chip.",
    state: sharingEmailState,
    steps: accountSettings("Client"),
  },
  {
    id: "settings-accounts-sharing-in-use",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, sharing cannot be changed while the account is in use",
    description: "Work's sharing switch is faded but still focusable, and the lines under the row's description, which say why, read that sharing cannot be changed while 2 of the account's sessions are working and to wait for them to finish, and that it cannot be changed while the account is being signed in.",
    state: sharingInUseState,
    steps: accountSettings("Work"),
  },
  {
    id: "settings-accounts-sharing-progress",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, sharing change running",
    description: "Work is turning sharing on and changing entries, so its switch is faded but still focusable, with no lock line and no state description under the label. A determinate progress bar under the row reads 3 of 9, with the entry being processed, plugins/, as a chip under it.",
    state: sharingProgressState,
    steps: accountSettings("Work"),
  },
  {
    id: "settings-accounts-sharing-progress-restart",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, sharing change ending and resuming sessions",
    description: "Work is turning sharing on and is ending the account's idle sessions first: its switch is faded but still focusable, with no lock line and no state description under the label, and an indeterminate bar under the row reads \"Ending idle sessions…\" in place of the count, with no entry.",
    state: sharingRestartState,
    steps: accountSettings("Work"),
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
    description: "The error alert is titled \"Could not save the permission approval\" and says only \"Access was denied.\" with the permissions remedy (the daemon sent the system's text with no code to word it by, so the application names the cause from that text); that text, which says the account settings could not be saved with Permission denied (os error 13), is only in the Error details disclosure, closed here. Codex's Permission approval row keeps Follow account configuration behind the alert; acknowledging the error leaves that saved selection intact.",
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
    description: "Confirming Approve everything was refused. The error alert is titled \"Could not save the permission approval\" and says only \"Access was denied.\" with the permissions remedy, and the system's text (the account settings could not be saved, Permission denied, os error 13) is only in its Error details disclosure, closed here, with the confirmation dialog still open beneath it; the Permission approval row still reads Automatic.",
    state: quotaState,
    accountApprovalError: { code: "store_io_failed", params: { detail: "Permission denied (os error 13)" }, message: "The account settings could not be saved: Permission denied (os error 13)" },
    steps: [
      ...bypassSteps,
      (ui) => ui.confirm(),
    ],
  },
  {
    id: "settings-account-claude-compatibility-error",
    group: GROUP,
    category: "exceptional",
    title: "Claude compatibility could not be saved",
    description: "Turning Claude compatibility off on Codex's tab was refused. The error alert is titled \"Could not save Claude compatibility\" and says only \"Access was denied.\" with the permissions remedy, the system's text being only in its Error details disclosure, closed here; the switch is available again and still on.",
    state: quotaState,
    claudeCompatibilityError: { code: "store_io_failed", params: { detail: "Permission denied (os error 13)" }, message: "The settings could not be saved: Permission denied (os error 13)" },
    steps: [
      ...openAccounts,
      (ui) => ui.press(AGENT_LABEL.codex),
      (ui) => ui.press(ui.t("settings.accounts.claudeCompat.label", { agent: AGENT_LABEL.codex })),
    ],
  },
  {
    id: "settings-accounts-sharing-failed",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change could not be finished",
    description: "In Work's settings dialog, turning sharing off did not finish, so the switch sits at its position before the change, on, and the row shows no state description. A warning triangle named \"Last sharing change\" sits right of the switch in the same row, with nothing written beside it, and nothing about the failure is under the row (no alert, no Details button, no Try again). The row keeps its label and switch on their lines at the dialog's width, the triangle not wrapping under the switch. Its popover is in the next scenario.",
    state: sharingFailedState,
    steps: accountSettings("Work"),
  },
  {
    id: "settings-accounts-sharing-failed-details",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change could not be finished, several causes, popover open",
    description: "The triangle right of the switch is pressed, so its popover is open with focus inside it. In order: the title, \"Could not finish turning sharing off\"; \"Some files or folders could not be changed.\"; the way to try again, turning the switch off again; one compact line per cause (no permission to change skills/ and 1 more, plugins/ in use, and \"Could not change settings.json.\", whose fix reads that the system's own reason is below), each followed by its fix; the note that changing sharing ends the account’s 2 idle sessions and resumes them afterwards; then the raw side: each unfinished entry one tight row, its chip and what it was left as in words (the conversation abc with the titles of the sessions that recorded it, Fix the summary layout and Update the dependencies, joined as a list), followed by the system's original reason as small inline code on the same row.",
    state: sharingFailedState,
    steps: [...accountSettings("Work"), (ui) => ui.touchPressStatusMark(ui.t("settings.accounts.sharing.unfinishedMark"))],
  },
  {
    id: "settings-accounts-sharing-default-moved-details",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change failed, the default directory has moved, popover open",
    description: "The triangle right of Work's switch (on, its recorded position) is pressed. The popover holds the title \"Could not finish turning sharing on\", the single reason that the default account's directory has moved since sharing was turned on (no entry is named), and the advice to turn the switch off, then on again; below them only the raw reason, alone in a block on its own tinted surface, not as inline code.",
    state: sharingDefaultMovedState,
    steps: [...accountSettings("Work"), (ui) => ui.touchPressStatusMark(ui.t("settings.accounts.sharing.unfinishedMark"))],
  },
  {
    id: "settings-accounts-sharing-no-space-details",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change failed, not enough disk space, popover open",
    description: "Turning sharing on did not finish, so Work's switch sits at its position before the change, off, and the triangle right of it is pressed. The popover holds the title \"Could not finish turning sharing on\", the single cause \"There is not enough disk space.\" and its fix \"Free up disk space.\" as the two sentences after it (no entry is named), then the advice to turn the switch on again to try once more, then the raw reason alone in a block on its own tinted surface.",
    state: sharingNoSpaceState,
    steps: [...accountSettings("Work"), (ui) => ui.touchPressStatusMark(ui.t("settings.accounts.sharing.unfinishedMark"))],
  },
  {
    id: "settings-accounts-sharing-incomplete-details",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change left unfinished by a restart, popover open",
    description: "The choice is recorded as on, but the daemon was restarted in the middle of the change, so Work's switch sits at its position before the change, off, with the triangle right of it pressed. The popover is short: the title, \"The last attempt to turn sharing on did not finish\", and the advice to turn the switch on again. There is no reason, no cause line and no raw block, since no entries are known after a restart.",
    state: sharingIncompleteState,
    steps: [...accountSettings("Work"), (ui) => ui.touchPressStatusMark(ui.t("settings.accounts.sharing.unfinishedMark"))],
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
    steps: [...accountSettings("Work"), (ui) => ui.press(ui.t("settings.accounts.sharing.switchLabel", { name: "Work" }))],
  },
  {
    id: "settings-accounts-sharing-unfinished-asked",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change the user asked for could not be finished",
    description: "In Work's settings dialog the switch was pressed to turn sharing on and the change did not finish. The window that asked is told in an error alert dialog over the settings dialog, which says what could not be finished for Work and why, with the raw entries behind its Error details; the switch sits at its position before the change, off, and the warning triangle \"Last sharing change\" right of it stays as the lasting state once the alert is acknowledged. The same failure seen only through another window shows the triangle alone (the scenarios before this one).",
    state: sharingState,
    accountSharingError: SHARING_TURN_ON_ERROR,
    steps: [...accountSettings("Work"), (ui) => ui.press(ui.t("settings.accounts.sharing.switchLabel", { name: "Work" }))],
  },
  {
    id: "settings-accounts-quota-unavailable",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, Claude Code usage unavailable or out of date",
    description: "Claude Code's default account and Work have nothing to show: their Usage cells are a dash followed by a danger-coloured circled \"!\" (\"Usage unavailable\"), one a timeout and one a failed command; neither has a plan or a status, and their names carry no mark of their own. Client has a plan and reads \"Logged in\", but its latest read failed: its Usage cell still draws the last windows it read, a two-layer bar in Claude Code's colour, followed by a warning-coloured triangle, \"Out of date\" (the scenario after this one opens why). No sentence of error text sits in the table; hovering a mark, or tapping it with a finger, opens one small popover with its name and its reason.",
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
    title: "Settings: accounts, the reason an out-of-date reading carries",
    description: "The warning-coloured triangle after the bar in Client's Usage cell is tapped with a finger: its popover holds why the newer reading failed. Client's settings dialog shows the last windows with the same triangle after \"Updated 12 minutes ago\".",
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
    description: "The Codex tab is selected and carries a warning icon after its name; its Permission approval row is there as usual. Neither the default account nor Personal has a plan, a status or a usage reading: their Usage cells are dashes and their Status cells empty.",
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
    description: "A warning modal titled Remove account: the account name is a chip in the first sentence of the body. Under the message, an unticked option \"Also move the account directory to the Trash\" with a hint that the whole directory goes, including login, settings and conversation history, and can be recovered from the Trash. The confirming button reads Remove.",
    state: browserState,
    steps: removeWork,
  },
  {
    id: "settings-account-delete-trash",
    group: GROUP,
    category: "normal",
    title: "Remove account to the Trash",
    description: "The option is ticked and Work has no live sessions: the message now says the directory is moved to the Trash and that its archived sessions can no longer be resumed, the hint stays, and Remove is available.",
    state: browserState,
    steps: removeWorkToTrash,
  },
  {
    id: "settings-account-delete-trash-running",
    group: GROUP,
    category: "normal",
    title: "Remove account to the Trash, sessions still running",
    description: "The option is ticked and Work has four live sessions, so a plain line reads \"The account still has 4 sessions that are running or starting\" and says they must be ended or switched to another account first, over the list of them with their status marks, agent icons and titles; Remove is disabled. Neither Work's archived session nor Client's session is listed. Unticking the option removes the list and enables Remove.",
    state: workRunningState,
    steps: removeWorkToTrash,
  },
  {
    id: "settings-account-delete-trash-missing",
    group: GROUP,
    category: "normal",
    title: "Remove account to the Trash, directory not there",
    description: "The option is ticked and nothing is at Work's directory: the hint now says the account's directory does not exist, so there is nothing to move, the message stays the plain one for an unticked removal (nothing about the Trash or archived sessions), and Remove is available.",
    state: browserState,
    accountTrashInspection: { status: "missing" },
    steps: removeWorkToTrash,
  },
  {
    id: "settings-account-delete-trash-checking",
    group: GROUP,
    category: "normal",
    title: "Remove account to the Trash, directory being checked",
    description: "The option is ticked and the check of the directory has not answered: Remove is faded as waiting and cannot be pressed, with no alert under the option yet.",
    state: browserState,
    accountTrashInspection: "pending",
    steps: removeWorkToTrash,
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
  ...TRASH_PREVENTED.map(({ key, outcome, title, description }): Scenario => ({
    id: `settings-account-delete-trash-prevented-${key}`,
    group: GROUP,
    category: "exceptional",
    title,
    description,
    state: browserState,
    accountTrashInspection: outcome,
    steps: removeWorkToTrash,
  })),
  {
    id: "settings-account-delete-trash-inspect-failed",
    group: GROUP,
    category: "exceptional",
    title: "Remove account to the Trash prevented, the directory could not be checked",
    description: "The request that checks the directory failed. A danger alert under the ticked option says it could not check whether the directory can be moved to the Trash, adds a general unexpected-error line, and offers Details and Try again; Remove is disabled.",
    state: browserState,
    accountTrashInspection: { code: "internal_error", params: { detail: "the daemon could not check the directory" }, message: "the daemon could not check the directory" },
    steps: removeWorkToTrash,
  },
  {
    id: "settings-account-delete-trash-inspect-retrying",
    group: GROUP,
    category: "exceptional",
    title: "Remove account to the Trash prevented, the check being retried",
    description: "Try again was pressed on the failed check. The danger alert under the ticked option stays with its text and Details, and Try again stays where it is, faded as waiting; Remove is faded and disabled.",
    state: browserState,
    accountTrashInspection: [{ code: "internal_error", params: { detail: "the daemon could not check the directory" }, message: "the daemon could not check the directory" }, "pending"],
    steps: [...removeWorkToTrash, (ui) => ui.press(ui.t("error.tryAgain"))],
  },
  ...TRASH_REFUSALS.map(({ key, error, inspection, title, description }): Scenario => ({
    id: `settings-account-delete-trash-${key}`,
    group: GROUP,
    category: "exceptional",
    title,
    description,
    state: browserState,
    accountDeleteError: error,
    ...(inspection && { accountTrashInspection: inspection }),
    steps: [...removeWorkToTrash, (ui) => ui.confirm()],
  })),
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
    description: "Remove on the first folder asks first: the title is the plain \"Remove trusted folder\", the body's first sentence names the home-abbreviated path (~/code) as a chip, and the message says trust confirmations under it are asked about again unless the project's own permission or another trusted folder covers them and then what is kept, and the confirming button is destructive.",
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
    description: "The browser platform: the row shows the browser's own notification permission and, where it can still be asked for, an Enable button.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.notifications")),
    ],
  },
  {
    id: "settings-notifications-system",
    group: GROUP,
    category: "normal",
    title: "Settings: notifications, managed by macOS (desktop app)",
    description: "The row has no switch: its description says the app has none and macOS manages these notifications, with the app name, macOS and System Settings each a chip, and a secondary Open System Settings button sits at the row's end. Look at how the longer copy wraps beside the button and how the two align.",
    state: browserState,
    systemNotifications: true,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.notifications")),
    ],
  },
  {
    id: "settings-notifications-system-open-failed",
    group: GROUP,
    category: "exceptional",
    title: "Settings: System Settings did not open (desktop app)",
    description: "After Open System Settings is pressed and the system does not open the pane, an error alert titled \"Could not open System Settings\" says it did not open and to open it yourself and choose Notifications. The title is plain text; System Settings and Notifications in the body are each a chip. OK is its only button: no recovery action and no Error details.",
    state: browserState,
    systemNotifications: { openError: "the system did not open x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=dev.octoboard.desktop" },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.notifications")),
      (ui) => ui.press(ui.t("settings.notifications.openSettings")),
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
