import type { Agent, AgentAvailability } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Empty";

/** Nothing on the machine's PATH to run a session with. */
const NO_AGENTS = new Map<Agent, AgentAvailability>([
  ["claude", { agent: "claude", availability: "unavailable" }],
  ["codex", { agent: "codex", availability: "unavailable" }],
  ["grok", { agent: "grok", availability: "unavailable" }],
]);

export const emptyScenarios: Scenario[] = [
  {
    id: "empty-no-consoles",
    group: GROUP,
    title: "Fresh install and first console",
    description:
      "A fresh install: the welcome screen fills the content area with New console and, quieter beneath it, Import configuration, and there is no sidebar, terminal hint or top bar sidebar toggle. New console opens its dialog over this welcome screen. Enter Work as its name and Create: the real fixture reply adds the first console, replacing welcome with the sidebar and terminal. The rail keeps its New console, bell and Settings. Narrow the window too: the welcome screen still fills the content area, and there is no drawer to open.",
    state: snapshotState({}),
  },
  {
    id: "empty-no-consoles-no-agent",
    group: GROUP,
    title: "No consoles, no agent installed",
    description: "The welcome screen also carries the install prompt, and New console stays enabled.",
    state: {
      ...snapshotState({}),
      agentAvailability: NO_AGENTS,
    },
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
