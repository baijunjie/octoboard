import type { Agent, AgentAvailability } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Empty";

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
    title: "Fresh install, detected supported tools",
    description:
      "The welcome screen keeps all three supported CLI tools visible with their brand icons: Claude Code and Grok Build are Installed, while Codex is Not detected. New console and Import configuration remain available. Narrow or shorten the window to inspect wrapping and scrolling. New console still opens its dialog; creating the first console replaces the welcome screen with the sidebar and terminal.",
    state: { ...snapshotState({}), agentAvailability: DETECTED_AGENTS },
  },
  {
    id: "empty-no-consoles-no-agent",
    group: GROUP,
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
    title: "Fresh install, checking supported tools",
    description: "Before PATH detection completes, every supported tool is already visible with its brand icon and a Checking badge. New console and Import configuration remain available while detection is pending.",
    state: snapshotState({}),
  },
  {
    id: "empty-console-no-agent",
    group: GROUP,
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
    group: GROUP,
    title: "A console with no projects",
    description:
      "With neither a project nor a session, the terminal's area offers Add project instead of asking for a session to be selected, and F6 reaches that button; the sidebar's Projects section carries the same offer as a row.",
    state: snapshotState({ consoles: [consoleOf("c-1", "Main")] }),
  },
  {
    id: "empty-project-no-sessions",
    group: GROUP,
    title: "A project with no sessions",
    state: snapshotState({
      consoles: [consoleOf("c-1", "Main")],
      projects: [projectOf("p-1", "c-1", "Website")],
    }),
  },
];
