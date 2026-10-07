import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Session statuses";

const console_ = consoleOf("c-1", "Main");
const second = consoleOf("c-2", "Second");
const project = projectOf("p-1", console_.id, "Website");
const other = projectOf("p-2", second.id, "Billing");

const allStatuses = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [
    sessionOf("s-hub", console_.id, undefined, "Hub", "working"),
    sessionOf("s-working", console_.id, project.id, "Working on the layout", "working"),
    sessionOf("s-waiting", console_.id, project.id, "Waiting for an answer", "waiting_user"),
    sessionOf("s-idle", console_.id, project.id, "Awaiting instructions", "idle"),
    sessionOf("s-interrupted", console_.id, project.id, "Interrupted by a restart", "interrupted"),
    sessionOf("s-archived", console_.id, project.id, "Archived and done", "archived"),
  ],
});

export const statusScenarios: Scenario[] = [
  {
    id: "status-all",
    group: GROUP,
    title: "All five statuses",
    description: "One project session in each status and a working hub; the working session is selected.",
    state: allStatuses,
    steps: [(ui) => ui.press(ui.session("Working on the layout"))],
  },
  {
    id: "status-waiting",
    group: GROUP,
    title: "Raised hands",
    description:
      "Three sessions waiting, one of them a hub and one in another console: the top bar's waiting count and the console switcher's marker.",
    state: snapshotState({
      consoles: [console_, second],
      projects: [project, other],
      sessions: [
        sessionOf("s-hub", console_.id, undefined, "Hub", "waiting_user"),
        sessionOf("s-1", console_.id, project.id, "Needs permission to run tests", "waiting_user"),
        sessionOf("s-2", console_.id, project.id, "Working quietly", "working"),
        sessionOf("s-3", second.id, other.id, "Asks which branch to use", "waiting_user"),
      ],
    }),
    steps: [(ui) => ui.press(ui.session("Needs permission to run tests"))],
  },
];
