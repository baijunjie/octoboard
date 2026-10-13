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

// A lead session: an unbound project session later bound to a console session, which kept the
// sessions it had started. Besides it, `s-api-free` in the other project is unbound and has a session
// of its own.
const lead = sessionOf("s-web-lead", console_.id, web.id, "Rework the gallery", "idle", { bound_to: hub1.id });

const sessions = [
  hub1,
  hub2,
  hub3,
  sessionOf("s-web-1", console_.id, web.id, "Fix the summary layout", "working", { bound_to: hub1.id }),
  sessionOf("s-web-2", console_.id, web.id, "Update the dependencies", "idle"),
  sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "waiting_user", { bound_to: hub2.id }),
  sessionOf("s-api-2", console_.id, api.id, "Write the migration", "idle", { bound_to: hub3.id }),
  lead,
  sessionOf("s-web-lead-1", console_.id, web.id, "Chase the flaky test", "working", { bound_to: lead.id }),
  sessionOf("s-web-lead-2", console_.id, web.id, "Measure the first paint", "waiting_user", { bound_to: lead.id }),
  sessionOf("s-api-free", console_.id, api.id, "Plan the reindex", "idle"),
  sessionOf("s-api-free-1", console_.id, api.id, "Check the index sizes", "interrupted", { bound_to: "s-api-free" }),
];

export const consoleSessionsScenarios: Scenario[] = [
  {
    id: "console-sessions-section",
    group: GROUP,
    title: "Console sessions, and project sessions bound, unbound and nested",
    description:
      "The section above the project list lists every console session, each in its own colour. A bound project " +
      "session carries that colour as a binding badge in its sidebar row; an unbound one carries none. The sessions " +
      "bound to a project session are listed under it, a level below: the lead session keeps the binding badge of " +
      "the console session it reports to, and the sessions under either owner carry none. The pointer rests " +
      "on the status glyph of \"Add idempotency keys\", whose tooltip reads Waiting for you; every row's glyph " +
      "has one (Working, Waiting for you, Awaiting instructions, Interrupted), next to the agent mark's own.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    steps: [(ui) => ui.hoverRowStatus(ui.session("Add idempotency keys"))],
  },
];
