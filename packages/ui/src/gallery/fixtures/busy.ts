import type { Agent, Project, Session, SessionStatus } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Busy";

const AGENTS: Agent[] = ["claude", "codex", "grok"];
const STATUSES: SessionStatus[] = ["working", "idle", "waiting_user", "interrupted", "archived", "idle"];

const LONG_TITLE =
  "Investigate why the nightly indexing job double-counts documents that were updated across a shard boundary";
const SECOND_LONG_TITLE =
  "Trace the duplicated cache entries and list every boundary condition the eviction path can reach";
const LONG_PATH =
  "/Users/dev/code/clients/acme-corporation/internal-platform/services/search-indexing/packages/index-core";

const consoles = [
  consoleOf("c-1", "Work", { default_agent: "claude" }),
  consoleOf("c-2", "Personal", { default_agent: "codex" }),
  consoleOf("c-3", "A console with a name that is far too long to fit the switcher header"),
  consoleOf("c-4", "Experiments", { default_agent: "grok" }),
];

const projectNames = [
  "Website",
  "Search API",
  "Media gateway",
  "A project whose name keeps going well past the width of the sidebar",
  "Mobile app",
  "Design system",
  "index-core",
  "Docs",
  "Infrastructure",
  "Internal tools",
  "Analytics",
  "Admin console",
];

const projects: Project[] = projectNames.map((name, i) =>
  projectOf(`p-${i}`, "c-1", name, {
    pinned: i === 2 || i === 5,
    ...(name === "index-core" ? { path: LONG_PATH } : {}),
    ...(i === 3 ? { path: `${LONG_PATH}/${LONG_PATH.slice(1)}` } : {}),
  }),
);
projects.push(projectOf("p-other", "c-2", "Side project"), projectOf("p-exp", "c-4", "Prototype"));

const sessions: Session[] = [sessionOf("s-console-1", "c-1", undefined, "Hub", "working")];
projects.slice(0, projectNames.length).forEach((project, p) => {
  const count = 2 + (p % 5);
  for (let i = 0; i < count; i++) {
    const title =
      p === 0 && i === 0 ? LONG_TITLE : p === 2 && i === 0 ? SECOND_LONG_TITLE : `${["Refactor", "Fix", "Investigate", "Write tests for", "Review"][(p + i) % 5]} task ${p}-${i}`;
    sessions.push(
      sessionOf(`s-${p}-${i}`, "c-1", project.id, title, STATUSES[(p + i) % STATUSES.length], {
        agent: AGENTS[(p + i) % 3],
        pinned: p === 0 && i === 1,
        started_at: minutesAgo(10 + p * 7 + i * 3),
      }),
    );
  }
});
sessions.push(
  sessionOf("s-c2", "c-2", "p-other", "Try the new parser", "waiting_user"),
  sessionOf("s-c4", "c-4", "p-exp", "Spike", "working"),
);

const state = snapshotState({ consoles, projects, sessions });

export const busyScenarios: Scenario[] = [
  {
    id: "busy-many",
    group: GROUP,
    title: "Many consoles, projects and sessions",
    description:
      "Twelve projects with long and pinned names and long paths, a pinned session, all three agents, a waiting session in another console.",
    state,
  },
  {
    id: "busy-long-selected",
    group: GROUP,
    title: "Two long session titles, one selected",
    description: "The breadcrumb in the top bar and the sidebar row with a title far wider than either.",
    state,
    steps: [(ui) => ui.press(ui.session(LONG_TITLE))],
  },
];
