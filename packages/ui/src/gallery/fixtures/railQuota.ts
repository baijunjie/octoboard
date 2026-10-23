import type { Account, Agent, AgentAvailability, AccountQuota, Settings } from "../../protocol";
import { describing } from "../interact";
import type { Scenario } from "../scenario";
import { ALL_AGENTS, SAMPLE, minutesAgo, snapshotState } from "./builders";
import { accountAdded, accountRemoved, agentAvailability } from "./liveChanges";
import { manyClaudeAccounts, quotaFailure, quotaOf, quotaWindow } from "./quotas";

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
  { installed = ["claude", "codex"], added = ALL_AGENTS, defaultNames }: { installed?: Agent[]; added?: Agent[]; defaultNames?: Settings["default_account_names"] } = {},
) {
  return {
    ...snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions,
      settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: added, default_clone_dir: "/Users/dev/Projects", accounts, default_account_names: defaultNames },
      accountQuotas,
    }),
    agentAvailability: new Map<Agent, AgentAvailability>(
      (["claude", "codex", "grok"] as const).map((agent) => [
        agent,
        { agent, availability: installed.includes(agent) ? "available" : "unavailable", default_account_dir: `/Users/dev/.${agent}` },
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
  quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)], { fetched_at: minutesAgo(3) }),
  quotaOf("codex", "a-codex-auto", null, [quotaWindow("session", 3, 295), quotaWindow("weekly", 9, 6 * 24 * 60)]),
], { defaultNames: { claude: "Studio laptop" } });

/** Every agent installed and read, but only Codex then Claude Code added, in that order: Grok Build has a reading and
 * no tab. */
const reorderedState = railQuotaState(
  [...claudeAccounts, ...codexAccounts],
  [...readState.accountQuotas!, quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 23, 4 * 24 * 60 + 11 * 60)])],
  { installed: ALL_AGENTS, added: ["codex", "claude"], defaultNames: { claude: "Studio laptop" } },
);

/** Every agent installed, added and read, one tab each. */
const liveState = railQuotaState(
  [...claudeAccounts, ...codexAccounts],
  [...readState.accountQuotas!, quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 23, 4 * 24 * 60 + 11 * 60)])],
  { installed: ALL_AGENTS, defaultNames: { claude: "Studio laptop" } },
);

/** An account another window adds to Claude Code's, between its default account and Work. */
const sideProject: Account = { id: "a-side", agent: "claude", name: "Side project", config_dir: "/Users/dev/.claude-side" };

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
      unavailable: quotaFailure("quota_command_failed", { agent: "Claude Code", detail: "exit status 1: Error: not logged in\n    /Users/dev/.claude-work/مرحبا/credentials.json: permission denied" }, "Claude Code could not report its quota: exit status 1: Error: not logged in"),
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
  { installed: ALL_AGENTS },
);

/** Claude Code's default account and eight more (`manyClaudeAccounts`), more than the panel's list is tall. Hub runs
 * on Mobile, the sixth of the eight and the seventh row of the list. */
const many = manyClaudeAccounts();
const longListState = railQuotaState(many.accounts, [
  quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
  ...many.quotas,
]);

/** Hub on Claude Code's default account, whose session window was spent when it was read nine minutes ago and reset a
 * minute ago, with Work beside it, spent and not yet reset. Hub's weekly window is unaffected. */
const resetPassedState = railQuotaState(
  [{ id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" }],
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 100, -1), quotaWindow("weekly", 41, 3 * 24 * 60 + 4 * 60)], { fetched_at: minutesAgo(9) }),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 42), quotaWindow("weekly", 63, 2 * 24 * 60 + 5 * 60)], { fetched_at: minutesAgo(9) }),
  ],
);

/** Claude Code's default account usable, Work used up by its session window (its weekly window has room), and Side
 * project used up by its weekly window while its session window has room. */
const usedUpState = railQuotaState(
  [
    { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" },
    { id: "a-side", agent: "claude", name: "Side project", config_dir: "/Users/dev/.claude-side" },
  ],
  [
    quotaOf("claude", null, "max", [quotaWindow("session", 18, 117), quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 42), quotaWindow("weekly", 63, 2 * 24 * 60 + 5 * 60)], { fetched_at: minutesAgo(7) }),
    quotaOf("claude", "a-side", "pro", [quotaWindow("session", 40, 195), quotaWindow("weekly", 100, 2 * 24 * 60 + 5 * 60)], { fetched_at: minutesAgo(5) }),
  ],
);

export const railQuotaScenarios: Scenario[] = [
  {
    id: "rail-quota-selected-session",
    group: GROUP,
    category: "normal",
    title: "Rail quota button and panel, selected session's named account",
    description: "Hub uses Codex's Personal account: the compact indicator on the quota button is two outlined capsules, the 5h window's (88% remaining) over a slightly thinner one for the week (60%), and the button's accessible name identifies Codex, Personal and both remainders. The button has no tooltip. The panel floated in beside it opens on Codex's tab, the second tab, because the button shows Codex's quota; Personal's bar in the panel matches the button's. Select a Claude Code session to see the indicator and the open tab follow that session's agent instead.",
    state: {
      ...readState,
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, agent: "codex", account_id: "a-codex" } : session])),
    },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      // With a session's bar showing, the button's name starts with the plain one and goes on to the reading.
      (ui) => ui.hoverPanel(describing("quota button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-selected-weekly",
    group: GROUP,
    category: "normal",
    title: "Rail quota button, weekly fallback",
    description: "Hub's default Claude Code account has no session window. The compact indicator is a single outlined capsule showing its weekly 65% remaining, ignoring the model-specific weekly window; the accessible name identifies the default account and the \"Weekly · all models\" remainder, so named because the reading also has a model's weekly window.",
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
    description: "Hub uses Grok Build's default account, which has only a credit-period window. The compact indicator is a single outlined capsule showing 77% remaining; the accessible name identifies Grok Build, the default account and credit-period remainder.",
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
    description: "No session is selected, so the quota button has no mini bar. The panel floats in beside it, above Settings, after the pointer rests on it, with the button marked open. Claude Code's default account, under the name the user gave it (Studio laptop), shows its session and weekly quota as one two-layer bar (the thick bar for the session, the thin one beneath it for the week, headlined \"Weekly · all models\" because Fable has its own weekly window) and Fable last; Work, with no model window, headlines its week plain \"Weekly\". Codex's tab names its short window \"5h\". Each account's name line ends, at the far right, with a small muted \"Updated N ago\" (the default account's says 1 minute). Work (spent, so receded with a \"Used up\" badge, both bars empty, read 7 minutes ago) follows, then an account that is nearly spent, whose clipped name fades out before its plan badge. Manage accounts is at the bottom. Grok Build is added but not installed, so it has no tab.",
    state: readState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-scrolled-to-account",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel, long list opened on the selected session's account",
    description: "Hub runs on Mobile, far down Claude Code's nine accounts. The panel opens on Claude Code's tab with its list already scrolled so that Mobile is the first whole row in view, only the tail of the row above it showing under the top fade, and the rows after it below; the quota button's compact indicator and accessible name are Mobile's. Nothing else marks the row, and the list scrolls freely from there.",
    state: {
      ...longListState,
      sessions: new Map(sessions.map((session) => [session.id, session.id === "s-console" ? { ...session, account_id: "a-mobile" } : session])),
    },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.hoverPanel(describing("quota button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-reset-passed",
    group: GROUP,
    category: "normal",
    title: "Rail quota button and panel, a window whose reset has passed",
    description: "Hub uses Claude Code's default account, whose reading (taken 9 minutes ago) has its session window spent, but that window's reset was a minute ago and no newer reading has arrived. The quota button's indicator shows that window full (100% remaining, over the weekly 59%), and in the panel the default account's session bar is full and its headline reads 100% remaining with \"resets now\". Work, whose session window is also spent but resets later, stays empty by comparison and is receded with a \"Used up\" badge, which the default account does not get.",
    state: resetPassedState,
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.hoverPanel(describing("quota button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-used-up",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel, accounts that are used up beside a usable one",
    description: "The panel floated in on Claude Code's tab lists the default account as a normal row, then two accounts that cannot be used and recede: Work, whose session window is spent (its weekly window still has 37% remaining), and Side project, whose weekly window is spent while its session window still has 60% remaining. A receded row has its name and headline muted, a soft \"Used up\" badge after the plan badge, and neutral grey bars in place of Claude Code's colour.",
    state: usedUpState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-pinned-codex",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel, pinned from the keyboard on Codex's tab",
    description: "Pressing the button with the keyboard pins the panel and puts focus on its tabs, and the arrow key has moved on to Codex: its tab is selected and carries the focus ring. Automation has no plan badge.",
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
    title: "Rail quota panel, accounts and agents changing while it is open",
    description: "The panel is pinned open on Claude Code's tab and stays open while the \"Live changes\" buttons make what another window or the machine would: Side project is added between the default account and Work, or removed again, and Codex is found uninstalled, or installed again, taking its tab away from the middle of the strip or giving it back. Added and removed rows and tabs fade while the ones after them slide into place; with reduced motion they appear and disappear at once. Nothing moves when the panel first opens, when its list scrolls or when the window is resized.",
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
    title: "Rail quota panel, agents in the order they were added",
    description: "Codex was added before Claude Code, so its tab comes first and is the one open. Grok Build is installed and has a reading but is not added, so it has no tab.",
    state: reorderedState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-none-added",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel with agents installed but none added",
    description: "Every agent is installed but none is added, so the panel shows the empty state (a bot glyph, \"No agent added\", \"Add an agent to see its quota here.\" and an Add agent… button) rather than the PATH hint. The panel has no footer: its Manage accounts link would only repeat the button.",
    state: railQuotaState([], [], { installed: ALL_AGENTS, added: [] }),
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-removed-agent-session",
    group: GROUP,
    category: "normal",
    title: "Rail quota, selected session of an agent no longer added",
    description: "Hub runs on Claude Code, which is no longer added: the quota button still carries Hub's compact indicator of two capsules (82% remaining in its session window over 95% in its weekly one) and an accessible name for it, while the panel beside it lists only Codex.",
    state: { ...reorderedState, settings: { ...reorderedState.settings!, added_agents: ["codex"] } },
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      // With a session's bar showing, the button's name starts with the plain one and goes on to the reading.
      (ui) => ui.hoverPanel(describing("quota button with a session's bar", (name) => name.startsWith(ui.t("rail.quota"))), ui.t("rail.quota")),
    ],
  },
  {
    id: "rail-quota-unread",
    group: GROUP,
    category: "exceptional",
    title: "Rail quota panel, accounts without a whole reading",
    description: "Hub uses Work, whose failed refresh leaves an older reading: the quota button has no mini bar. Three accounts have no current reading: the default account has no bars and a danger mark after its name (Quota unavailable); Work keeps its older windows, with \"Updated 12 minutes ago\" at the end of its name line followed by a warning mark (Out of date); Client says the quota is being read. The reasons are in the marks' popovers.",
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
    title: "Rail quota panel, the reason an account's reading failed",
    description: "The panel is pinned open from the keyboard and the default account's danger mark (Quota unavailable) is pressed: its popover, holding why Claude Code's reading could not be had, is open beside the mark, and the panel stays open under it.",
    state: unreadState,
    steps: [
      (ui) => ui.focusControl(ui.t("rail.quota")),
      (ui) => ui.key("Enter"),
      (ui) => ui.press(ui.t("quota.unavailable")),
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
