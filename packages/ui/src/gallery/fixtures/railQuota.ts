import { AGENT_LABEL } from "../../agents/agents";
import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import { AGENTS, type Account, type Agent, type AgentAvailability, type AccountQuota, type Settings } from "../../protocol";
import { describing, type Step } from "../../fixtures/interact";
import type { FixtureError } from "../../fixtures/sampleFiles";
import type { Scenario } from "../scenario";
import { accountOf, SAMPLE, minutesAgo, snapshotState } from "../../fixtures/builders";
import { accountAdded, accountRemoved, agentAvailability } from "./liveChanges";
import { USED_UP_MIX, manyClaudeAccounts, quotaFailure, quotaOf, quotaWindow, PERSONAL_RESET_CREDITS, resetCreditOf, resetCreditsOf } from "../../fixtures/quotas";

const GROUP = "Window layout";

const { console: console_, web, api, sessions } = SAMPLE;

/** A name too long for the panel's width, so it clips with its fade and the plan badge stays whole beside it. */
const LONG_NAME = "Acme Corporation internal platform research and billing";

/** The window under the rail's quota entry: the sample console, and the accounts the panel lists. By default Claude
 * Code and Codex are installed and every agent is added, so each of those two has a tab and Grok Build, not
 * installed, has none; `installed` and `added` (in the order the user added them) say otherwise. */
function railQuotaState(
  accounts: Account[],
  accountQuotas: AccountQuota[],
  { installed = ["claude", "codex"], added = AGENTS, defaultNames, turnedOffDefaults }: { installed?: readonly Agent[]; added?: readonly Agent[]; defaultNames?: Settings["default_account_names"]; turnedOffDefaults?: Agent[] } = {},
) {
  return {
    ...snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions,
      settings: { ...DEFAULT_SETTINGS, added_agents: [...added], default_clone_dir: "/Users/dev/Projects", accounts, default_account_names: defaultNames ?? {}, disabled_default_accounts: turnedOffDefaults ?? [] },
      accountQuotas,
    }),
    agentAvailability: new Map<Agent, AgentAvailability>(
      AGENTS.map((agent) => [
        agent,
        { agent, availability: installed.includes(agent) ? "available" : "unavailable", default_account_dir: `/Users/dev/.${agent}` },
      ]),
    ),
  } satisfies Scenario["state"];
}

const claudeAccounts: Account[] = [
  accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
  accountOf("a-long", "claude", LONG_NAME, "/Users/dev/.claude-long"),
];
const codexAccounts: Account[] = [
  accountOf("a-codex", "codex", "Personal", "/Users/dev/.codex-personal"),
  accountOf("a-codex-auto", "codex", "Automation", "/Users/dev/.codex-automation"),
];

/** Every account of both tabs read: Claude Code's default account, which the user has named, with three windows, Work
 * spent and an account with a clipped name that is nearly spent; Codex's default account, Personal, and Automation, which has no plan. */
const readState = railQuotaState([...claudeAccounts, ...codexAccounts], [
  quotaOf("claude", null, "max", [
    quotaWindow("weekly_model", 0, null, "Fable"),
    quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60),
    quotaWindow("session", 18, 117),
  ]),
  quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 42), quotaWindow("weekly", 137, 2 * 24 * 60 + 5 * 60)], { fetched_at: minutesAgo(7) }),
  quotaOf("claude", "a-long", "pro", [quotaWindow("session", 96, 83), quotaWindow("weekly", 88, 2 * 24 * 60 + 9 * 60)], { fetched_at: minutesAgo(3) }),
  quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
  quotaOf("codex", "a-codex", "self_serve_business_prolite", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)], { fetched_at: minutesAgo(3) }),
  quotaOf("codex", "a-codex-auto", null, [quotaWindow("session", 3, 295), quotaWindow("weekly", 9, 6 * 24 * 60)]),
], { defaultNames: { claude: "Studio laptop" } });

/** Every agent installed and read, but only Codex then Claude Code added, in that order: Grok Build has a reading and
 * no tab. */
const reorderedState = railQuotaState(
  [...claudeAccounts, ...codexAccounts],
  [...readState.accountQuotas!, quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 23, 4 * 24 * 60 + 11 * 60)])],
  { installed: AGENTS, added: ["codex", "claude"], defaultNames: { claude: "Studio laptop" } },
);

/** Every agent installed, added and read, one tab each. */
const liveState = railQuotaState(
  [...claudeAccounts, ...codexAccounts],
  [...readState.accountQuotas!, quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 23, 4 * 24 * 60 + 11 * 60)])],
  { installed: AGENTS, defaultNames: { claude: "Studio laptop" } },
);

/** An account another window adds to Claude Code's, between its default account and Work. */
const sideProject: Account = accountOf("a-side", "claude", "Side project", "/Users/dev/.claude-side");

/** Claude Code's accounts that give no whole reading: the default account never read (its attempt timed out), Work
 * with windows from twelve minutes ago and a failed newer attempt, and Client with no entry yet. */
const unreadState = railQuotaState(
  [
    accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
    accountOf("a-client", "claude", "Client", "/Users/dev/.claude-client"),
  ],
  [
    quotaOf("claude", null, null, [], { unavailable: quotaFailure("quota_timed_out", { agent: "Claude Code" }, "Claude Code took too long to report its usage.") }),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)], {
      fetched_at: minutesAgo(12),
      unavailable: quotaFailure("quota_command_failed", { agent: "Claude Code", detail: "exit status 1: Error: not logged in\n    /Users/dev/.claude-work/مرحبا/credentials.json: permission denied" }, "Claude Code could not report its usage: exit status 1: Error: not logged in"),
    }),
  ],
);

/** Claude Code's accounts that were never signed in: Work has a configuration directory without credentials, Client's
 * directory does not exist yet, and the default account is read. */
const notSignedInState = railQuotaState(
  [
    accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
    accountOf("a-client", "claude", "Client", "/Users/dev/.claude-client"),
  ],
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", null, [], { unavailable: quotaFailure("quota_not_signed_in", { agent: "Claude Code" }, "Claude Code is not signed in: Error: not logged in") }),
    quotaOf("claude", "a-client", null, [], { unavailable: quotaFailure("quota_config_dir_missing", { agent: "Claude Code", dir: "/Users/dev/.claude-client" }, "The config directory does not exist") }),
  ],
);

/** Three tabs whose lists all sit under the panel's scroll limit and differ in height: Claude Code's default account and
 * Work with two windows each, Codex's default account with two, and Grok Build's default account with its one. */
const shortTabsState = railQuotaState(
  [accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work")],
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)]),
    quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
    quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 23, 4 * 24 * 60 + 11 * 60)]),
  ],
  { installed: AGENTS },
);

/** Claude Code's default account and eight more (`manyClaudeAccounts`), more than the panel's list is tall. Hub runs
 * on Mobile, the sixth of the eight and the seventh row of the list. */
const many = manyClaudeAccounts();
const longListState = railQuotaState(many.accounts, [
  quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
  ...many.quotas,
]);

/** Claude Code's default account usable, Work used up by its session window (its weekly window has room), and Side
 * project used up by its weekly window while its session window has room. */
const usedUpState = railQuotaState(
  [
    accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
    accountOf("a-side", "claude", "Side project", "/Users/dev/.claude-side"),
  ],
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 42), quotaWindow("weekly", 63, 2 * 24 * 60 + 5 * 60)], { fetched_at: minutesAgo(7) }),
    quotaOf("claude", "a-side", "pro", [quotaWindow("session", 40, 195), quotaWindow("weekly", 100, 2 * 24 * 60 + 5 * 60)], { fetched_at: minutesAgo(5) }),
  ],
);

/** `accounts` with those named by `ids` turned off. */
function turnedOff(accounts: Account[], ...ids: string[]): Account[] {
  return accounts.map((account) => (ids.includes(account.id) ? { ...account, disabled: true } : account));
}

/** Every agent installed and read. Claude Code: its default account and Work turned off, Side project on. Codex: its
 * default account and Personal, its only accounts, turned off. Grok Build's default account on. */
const turnedOffHiddenState = railQuotaState(
  turnedOff([sideProject, accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"), accountOf("a-codex", "codex", "Personal", "/Users/dev/.codex-personal")], "a-work", "a-codex"),
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)]),
    quotaOf("claude", "a-side", "pro", [quotaWindow("session", 30, 150), quotaWindow("weekly", 22, 4 * 24 * 60)]),
    quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
    quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)]),
    quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 23, 4 * 24 * 60 + 11 * 60)]),
  ],
  { installed: AGENTS, turnedOffDefaults: ["claude", "codex"] },
);

/** Hub runs on Claude Code's Work, which is turned off, as is Side project; the default account and Client are on. */
const turnedOffRunningOwnedState = railQuotaState(
  turnedOff(
    [
      accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
      sideProject,
      accountOf("a-client", "claude", "Client", "/Users/dev/.claude-client"),
    ],
    "a-work",
    "a-side",
  ),
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)]),
    quotaOf("claude", "a-side", "pro", [quotaWindow("session", 30, 150), quotaWindow("weekly", 22, 4 * 24 * 60)]),
    quotaOf("claude", "a-client", "pro", [quotaWindow("session", 8, 240), quotaWindow("weekly", 14, 5 * 24 * 60)]),
    quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
  ],
);

/** Codex with every account turned off (the default one, Personal and Automation), Hub running on its default
 * account; Claude Code's default account and Work are on. */
const turnedOffAgentRunningState = railQuotaState(
  turnedOff(
    [
      accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
      ...codexAccounts,
    ],
    "a-codex",
    "a-codex-auto",
  ),
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)]),
    quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
    quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)]),
    quotaOf("codex", "a-codex-auto", null, [quotaWindow("session", 3, 295), quotaWindow("weekly", 9, 6 * 24 * 60)]),
  ],
  { turnedOffDefaults: ["codex"] },
);

/** Every account of both installed agents turned off, with readings that stay: Claude Code's default account and Work,
 * Codex's default account and Personal. */
const noneOnState = railQuotaState(
  turnedOff([...claudeAccounts.slice(0, 1), codexAccounts[0]], "a-work", "a-codex"),
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)]),
    quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
    quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)]),
  ],
  { turnedOffDefaults: ["claude", "codex"] },
);

/** Claude Code's accounts in every state a person meets (`USED_UP_MIX`), the default account being the usable one. */
const usedUpMixState = railQuotaState(USED_UP_MIX.accounts, USED_UP_MIX.quotas);

/** Codex's accounts in every state of reset credits a person meets. Default: none. Personal: its
 * weekly window used up, two credits, the soonest expiring in 6d 3h. Production: three credits of which Codex gave only
 * the count. Contractor: one credit whose expiry passed a minute ago, with no newer reading. */
const resetCreditsState = {
  ...railQuotaState(
    [
      accountOf("a-codex", "codex", "Personal", "/Users/dev/.codex-personal"),
      accountOf("a-codex-user", "codex", "Production", "/Users/dev/.codex-production"),
      accountOf("a-codex-temp", "codex", "Contractor", "/Users/dev/.codex-contractor"),
    ],
    [
      quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
      quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 34, 190), quotaWindow("weekly", 100, 2 * 24 * 60 + 5 * 60)], {
        fetched_at: minutesAgo(3),
        reset_credits: PERSONAL_RESET_CREDITS,
      }),
      quotaOf("codex", "a-codex-user", "plus", [quotaWindow("session", 52, 120), quotaWindow("weekly", 62, 3 * 24 * 60)], {
        reset_credits: resetCreditsOf(null, 3),
      }),
      quotaOf("codex", "a-codex-temp", "plus", [quotaWindow("session", 20, 200), quotaWindow("weekly", 91, 2 * 24 * 60)], {
        fetched_at: minutesAgo(9),
        reset_credits: resetCreditsOf([resetCreditOf("credit-4", -1)]),
      }),
    ],
  ),
};

/** `resetCreditsState` with Hub running on the Codex account `account`, so the panel opens on Codex's tab with the list
 * scrolled to that account. */
const resetCreditsOn = (account: string) => ({
  ...resetCreditsState,
  sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, agent: "codex" as const, account_id: account } : session])),
});

/** The steps that float the usage panel in on Codex's tab (Hub is on a Codex account) and press the "Redeem reset" of the account named,
 * which closes the panel and opens the confirmation. */
const redeemFromPanel = (account: string): Step[] => [
  (ui) => ui.press(ui.session("Hub")),
  (ui) => ui.hoverPanel(describing("usage button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
  (ui) => ui.press(ui.t("quota.resetCredits.useFor", { account })),
];

/** A failure of Codex's own command, as the daemon words it. */
const RESET_CREDIT_FAILED: FixtureError = {
  code: "reset_credit_failed",
  params: { agent: "Codex", detail: "unexpected status 503 Service Unavailable: upstream connect error" },
  message: "Codex could not use the reset: unexpected status 503 Service Unavailable: upstream connect error",
};

export const railQuotaScenarios: Scenario[] = [
  {
    id: "rail-quota-selected-session",
    group: GROUP,
    category: "normal",
    title: "Rail usage button and panel, selected session's named account",
    description: "Codex lists Automation, Personal and Default in the saved account order. Hub uses Codex's Personal account: the compact indicator on the usage button is two outlined capsules, the 5h window's (88% remaining) over a slightly thinner one for the week (60%), and the button's accessible name identifies Codex, Personal and both remainders. The button has no tooltip. The panel floated in beside it opens on Codex's tab, the second tab, because the button shows Codex's usage; Personal's bar in the panel matches the button's. While the pointer is on the button every other account's row, Default and Automation, is dimmed by the same veil the sidebar puts over inactive projects, so Personal stands out as the button's account; moving the pointer into the panel takes the veil off. Select a Claude Code session to see the indicator and the open tab follow that session's agent instead.",
    state: {
      ...readState,
      settings: { ...readState.settings!, account_order: { codex: ["a-codex-auto", "a-codex", null] } },
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, agent: "codex", account_id: "a-codex" } : session])),
    },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      // With a session's bar showing, the button's name starts with the plain one and goes on to the reading.
      (ui) => ui.hoverPanel(describing("usage button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-selected-used-up",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, the selected session's account has reached its limit",
    description: "Hub uses Work, whose session window is spent, and the pointer is on the usage button, so the panel floated in beside it is on Claude Code's tab with the button's account marked by the veil: Default and Side project are dimmed, while Work is not: it keeps its \"Limit reached\" badge and grey bars, but its name and headlines are at full strength, unlike Side project's, which has reached its limit and is dimmed as well.",
    state: {
      ...usedUpState,
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, account_id: "a-work" } : session])),
    },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.hoverPanel(describing("usage button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-selected-weekly",
    group: GROUP,
    category: "normal",
    title: "Rail usage button, weekly fallback",
    description: "Hub's default Claude Code account has no session window. The compact indicator is a single outlined capsule showing its weekly 65% remaining, ignoring the model-specific weekly window; the accessible name identifies the default account and the \"Weekly · all models\" remainder, so named because the reading also has a model's weekly window. A Grok Build credit-period window reads alike as a single capsule.",
    state: railQuotaState([], [quotaOf("claude", null, "max", [
      quotaWindow("weekly_model", 90, null, "Fable"),
      quotaWindow("weekly", 35, 3 * 24 * 60),
    ])]),
    steps: [(ui) => ui.press(ui.session("Hub"))],
  },
  {
    id: "rail-quota-hover",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, floated in by hovering",
    description: "No session is selected, so the usage button has no mini bar. The panel floats in beside it, above Settings, after the pointer rests on it, with the button marked open. Claude Code's default account, under the name the user gave it (Studio laptop), shows its session and weekly usage as one two-layer bar (the thick bar for the session, the thin one beneath it for the week, headlined \"Weekly · all models\" because Fable has its own weekly window) and Fable last; Work, with no model window, headlines its week plain \"Weekly\". Codex's tab names its short window \"5h\". Each account's name line ends, at the far right, with a small muted \"Updated N ago\" (the default account's says 1 minute). Work (spent, so receded with a \"Limit reached\" badge after its plan badge and a muted name, both bars empty and grey, read 7 minutes ago) follows, then an account that is nearly spent, whose clipped name fades out before its plan badge, with its \"Updated 3 minutes ago\" on the line below. Manage accounts is at the bottom. Grok Build is added but not installed, so it has no tab. Choose Phone (420px): the rail comes in with the sidebar's drawer, at the start of its card, and the panel is held to the width left beside it instead of its own, over the drawer's card and backdrop, the account names and meters fitting it.",
    state: readState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-scrolled-to-account",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, long list opened on the selected session's account",
    description: "Hub runs on Mobile, far down Claude Code's nine accounts. The panel opens on Claude Code's tab with its list already scrolled so that Mobile is the first whole row in view, only the tail of the row above it showing under the top fade, and the rows after it below; the usage button's compact indicator and accessible name are Mobile's. Nothing else marks the row, and the list scrolls freely from there.",
    state: {
      ...longListState,
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, account_id: "a-mobile" } : session])),
    },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.hoverPanel(describing("usage button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-used-up",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, accounts at their limit in different windows beside a usable one",
    description: "The panel floated in on Claude Code's tab lists the default account as a normal row (72% of its session and 75% of its week left), then four accounts that cannot be used and recede: Work (Team), whose session window is spent and whose latest read failed, so its row also says it is out of date; Second, whose weekly window is spent while its session window is untouched; Third and Fourth, whose session windows are spent with 15% and 86% of the week left. A receded row has a warning-coloured \"Limit reached\" badge after the plan badge, its name and headline muted, and neutral grey bars in place of Claude Code's colour; the default account has none of these. In a short window Third and Fourth are reached by scrolling the list.",
    state: usedUpMixState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-turned-off-hidden",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, turned-off accounts left out",
    description: "Claude Code's tab lists Side project alone: its default account and Work are turned off and have no row, though both have readings. The tabs are Claude Code and Grok Build; Codex has none, since both its accounts are turned off. Manage accounts is the way to the turned-off ones.",
    state: turnedOffHiddenState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-turned-off-running-owned",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, the selected session runs on a turned-off account",
    description: "Hub runs on Work, which is turned off, and keeps running: the usage button carries Hub's compact indicator, and the panel on Claude Code's tab still lists Work, with a warning-coloured \"Disabled\" badge after its name, with the default account and Client, which are on. While the pointer is on the button the other two rows are dimmed and Work stands out. Side project, also turned off, has no row.",
    state: {
      ...turnedOffRunningOwnedState,
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, account_id: "a-work" } : session])),
    },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.hoverPanel(describing("usage button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-turned-off-agent-running",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, the selected session runs on the default account of an agent with every account turned off",
    description: "Hub runs on Codex's default account, and all of Codex's accounts are turned off. Codex keeps its tab, open beside Claude Code's, and lists the default account alone, with the warning-coloured \"Disabled\" badge after its name: Personal and Automation have no row.",
    state: {
      ...turnedOffAgentRunningState,
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, agent: "codex" } : session])),
    },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.hoverPanel(describing("usage button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-pinned-codex",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, opened from the keyboard on Codex's tab",
    description: "Pressing the button with the keyboard opens the panel with focus on its tabs, and the arrow key has moved on to Codex: its tab is selected and carries the focus ring. Automation has no plan badge.",
    state: readState,
    steps: [
      (ui) => ui.focusControl(ui.t("rail.quota")),
      (ui) => ui.key("Enter"),
      // The arrow key a keyboard user takes to the next tab; it selects it and moves the focus ring along.
      (ui) => ui.key("ArrowRight"),
    ],
  },
  {
    id: "rail-quota-live",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, accounts and agents changing while it is open",
    description: "The panel is opened from the keyboard on Claude Code's tab and stays open while the \"Live changes\" buttons make what another window or the machine would: Side project is added between the default account and Work, or removed again, and Codex is found uninstalled, or installed again, taking its tab away from the middle of the strip or giving it back. Added and removed rows and tabs fade while the ones after them slide into place; with reduced motion they appear and disappear at once. Nothing moves when the panel first opens, when its list scrolls or when the window is resized.",
    state: liveState,
    steps: [
      (ui) => ui.focusControl(ui.t("rail.quota")),
      (ui) => ui.key("Enter"),
    ],
    liveChanges: [
      accountAdded("add-account", "Add account Side project", sideProject, quotaOf("claude", "a-side", "pro", [quotaWindow("session", 34, 120), quotaWindow("weekly", 21, 4 * 24 * 60)]), "a-work"),
      accountRemoved("remove-account", "Remove account Side project", sideProject),
      agentAvailability("uninstall-codex", "Codex uninstalled", "codex", "unavailable"),
      agentAvailability("install-codex", "Codex installed", "codex", "available"),
    ],
  },
  {
    id: "rail-quota-short-tabs",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, tabs whose lists are all short, on the shortest",
    description: "Grok Build's tab is selected and lists a single account, with empty space under it: the panel keeps the height of the tallest tab (Claude Code's two accounts) even though none of the lists reaches the scroll limit. Switch to Codex and Claude Code: the panel's top edge and the tab strip stay where they are and only the space under the list changes.",
    state: shortTabsState,
    steps: [
      (ui) => ui.focusControl(ui.t("rail.quota")),
      (ui) => ui.key("Enter"),
      // Two arrow keys from Claude Code's tab to Grok Build's, the last of the three.
      (ui) => ui.key("ArrowRight"),
      (ui) => ui.key("ArrowRight"),
    ],
  },
  {
    id: "rail-quota-none",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel with no agent available",
    description: "Hub is selected but its agent is unavailable, so the usage button has no mini bar. With no agent installed the panel shows the empty state (a glyph, \"No agent available\" and the hint) and Manage accounts is still offered.",
    state: railQuotaState([], [], { installed: [] }),
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-added-order",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, agents in the order they were added",
    description: "Codex was added before Claude Code, so its tab comes first and is the one open. Grok Build is installed and has a reading but is not added, so it has no tab.",
    state: reorderedState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-none-added",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel with agents installed but none added",
    description: "Every agent is installed but none is added, so the panel shows the empty state (a bot glyph, \"No agent added\", \"Add an agent to see its usage here.\" and an Add agent button) rather than the PATH hint. The panel has no footer: its Manage accounts link would only repeat the button.",
    state: railQuotaState([], [], { installed: AGENTS, added: [] }),
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-none-on",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel with every account turned off",
    description: "Every account of both agents is turned off, so the panel shows the empty state (a gauge glyph, \"No enabled account\" and \"Disabled accounts are not shown here.\") with Manage accounts as the empty state's own button, and no tabs and no footer link.",
    state: noneOnState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-removed-agent-session",
    group: GROUP,
    category: "normal",
    title: "Rail usage, selected session of an agent no longer added",
    description: "Hub runs on Claude Code, which is no longer added: the usage button still carries Hub's compact indicator of two capsules (82% remaining in its session window over 95% in its weekly one) and an accessible name for it, while the panel beside it lists only Codex.",
    state: { ...reorderedState, settings: { ...reorderedState.settings!, added_agents: ["codex"] } },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      // With a session's bar showing, the button's name starts with the plain one and goes on to the reading.
      (ui) => ui.hoverPanel(describing("usage button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-unread",
    group: GROUP,
    category: "exceptional",
    title: "Rail usage panel, accounts without a whole reading",
    description: "Hub uses Work, whose failed refresh leaves an older reading: the usage button has no mini bar. Three accounts have no current reading: the default account has no bars and a danger mark after its name (Usage unavailable); Work keeps its older windows, with \"Updated 12 minutes ago\" at the end of its name line followed by a warning mark (Out of date); Client says its usage is being read. The reasons are in the marks' popovers.",
    state: {
      ...unreadState,
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, account_id: "a-work" } : session])),
    },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-failure-reason",
    group: GROUP,
    category: "exceptional",
    title: "Rail usage panel, the reason an account's reading failed",
    description: "The panel is opened from the keyboard and the default account's danger mark (Usage unavailable) is tapped with a finger: its small tooltip-like popover, holding why Claude Code's reading could not be had, is open above the mark with focus inside it, and the panel stays open under it. Resting the pointer on the mark opens the same popover without moving focus.",
    state: unreadState,
    steps: [
      (ui) => ui.focusControl(ui.t("rail.quota")),
      (ui) => ui.key("Enter"),
      (ui) => ui.touchPressStatusMark(ui.t("quota.unavailable")),
    ],
  },
  {
    id: "rail-quota-not-signed-in",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, accounts not logged in yet",
    description: "Work (never logged in) and Client (whose config directory does not exist yet) both read \"Not logged in yet. Log in to use this account.\" in place of a usage reading and a raw CLI error; the default account above them has its windows.",
    state: notSignedInState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-reset-credits",
    group: GROUP,
    category: "normal",
    title: "Rail usage panel, an account holding rate limit reset credits",
    description: "Codex's tab: Personal, used up by its weekly window, reads \"2 usage limit resets available · expires in 6d 3h\" under its meters beside a \"Redeem reset\" button, while the default account above it, which holds no credit, has no such line. Production and Contractor show a count alone, or an expiry that has passed, in the same line.",
    state: resetCreditsState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota")), (ui) => ui.press(AGENT_LABEL.codex)],
  },
  {
    id: "rail-quota-reset-credit-confirm",
    group: GROUP,
    category: "normal",
    title: "Redeem reset confirmation, the credit that would be used",
    description: "\"Redeem reset\" on Personal has closed the panel and opened the confirmation, titled Redeem reset: \"One usage limit reset will be used on Personal.\", the first credit's title and description as Codex gives them, \"Expires in 6d 3h\" (the soonest of the two), Personal's usage meters as they stand (weekly 0% remaining), and the note that the reset applies to the account's eligible usage windows now, then Cancel and Use reset.",
    state: resetCreditsOn("a-codex"),
    steps: redeemFromPanel("Personal"),
  },
  {
    id: "rail-quota-reset-credit-confirm-count-only",
    group: GROUP,
    category: "normal",
    title: "Redeem reset confirmation, a credit Codex gave no details of",
    description: "\"Redeem reset\" on Production: Codex gave only the count, so the confirmation names no credit, with no title, description or expiry, over Production's usage meters, and keeps the note that the reset applies to the account's eligible usage windows now.",
    state: resetCreditsOn("a-codex-user"),
    steps: redeemFromPanel("Production"),
  },
  {
    id: "rail-quota-reset-credit-busy",
    group: GROUP,
    category: "normal",
    title: "Redeem reset confirmation, waiting for Codex",
    description: "Use reset has been confirmed on Personal and Codex has not answered: the confirm button reads \"Resetting usage…\" and Cancel and the close button are held, so the dialog cannot be left meanwhile; the usage meters stay as they were.",
    state: resetCreditsOn("a-codex"),
    resetCreditUse: "pending",
    steps: [...redeemFromPanel("Personal"), (ui) => ui.confirm()],
  },
  {
    id: "rail-quota-reset-credit-failed",
    group: GROUP,
    category: "exceptional",
    title: "Redeem reset confirmation, Codex failed to reset the usage",
    description: "Use reset has been confirmed on Personal and Codex failed: an error alert titled \"Could not reset usage\" is up over the confirmation, with the remedy for a failed reset (check that Codex runs in the login shell and is logged in to the account) and Codex's raw reason under Error details; the confirmation stays open behind it, to try again or cancel once the alert is dismissed.",
    state: resetCreditsOn("a-codex"),
    resetCreditUse: RESET_CREDIT_FAILED,
    steps: [...redeemFromPanel("Personal"), (ui) => ui.confirm()],
  },
  {
    id: "rail-quota-reset-credit-not-signed-in",
    group: GROUP,
    category: "exceptional",
    title: "Redeem reset confirmation, the account is not logged in",
    description: "Use reset has been confirmed on Personal and Codex refused it because the account is not logged in: an error alert titled \"Could not reset usage\" is up over the confirmation, saying Personal (Codex) is not logged in so a reset cannot be redeemed on it and to log in, then redeem again, with a Manage accounts button beside OK; the confirmation stays open behind it.",
    state: resetCreditsOn("a-codex"),
    resetCreditUse: {
      code: "reset_credit_not_signed_in",
      params: { agent: "Codex", account: "Personal" },
      message: "Personal (Codex) is not logged in, so a reset cannot be used on it.",
    },
    steps: [...redeemFromPanel("Personal"), (ui) => ui.confirm()],
  },
  {
    id: "rail-quota-reset-credit-waiting",
    group: GROUP,
    category: "normal",
    title: "Redeem reset, waiting for the new usage reading",
    description: "Codex answered that the reset was used on Personal, and the reading that follows has not come: the dialog is titled \"Usage reset\" and says \"Usage of Personal has been reset.\", the usage meters are frozen as they were (weekly 0% remaining), the status line reads \"Updating usage…\", and Done is the only button.",
    state: resetCreditsOn("a-codex"),
    resetCreditReading: "never",
    steps: [...redeemFromPanel("Personal"), (ui) => ui.confirm(), (ui) => ui.wait(300)],
  },
  {
    id: "rail-quota-reset-credit-refilled",
    group: GROUP,
    category: "normal",
    title: "Redeem reset, the meters refilling when the new reading arrives",
    description: "Use reset was confirmed on Personal. The dialog first waits (\"Usage reset\", \"Updating usage…\", meters as before), then about a second and a half later the reading arrives and both bars grow to full over a third of a second, the status line turning to \"Usage updated.\" with Done as the only button. The picture settles on that.",
    state: resetCreditsOn("a-codex"),
    resetCreditReading: { after: 1500 },
    steps: [...redeemFromPanel("Personal"), (ui) => ui.confirm()],
  },
  {
    id: "rail-quota-reset-credit-update-failed",
    group: GROUP,
    category: "exceptional",
    title: "Redeem reset, the usage reading after it failed",
    description: "Codex answered that the reset was used on Personal, but the reading that follows failed: the dialog reads \"Usage of Personal has been reset.\" over the meters as last read (weekly 0% remaining) with the out-of-date mark, the status line \"Usage could not be updated yet.\", and Done as the only button.",
    state: resetCreditsOn("a-codex"),
    resetCreditReading: { unavailable: quotaFailure("quota_timed_out", { agent: "Codex" }, "Codex took too long to report its usage.") },
    steps: [...redeemFromPanel("Personal"), (ui) => ui.confirm(), (ui) => ui.wait(500)],
  },
  {
    id: "rail-quota-reset-credit-none-left",
    group: GROUP,
    category: "exceptional",
    title: "Redeem reset confirmation, the credit was used or expired meanwhile",
    description: "The confirmation of Personal is open when a new reading arrives with no credit in it: it says \"Personal has no usage limit resets available.\" with no credit named and no note about what a reset does, and Use reset is disabled, leaving Cancel.",
    state: resetCreditsOn("a-codex"),
    steps: [
      ...redeemFromPanel("Personal"),
      (ui) => ui.daemonEvents((state) => ({
        type: "account_quotas_updated",
        account_quotas: state.accountQuotas.map((quota) => {
          if (quota.agent !== "codex" || quota.account !== "a-codex") return quota;
          const { reset_credits: _, ...unspent } = quota;
          return unspent;
        }),
      })),
    ],
  },
  {
    id: "rail-quota-reset-credit-nothing-to-reset",
    group: GROUP,
    category: "exceptional",
    title: "Redeem reset, no window needed a reset",
    description: "Use reset was confirmed on Personal and Codex found no usage window eligible for a reset: the dialog has closed and a notice reads \"The usage of Personal does not need a reset right now.\" A reset with no credit left reads alike.",
    state: resetCreditsOn("a-codex"),
    resetCreditUse: "nothing_to_reset",
    steps: [...redeemFromPanel("Personal"), (ui) => ui.confirm()],
  },
];
