import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Sidebar sessions";

const console_ = consoleOf("c-1", "Main");
const web = projectOf("p-web", console_.id, "Website");
const api = projectOf("p-api", console_.id, "Search API");

// Three console sessions, each its own colour from the palette (`style.css`), one of each status a
// console session can be in while live.
const hub1 = sessionOf("s-hub-1", console_.id, undefined, "Hub", "idle", { colour: "olive" });
const hub2 = sessionOf("s-hub-2", console_.id, undefined, "Hub 2", "working", { colour: "jade" });
const hub3 = sessionOf("s-hub-3", console_.id, undefined, "Hub 3", "waiting_user", { colour: "teal" });

const sessions = [
  hub1,
  hub2,
  hub3,
  sessionOf("s-web-1", console_.id, web.id, "Fix the summary layout", "working", { bound_to: hub1.id }),
  sessionOf("s-web-2", console_.id, web.id, "Update the dependencies", "idle"),
  sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "waiting_user", { bound_to: hub2.id }),
  sessionOf("s-api-2", console_.id, api.id, "Write the migration", "idle", { bound_to: hub3.id }),
];

export const consoleSessionsScenarios: Scenario[] = [
  {
    id: "console-sessions-section",
    group: GROUP,
    title: "Three console sessions, bound and unbound project sessions",
    description:
      "The section above the project list lists every console session, each in its own colour. A bound project " +
      "session carries that colour as a binding badge in its sidebar row; an unbound one carries none.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
  },
];
