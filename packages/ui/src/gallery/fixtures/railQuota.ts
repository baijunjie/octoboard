import type { Account, Agent, AgentAvailability, AccountQuota } from "../../protocol";
import type { Scenario } from "../scenario";
import { SAMPLE, minutesAgo, snapshotState } from "./builders";
import { quotaFailure, quotaOf, quotaWindow } from "./quotas";

const GROUP = "Window layout";

const { console: console_, web, api, sessions } = SAMPLE;

/** A name too long for the panel's width, so it clips with its fade and the plan chip stays whole beside it. */
const LONG_NAME = "Acme Corporation internal platform research and billing";

/** The window under the rail's quota entry: the sample console, and the accounts the panel lists. Claude Code and
 * Codex are available, so each has a tab; Grok Build is not installed and has none. */
function railQuotaState(accounts: Account[], accountQuotas: AccountQuota[], available: Agent[] = ["claude", "codex"]) {
  return {
    ...snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions,
      settings: { auto_pull_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts },
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

/** Every account of both tabs read: Claude Code's default account with three windows, Work spent and an account with a
 * clipped name; Codex's default account, Personal, and Automation, which has no plan. */
const readState = railQuotaState([...claudeAccounts, ...codexAccounts], [
  quotaOf("claude", null, "max", [
    quotaWindow("session", 18, 117),
    quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60),
    quotaWindow("weekly_model", 0, null, "Fable"),
  ]),
  quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 42), quotaWindow("weekly", 137, 2 * 24 * 60 + 5 * 60)]),
  quotaOf("claude", "a-long", "pro", [quotaWindow("session", 46, 83), quotaWindow("weekly", 62, 2 * 24 * 60 + 9 * 60)]),
  quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
  quotaOf("codex", "a-codex", "pro", [quotaWindow("session", 12, 285), quotaWindow("weekly", 40, 24 * 60 + 90)]),
  quotaOf("codex", "a-codex-auto", null, [quotaWindow("session", 3, 295), quotaWindow("weekly", 9, 6 * 24 * 60)]),
]);

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

export const railQuotaScenarios: Scenario[] = [
  {
    id: "rail-quota-hover",
    group: GROUP,
    category: "normal",
    title: "Rail quota panel, floated in by hovering",
    description: "The panel floats in beside the rail's Account quota button, above Settings, after the pointer rests on it, with the button marked open and no tooltip. Claude Code's tab is selected beside Codex's, the default account first and then Work (spent) and an account whose clipped name fades out before its plan chip. Manage accounts is at the bottom.",
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
    description: "With no agent installed the panel shows the empty state (a glyph, \"No agent available\" and the hint) and Manage accounts is still offered.",
    state: railQuotaState([], [], []),
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
  {
    id: "rail-quota-unread",
    group: GROUP,
    category: "exceptional",
    title: "Rail quota panel, accounts without a whole reading",
    description: "Three accounts, none with a current reading: the default account has nothing to show, with the timeout as the reason; Work keeps its older windows under \"Out of date\" with the failure above them; Client says the quota is being read.",
    state: unreadState,
    steps: [(ui) => ui.hoverPanel(ui.t("rail.quota"), ui.t("rail.quota"))],
  },
];
