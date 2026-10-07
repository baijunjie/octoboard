import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const other = projectOf("p-2", console_.id, "Website");

export const focusScenarios: Scenario[] = [
  {
    id: "focus-project",
    group: "Focus mode",
    title: "Focus mode on a project",
    description: "The sidebar given over to one project's sessions, with its recent archive.",
    preferences: { sidebarConsole: console_.id, sidebarFocusProject: project.id },
    state: snapshotState({
      consoles: [console_],
      projects: [project, other],
      sessions: [
        sessionOf("s-console", console_.id, undefined, "Hub 1", "idle", { colour: "teal" }),
        sessionOf("s-1", console_.id, project.id, "Add idempotency keys", "working", { pinned: true, bound_to: "s-console" }),
        sessionOf("s-2", console_.id, project.id, "Fix the rounding of partial results", "waiting_user", {
          bound_to: "s-console",
        }),
        // Not bound to the console session — the contrast case, left as the builder's default.
        sessionOf("s-3", console_.id, project.id, "Write the migration", "idle"),
        sessionOf("s-4", console_.id, project.id, "Older investigation", "archived", {
          ended_at: minutesAgo(60 * 5),
          bound_to: "s-console",
        }),
        sessionOf("s-5", console_.id, project.id, "Last week's spike", "archived", {
          ended_at: minutesAgo(60 * 24 * 6),
          bound_to: "s-console",
        }),
        sessionOf("s-6", console_.id, other.id, "Unrelated work", "working", { bound_to: "s-console" }),
      ],
    }),
    steps: [(ui) => ui.press(ui.session("Add idempotency keys"))],
  },
];
