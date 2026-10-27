import type { Agent, AgentAvailability } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";
import { DEFAULT_GIT_WORKFLOW } from "../../gitWorkflow";

const GROUP = "Welcome";

/** Detection has resolved; supported tools can differ in availability. */
const DETECTED_AGENTS = new Map<Agent, AgentAvailability>([
  ["claude", { agent: "claude", availability: "available", default_account_dir: "/Users/dev/.claude" }],
  ["codex", { agent: "codex", availability: "unavailable", default_account_dir: "/Users/dev/.codex" }],
  ["grok", { agent: "grok", availability: "available", default_account_dir: "/Users/dev/.grok" }],
]);

/** Nothing on the machine's PATH to run a session with. */
const NO_AGENTS = new Map<Agent, AgentAvailability>([
  ["claude", { agent: "claude", availability: "unavailable", default_account_dir: "/Users/dev/.claude" }],
  ["codex", { agent: "codex", availability: "unavailable", default_account_dir: "/Users/dev/.codex" }],
  ["grok", { agent: "grok", availability: "unavailable", default_account_dir: "/Users/dev/.grok" }],
]);

/** Every agent is on the machine's PATH. */
const ALL_INSTALLED = new Map<Agent, AgentAvailability>(
  (["claude", "codex", "grok"] as const).map((agent) => [agent, { agent, availability: "available", default_account_dir: `/Users/dev/.${agent}` }]),
);

/** A console with a project and a leftover console session, every agent installed and none added. */
export const noneAddedState = {
  ...snapshotState({
    consoles: [consoleOf("c-1", "Main")],
    projects: [projectOf("p-1", "c-1", "Landing page")],
    sessions: [sessionOf("s-hub", "c-1", undefined, "Hub", "interrupted")],
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/Users/dev/Projects", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [] },
  }),
  agentAvailability: ALL_INSTALLED,
} satisfies Scenario["state"];

export const emptyScenarios: Scenario[] = [
  {
    id: "empty-no-consoles",
    group: GROUP,
    category: "normal",
    title: "Fresh install, detected supported tools",
    description: "New console is the primary action beside the secondary Import configuration button, in a centred row that wraps on narrow windows. Claude Code and Grok Build carry a soft success Installed badge with a check, and Codex a neutral Not detected one, each tool with its brand icon. Narrow or shorten the window to inspect wrapping and scrolling.",
    state: { ...snapshotState({}), agentAvailability: DETECTED_AGENTS },
  },
  {
    id: "empty-no-consoles-phone",
    group: GROUP,
    category: "normal",
    title: "Fresh install in a phone-width window",
    width: 390,
    description: "With no console there is no sidebar drawer to bring the rail in, so below 782px it stays in the row beside the content as the way to New console and Settings, and the content panel keeps its rounded top corner at the rail.",
    state: { ...snapshotState({}), agentAvailability: DETECTED_AGENTS },
  },
  {
    id: "empty-no-consoles-no-agent",
    group: GROUP,
    category: "normal",
    title: "No consoles, no agent installed",
    description: "Every supported tool remains listed with its brand icon and a neutral Not detected badge. The PATH hint explains how to make a tool available, while New console and Import configuration stay enabled.",
    state: {
      ...snapshotState({}),
      agentAvailability: NO_AGENTS,
    },
  },
  {
    id: "empty-no-consoles-checking-agents",
    group: GROUP,
    category: "normal",
    title: "Fresh install, checking supported tools",
    description: "Before PATH detection completes, every supported tool is already visible with its brand icon and a neutral Checking badge. New console and Import configuration remain available while detection is pending.",
    state: snapshotState({}),
  },
  {
    id: "empty-console-no-agent",
    group: "Sidebar sessions",
    category: "normal",
    title: "A console, no agent installed",
    description:
      "Under Console sessions the row says no agent was found on PATH, beside a crossed-out bot glyph rather than a warning, and the console header's \"+\" (New console session) is disabled; the console session left over from before still shows beneath it. Landing page's \"No sessions yet\" row has no action, and the project's \"+\", which a hover over its row reveals, is disabled too. The main area says the same as the row under a crossed-out bot glyph, with the PATH hint beneath and no button. Modifier: Held changes nothing here.",
    state: {
      ...snapshotState({
        consoles: [consoleOf("c-1", "Main")],
        projects: [projectOf("p-1", "c-1", "Landing page")],
        sessions: [sessionOf("s-hub", "c-1", undefined, "Hub", "interrupted")],
      }),
      agentAvailability: NO_AGENTS,
    },
    steps: [(ui) => ui.press("Landing page")],
  },
  {
    id: "empty-console-no-agent-added",
    group: "Sidebar sessions",
    category: "normal",
    title: "A console, agents installed but none added",
    description:
      "Under Console sessions the row says \"No agent added\" with nothing about PATH, and the console header's \"+\" (New console session) is disabled; the console session left over from before still shows beneath it. Landing page's \"No sessions yet\" row has no action, and the project's \"+\", which a hover over its row reveals, is disabled too. The row carries a bot glyph. With Landing page selected, the main area has a bot glyph too, says the same and asks for an agent to be added, with an Add agent… button beneath that opens Settings → Agent accounts.",
    state: noneAddedState,
    steps: [(ui) => ui.press("Landing page")],
  },
  {
    id: "empty-console-no-projects",
    group: "Projects",
    category: "normal",
    title: "A console with no projects",
    description:
      "The sidebar headings show Console sessions (1) and Projects (0). The main area explains that you can create a project yourself or ask a console session to create one, alongside Add project. F6 reaches that button; the sidebar's Projects section carries the same offer as a row.",
    state: snapshotState({
      consoles: [consoleOf("c-1", "Main")],
      sessions: [sessionOf("s-hub", "c-1", undefined, "Hub", "idle")],
    }),
  },
  {
    id: "empty-no-console-session",
    group: "Sidebar sessions",
    category: "normal",
    title: "No console session: create a console session",
    description: "The headings show Console sessions (0) and Projects (1). Console sessions keeps its compact creation row, while Website starts folded. The main area offers New console session.",
    state: snapshotState({
      consoles: [consoleOf("c-1", "Main")],
      projects: [projectOf("p-1", "c-1", "Website")],
    }),
  },
  {
    id: "empty-console-session-one-project",
    group: "Sidebar sessions",
    category: "normal",
    title: "A console session and one project: create a project session",
    description: "The console session remains unselected and Website starts folded. The main area offers New session for Website, whose sidebar header offers the same action.",
    state: snapshotState({
      consoles: [consoleOf("c-1", "Main")],
      projects: [projectOf("p-1", "c-1", "Website")],
      sessions: [sessionOf("s-hub", "c-1", undefined, "Hub", "idle")],
    }),
  },
  {
    id: "empty-console-session-several-projects",
    group: "Sidebar sessions",
    category: "normal",
    title: "A console session and several projects: select a session",
    description: "The console session remains unselected and both projects start folded. The main area asks for a session to be selected, with no creation button.",
    state: snapshotState({
      consoles: [consoleOf("c-1", "Main")],
      projects: [projectOf("p-1", "c-1", "Website"), projectOf("p-2", "c-1", "Search API")],
      sessions: [sessionOf("s-hub", "c-1", undefined, "Hub", "idle")],
    }),
  },
];
