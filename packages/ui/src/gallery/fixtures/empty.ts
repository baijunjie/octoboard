import type { Agent, AgentAvailability } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

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

export const emptyScenarios: Scenario[] = [
  {
    id: "empty-no-consoles",
    group: GROUP,
    category: "normal",
    title: "Fresh install, detected supported tools",
    description:
      "New console is the primary action, beside the secondary Import configuration button in a centered row that wraps on narrow windows. All three supported CLI tools remain visible with their brand icons: Claude Code and Grok Build are Installed, while Codex is Not detected. Narrow or shorten the window to inspect wrapping and scrolling. New console opens its dialog; creating the first console replaces the welcome screen with the sidebar and terminal.",
    state: { ...snapshotState({}), agentAvailability: DETECTED_AGENTS },
  },
  {
    id: "empty-no-consoles-no-agent",
    group: GROUP,
    category: "exceptional",
    title: "No consoles, no agent installed",
    description: "Every supported tool remains listed with its brand icon and Not detected badge. The PATH hint explains how to make a tool available, while New console and Import configuration stay enabled.",
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
    description: "Before PATH detection completes, every supported tool is already visible with its brand icon and a Checking badge. New console and Import configuration remain available while detection is pending.",
    state: snapshotState({}),
  },
  {
    id: "empty-console-no-agent",
    group: "Sidebar sessions",
    category: "exceptional",
    title: "A console, no agent installed",
    description:
      "Under Console sessions the row says no agent was found on PATH and the section's New console session is disabled; the console session left over from before still shows beneath it. The project below keeps its own empty row.",
    state: {
      ...snapshotState({
        consoles: [consoleOf("c-1", "Main")],
        projects: [projectOf("p-1", "c-1", "Website")],
        sessions: [sessionOf("s-hub", "c-1", undefined, "Hub", "interrupted")],
      }),
      agentAvailability: NO_AGENTS,
    },
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
    description: "The headings show Console sessions (0) and Projects (1); their sections keep compact creation rows. The main area offers New console session.",
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
    description: "The console session remains unselected. The main area offers New session for Website, whose sidebar row offers the same action.",
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
    description: "The console session remains unselected and both projects keep creation rows. The main area asks for a session to be selected, with no creation button.",
    state: snapshotState({
      consoles: [consoleOf("c-1", "Main")],
      projects: [projectOf("p-1", "c-1", "Website"), projectOf("p-2", "c-1", "Search API")],
      sessions: [sessionOf("s-hub", "c-1", undefined, "Hub", "idle")],
    }),
  },
];
