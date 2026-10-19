import type { Account, Agent, AgentAvailability, AccountQuota, Settings } from "../../protocol";
import type { Scenario } from "../scenario";
import { SAMPLE, minutesAgo, snapshotState } from "./builders";
import { quotaFailure, quotaOf, quotaWindow } from "./quotas";

const GROUP = "Window layout";

const { console: console_, web, api, sessions } = SAMPLE;

/** A name too long for the panel's width, so it clips with its fade and the plan chip stays whole beside it. */
const LONG_NAME = "Acme Corporation internal platform research and billing";

/** The window under the rail's quota entry: the sample console, and the accounts the panel lists. Claude Code and
 * Codex are available, so each has a tab; Grok Build is not installed and has none. */
function railQuotaState(accounts: Account[], accountQuotas: AccountQuota[], available: Agent[] = ["claude", "codex"], defaultNames?: Settings["default_account_names"]) {
  return {
    ...snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions,
      settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: available, default_clone_dir: "/Users/dev/Projects", accounts, default_account_names: defaultNames },
      accountQuotas,
    }),
    agentAvailability: new Map<Agent, AgentAvailability>(
      (["claude", "codex", "grok"] as const).map((agent) => [
        agent,
        { agent, availability: available.includes(agent) ? "available" : "unavailable", default_account_dir: `/Users/dev/.${agent}` },
      ]),
    ),
  } satisfies Scenario["state"];
}

const claudeAccounts: Account[] = [
  { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" },
  { id: "a-long", agent: "claude", name: LONG_NAME, config_dir: "/Users/dev/.claude-long" },
];
const codexAccounts: Account[] = [
  { id: "a-codex", agent: "codex", name: "Personal", config_dir: "/Users/dev/.codex-personal" },
  { id: "a-codex-auto", agent: "codex", name: "Automation", config_dir: "/Users/dev/.codex-automation" },
];

/** Every account of both tabs read: Claude Code's default account, which the user has named, with three windows, Work
 * spent and an account with a clipped name; Codex's default account, Personal, and Automation, which has no plan. */
const readState = railQuotaState([...claudeAccounts, ...codexAccounts], [
  quotaOf("claude", null, "max", [
    quotaWindow("weekly_model", 0, null, "Fable"),
    quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60),
    quotaWindow("session", 18, 117),
  ]),
  quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 42), quotaWindow("weekly", 137, 2 * 24 * 60 + 5 * 60)]),
  quotaOf("claude", "a-long", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)]),
  quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
  quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)]),
  quotaOf("codex", "a-codex-auto", null, [quotaWindow("session", 3, 295), quotaWindow("weekly", 9, 6 * 24 * 60)]),
], ["claude", "codex"], { claude: "Studio laptop" });

/** Claude Code's accounts that give no whole reading: the default account never read (its attempt timed out), Work
 * with windows from twelve minutes ago and a failed newer attempt, and Client with no entry yet. */
const unreadState = railQuotaState(
  [
    { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" },
    { id: "a-client", agent: "claude", name: "Client", config_dir: "/Users/dev/.claude-client" },
  ],
  [
    quotaOf("claude", null, null, [], { unavailable: quotaFailure("quota_timed_out", { agent: "Claude Code" }, "Claude Code took too long to report its quota.") }),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)], {
      fetched_at: minutesAgo(12),
      unavailable: quotaFailure("quota_command_failed", { agent: "Claude Code", detail: "exit status 1: Error: not logged in" }, "Claude Code could not report its quota: exit status 1: Error: not logged in"),
    }),
  ],
);

/** Claude Code's accounts that were never signed in: Work has a configuration directory without credentials, Client's
 * directory does not exist yet, and the default account is read. */
const notSignedInState = railQuotaState(
  [
    { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" },
    { id: "a-client", agent: "claude", name: "Client", config_dir: "/Users/dev/.claude-client" },
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
  [{ id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" }],
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)]),
    quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
    quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 23, 4 * 24 * 60 + 11 * 60)]),
  ],
  ["claude", "codex", "grok"],
);

export const railQuotaScenarios: Scenario[] = [
  {
    id: "rail-quota-selected-session",
    group: GROUP,
    category: "normal",
    title: "Rail quota button, selected session's named account",
    description: "Hub uses Codex's Personal account: the compact bar on the quota button shows 88% remaining in its five-hour window, ahead of its weekly limit. The button's accessible name identifies Codex, Personal and the five-hour remainder. Select a Claude Code session to see the bar follow that session's default account instead.",
    state: {
      ...readState,
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, agent: "codex", account_id: "a-codex" } : session])),
    },
    steps: [(ui) => ui.press(ui.session("Hub"))],
  },
  {
    id: "rail-quota-selected-weekly",
    group: GROUP,
    category: "normal",
    title: "Rail quota button, weekly fallback",
    description: "Hub's default Claude Code account has no five-hour window. The bar shows its weekly 65% remaining, ignoring the model-specific weekly window; the accessible name identifies the default account and weekly remainder.",
    state: railQuotaState([], [quotaOf("claude", null, "max", [
      quotaWindow("weekly_model", 90, null, "Fable"),
      quotaWindow("weekly", 35, 3 * 24 * 60),
    ])]),
    steps: [(ui) => ui.press(ui.session("Hub"))],
  },
  {
    id: "rail-quota-selected-credit",
    group: GROUP,
    category: "normal",
    title: "Rail quota button, credit-period fallback",
    description: "Hub uses Grok Build's default account, which has only a credit-period window. The bar shows 77% remaining in the gauge's colour; the accessible name identifies Grok Build, the default account and credit-period remainder.",
    state: {
      ...shortTabsState,
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, agent: "grok" } : session])),
    },
    steps: [(ui) => ui.press(ui.session("Hub"))],
  },
  {
    id: "rail-quota-hover",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel, floated in by hovering",
    description: "No session is selected, so the quota button has no mini bar. The panel floats in beside it, above Settings, after the pointer rests on it, with the button marked open and no tooltip. Claude Code's default account, under the name the user gave it (Studio laptop), lists five-hour quota first, weekly next and Fable last. Work (spent) follows, then an account whose clipped name fades out before its plan chip. Manage accounts is at the bottom.",
    state: readState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-pinned-codex",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel, pinned from the keyboard on Codex's tab",
    description: "Pressing the button with the keyboard pins the panel and puts focus on its tabs, and the arrow key has moved on to Codex: its tab is selected and carries the focus ring. Automation has no plan chip.",
    state: readState,
    steps: [
      (ui) => ui.focusControl(ui.t("rail.quota")),
      (ui) => ui.key("Enter"),
      // The arrow key a keyboard user takes to the next tab; it selects it and moves the focus ring along.
      (ui) => ui.key("ArrowRight"),
    ],
  },
  {
    id: "rail-quota-short-tabs",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel, tabs whose lists are all short, on the shortest",
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
    id: "rail-quota-narrow",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel in a narrow window",
    description: "In a window this narrow the panel is held to the width left beside the rail instead of its own, and the account names and meters fit it.",
    width: 390,
    state: readState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-none",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel with no agent available",
    description: "Hub is selected but its agent is unavailable, so the quota button has no mini bar. With no agent installed the panel shows the empty state (a glyph, \"No agent available\" and the hint) and Manage accounts is still offered.",
    state: railQuotaState([], [], []),
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-unread",
    group: GROUP,
    category: "exceptional",
    title: "Rail quota panel, accounts without a whole reading",
    description: "Hub uses Work, whose failed refresh leaves an older reading: the quota button has no mini bar. Three accounts have no current reading: the default account has nothing to show, with the timeout as the reason; Work keeps its older windows under \"Out of date\" with the failure above them; Client says the quota is being read.",
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
    id: "rail-quota-not-signed-in",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel, accounts not logged in yet",
    description: "Work (never logged in) and Client (whose config directory does not exist yet) both read \"Not logged in yet. Log in to use this account.\" in place of a quota and a raw CLI error; the default account above them has its windows.",
    state: notSignedInState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
];
