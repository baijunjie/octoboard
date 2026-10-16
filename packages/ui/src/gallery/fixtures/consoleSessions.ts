import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Sidebar sessions";

const console_ = consoleOf("c-1", "Main");
const web = projectOf("p-web", console_.id, "Website");
const api = projectOf("p-api", console_.id, "Search API");

// Four console sessions, each its own colour from the palette (`sessionColours.ts`), one of each status a
// console session can be in while live.
const hub1 = sessionOf("s-hub-1", console_.id, undefined, "Hub", "idle", { colour: "yellow", terminal_title: "Review the release plan" });
const hub2 = sessionOf("s-hub-2", console_.id, undefined, "Hub 2", "working", { colour: "lime" });
const hub3 = sessionOf("s-hub-3", console_.id, undefined, "Hub 3", "waiting_user", { colour: "teal" });
const hub4 = sessionOf("s-hub-4", console_.id, undefined, "Hub 4", "interrupted", { colour: "violet" });

// A lead session: an unbound project session later bound to a console session, which kept the
// sessions it had started. Besides it, `s-api-free` in the other project is unbound and has a session
// of its own.
const lead = sessionOf("s-web-lead", console_.id, web.id, "Rework the gallery", "idle", { bound_to: hub1.id, terminal_title: "Compare the sidebar layouts" });

const sessions = [
  hub1,
  hub2,
  hub3,
  hub4,
  sessionOf("s-web-1", console_.id, web.id, "Fix the summary layout", "working", { bound_to: hub1.id, terminal_title: "Explain the differences between the three galleries" }),
  sessionOf("s-web-2", console_.id, web.id, "Update the dependencies", "idle"),
  sessionOf("s-web-3", console_.id, web.id, "Asks which region to use", "waiting_user"),
  sessionOf("s-web-4", console_.id, web.id, "Old layout draft", "interrupted"),
  sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "waiting_user", { bound_to: hub2.id }),
  sessionOf("s-api-2", console_.id, api.id, "Write the migration", "idle", { bound_to: hub3.id }),
  lead,
  sessionOf("s-web-lead-1", console_.id, web.id, "Chase the flaky test", "working", { bound_to: lead.id }),
  sessionOf("s-web-lead-2", console_.id, web.id, "Measure the first paint", "waiting_user", { bound_to: lead.id }),
  sessionOf("s-api-free", console_.id, api.id, "Plan the reindex", "idle"),
  sessionOf("s-api-free-1", console_.id, api.id, "Check the index sizes", "interrupted", { bound_to: "s-api-free" }),
];

const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

export const consoleSessionsScenarios: Scenario[] = [
  {
    id: "console-sessions-section",
    group: GROUP,
    category: "normal",
    title: "Console sessions, and project sessions bound, unbound and nested",
    description:
      "The section headings show Console sessions (4) and Projects (2), counting only the rows in each section. " +
      "Agent task titles appear before the original session names, with untitled sessions keeping their names. " +
      "Console sessions and each project's sibling sessions put Waiting for you first, then Awaiting instructions, " +
      "Working and Interrupted. Above the projects, Hub 3, the release-plan task under Hub, Hub 2 and Hub 4 show that order, each in its own " +
      "colour; Website shows the same status order. A bound project " +
      "session carries that colour as a binding badge in its sidebar row; an unbound one carries none. The sessions " +
      "bound to a project session are listed under it, a level below: the lead session keeps the binding badge of " +
      "the console session it reports to, and the sessions under either owner carry none. The pointer rests " +
      "on the status glyph of \"Add idempotency keys\", whose tooltip reads Waiting for you; every row's glyph " +
      "has one (Working, Waiting for you, Awaiting instructions, Interrupted), next to the agent mark's own.",
    state,
    steps: [(ui) => ui.hoverRowStatus(ui.session("Add idempotency keys"))],
  },
  {
    id: "console-session-colour-submenu",
    group: GROUP,
    category: "normal",
    title: "Choose a console session's colour",
    description:
      "The Colour submenu opens beside Hub 2's action menu, a grid of sixteen swatches in two rows, one per colour family. Hub 2's current colour carries the check mark and a ring in its own colour; " +
      "no colour name is written out.",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Hub 2" })),
      (ui) => ui.press(ui.t("sidebar.session.colour")),
    ],
  },
  {
    id: "session-rename-with-terminal-title",
    group: GROUP,
    category: "normal",
    title: "Rename a session with an agent task title",
    description: "The sidebar keeps the agent task beside the session name. Rename edits only the original session name.",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: ui.t("session.displayTitle", { title: "Explain the differences between the three galleries", name: "Fix the summary layout" }) })),
      (ui) => ui.press(ui.t("sidebar.session.rename")),
    ],
  },
];
