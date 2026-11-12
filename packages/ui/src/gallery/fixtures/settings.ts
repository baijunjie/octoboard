import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import { FIXTURE_AGENTS, FIXTURE_PROVIDERS, UNPRESENTED_PROVIDER } from "../../fixtures/fixtureProviders";
import { providerLabel } from "../../agents/providers";
import { describing, type Matcher, type Ui, type Step } from "../../fixtures/interact";
import { type AccountLogin, type AccountSharing, type Agent, type AgentAvailability, type AgentCliStatus, type LocalMcpServer, type McpInventoryFailure, type PlatformConnector } from "../../protocol";
import type { AccountMcpAnswer } from "../../fixtures/fixture";
import type { FixtureError } from "../../fixtures/sampleFiles";
import type { Scenario } from "../scenario";
import { accountOf, agentCliStatusOf, agentCliUpdateOf, MACOS_MACHINE, SAMPLE, accountSharingOf, minutesAgo, sessionOf, snapshotState, WINDOWS_MACHINE } from "../../fixtures/builders";
import { defaultAgentCliStatuses } from "../../fixtures/fixtureAgentClis";
import { browserHost } from "../../platform/host";
import { MAC_HOST, WINDOWS_HOST } from "../../test/hosts";
import { noneAddedState } from "./empty";
import { accountLoginSucceeded, accountQuotaUpdated, agentAvailability } from "./liveChanges";
import { USED_UP_MIX, manyClaudeAccounts, quotaFailure, quotaOf, quotaWindow, PERSONAL_RESET_CREDITS, resetCreditOf, resetCreditsOf } from "../../fixtures/quotas";

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

/** The desktop app with "Resume sessions on start" on, so the description it gives the macOS app shows, and Prevent sleep
 * on in a daemon whose host can hold the computer awake. */
const resumingState: Scenario["state"] = {
  ...state,
  host: MACOS_MACHINE.host,
  settings: { ...state.settings!, resume_sessions_on_start: true, keep_awake: true },
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
    added_agents: [...FIXTURE_AGENTS],
    provider_settings: { claude: { approval_mode: "auto" }, codex: { approval_reviewer: "auto_review" } },
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

/** `quotaState` with a fourth provider served that this build has no presentation module for, added and installed, holding
 * its default account and one account of its own with readings: the agent tabs and the accounts below them name it by
 * the label the daemon serves, with the generic mark. */
const unpresentedState: Scenario["state"] = {
  ...quotaState,
  providers: [...FIXTURE_PROVIDERS, UNPRESENTED_PROVIDER],
  settings: {
    ...quotaState.settings!,
    added_agents: [...FIXTURE_AGENTS, UNPRESENTED_PROVIDER.id],
    accounts: [...quotaState.settings!.accounts, accountOf("a-acme", UNPRESENTED_PROVIDER.id, "Studio", "/Users/dev/.acme-studio")],
    provider_settings: { ...quotaState.settings!.provider_settings, [UNPRESENTED_PROVIDER.id]: {} },
  },
  agentAvailability: new Map(quotaState.agentAvailability).set(UNPRESENTED_PROVIDER.id, { agent: UNPRESENTED_PROVIDER.id, availability: "available", default_account_dir: "/Users/dev/.acme" }),
  accountQuotas: [
    ...quotaState.accountQuotas!,
    quotaOf(UNPRESENTED_PROVIDER.id, null, "Team", [quotaWindow("session", 35, 140), quotaWindow("weekly", 58, 3 * 24 * 60 + 2 * 60)]),
    quotaOf(UNPRESENTED_PROVIDER.id, "a-acme", "Team", [quotaWindow("session", 8, 250), quotaWindow("weekly", 12, 5 * 24 * 60)]),
  ],
};

/** Every agent added, the three of them carrying tags: what an agent's Tags row shows, and the others' tags it offers. */
const taggedState: Scenario["state"] = {
  ...quotaState,
  settings: {
    ...quotaState.settings!,
    agent_tags: { claude: ["frontend", "backend"], codex: ["review", "tests"], grok: ["research"] },
  },
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

/** The readings that did not come out whole, on Claude Code's tab. Codex is not installed, so its accounts have no
 * entry and Personal says it is not usable. Grok Build is installed: a tab flags only an agent that is unavailable, so
 * this changes nothing on the tabs, but the add-agent picker lists an installed agent as ready to add and one whose
 * availability is not determined as still being checked. */
const unavailableQuotaState: Scenario["state"] = {
  ...browserState,
  settings: { ...browserState.settings!, added_agents: [...FIXTURE_AGENTS] },
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
  ],
};

/** `state` with every installed agent's CLI current, but for the one `status` is of. */
const withCliStatus = (state: Scenario["state"], status: AgentCliStatus): Scenario["state"] => ({
  ...state,
  agentCliStatuses: new Map(defaultAgentCliStatuses(state!.agentAvailability!)).set(status.agent, status),
});

/** Claude Code installed with npm at 2.1.319 while 2.1.320 is out: what the Version row offers an Update for. */
const OUTDATED_CLI = agentCliStatusOf("claude", { install_method: "npm", latest_version: "2.1.320" });

/** What npm prints when an update cannot write to the global prefix, the last line a failed update carries. */
const CLI_UPDATE_DETAIL = "npm error code EACCES: permission denied, mkdir '/usr/local/lib/node_modules/@anthropic-ai'";

/** A model's display name as long as one can be, long enough to wrap in a narrow window. */
const LONG_MODEL_NAME = "Claude Opus Extended Context Research Preview";

/** Claude Code's default account and Work turned off for new sessions, and Trial, turned off too and signed out. The
 * sample's sessions run on the default account, so it keeps its reading; a session started since runs on Trial, so the
 * daemon reads it, and finds it signed out. The daemon reads no quota for Work, which has none, and Client stays on, its
 * first reading not in yet. */
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
  sessions: new Map([...quotaState.sessions!.values(), sessionOf("s-web-trial", SAMPLE.console.id, SAMPLE.web.id, "Try the sign-in flow", "working", { account_id: "a-trial" })].map((session) => [session.id, session])),
  accountQuotas: [
    ...quotaState.accountQuotas!.filter((quota) => quota.agent !== "claude" || quota.account !== "a-work"),
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

/** `quotaState` with Codex's Personal account used up by its weekly window and holding two reset credits, the soonest
 * expiring in 6d 3h; the other Codex accounts hold none. */
const resetCreditsState: Scenario["state"] = {
  ...quotaState,
  accountQuotas: quotaState.accountQuotas!.map((quota) =>
    quota.agent === "codex" && quota.account === "a-codex"
      ? quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 34, 190), quotaWindow("weekly", 100, 2 * 24 * 60 + 5 * 60)], {
        fetched_at: minutesAgo(3),
        reset_credits: PERSONAL_RESET_CREDITS,
      })
      : quota),
};

/** `resetCreditsState` with Personal holding a single credit, the one a use takes. */
const lastResetCreditState: Scenario["state"] = {
  ...resetCreditsState,
  accountQuotas: resetCreditsState.accountQuotas!.map((quota) =>
    quota.agent === "codex" && quota.account === "a-codex" ? { ...quota, reset_credits: resetCreditsOf([resetCreditOf("credit-1", 6 * 24 * 60 + 3 * 60)]) } : quota),
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

/** The ⋮ button at the end of `agent`'s tab on the strip (every tab has its own, so the agent's tab need not be selected).
 * While an update of the agent's CLI is waiting (`update`) the button's name says so. */
const pressAgentActions = (agent: Agent, update = false): Step => (ui) => ui.press(ui.t(
  update ? "settings.accounts.agentActionsUpdate" : "settings.accounts.agentActions",
  { agent: providerLabel(ui.t, [...FIXTURE_PROVIDERS, UNPRESENTED_PROVIDER], agent) },
));

/** The Settings item of the ⋮ menu at the end of `agent`'s tab: the agent's own settings in a modal. A menu item, since the
 * rail has a Settings button of its name. */
const openAgentSettings = (agent: Agent): Step => async (ui) => {
  await pressAgentActions(agent)(ui);
  await ui.pressMenuItem(ui.t("settings.accounts.settingsItem"));
};

/** The Version item of the same menu: the agent's CLI version and update in a modal. `update` is whether an update of the
 * CLI is waiting, which changes the button's name. */
const openAgentVersion = (agent: Agent, update = false): Step => async (ui) => {
  await pressAgentActions(agent, update)(ui);
  await ui.pressMenuItem(ui.t("settings.accounts.cli.label"));
};

/** Settings → Agent accounts with the Agent settings of `agent` in front. */
const agentSettings = (agent: Agent): Step[] => [...openAccounts, openAgentSettings(agent)];

/** Settings → Agent accounts with the Agent version dialog of `agent` in front. */
const agentVersion = (agent: Agent, update = false): Step[] => [...openAccounts, openAgentVersion(agent, update)];

/** A URL far wider than the details dialog, for a server whose name is as long. */
const LONG_SERVER_URL = "https://mcp.analytics.acme-corporation.example.com/v2/workspaces/engineering-platform/projects/internal-platform/servers/warehouse-query-service/sse?region=eu-west-1";
/** What Claude Code says of a plugin's server that did not start: several lines, shown as it said them. */
const FAILED_SERVER_MESSAGE = "Failed to connect: spawn npx ENOENT\n    at ChildProcess._handle.onexit (node:internal/child_process:285:19)\n    at onErrorNT (node:internal/child_process:483:16)\nThe server exited before the handshake completed (stderr: /bin/sh: npx: command not found)";
/** The claude.ai connectors of Work's reading in each state they can be in. */
const CLAUDE_CONNECTORS: PlatformConnector[] = [
  { id: "claude_ai_Gmail", name: "Gmail", state: "connected", plugins: [] },
  { id: "claude_ai_Google_Calendar", name: "Google Calendar", state: "connected", plugins: [] },
  { id: "claude_ai_Slack", name: "Slack", state: "not_connected", plugins: [] },
  { id: "claude_ai_Notion", name: "Notion", state: "unknown", plugins: [] },
];
/** Its local servers of every kind a person meets, one with a name and a URL far wider than the dialog. */
const CLAUDE_SERVERS: LocalMcpServer[] = [
  { name: "linear", origin: "https://mcp.linear.app/sse", plugin: null, sign_in: "needs_sign_in", runtime: "unknown", runtime_message: null, tool_count: null, oauth_sign_in: false },
  { name: "github", origin: null, plugin: "github", sign_in: "unknown", runtime: "failed", runtime_message: FAILED_SERVER_MESSAGE, tool_count: null, oauth_sign_in: false },
  { name: "sentry", origin: "https://mcp.sentry.dev/mcp", plugin: null, sign_in: "signed_in", runtime: "running", runtime_message: null, tool_count: null, oauth_sign_in: false },
  { name: "filesystem", origin: null, plugin: null, sign_in: "not_needed", runtime: "running", runtime_message: null, tool_count: null, oauth_sign_in: false },
  { name: "internal-analytics-warehouse-query-service-for-the-platform-team", origin: LONG_SERVER_URL, plugin: null, sign_in: "not_needed", runtime: "running", runtime_message: null, tool_count: null, oauth_sign_in: false },
];

/** The Codex account's connectors, the same in every scenario of its sign-in to a server. */
const CODEX_CONNECTORS: PlatformConnector[] = [
  { id: "connector_github", name: "GitHub", state: "connected", plugins: ["github", "pull-request-review"] },
  { id: "connector_google_drive", name: "Google Drive", state: "connected", plugins: ["google-drive"] },
  { id: "connector_notion", name: "Notion", state: "not_connected", plugins: ["notion"] },
];
/** Personal's local servers: two that Codex signs in to with OAuth through its own login command (figma, which needs it,
 * and linear, which has it), and two that have none to offer. */
const CODEX_SERVERS: LocalMcpServer[] = [
  { name: "openai-docs", origin: "https://developers.openai.com/mcp", plugin: null, sign_in: "not_needed", runtime: "running", runtime_message: null, tool_count: 3, oauth_sign_in: false },
  { name: "playwright", origin: null, plugin: "browser-testing", sign_in: "not_needed", runtime: "running", runtime_message: null, tool_count: 1, oauth_sign_in: false },
  { name: "figma", origin: "https://mcp.figma.com/mcp", plugin: null, sign_in: "needs_sign_in", runtime: "unknown", runtime_message: null, tool_count: null, oauth_sign_in: true },
  { name: "linear", origin: "https://mcp.linear.app/mcp", plugin: null, sign_in: "signed_in", runtime: "running", runtime_message: null, tool_count: 12, oauth_sign_in: true },
];
const CODEX_MCP: AccountMcpAnswer = { ageMinutes: 12, connectors: CODEX_CONNECTORS, servers: CODEX_SERVERS };
/** The default Grok Build account's local servers, as `grok mcp list` joined with `grok mcp doctor` reports them: a running
 * one with its tools, one that asks for OAuth sign-in and one that failed. It brings no connectors, no plugins and no
 * OAuth sign-in of its own. */
const GROK_SERVERS: LocalMcpServer[] = [
  { name: "notes", origin: null, plugin: null, sign_in: "not_needed", runtime: "running", runtime_message: null, tool_count: 4, oauth_sign_in: false },
  { name: "linear", origin: "https://mcp.linear.app/mcp", plugin: null, sign_in: "unknown", runtime: "unknown", runtime_message: null, tool_count: null, oauth_sign_in: false },
  { name: "sentry", origin: "https://mcp.sentry.dev/mcp", plugin: null, sign_in: "needs_sign_in", runtime: "unknown", runtime_message: null, tool_count: null, oauth_sign_in: false },
  { name: "warehouse", origin: null, plugin: null, sign_in: "not_needed", runtime: "failed", runtime_message: "Failed to start: spawn uvx ENOENT", tool_count: null, oauth_sign_in: false },
];
/** What Codex prints when the OAuth metadata of a server cannot be found: several lines, shown as they came. */
const SIGN_IN_FAILED_DETAIL = "Error: Metadata error: OAuth metadata discovery failed for https://mcp.figma.com/mcp\nCaused by: HTTP request failed: error sending request for url (https://mcp.figma.com/.well-known/oauth-authorization-server)\nCaused by: dns error: failed to lookup address information: nodename nor servname provided, or not known";
/** The Sign in button of the server `server` in the Connectors and MCP dialog. */
const pressSignIn = (server: string): Step => (ui) => ui.press(ui.t("settings.accounts.mcp.server.signInLabel", { server }));

/** A reading that could not be taken, for `reason`. */
const mcpUnavailable = (reason: McpInventoryFailure, detail: string | null = null): AccountMcpAnswer => ({ unavailable: { reason, detail } });

/** The `item` (Details, Settings or Connectors and MCP) of the actions menu of the account `name` of `agent` on the tab in view (the
 * default account's name is the language's own); a menu item, since the rail has a Settings button of its name. */
const openAccountDialog = (item: "settings.accounts.detailsItem" | "settings.accounts.settingsItem" | "settings.accounts.mcp.title", name: string | ((ui: Ui) => string), agent: Agent): Step => async (ui) => {
  await ui.press(ui.t("settings.accounts.actionsLabel", { name: typeof name === "string" ? name : name(ui), agent: providerLabel(ui.t, FIXTURE_PROVIDERS, agent) }));
  await ui.pressMenuItem(ui.t(item));
};
const openDetails = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step => openAccountDialog("settings.accounts.detailsItem", name, agent);
const openSettings = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step => openAccountDialog("settings.accounts.settingsItem", name, agent);
const openMcp = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step => openAccountDialog("settings.accounts.mcp.title", name, agent);
const defaultAccount = (ui: Ui) => ui.t("settings.accounts.defaultName");

/** Settings → Agent accounts with the read-only details of the account `name` in front. */
const accountDetails = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step[] => [...openAccounts, openDetails(name, agent)];

/** Settings → Agent accounts on Codex's tab with Personal's details in front. */
const codexDetails: Step[] = [...openAccounts, (ui) => ui.press(providerLabel(ui.t, FIXTURE_PROVIDERS, "codex")), openDetails("Personal", "codex")];
/** Settings → Agent accounts with the Connectors and MCP dialog of the account `name` in front. */
const accountMcp = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step[] => [...openAccounts, openMcp(name, agent)];

/** Settings → Agent accounts on Codex's tab with Personal's Connectors and MCP dialog in front. */
const codexMcp: Step[] = [...openAccounts, (ui) => ui.press(providerLabel(ui.t, FIXTURE_PROVIDERS, "codex")), openMcp("Personal", "codex")];
/** Settings → Agent accounts on Grok Build's tab with the default account's Connectors and MCP dialog in front. */
const grokMcp: Step[] = [...openAccounts, (ui) => ui.press(providerLabel(ui.t, FIXTURE_PROVIDERS, "grok")), openMcp(defaultAccount, "grok")];
/** The Refresh button in the Connectors and MCP dialog. */
/** How long the Connectors and MCP dialog's first reading runs before it says it is still reading
 * (`SLOW_READING_MS` in `useAccountMcpInventory.tsx`, which this catalog cannot import: it would pull the store into the
 * tests that run without a window). */
const SLOW_READING_MS = 15_000;
const refreshMcp: Step = (ui) => ui.press(ui.t("settings.accounts.mcp.refresh"));
const redeemFromDetails: Step = (ui) => ui.press(ui.t("quota.resetCredits.useFor", { account: "Personal" }));

/** Settings → Agent accounts with the settings (the switches) of the account `name` in front. */
const accountSettings = (name: string | ((ui: Ui) => string), agent: Agent = "claude"): Step[] => [...openAccounts, openSettings(name, agent)];

const openSharingHelp = (name: string, agent: Agent = "claude"): Step[] => [
  ...accountSettings(name, agent),
  (ui) => ui.touchPress(ui.t("settings.accounts.sharing.helpLabel")),
];

/** An option of an open permission approval list, found by its label. The option's accessible name is the label alone,
 * but the step driver reads an option's whole text (the label, the summary read by screen readers and the visible note
 * under it), so what it sees starts with the label instead of equalling it. This is a prefix match: do not give it a
 * label that starts another option's label. */
const approvalOption = (label: string): Matcher => describing(`permission approval option(${label})`, (name) => name.startsWith(label));

/** Claude Code's Agent settings, its permission approval set to Bypass permissions: the confirmation dialog it opens. */
const bypassSteps: Step[] = [
  ...agentSettings("claude"),
  (ui) => ui.press(ui.t("provider.claude.setting.approval_mode.label", { agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "claude") })),
  (ui) => ui.press(approvalOption(ui.t("provider.claude.setting.approval_mode.choice.bypass"))),
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

/** A Windows daemon host that cannot make file links (Developer Mode is off), with Work sharing or not as `share` says.
 * The paths are the Windows ones the daemon reports. */
function sharingUnlinkableState(share: boolean): Scenario["state"] {
  return snapshotState({
    consoles: [console_],
    projects: [web, api],
    sessions,
    settings: {
      ...DEFAULT_SETTINGS,
      added_agents: ["claude"],
      default_clone_dir: "C:\\Users\\dev\\Projects",
      accounts: [accountOf("a-work", "claude", "Work", "C:\\Users\\dev\\.claude-work", { share_default_config: share })],
    },
    accountQuotas: [quotaOf("claude", "a-work", "pro", [quotaWindow("session", 38, 150), quotaWindow("weekly", 66, 2 * 24 * 60 + 5 * 60)], { email: WORK_EMAIL })],
    host: { ...WINDOWS_MACHINE.host, file_links: false },
    homeDir: WINDOWS_MACHINE.homeDir,
  });
}

/** Work does not share: its switch is locked, with the reason under it. */
const sharingUnavailableState = sharingUnlinkableState(false);

/** Work already shares: its switch can be turned off, and its line says turning it on again is not possible there. */
const sharingUnlinkableOnState = sharingUnlinkableState(true);

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
  (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Work", agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "claude") })),
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

/** What the daemon refuses, once Remove is pressed, a removal that asks for Work's directory to be moved to the Trash
 * with: what the quick look before could not foresee. Two of the cases are shown, one scenario each: the daemon's
 * refusal of sessions started since, and the move itself failing; the other causes that fail the same way (a state that
 * changed since, a look that outran its time) put their own sentence in the same dialog. */
const TRASH_REFUSALS = [
  {
    key: "sessions-started",
    error: { code: "account_has_running_sessions", params: { account: "a-work", name: "Work", sessions: "2" }, message: "account `Work` still has 2 sessions with a process" },
    title: "Remove account to the Trash refused, sessions started since",
    description: "Work's confirmation was opened with no live sessions, so the option was ticked and Remove was available; two sessions started on Work meanwhile, and the daemon refused. An error dialog titled Could not remove the account, over the confirmation, says only that Account Work still has 2 sessions that are running or starting, so its directory cannot be moved to the Trash. Behind it the dialog is unchanged and Work is still listed.",
  },
  {
    key: "failed",
    error: { code: "account_directory_trash_failed", params: { path: WORK_DIR, detail: "Operation not permitted (os error 1)" }, message: "could not move the directory to the Trash: Operation not permitted (os error 1)" },
    title: "Account directory could not be moved to the Trash",
    description: "An error dialog titled Could not remove the account, over the confirmation, says that the directory could not be moved to the Trash and, as the system's text names it (Operation not permitted, os error 1), that access was denied, with no remedy; the system's own text is in its Error details disclosure, closed here. A directory that did not answer in time, could not be read or checked, or a login that did not end puts its own sentence in the same dialog.",
  },
] as const satisfies readonly { key: string; error: FixtureError; title: string; description: string }[];

export const settingsScenarios: Scenario[] = [
  {
    id: "settings-general",
    group: GROUP,
    category: "normal",
    title: "Browser General settings",
    description: "The Language row follows Appearance, then Markdown style starts on Tailwind, and a Sorting group ends the section: three select rows, Sessions (Status), Active projects (Status) and Inactive projects (Name), each with a short description of what it orders; the project rows say which projects they cover and where those sit relative to the other kind. No other rows are listed, since the background and terminal-cleanup switches are in App behavior and the archive-cleanup switch is in Cleanup. Choose Phone (390px) to see the controls that stack beneath their labels: the Appearance buttons sit against the end edge, while the Language, Markdown style and Sorting selects stretch across the row. At Tablet portrait (768px) and narrower, Settings fills the window with horizontal section tabs and controls stacked beneath their labels. Above 768px the sections stay a menu column under the search field, the selected one a filled row; choose a short height to see the list scroll inside the column, with faded edges and no scroll buttons. Widths below 390px use compact spacing; Desktop (1440px) and Tablet landscape (1024px) keep the same desktop layout.",
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
    id: "settings-search-matches",
    group: GROUP,
    category: "normal",
    title: "Settings search, matches in several sections",
    description: "The search field sits at the top of the sections column, above the section list, which keeps only the sections that have a match, each with a count badge at the end of its row; the longer names (Agent behavior, Trusted folders, Keyboard shortcuts) fade out at the end instead of being cut, with the badge fully visible beside them; hovering a clipped name runs a marquee over it and shows a tooltip with the full name. General is selected and shows only its one matching row, Appearance: the Sorting group has no match, so it is gone with its pinned heading, and the row has no divider under it. At Tablet portrait (768px) and narrower the field is the top row, clear of the close button, with the horizontal tab strip below it. The query is the English word \"app\", so another language shows what it finds in its own copy, or the empty state.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.focus(ui.t("settings.search.label")),
      (ui) => ui.type("app"),
    ],
  },
  {
    id: "settings-search-no-match",
    group: GROUP,
    category: "normal",
    title: "Settings search, nothing matches",
    description: "The section list and the section panel are gone: the dialog shows only the search field and, in place of the panel, the empty state with its icon and \"No matching settings\", centred in the area the panel takes. The close button stays in the corner. Clear the field to bring every section back on the one that was selected.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.focus(ui.t("settings.search.label")),
      (ui) => ui.type("zzzz"),
    ],
  },
  {
    id: "settings-app-behavior",
    group: GROUP,
    category: "normal",
    title: "Browser App behavior settings",
    description: "The section after General, as a browser shows it: two groups. Startup holds only \"Resume sessions on start\", off, since a browser cannot be launched at login, and Terminals holds only the terminal-cleanup switch, off, whose description says closing the panel closes idle terminals and keeps running commands. Window is not offered, and neither is Power, as the daemon's host cannot be held awake. At Tablet portrait (768px) and narrower the section tab is among the horizontal ones, with the controls stacked beneath their labels.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.appBehavior")),
    ],
  },
  {
    id: "settings-app-behavior-browser-prevent-sleep",
    group: GROUP,
    category: "normal",
    title: "Browser App behavior settings, daemon host can stay awake",
    description: "A browser against a daemon whose host can be held awake: the Power group is offered here too, between Startup and Terminals, with \"Prevent sleep\" on above \"When to prevent sleep\", a select reading \"While sessions work\". Window is still not offered.",
    state: { ...browserState, host: MACOS_MACHINE.host, settings: { ...browserState!.settings!, keep_awake: true } },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.appBehavior")),
    ],
  },
  {
    id: "settings-app-behavior-desktop",
    group: GROUP,
    category: "normal",
    title: "Desktop App behavior settings",
    description: "The four groups of the desktop app on macOS, in order. Startup: \"Launch at login\", on, above \"Resume sessions on start\", on, whose description ends with the sentence that a logout, restart or shutdown quits the app without asking (only the macOS app says it). Window: \"Keep running in the background\", on, above \"Show menu bar icon\", on. Power: \"Prevent sleep\", on, so \"When to prevent sleep\" follows it as a select reading \"While sessions work\"; at Phone width (390px) the select stretches beneath its label, and switching Prevent sleep off removes that row with the list motion. Terminals, holding only the terminal-cleanup switch, ends the section, below the fold of a short window, so scroll the dialog to it. Each group heading stays clear while pinned; as the next group replaces it, only the outgoing heading fades at its pinned boundary, on the opaque dialog surface. The incoming heading and controls below it keep their usual appearance.",
    state: resumingState,
    host: MAC_HOST,
    desktopApp: true,
    backgroundRun: true,
    menuBarIcon: true,
    launchAtLogin: "enabled",
    shellSettings: { runInBackground: true },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.appBehavior")),
    ],
  },
  {
    id: "settings-app-behavior-launch-approval",
    group: GROUP,
    category: "normal",
    title: "App behavior, Launch at login awaiting approval",
    description: "The macOS app has registered its login entry, but the system holds it for approval. Startup: the \"Launch at login\" row reads on, and its description says the app is waiting for approval and to allow it in System Settings, the app and System Settings each as an inline chip. An \"Open System Settings\" secondary button sits before the switch. At Phone width (390px) the button and switch drop beneath the text, at its end edge, instead of sitting beside it; check that nothing overflows. Only Startup and Terminals are shown, since the window offers no Window or Power setting here.",
    state,
    host: MAC_HOST,
    desktopApp: true,
    launchAtLogin: "requiresApproval",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.appBehavior")),
    ],
  },
  {
    id: "settings-permissions",
    group: GROUP,
    category: "normal",
    title: "Permissions",
    description: "Neither Screen Recording nor Accessibility has been granted, so each row ends in a Grant button and no badge. Press each Grant to see the refused request point at System Settings; revisiting the section offers Grant again.",
    state,
    host: MAC_HOST,
    macosPermissions: { screenRecording: false, accessibility: false },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
    ],
  },
  {
    id: "settings-permissions-granted",
    group: GROUP,
    category: "normal",
    title: "Permissions, both granted",
    description: "Both permissions are granted, each marked by a soft success Granted badge at the end of its row, where the Grant button would be, and its description no longer says Granted.",
    state,
    host: MAC_HOST,
    macosPermissions: { screenRecording: true, accessibility: true },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
    ],
  },
  {
    id: "settings-permissions-browser-on-mac",
    group: GROUP,
    category: "normal",
    title: "Permissions, a browser on a Mac",
    description: "A browser on a Mac has no grants to manage, but the Permissions section is still listed, so the list is the macOS app's. Its page is the section heading and one muted line, \"Permissions can only be managed in the macOS app.\", with no rows and no Grant button. A Windows browser and the Windows app do not list the section (see Keyboard shortcuts in the Windows app for the Windows app's list).",
    state,
    host: browserHost("MacIntel"),
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
    host: MAC_HOST,
    macosPermissions: "unreadable",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.permissions")),
    ],
  },
  {
    id: "settings-app-behavior-launch-approval-open-failed",
    group: GROUP,
    category: "exceptional",
    title: "App behavior, login items did not open",
    description: "After the \"Open System Settings\" button of the awaiting-approval Launch at login row is pressed, the system does not open its list of login items: an error alert titled \"Could not open System Settings\" says System Settings did not open and to open it yourself and choose \"General › Login Items\", each as an inline chip. The row behind it still reads on with the button, the button and switch beneath the text at Phone width (390px).",
    state,
    host: MAC_HOST,
    desktopApp: true,
    launchAtLogin: "requiresApproval",
    launchAtLoginOpenError: "the system did not open the Login Items pane",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.appBehavior")),
      (ui) => ui.press(ui.t("settings.launchAtLogin.openSettings")),
    ],
  },
  {
    id: "settings-permissions-system-open-failed",
    group: GROUP,
    category: "exceptional",
    title: "Permissions, System Settings did not open",
    description: "After Grant is refused on Screen Recording and its Open System Settings is pressed, the system does not open the pane: an error alert titled \"Could not open System Settings\" says it did not open and to open it yourself, choose Privacy & Security, then choose Screen & System Audio Recording. The title is plain text; System Settings, Privacy & Security and Screen & System Audio Recording in the body are each a chip. OK is its only button: no recovery action and no Error details. The Notifications pane failing to open reads alike.",
    state,
    host: MAC_HOST,
    macosPermissions: { screenRecording: false, accessibility: false },
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
    description: "The lead line sits under the section heading, above a rule that opens the five rows: the session-request, resume-request, account-switch and handoff switches, all off, then Ask questions as choices, on, which is its default. Each of the four approval descriptions starts with \"Once this is on\" and says only what being on allows; the resume one names reopening archived sessions or resuming interrupted sessions and existing conversations, and none says what happens until then. The last row's description states what being on does, ending with the sentence that a change reaches sessions started afterwards. Switch a row to read its other description; none moves another.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.agentPermissions")),
    ],
  },
  {
    id: "settings-cleanup",
    group: GROUP,
    category: "normal",
    title: "Cleanup",
    description: "The section after Keyboard shortcuts, directly above Configuration, with a single row and no group heading: \"Automatically delete archived sessions whose conversation is gone\", on, its description saying when archived sessions are checked and that pinned and locked sessions are kept. Switch it off to read the description that starts with \"Once this is on\".",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.cleanup")),
    ],
  },
  {
    id: "settings-git",
    group: GROUP,
    category: "normal",
    title: "Settings: git",
    description: "The section list reads General, App behavior, Agent behavior, Agent accounts, Git, Trusted folders, Notifications, Keyboard shortcuts, Cleanup, Configuration, About. Git has three headed groups holding seven rows. Repository: Default clone directory (its explanation describes when the chosen directory is used, and the path appears in the input), then Automatically pull repositories, off, whose description starts with \"Once this is on\" and says a project's branch is pulled when it is behind its upstream and can be fast-forwarded, with a small muted \"?\" after the label holding the sentence about the remote being checked. Development: Where sessions develop reads In a worktree of its own, with the explanation for that choice; Squash commits before delivery is off, its explanation saying what turning it on would do; Rebase before delivery is on, with the explanation that says what being on does; Delivery, last, reads Ask each time in a select at the end of its row. Protection: Revert gate is off, with a small muted \"?\" at the end of its label and an explanation starting \"Once this is on, in a project session\". Choose Phone (390px) to see the Default clone directory field with its Browse button, and the selects, stretch across their rows beneath the labels, the Browse button keeping its size.",
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
    state: { ...browserState, settings: { ...browserState.settings!, auto_pull_repositories: true, git_workflow: { development: "branch", squash_before_delivery: true, rebase_before_delivery: false, delivery: "pull_request", revert_gate: true } } },
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
    state: { ...browserState, settings: { ...browserState.settings!, git_workflow: { development: "in_place", squash_before_delivery: true, rebase_before_delivery: true, delivery: "pull_request", revert_gate: true } } },
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
    description: "Claude Code's tab. Account rows have drag grips in a separate unlabelled first column, including Default; the Account header aligns with the names, and their menus offer Move earlier and Move later. Drag a grip to save a new order, or use the menu actions with a keyboard; Escape cancels a drag. The agents are tabs in an underline strip, each with its own small ⋮ button at its end and the strip ending in two icon buttons, Add agent and Add account. The tab holds only the table: the agent's own settings (Tags, Permission approval) are in the dialog behind the ⋮ button's Settings entry, shown by the Agent settings scenarios, and the CLI version in the one behind its Version entry, shown by the CLI version scenarios. Under it, with nothing between, one table lists the agent's accounts, Work, Default and Client in the saved order, under the headers Account, Usage, Status and Disable (with a small \"?\") and an unlabelled last column holding only the account's ⋮ menu. Each row has the name with its plan badge; a two-layer usage bar like the one the account pickers show, the session window's over the weekly one, with no numbers and, unlike the pickers' bars, no hairline edge (Work has reached its limit, so its bar is empty and grey, also where use went past 100%, followed in the same cell by a warning-coloured \"Limit reached\" badge, while its name stays muted with no badge beside it, unlike the rail's usage panel; the default account and Client have neither); a soft success \"Logged in\" badge in the Status cell after it; and the Disable switch, off. Client has no reading yet, so it has no plan or badge and its Usage cell says it is being read. Above the tab strip, the account-selection switch is followed directly by the \"Start usage windows automatically\" switch, both off, each description starting with \"Once this is on\"; the usage-window one is a single short sentence on what the switch does and its usage cost, with a small muted \"?\" at the end of its label. Remove Work or Client: the row fades out while sliding to the right while the remaining rows slide into place. \"Codex uninstalled\" among the \"Live changes\" buttons is the machine finding Codex missing while the strip is shown: its tab gains the warning mark and the tooltip that says why, widening in place while the tab after it slides along; \"Codex installed\" takes the mark away again. With reduced motion the strip changes at once. Choose 560px to see the narrow layout: at 768px and narrower the table is wider than the section, so it scrolls sideways, its columns keeping their widths, the later columns cut at the edge and the ⋮ column pinned at the end with a hairline at its start while there is more beside it; the tab strip, the two switches above it and the tab buttons fit the width, and the scroller is a focusable region while it overflows.",
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
    description: "The default account, Work and Trial are turned off: their Disable switches are on and their Status cells read a neutral \"Disabled\" badge in place of the login state. The default account has a session running on it, so it keeps its name, plan and usage bar. Work has no usage reading, as none is taken for a turned-off account: a dash in the usage cell, not \"Reading usage…\", and no plan badge, beside \"Disabled\"; its details, from its menu, read \"Unknown\" for the login with a dash under Usage. Trial has a session running on it and is signed out: a dash in the usage cell and no plan, and no Log in icon beside \"Disabled\", where a signed-out account that is on has one; its menu still offers Log in. Client's switch is off, and its Status cell has no badge as its first reading is still coming (\"Reading usage…\"). The header's \"?\" next to Disable explains that a disabled account no longer appears in account choices or usage displays, except while a running session still uses it, new sessions do not use it, and running sessions keep running. Turn Disable off for any of them and the switch moves back; turn it on for Client and it does.",
    // Four rows under the three switches and the tab strip: taller than the default window, whose body would scroll.
    height: 1060,
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
    description: "The default account's details, in a modal titled Account details: the body starts with the bold name Default followed on the same line by the Max badge and \"Logged in\", its muted email dev@octoboard.example on the line under them, and the config directory is the line under that, ending right after the path with the small \"?\" that explains the default account. Usage shows all three windows, the model-specific one under a model name as long as one can be. Choose 560px to see the details stack at 768px and narrower: that name wraps beside its reset time on the same line, with its bar under it. Nothing in the dialog can be changed: the Disable and sharing switches are in Account settings.",
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
    steps: [...openAccounts, (ui) => ui.press(ui.t("agents.notInstalled", { agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "codex") })), openDetails("Personal", "codex")],
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
    id: "settings-account-details-reset-credits",
    group: GROUP,
    category: "normal",
    title: "Account details, a used-up Codex account holding reset credits",
    description: "Personal's details on the Codex tab: its weekly window is used up (0% remaining), and under the Usage meters a line reads \"2 usage limit resets available · expires in 6d 3h\" beside a \"Redeem reset\" button.",
    state: resetCreditsState,
    steps: codexDetails,
  },
  {
    id: "settings-account-details-reset-credit-used",
    group: GROUP,
    category: "normal",
    title: "Account details, right after a reset was used",
    description: "Use reset was confirmed in Personal's details and the new reading has arrived: the confirmation, kept open above the details, is titled \"Usage reset\" with \"Usage updated.\" under both meters, which are back to 100% remaining, and Done as its only button. In the details behind it the line has dropped to \"1 usage limit reset available · expires in 21d 9h\", the second credit's expiry.",
    state: resetCreditsState,
    steps: [...codexDetails, redeemFromDetails, (ui) => ui.confirm(), (ui) => ui.wait(700)],
  },
  {
    id: "settings-account-details-reset-credit-last-used",
    group: GROUP,
    category: "normal",
    title: "Account details, after the last reset was used and Done pressed",
    description: "Personal holds a single credit, which was used: the confirmation has been dismissed with Done and the details dialog is back in front with both meters at 100% remaining and no reset line under them, the \"Redeem reset\" button gone with the credit. Focus is where the details dialog puts it on opening, not on the removed button.",
    state: lastResetCreditState,
    steps: [...codexDetails, redeemFromDetails, (ui) => ui.confirm(), (ui) => ui.wait(700), (ui) => ui.press(ui.t("quota.resetCredits.doneAction"))],
  },
  {
    id: "settings-account-mcp-reading",
    group: GROUP,
    category: "normal",
    title: "Connectors and MCP, being read for the first time",
    description: "Work's Connectors and MCP dialog, opened from the third entry of its ⋮ menu, which has no answer yet (a Claude Code reading can take over a minute): the account's name and email, and under them one progress line, a spinner beside \"Reading connectors and MCP servers…\". There is no Refresh button and no \"Updated\" time yet; both come with the first answer. The dialog has no footer and can be closed meanwhile.",
    state: detailsWorkState,
    accountMcpInventory: "pending",
    steps: accountMcp("Work"),
  },
  {
    id: "settings-account-mcp-reading-slow",
    group: GROUP,
    category: "normal",
    title: "Connectors and MCP, the first reading is slow",
    // The only way to this view is the time: the dialog says it is still reading once the first reading has run past
    // `SLOW_READING_MS`, and the gallery has no clock to advance, so the step waits it out in real time.
    description: "The first reading of Work's Connectors and MCP dialog has not answered after 15 seconds (the page takes that long to get here): the progress line has become \"Still reading… Claude Code can take about a minute.\" beside the same spinner, still with no Refresh button and no \"Updated\" time.",
    state: detailsWorkState,
    accountMcpInventory: "pending",
    steps: [...accountMcp("Work"), (ui) => ui.wait(SLOW_READING_MS + 500)],
  },
  {
    id: "settings-account-mcp-claude",
    group: GROUP,
    category: "normal",
    title: "Connectors and MCP, a Claude Code account's",
    description: "Work's Connectors and MCP dialog, read 4 minutes ago (the account's name and email on the first row, with \"Updated 4 minutes ago\" and the Refresh button at its end). Platform connectors, a table of Name, Status and Details: Gmail and Google Calendar with a green Connected badge, Slack Not connected and Notion State unknown, with \"Manage on claude.ai\" at the end of the group heading and, under the table, the muted line telling the person to log in to claude.ai with the account this one uses. Local MCP servers, a table of Name, Sign-in, Runtime and Details, with no Actions column as none of these servers has OAuth sign-in: linear, Needs sign-in with Unknown runtime, its URL and the hint to run /mcp in a session of this account under it; github, brought by the github plugin, Unknown sign-in and a red Failed badge with an icon-only mark right after it (its message is behind it); sentry, Signed in and Running, with its URL; filesystem, Not needed and Running, with no details; and a server whose name and URL are far wider than their columns, both wrapping inside them. The body scrolls to reach the last row; choose a narrow width (under about 600px) to see each table scroll sideways inside its card.",
    state: detailsWorkState,
    accountMcpInventory: { ageMinutes: 4, connectors: CLAUDE_CONNECTORS, servers: CLAUDE_SERVERS },
    steps: accountMcp("Work"),
  },
  {
    id: "settings-account-mcp-server-failed",
    group: GROUP,
    category: "normal",
    title: "Connectors and MCP, why a local MCP server failed",
    description: "The red mark right after the Failed badge of the github server in Work's Connectors and MCP dialog is hovered, so its popover is open: it holds only what Claude Code said as raw text, on several lines and in the monospace of an error message, with nothing reworded and no title; the popover stays while the pointer is on the mark or the popover, and its text can be selected.",
    state: detailsWorkState,
    accountMcpInventory: { ageMinutes: 4, connectors: CLAUDE_CONNECTORS, servers: CLAUDE_SERVERS },
    steps: [...accountMcp("Work"), (ui) => ui.hoverStatusMark(ui.t("settings.accounts.mcp.server.failedLabel"))],
  },
  {
    id: "settings-account-mcp-codex",
    group: GROUP,
    category: "normal",
    title: "Connectors and MCP, a Codex account's",
    description: "Personal's Connectors and MCP dialog on the Codex tab: the link reads \"Manage on ChatGPT\" and the line under the connectors table names ChatGPT. Each connector's Details cell says which plugins bring it (\"From plugins github and pull-request-review\", the singular for one). In the Local MCP servers table a server's Details cell shows how many tools it offers (\"3 tools\", \"1 tool\") under its URL or plugin, and the one that needs sign-in has no hint to run /mcp, as Codex has no such command. Two servers have OAuth sign-in, so the table has a visible Actions column: figma, Needs sign-in, has \"Sign in\", and linear, with its Signed in badge, has \"Sign out\"; the servers without OAuth (openai-docs, playwright), \"Not needed\", have an empty Actions cell. This is also the end of a successful sign-in: the entry reads Signed in with Sign out. Choose a narrow width (under about 700px) to see the table scroll sideways inside its card.",
    state: quotaState,
    accountMcpInventory: CODEX_MCP,
    steps: codexMcp,
  },
  {
    id: "settings-account-mcp-grok",
    group: GROUP,
    category: "normal",
    title: "Connectors and MCP, a Grok Build account's",
    description: "The default account's Connectors and MCP dialog on the Grok Build tab holds the one group of Local MCP servers: no Platform connectors group above it and no link to a platform's connector settings, so the table starts right under the heading. notes is Not needed and Running, with \"4 tools\"; linear, a remote server with its URL, reads Unknown for both sign-in and runtime, in plain text rather than a badge; sentry, Needs sign-in, has no hint to run a command in a session and, with no Actions column, no Sign in button, as Grok Build offers neither; warehouse has a red Failed badge with its mark.",
    state: quotaState,
    accountMcpInventory: { ageMinutes: 2, servers: GROK_SERVERS },
    steps: grokMcp,
  },
  {
    id: "settings-account-mcp-sign-in-waiting",
    group: GROUP,
    category: "normal",
    title: "Connectors and MCP, signing in to a server in the browser",
    description: "Sign in was pressed on figma in Personal's Connectors and MCP dialog and the browser has not finished: in the Actions column the Sign in button shows its pending state with a Cancel button before it, and in the Details cell, under its URL, a spinner beside \"Waiting for the sign-in in your browser…\". The entry still reads Needs sign-in, and the other servers keep their own buttons.",
    state: quotaState,
    accountMcpInventory: CODEX_MCP,
    accountMcpServerAuth: new Map([["figma", "pending"]]),
    steps: [...codexMcp, pressSignIn("figma")],
  },
  {
    id: "settings-account-mcp-refreshing",
    group: GROUP,
    category: "normal",
    title: "Connectors and MCP, a refresh under way",
    description: "Refresh was pressed in Work's Connectors and MCP dialog and the new reading has not come: the aside at the end of the account line reads \"Refreshing…\" with the pending Refresh button (a spinner) in place of \"Updated 4 minutes ago\", and both tables stay as they were instead of being replaced by the reading line.",
    state: detailsWorkState,
    accountMcpInventory: [{ ageMinutes: 4, connectors: CLAUDE_CONNECTORS, servers: CLAUDE_SERVERS }, "pending"],
    steps: [...accountMcp("Work"), refreshMcp],
  },
  {
    id: "settings-account-mcp-empty",
    group: GROUP,
    category: "normal",
    title: "Connectors and MCP, an account with none",
    description: "Work's Connectors and MCP dialog read just now with nothing in either group: under each group heading a compact empty row, \"No platform connectors\" and \"No local MCP servers\", while \"Manage on claude.ai\" and the line about logging in to claude.ai are still there.",
    state: detailsWorkState,
    accountMcpInventory: {},
    steps: accountMcp("Work"),
  },
  {
    id: "settings-account-mcp-unavailable",
    group: GROUP,
    category: "exceptional",
    title: "Connectors and MCP, could not be read, for a reason a retry can change",
    description: "Work, never logged in, cannot be read: under its name, in place of the two groups a danger alert titled \"Account not logged in\" says its connectors and MCP servers can be read once it is logged in, with \"Log in to the account, then try again.\" and a Try again button. The first row has Refresh but no \"Updated\" time. The other reasons that read the same way, each in its own words, are in the other accounts' Connectors and MCP dialogs (close this one): the default account's, \"Claude Code is not available\" (its command-line tool could not be found or started, with a remedy to install it), and Client's, \"Reading timed out\" (Claude Code did not answer in time, with a remedy about reachable servers; the dialog shows the same alert when no answer has come after 100 seconds).",
    state: notSignedInState,
    accountMcpInventory: new Map([["a-work", mcpUnavailable("not_signed_in")], [null, mcpUnavailable("cli_missing")], ["a-long", mcpUnavailable("timed_out")]]),
    steps: accountMcp("Work"),
  },
  {
    id: "settings-account-mcp-sign-in-failed",
    group: GROUP,
    category: "exceptional",
    title: "Connectors and MCP, signing in to a server failed",
    description: "The sign-in to figma ended with an error from Codex: an alert titled \"Could not sign in to the MCP server\" over the Connectors and MCP dialog says the sign-in to figma (the name as a chip) did not finish, with \"Error details\" open and holding Codex's message as raw text on several lines. Under the sentence is the advice \"Make sure the server is reachable, then sign in again.\" Behind it figma reads Needs sign-in with its Sign in button back. A sign-in that timed out (advice: sign in again and finish in the browser before time runs out) and a failed sign-out (advice: make sure the server is reachable, then sign out again) raise the same alert in their own words.",
    state: quotaState,
    accountMcpInventory: CODEX_MCP,
    accountMcpServerAuth: new Map([["figma", { status: "failed", detail: SIGN_IN_FAILED_DETAIL }]]),
    steps: [...codexMcp, pressSignIn("figma"), (ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "settings-account-mcp-failed",
    group: GROUP,
    category: "exceptional",
    title: "Connectors and MCP, could not be read, with the agent's message",
    description: "Work's reading failed for a reason the application cannot name: a danger alert titled \"Could not read connectors and MCP servers\" with no summary line, \"Try again in a moment.\" under it, Try again, and its Details button pressed, so a popover shows what Claude Code printed as raw text on several lines, unreworded.",
    state: detailsWorkState,
    accountMcpInventory: mcpUnavailable("failed", "error: unknown option '--json'\nUsage: claude mcp list [options]\nTry 'claude mcp list --help' for more information."),
    steps: [...accountMcp("Work"), (ui) => ui.press(ui.t("error.details"))],
  },
  {
    id: "settings-account-mcp-unsupported",
    group: GROUP,
    category: "exceptional",
    title: "Connectors and MCP, an agent that cannot list them",
    description: "Work's reading came back unsupported although the agent is meant to offer it: a danger alert titled \"Not supported\" saying Claude Code cannot list its MCP servers or connectors, with no remedy line and no Try again, as no retry can change it.",
    state: detailsWorkState,
    accountMcpInventory: mcpUnavailable("unsupported_agent"),
    steps: accountMcp("Work"),
  },
  {
    id: "settings-account-mcp-refused",
    group: GROUP,
    category: "exceptional",
    title: "Connectors and MCP, the request to read refused",
    description: "The daemon refused the request itself instead of reading: in place of the two groups a danger alert titled \"Could not read connectors and MCP servers\" says \"Unknown account Work.\" with the name a chip, then a Try again button, with no remedy line and no Details button, as the refusal has nothing more to say. The first row has Refresh but no \"Updated\" time.",
    state: detailsWorkState,
    accountMcpInventory: { code: "unknown_account", params: { account: "a-work" }, message: "Unknown account a-work." },
    steps: accountMcp("Work"),
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
    height: 1100,
    state: emailsState,
    steps: openAccounts,
  },
  {
    id: "settings-accounts-tab-row-tooltip",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, the icon buttons at the end of the tab row",
    description: "Add account, the last icon button in the tab row (the other, Add agent, is beside it), has the pointer on it, so its tooltip, \"Add account\", is open above it. The two buttons are plain icons in the same style as the ⋮ buttons, with no words.",
    state: quotaState,
    steps: [...openAccounts, (ui) => ui.hoverTitledControl(ui.t("settings.accounts.add"))],
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
    description: "Codex's tab holds only its accounts table, right under the tab strip: Codex has no model-specific window, so each Usage cell is the two-layer bar, the 5h window over the weekly one. Automation has no plan, so no plan badge, and its 5h window, read as spent 16 minutes ago, reset a minute ago: its top layer is full; Production, further down, is nearly spent: both layers are almost empty. Codex's own settings are in the dialog behind its ⋮ button, shown by \"Settings: accounts, Codex's agent settings\".",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(providerLabel(ui.t, FIXTURE_PROVIDERS, "codex")),
    ],
  },
  {
    id: "settings-accounts-agent-tags",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, an agent's tags and the suggestions",
    description: "Codex's Agent settings with its Tags row first, above the Permission approval row: the label Tags with a help button beside it, \"Applies to every Codex account\" under it and the field below holding the tags review and tests as removable chips ahead of the input, with the suggestions button at its end. The suggestions list is open under the field and offers the tags the other added agents carry (frontend, backend and research), not the two already picked. Pick one or type a new tag and press Enter to save it at once; remove a chip to save the shorter list. The other agents' settings show the same row with their own tags, or the \"No tags\" placeholder when they have none.",
    state: taggedState,
    steps: [
      ...agentSettings("codex"),
      (ui) => ui.press(ui.t("dialog.project.tagsSuggestions")),
    ],
  },
  {
    id: "settings-accounts-agent-tags-help",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, agent tags explanation",
    description: "The help button beside Tags is touched with a finger. Its popover explains that tags describe the tasks an agent is good at, such as code review or UI development, so other sessions can use them when choosing an agent. The explanation stays beside the Tags label above the field; the same help popover presents the other settings' explanations.",
    state: taggedState,
    steps: [
      ...agentSettings("codex"),
      (ui) => ui.touchPress(ui.t("settings.accounts.tags.helpLabel")),
    ],
  },
  {
    id: "settings-accounts-agent-version",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code's agent version",
    description: "The Version entry of the ⋮ button on Claude Code's tab opens a modal titled \"Agent version\" with a close button in its header and no footer button; the tab behind it holds only the accounts table. The body's first line is Claude Code's icon and its name in bold. Under it the one Version row: its label with a help button (\"About updating\"), \"Version 2.1.319 is installed and up to date.\" with \"Installed with its own installer.\" under it in smaller text, and nothing at its end, since there is nothing to update.",
    state: quotaState,
    steps: agentVersion("claude"),
  },
  {
    id: "settings-accounts-cli-checking",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, CLI version still being checked",
    description: "Claude Code's Agent version dialog, opened from the Version entry of its ⋮ menu, before the daemon has reported any CLI version: under the agent's icon and name the Version row has its label and help button, and says only \"Checking the version…\" in the same size as any description, with no install line and no button at its end.",
    state: { ...quotaState, agentCliStatuses: new Map() },
    steps: agentVersion("claude"),
  },
  {
    id: "settings-accounts-cli-update-available",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, a newer CLI version to update to",
    description: "Claude Code's CLI, installed with npm, has a newer version out. In its Agent version dialog (the Version entry of its ⋮ menu; the dialog has a close button and no footer, and under the agent's icon and name holds only this row) the row says \"Version 2.1.319 is installed. Version 2.1.320 is available.\" with \"Installed with npm.\" under it in smaller text, and a filled primary Update button at its end. Other install methods change only that line (\"Installed with Homebrew.\", \"Installed with its own installer.\").",
    state: withCliStatus(quotaState, OUTDATED_CLI),
    steps: agentVersion("claude", true),
  },
  {
    id: "settings-accounts-cli-updating",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, CLI update running",
    description: "An update of Claude Code's CLI is already running, started here or in another window: the Version row of its Agent version dialog says \"Updating…\" in place of the versions, keeps \"Installed with npm.\" under it, and its Update button is pending (a spinner, not pressable). Running sessions are not touched.",
    state: withCliStatus(quotaState, { ...OUTDATED_CLI, update: agentCliUpdateOf() }),
    steps: agentVersion("claude", true),
  },
  {
    id: "settings-accounts-cli-update-tab-mark",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, a CLI update marked on the tab's actions button",
    description: "Claude Code's CLI has a newer version out, and its ⋮ button, named \"Actions for Claude Code, update available\", is open. A small accent badge sits on the corner of the button, beside the top of its glyph, not covering the three dots (the top right corner, the top left under a right-to-left language), and the button's tooltip reads \"More actions (update available)\"; the menu's Version entry ends in a small accent dot followed by the installed version, 2.1.319, in muted small text (screen readers get \"2.1.319, update available\"). The tab itself has no mark, and the other tabs' buttons, whose CLIs are current, have no badge. The tab's right-click menu has the same entry. An agent that is not installed has no Version entry and shows its not-installed mark on the tab instead.",
    state: withCliStatus(quotaState, OUTDATED_CLI),
    steps: [...openAccounts, pressAgentActions("claude", true)],
  },
  {
    id: "settings-accounts-cli-unknown-install",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, CLI installed in a way that cannot be told",
    description: "Claude Code's CLI lies where no installer of it is recognised, so the Version row of its Agent version dialog says \"Version 2.1.319 is installed.\" with a visible second line in smaller text in the place of the install method, \"Octoboard cannot tell how this was installed. Update it the way you installed it.\", and no Update button. The mouse rests on the help button \"About updating\", whose popover has only the one paragraph: that updating does not stop running sessions, which keep the version they started with while new sessions use the new one.",
    state: withCliStatus(quotaState, agentCliStatusOf("claude", { install_method: "unknown", latest_version: null })),
    steps: [...agentVersion("claude"), (ui) => ui.hoverStatusMark(ui.t("settings.accounts.cli.helpLabel"))],
  },
  {
    id: "settings-accounts-grok",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Grok Build tab",
    description: "Grok Build's tab holds only its accounts table, right under the tab strip, with its one default account, a single weekly layer in its Usage cell and the columns Account, Usage, Status and Disable. Its settings (Tags, Permission approval with Ask, Auto and Always-approve, Claude compatibility, Codex compatibility) are in the dialog behind its ⋮ button, laid out as Codex's with one switch row more, and its version in the dialog behind that button's Version entry.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(providerLabel(ui.t, FIXTURE_PROVIDERS, "grok")),
    ],
  },
  {
    id: "settings-accounts-provider-without-presentation",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, a provider this build has no presentation for",
    description: "The daemon serves a fourth provider the UI knows nothing of. Its tab, the last, is named Acme Agent as the daemon calls it, and carries the generic mark in the neutral colour where the others carry their provider marks. Its tab is open and holds only the table of its default account and Studio, whose usage bars are in the neutral foreground, not a provider's blue or orange.",
    state: unpresentedState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(UNPRESENTED_PROVIDER.label),
    ],
  },
  {
    id: "settings-accounts-agent-settings-without-presentation",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, agent settings of a provider this build has no presentation for",
    description: "The Agent settings of the fourth provider the UI knows nothing of: the first line of the body is the generic mark in the neutral colour and the name Acme Agent as the daemon calls it. The provider declares one setting this build has no words for, which has no row, so the dialog holds only the Tags row, with no Permission approval or Claude compatibility row.",
    state: unpresentedState,
    steps: [...openAccounts, (ui) => ui.press(UNPRESENTED_PROVIDER.label), openAgentSettings(UNPRESENTED_PROVIDER.id)],
  },
  {
    id: "settings-accounts-claude-compatibility-help-codex",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude compatibility explanation",
    description: "The help button beside the Claude compatibility label is touched with a finger. Its popover says what the switch does for this agent and that a change takes effect when a session next starts or resumes; the agent's name, Claude Code, CLAUDE.md and Octoboard are chips, and a chip may wrap across lines. Grok Build's tab words it for its agent in the same popover.",
    state: quotaState,
    steps: [
      ...agentSettings("codex"),
      (ui) => ui.touchPress(ui.t("provider.codex.setting.claude_compatibility.helpLabel")),
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
    description: "The menu a right-click on the Codex tab opens (the Menu key or Shift+F10 on a focused tab does the same): Settings first, with the icon the account menus' Settings entry has, then Version, with the installed version, 2.1.319, in muted small text at its end and no dot since Codex is up to date, after a separator Move earlier and Move later, both available for the middle tab, and after another separator Remove agent in the danger tone. The same entries are behind the tab's own ⋮ button, in the scenario after this one.",
    state: quotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.contextMenu(providerLabel(ui.t, FIXTURE_PROVIDERS, "codex")),
    ],
  },
  {
    id: "settings-accounts-agent-more-menu",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, tab actions button menu open",
    description: "The ⋮ button at the end of Grok Build's tab, named \"Actions for Grok Build\", is open: Settings and Version, the latter with the installed version in muted small text at its end, after a separator Move earlier and Move later, disabled because Grok Build is the last tab, and after another separator Remove agent in the danger tone. Every tab has its own button, so no tab needs to be selected first.",
    state: quotaState,
    steps: [
      ...openAccounts,
      (ui) => ui.press(ui.t("settings.accounts.agentActions", { agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "grok") })),
    ],
  },
  {
    id: "settings-accounts-agent-settings",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Claude Code's agent settings",
    description: "The Settings entry of the ⋮ button on Claude Code's tab opens a modal titled \"Agent settings\" with a close button in its header and no footer button; the tab behind it holds only the accounts table. The body's first line is Claude Code's icon and its name in bold. Under it the Tags row comes first: its label with a help button, \"Applies to every Claude Code account\" under it and the tag field below, empty here, showing the \"No tags\" placeholder and the suggestions button at its end. Then one Permission approval row for the whole agent: its label with a help button, \"Applies to every Claude Code account\" under it and the selector at the end, on Auto; its menu also offers Manual, Accept edits, Plan and Bypass permissions. Open it to see the native summary under each permission mode and an explanation under Follow account configuration; these notes wrap within the menu while the closed selector shows only its chosen label. Open the separate help to see two spaced paragraphs: the first explains when changes take effect and which computer the permissions apply to; the second explains native configuration and resume rules when no mode is specified, and that choosing a mode takes precedence over a saved mode. Last comes the Codex compatibility row under it, with its \"?\" after the name, the same \"Applies to every Claude Code account\" line and its switch at the end, off; the dialog has no Claude compatibility row, which only the other agents' settings have. Choose Phone (390px) to see the Permission approval select stretch across its row beneath the label, while the Codex compatibility switch stays beside its text.",
    state: quotaState,
    steps: agentSettings("claude"),
  },
  {
    id: "settings-accounts-agent-settings-codex",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Codex's agent settings",
    description: "Codex's Agent settings, laid out as Claude Code's. The empty Tags row is first, then its own Permission approval row, \"Applies to every Codex account\", on Approve for me; the menu also offers Ask for approval and Full access, with Custom (config.toml) using each account's configuration for new sessions and retaining saved native permissions for resumed sessions. Open the selector to see the native summary under every option, including Custom; the closed selector keeps only the chosen label. Open the separate help to see two paragraphs with spacing between them: the first explains when changes take effect and which computer the permissions apply to; the second explains Custom for new and resumed sessions and how choosing another mode replaces saved permissions. Last comes the Claude compatibility row, with its \"?\" after the name, the same \"Applies to every Codex account\" line and its switch at the end, off. Choose Phone (390px) to see the Permission approval select stretch across its row beneath the label, while the Claude compatibility switch stays beside its text.",
    state: quotaState,
    steps: agentSettings("codex"),
  },
  {
    id: "settings-accounts-agent-settings-grok",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Grok Build's agent settings",
    description: "Grok Build's Agent settings hold the rows of Codex's with one more: the empty Tags row, its own Permission approval row, \"Applies to every Grok Build account\", on Ask, Auto and Always-approve for the choices, and then two switch rows under it, Claude compatibility and Codex compatibility, each with its \"?\" after the name, the same \"Applies to every Grok Build account\" line and its switch at the end, both off. The two switch rows sit directly one under the other, spaced as the rows above them. Choose Phone (390px) to see the switches stay beside their text.",
    state: quotaState,
    steps: agentSettings("grok"),
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
      (ui) => ui.contextMenu(providerLabel(ui.t, FIXTURE_PROVIDERS, "grok")),
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
      (ui) => ui.contextMenu(providerLabel(ui.t, FIXTURE_PROVIDERS, "claude")),
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
    description: "The Add agent button under \"No agent added\" opens Settings on Agent accounts with the Add agent dialog already in front: all three agents are Installed with their Add buttons, none is Added, and the section behind shows no tabs.",
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
    id: "settings-add-agent-pending",
    group: GROUP,
    category: "normal",
    title: "Add agent, an Add waiting for the daemon",
    description: "Add was pressed for Codex and the daemon has not answered: its button shows the pending spinner, and Grok Build, still being checked, keeps its disabled Add. The footer's Close and the close button are disabled, and Escape and a click outside do nothing, so the dialog cannot be left until the request settles.",
    state: addAgentCheckingState,
    agentAdd: "pending",
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.addAgent")),
      (ui) => ui.press(ui.t("settings.addAgent.addLabel", { agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "codex") })),
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
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Work", agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "claude") })),
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
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: ui.t("settings.accounts.defaultName"), agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "claude") })),
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
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Work", agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "claude") })),
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
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: RENAMED_DEFAULT, agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "claude") })),
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
    description: "Work (never logged in) and Client (whose config directory does not exist yet) both have a dash in the Usage cell, which assistive technology reads as no usage reading, and after it the soft warning \"Not logged in\" badge in the Status column with a small Log in icon button beside it; they have no plan badge, and their Disable switches are off as usual. The default account above them is logged in: its plan, its \"Logged in\" badge and its usage bar. Open Work's inline Log in from the keyboard, then apply the live changes for its logged-in reading and login success in either order: focus returns to Work's actions button when the dialog closes. Restore its signed-out reading to try again. Applying the reading after moving focus to another control preserves that choice.",
    state: notSignedInState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
    ],
    liveChanges: [
      accountQuotaUpdated("work-quota-signed-in", "Work: logged-in reading", signedInState.accountQuotas!.find((quota) => quota.agent === "claude" && quota.account === "a-work")!),
      accountLoginSucceeded("work-login-succeeded", "Work: login succeeds", "claude", "a-work"),
      accountQuotaUpdated("work-quota-signed-out", "Work: signed-out reading", notSignedInState.accountQuotas!.find((quota) => quota.agent === "claude" && quota.account === "a-work")!),
    ],
  },
  {
    id: "settings-account-login-failed",
    group: GROUP,
    category: "exceptional",
    title: "Account login failed",
    description: "Log in was chosen for Work and the CLI gave up: the dialog becomes a fixed failure alert, with the danger icon in its header beside the title \"Could not log in\", no close button, and Escape does nothing. The body is only \"Logging in to Claude Code account Work failed.\" (agent and account as chips): the failure carries no specific cause of its own, so none follows in the sentence. \"Error details\" is open here, holding the CLI's multiline text, and nothing else, in a selectable block of its own. The footer reads Close, then Log in again as the main button, which has the focus. A login refused because the account is being removed ends the sentence with that cause and has no Error details, the layout of the Codex busy refusal below without its remedy paragraph. A login that timed out has no details either, and is the next scenario.",
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
    loginStartError: { code: "provider.codex.login_busy", params: {}, message: "Another Codex account is being signed in. Wait for it to finish, then try again." },
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(providerLabel(ui.t, FIXTURE_PROVIDERS, "codex")),
      (ui) => ui.press(ui.t("settings.accounts.actionsLabel", { name: "Personal", agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "codex") })),
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
      (ui) => ui.press(ui.t("settings.addAgent.addLabel", { agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "codex") })),
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
      (ui) => ui.contextMenu(providerLabel(ui.t, FIXTURE_PROVIDERS, "claude")),
      (ui) => ui.press(ui.t("settings.accounts.moveLater")),
    ],
  },
  {
    id: "settings-account-approval-bypass",
    group: GROUP,
    category: "normal",
    title: "Bypass permissions confirmation",
    description: "Choosing Bypass permissions in the Permission approval row of Claude Code's Agent settings opens a warning modal over them instead of saving: a warning icon, the choice as the title question, a body saying tools can change or delete files without a routine permission prompt in the sessions of every Claude Code account (the agent's name as a chip), while explicit permission rules and built-in safeguards still apply, and the danger-coloured button repeating the choice. Cancelling leaves the row on Auto.",
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
    id: "settings-accounts-sharing-unavailable-windows",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, no sharing where the daemon's computer cannot make file links",
    description: "Work's Account settings with the daemon on a Windows computer whose Developer Mode is off, so it cannot create the file links sharing needs: Share default configuration is still the row above Disable account, with its label and help button, but its switch is off and unavailable (faded, still focusable), and under the row's description a line says why: the computer running the daemon cannot create the file links sharing needs, to turn on Developer Mode in Settings on that computer, then quit Octoboard and open it again, or restart the daemon, with Developer Mode and Settings as chips. The refusal the daemon would give is never reached, since the switch cannot be turned.",
    state: sharingUnavailableState,
    host: WINDOWS_HOST,
    steps: accountSettings("Work"),
  },
  {
    id: "settings-accounts-sharing-on-unlinkable-windows",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, sharing already on where the daemon's computer cannot make file links",
    description: "Work's Account settings with the daemon on a Windows computer whose Developer Mode is off, and Work already sharing: unlike the locked off switch of the scenario above, Share default configuration is on and free (not faded), so it can be turned off. Nothing stands under the switch's description: the host's missing file links are not warned about while sharing is on, and only the in-use reasons would appear there. Turn it off to see the locked-off reason of the scenario above.",
    state: sharingUnlinkableOnState,
    host: WINDOWS_HOST,
    steps: accountSettings("Work"),
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
    id: "settings-accounts-sharing-help-codex",
    group: GROUP,
    category: "normal",
    title: "Settings: accounts, Codex sharing explanation",
    description: "The same explanation for a Codex account, without the idle-sessions paragraph. What is never shared is one sentence, that login credentials are never shared, and no file is named. Personal's switch is off, with nothing under it.",
    state: sharingHelpState("codex"),
    steps: openSharingHelp("Personal", "codex"),
  },
  {
    id: "settings-account-approval-error",
    group: GROUP,
    category: "exceptional",
    title: "Permission approval could not be saved",
    description: "The error alert is titled \"Could not save the permission approval\" and opens with the sentence \"Octoboard could not read or write its own data.\", then the remedy for the application's data folder (check that it is writable and its disk available, grant access in system privacy settings if needed, then retry; no suggestion to choose another folder); SQLite's text, \"attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY\", is only in the Error details disclosure, closed here. Codex's Permission approval row, in its Agent settings, keeps Custom (config.toml) behind the alert; acknowledging the error leaves that saved selection intact. Bypass permissions refused for the same reason gets the same alert over its confirmation dialog, which stays open; Claude compatibility refused the same way has its own title, \"Could not save Claude compatibility\", and leaves its switch available again.",
    state: approvalState,
    accountApprovalError: { code: "store_write_failed", params: { cause: "access", detail: "attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY" }, message: "Octoboard could not read or write its own data: attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY" },
    steps: [
      ...agentSettings("codex"),
      (ui) => ui.press(ui.t("provider.codex.setting.approval_reviewer.label", { agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "codex") })),
      (ui) => ui.press(approvalOption(ui.t("provider.codex.setting.approval_reviewer.choice.auto_review"))),
    ],
  },
  {
    id: "settings-accounts-agent-tags-error",
    group: GROUP,
    category: "exceptional",
    title: "Agent tags could not be saved",
    description: "A tag was picked from the suggestions in Codex's Agent settings and the save was refused: an error alert titled \"Could not save the tags\" opens with the same sentence and remedy for the application's data folder, and SQLite's text is in the Error details disclosure, closed here. Behind it the Tags row shows the stored tags again, review and tests, without the tag that was picked.",
    state: taggedState,
    agentTagsError: { code: "store_write_failed", params: { cause: "access", detail: "attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY" }, message: "Octoboard could not read or write its own data: attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY" },
    steps: [
      ...agentSettings("codex"),
      (ui) => ui.press(ui.t("dialog.project.tagsSuggestions")),
      (ui) => ui.press("frontend"),
    ],
  },
  {
    id: "settings-accounts-cli-latest-failed",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, the latest CLI version could not be looked up",
    description: "The lookup of Claude Code's latest version failed, which blocks nothing: in its Agent version dialog the Version row says \"Version 2.1.319 is installed.\" with \"Installed with npm.\" under it and, at its end, a warning mark before a plain secondary Update button (the version being current is not known). No alert is raised. The mouse rests on the mark, so its popover is open: \"The latest version could not be looked up.\" with the network error under it as raw text.",
    state: withCliStatus(quotaState, agentCliStatusOf("claude", { install_method: "npm", latest_version: null, latest_error: "agent_cli_latest_failed", latest_detail: "getaddrinfo ENOTFOUND registry.npmjs.org" })),
    steps: [...agentVersion("claude"), (ui) => ui.hoverStatusMark(ui.t("daemon.agent_cli_latest_failed"))],
  },
  {
    id: "settings-accounts-cli-update-timed-out",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, an update that did not finish, found in the row",
    description: "An earlier update of Claude Code's CLI, run in another window or before its Agent version dialog was opened, was stopped after ten minutes. The row is as it was before the update (\"Version 2.1.319 is installed. Version 2.1.320 is available.\", \"Installed with npm.\", a primary Update button) with a warning mark before the button; no alert is raised. The mouse rests on the mark, so its popover is open and says that updating Claude Code (the name as a chip) did not finish within ten minutes and was stopped, and that the install may be partly updated, without a remedy line. The mark stays until the CLI is current or the next update starts.",
    state: withCliStatus(quotaState, { ...OUTDATED_CLI, update: agentCliUpdateOf({ state: "failed", started_at: minutesAgo(11), finished_at: minutesAgo(1), error: "agent_cli_update_timed_out" }) }),
    steps: [...agentVersion("claude", true), (ui) => ui.hoverStatusMark(ui.t("daemon.agent_cli_update_timed_out.summary", { agent: "Claude Code" }))],
  },
  {
    id: "settings-accounts-cli-problems-combined",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, the latest CLI version lookup and the last update both failed",
    description: "Both things went wrong for Claude Code's CLI, found in the row when its Agent version dialog opened: one warning mark, not two, sits before a plain secondary Update button (the version being current is not known), under \"Version 2.1.319 is installed.\" and \"Installed with npm.\". The mouse rests on the mark, so its one popover is open and lists both reasons, each with its own text under it: that the latest version could not be looked up, with the network error as raw text, and that updating Claude Code (the name as a chip) failed, with the line npm printed, and neither reason has a remedy line. No alert is raised.",
    state: withCliStatus(quotaState, agentCliStatusOf("claude", {
      install_method: "npm",
      latest_version: null,
      latest_error: "agent_cli_latest_failed",
      latest_detail: "getaddrinfo ENOTFOUND registry.npmjs.org",
      update: agentCliUpdateOf({ state: "failed", started_at: minutesAgo(4), finished_at: minutesAgo(3), error: "agent_cli_update_failed", detail: CLI_UPDATE_DETAIL }),
    })),
    steps: [
      ...agentVersion("claude"),
      (ui) => ui.hoverStatusMark([ui.t("daemon.agent_cli_latest_failed"), ui.t("daemon.agent_cli_update_failed.summary", { agent: "Claude Code" })].join(ui.t("common.sentenceSeparator"))),
    ],
  },
  {
    id: "settings-accounts-cli-update-failed",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, CLI update failed",
    description: "Update was pressed on the Version row of Claude Code's Agent version dialog and the command failed: an error alert titled \"Could not update the agent\" says \"Updating Claude Code failed.\" and, as a line of its own, \"Fix the problem shown in the details, or update it the way you installed it.\", with the line npm printed (a permission error on its global folder) in the Error details disclosure, open here. Behind it the row is as it was, now with a warning mark before its Update button.",
    state: withCliStatus(quotaState, OUTDATED_CLI),
    agentCliUpdateFailure: { error: "agent_cli_update_failed", detail: CLI_UPDATE_DETAIL },
    steps: [...agentVersion("claude", true), (ui) => ui.press(ui.t("settings.accounts.cli.update")), (ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "settings-accounts-cli-update-failed-timed-out",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, CLI update did not finish",
    description: "Update was pressed on the Version row of Claude Code's Agent version dialog and the command was stopped after ten minutes: an error alert titled \"Could not update the agent\" says that updating Claude Code (the name as a chip) did not finish within ten minutes and was stopped and that the install may be partly updated, then, as a line of its own, \"Check the install, then try again.\" It has no Error details, since the command printed nothing to show. Behind it the row is as it was, now with a warning mark before its Update button.",
    state: withCliStatus(quotaState, OUTDATED_CLI),
    agentCliUpdateFailure: { error: "agent_cli_update_timed_out", detail: null },
    steps: [...agentVersion("claude", true), (ui) => ui.press(ui.t("settings.accounts.cli.update")), (ui) => ui.wait(600)],
  },
  {
    id: "settings-accounts-cli-update-refused",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, CLI update refused",
    description: "Update was pressed on the Version row of Claude Code's Agent version dialog, but the daemon found that the npm that installed it is no longer on its PATH: an error alert titled \"Could not update the agent\" says Octoboard cannot update Claude Code, that either it was not installed in a way that can be recognised or the npm it was installed with is not available, and to update it the way it was installed, with no Error details and no remedy. The row behind it is unchanged and its Update button is available again.",
    state: withCliStatus(quotaState, OUTDATED_CLI),
    agentCliUpdateError: { code: "agent_cli_update_unsupported", params: { agent: "Claude Code" }, message: "Claude Code cannot be updated: no command is known to update it" },
    steps: [...agentVersion("claude", true), (ui) => ui.press(ui.t("settings.accounts.cli.update")), (ui) => ui.wait(600)],
  },
  {
    id: "settings-accounts-sharing-failed",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change could not be finished",
    description: "In Work's settings dialog, turning sharing off did not finish, so the switch sits at its position before the change, on, and the row shows no state description. A warning triangle named \"Last sharing change\" sits after the switch in the same row, with nothing written beside it, and nothing about the failure is under the row (no alert, no Details button, no Try again). The row keeps its label and switch on their lines at the dialog's width, the triangle not wrapping under the switch. Its popover is in the next scenario.",
    state: sharingFailedState,
    steps: accountSettings("Work"),
  },
  {
    id: "settings-accounts-sharing-failed-details",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change could not be finished, several causes, popover open",
    description: "The triangle after the switch is pressed, so its popover is open with focus inside it. In order: the title, \"Could not finish turning sharing off\"; \"Some files or folders could not be changed.\"; the way to try again, turning the switch off again; one compact line per cause (no permission to change skills/ and 1 more, plugins/ in use, and \"Could not change settings.json.\", whose fix reads that the system's own reason is below), each followed by its fix; the note that changing sharing ends the account’s 2 idle sessions and resumes them afterwards; then the raw side: each unfinished entry one tight row, its chip and what it was left as in words (the conversation abc with the titles of the sessions that recorded it, Fix the summary layout and Update the dependencies, joined as a list), followed by the system's original reason as small inline code on the same row.",
    state: sharingFailedState,
    steps: [...accountSettings("Work"), (ui) => ui.touchPressStatusMark(ui.t("settings.accounts.sharing.unfinishedMark"))],
  },
  {
    id: "settings-accounts-sharing-no-space-details",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change failed, not enough disk space, popover open",
    description: "Turning sharing on did not finish, so Work's switch sits at its position before the change, off, and the triangle after it is pressed. The popover holds the title \"Could not finish turning sharing on\", the single cause \"There is not enough disk space.\" and its fix \"Free up disk space.\" as the two sentences after it (no entry is named), then the advice to turn the switch on again to try once more, then the raw reason alone in a block on its own tinted surface. A default account's directory that has moved since sharing was turned on is worded in the same place.",
    state: sharingNoSpaceState,
    steps: [...accountSettings("Work"), (ui) => ui.touchPressStatusMark(ui.t("settings.accounts.sharing.unfinishedMark"))],
  },
  {
    id: "settings-accounts-sharing-incomplete-details",
    group: GROUP,
    category: "exceptional",
    title: "Sharing change left unfinished by a restart, popover open",
    description: "The choice is recorded as on, but the daemon was restarted in the middle of the change, so Work's switch sits at its position before the change, off, with the triangle after it pressed. The popover is short: the title, \"The last attempt to turn sharing on did not finish\", and the advice to turn the switch on again. There is no reason, no cause line and no raw block, since no entries are known after a restart.",
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
    description: "In Work's settings dialog the switch was pressed to turn sharing on and the change did not finish. The window that asked is told in an error alert dialog over the settings dialog, which says what could not be finished for Work and why, with the raw entries behind its Error details; the switch sits at its position before the change, off, and the warning triangle \"Last sharing change\" after it stays as the lasting state once the alert is acknowledged. The same failure seen only through another window shows the triangle alone (the scenarios before this one).",
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
    id: "settings-accounts-codex-not-installed",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, Codex not installed",
    description: "The Codex tab is selected and carries a warning icon after its name. Neither the default account nor Personal has a plan, a status or a usage reading: their Usage cells are dashes and their Status cells empty.",
    state: unavailableQuotaState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("agents.notInstalled", { agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "codex") })),
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
        ui.t("agents.notInstalled", { agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "codex") }),
        ui.t("settings.accounts.notInstalledTip", { agent: providerLabel(ui.t, FIXTURE_PROVIDERS, "codex"), pathVar: "PATH" }),
      ),
    ],
  },
  {
    id: "settings-accounts-agent-more-menu-not-installed",
    group: GROUP,
    category: "exceptional",
    title: "Settings: accounts, actions menu of an agent that is not installed",
    description: "The ⋮ button at the end of Codex's tab, which carries the not-installed warning mark, is open: Settings, then no Version entry since there is no CLI to read, and after a separator Move earlier, Move later and, after another separator, Remove agent. Its Agent settings dialog is laid out as an installed agent's.",
    state: unavailableQuotaState,
    steps: [...openAccounts, pressAgentActions("codex")],
  },
  {
    id: "settings-account-add-directory",
    group: GROUP,
    category: "normal",
    title: "Add account, first available numbered directory",
    description: "The config directory is filled with ~/.claude-3: ~/.claude-1 belongs to an existing account even though its folder is missing, ~/.claude-2 still exists on disk without an account, and ~/.claude-4 also exists. The gap at 3 is reused. Changing the agent updates the suggested directory until it is edited or chosen with Browse; the name stays empty and focused.",
    state: {
      ...browserState,
      settings: { ...browserState.settings!, accounts: [...browserState.settings!.accounts, accountOf("a-numbered", "claude", "Archive", "/Users/dev/.claude-1")] },
    },
    directories: {
      "/Users/dev": [2, 4].map((number) => ({ name: `.claude-${number}`, path: `/Users/dev/.claude-${number}`, is_git_repo: false })),
    },
    steps: [...openAccounts, (ui) => ui.press(ui.t("settings.accounts.add"))],
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
      (ui) => ui.pressEach(providerLabel(ui.t, FIXTURE_PROVIDERS, "claude")),
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
    id: "settings-account-delete-trash-running",
    group: GROUP,
    category: "normal",
    title: "Remove account to the Trash, sessions still running",
    description: "The option is ticked and Work has four live sessions, so a plain line reads \"The account still has 4 sessions that are running or starting\" and says they must be ended or switched to another account first, over the list of them with their status marks, agent icons and titles; Remove is disabled. Neither Work's archived session nor Client's session is listed. Unticking the option removes the list and enables Remove.",
    state: workRunningState,
    steps: removeWorkToTrash,
  },
  {
    id: "settings-account-delete-trash-checking",
    group: GROUP,
    category: "normal",
    title: "Remove account to the Trash, directory being checked",
    description: "The option is ticked and the check of the directory has not answered: Remove shows its pending spinner ahead of its label and cannot be pressed, with no alert under the option yet.",
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
  {
    id: "settings-account-delete-trash-prevented-shared-account",
    group: GROUP,
    category: "exceptional",
    title: "Remove account to the Trash prevented, directory shared with an account",
    description: "A warning alert under the ticked option says the directory overlaps the directory of the Claude Code account Client, the agent and the account each a chip, so it cannot be moved to the Trash. Remove is disabled. The other causes (the home folder or Octoboard's own data, the default account's directory, a project's folder, a directory that could not be looked at, read or checked) put their own sentence in the same alert.",
    state: browserState,
    accountTrashInspection: { status: "shared", kind: "account", name: "Client", agent: "Claude Code" },
    steps: removeWorkToTrash,
  },
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
    description: "Try again was pressed on the failed check. The danger alert under the ticked option stays with its text and Details, and Try again stays where it is with its pending spinner ahead of the label; Remove, disabled, shows its spinner too.",
    state: browserState,
    accountTrashInspection: [{ code: "internal_error", params: { detail: "the daemon could not check the directory" }, message: "the daemon could not check the directory" }, "pending"],
    steps: [...removeWorkToTrash, (ui) => ui.press(ui.t("error.tryAgain"))],
  },
  ...TRASH_REFUSALS.map(({ key, error, title, description }): Scenario => ({
    id: `settings-account-delete-trash-${key}`,
    group: GROUP,
    category: "exceptional",
    title,
    description,
    state: browserState,
    accountDeleteError: error,
    steps: [...removeWorkToTrash, (ui) => ui.confirm()],
  })),
  {
    id: "settings-account-name-taken-existing",
    group: GROUP,
    category: "exceptional",
    title: "Add account, name already in use",
    description: "The conflicting account name is a chip in the field's validation message. Typing the default account's name, or \"Default\" while the default account is renamed, or renaming the default account to a name in use, is refused in the same place under the field, worded for its cause.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.add")),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.type("Work"),
      (ui) => ui.focus(ui.t("settings.accounts.configDir")),
      (ui) => ui.type("/Users/dev/.claude-studio"),
      (ui) => ui.confirm(),
    ],
  },
  {
    id: "settings-account-add-directory-failed",
    group: GROUP,
    category: "exceptional",
    title: "Add account, home directory could not be read",
    description: "Opened for Grok Build: the config directory stays empty with ~/.grok as its placeholder, and under it Grok Build's own note on the directory (it need not exist yet; a session needs a Grok Build home initialized by running the CLI against it) in place of the generic one. A danger alert beneath it says the directory could not be read, with Details and Try again. The user can retry or enter a config directory manually; entering one dismisses the alert.",
    state: { ...browserState, settings: { ...browserState.settings!, added_agents: [...FIXTURE_AGENTS] } },
    directories: {
      "/Users/dev": { code: "permission_denied", params: { path: "/Users/dev", detail: "Operation not permitted" }, message: "/Users/dev could not be read: Operation not permitted" },
    },
    steps: [...openAccounts, (ui) => ui.press(providerLabel(ui.t, FIXTURE_PROVIDERS, "grok")), (ui) => ui.press(ui.t("settings.accounts.add"))],
  },
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
    id: "settings-account-directory-taken-owned",
    group: GROUP,
    category: "exceptional",
    title: "Add account, config directory used by another account",
    description: "Once Add is pressed, the daemon's refusal, naming the account that already uses the directory (the default account, when it is that one's), is the error alert dialog's, raised over the dialog; the config directory field and the dialog itself show no error.",
    state: browserState,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.accounts")),
      (ui) => ui.press(ui.t("settings.accounts.add")),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.type("Studio"),
      (ui) => ui.focus(ui.t("settings.accounts.configDir")),
      (ui) => ui.type("~/.claude-work/"),
      (ui) => ui.confirm(),
    ],
  },
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
    host: MAC_HOST,
    desktopNotifications: true,
    steps: [
      (ui) => ui.press(ui.t("rail.settings")),
      (ui) => ui.press(ui.t("settings.section.notifications")),
    ],
  },
  {
    id: "settings-notifications-windows",
    group: GROUP,
    category: "normal",
    title: "Settings on a Windows host: notifications, no Permissions section",
    description: "The section list reads General, App behavior, Agent behavior, Agent accounts, Git, Trusted folders, Notifications, Keyboard shortcuts, Cleanup, Configuration, About: Permissions is not listed, which only macOS has. The Notifications row has no switch: its description says Windows manages these notifications and they are turned off or restyled in Settings, with no separate in-app switch, and a secondary Open Settings button sits at the row's end, which opens the Windows Settings notifications page. Look for Windows and Settings as chips, and for the plain name Settings, not System Settings, on the line and the button.",
    state: { ...browserState, ...WINDOWS_MACHINE },
    host: WINDOWS_HOST,
    desktopNotifications: true,
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
