import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Sidebar sessions";

const console_ = consoleOf("c-1", "Main");
const web = projectOf("p-web", console_.id, "Website");
const api = projectOf("p-api", console_.id, "Search API");

// Five console sessions, each its own colour, including an older background session ahead of newer work.
const hub1 = sessionOf("s-hub-1", console_.id, undefined, "Hub", "idle", { colour: "yellow", terminal_title: "Review the release plan" });
const hub2 = sessionOf("s-hub-2", console_.id, undefined, "Hub 2", "working", { colour: "lime", started_at: minutesAgo(5) });
const hub3 = sessionOf("s-hub-3", console_.id, undefined, "Hub 3", "waiting_user", { colour: "teal" });
const hub5 = sessionOf("s-hub-5", console_.id, undefined, "Preview server", "idle", { colour: "pink", started_at: minutesAgo(180), background_tasks: [{ id: "task-console-preview", type: "shell", description: "pnpm dev" }] });
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
  hub5,
  sessionOf("s-web-1", console_.id, web.id, "Fix the summary layout", "working", { bound_to: hub1.id, started_at: minutesAgo(5), terminal_title: "Explain the differences between the three galleries" }),
  // An older watcher sorts after idle sessions and before newer foreground work.
  sessionOf("s-web-2", console_.id, web.id, "Update the dependencies", "idle", { started_at: minutesAgo(180), background_tasks: [{ id: "task-watch", type: "shell", description: "pnpm test --watch" }] }),
  sessionOf("s-web-3", console_.id, web.id, "Asks which region to use", "waiting_user"),
  sessionOf("s-web-4", console_.id, web.id, "Old layout draft", "interrupted"),
  sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "waiting_user", { bound_to: hub2.id }),
  sessionOf("s-api-2", console_.id, api.id, "Write the migration", "idle", { bound_to: hub3.id }),
  lead,
  sessionOf("s-web-lead-1", console_.id, web.id, "Chase the flaky test", "working", { bound_to: lead.id, started_at: minutesAgo(5) }),
  sessionOf("s-web-lead-watch", console_.id, web.id, "Watch the gallery build", "idle", { bound_to: lead.id, started_at: minutesAgo(180), background_tasks: [{ id: "task-build", type: "shell", description: "pnpm build --watch" }] }),
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
    description: "The headings show Console sessions (5) and Projects (2), each counting only its own rows. Agent task titles come before the original session names. Hub 3, the release-plan task under Hub, Preview server, Hub 2 and Hub 4 show Waiting for you, Awaiting instructions, Working in background, Working and Interrupted in that order. The older Preview server stays ahead of the newer Hub 2. Website follows the same order: Update the dependencies, idle with a watcher, precedes the newer Explain the differences. Its pinging hollow green ring differs from the filled blue Working dot, and its hovered glyph says Working in background. Bound sessions carry their owner's colour; unbound ones carry none. Nested sessions stay under their lead without a badge, with Measure the first paint first, then the older Watch the gallery build ahead of the newer Chase the flaky test.",
    state,
    steps: [(ui) => ui.hoverRowStatus(ui.session("Update the dependencies"))],
  },
  {
    id: "console-session-colour-submenu",
    group: GROUP,
    category: "normal",
    title: "Choose a console session's colour",
    description:
      "The Colour submenu opens beside Hub 2's action menu, a grid of sixteen swatches in two rows, one per colour family. Hub 2's current colour carries the check mark and a ring in its own colour; " +
      "no colour name is written out. Copy session ID sits directly after Rename in the parent menu.",
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
