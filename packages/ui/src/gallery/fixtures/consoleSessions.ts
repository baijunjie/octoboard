import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Sidebar sessions";

const console_ = consoleOf("c-1", "Main");
const web = projectOf("p-web", console_.id, "Website");
const api = projectOf("p-api", console_.id, "Search API");
// Idle only, so it ranks below the two projects that wait for the user, though it comes first by name.
const billing = projectOf("p-billing", console_.id, "Billing");
// Nothing but an archived session, which dims it and leaves it last whatever its name or the ordering.
const admin = projectOf("p-admin", console_.id, "Admin console");

// Five console sessions, each its own colour, including an older background session ahead of newer work.
const hub1 = sessionOf("s-hub-1", console_.id, undefined, "Hub", "idle", { colour: "yellow", terminal_title: "Review the release plan" });
const hub2 = sessionOf("s-hub-2", console_.id, undefined, "Hub 2", "working", { colour: "lime", started_at: minutesAgo(5) });
const hub3 = sessionOf("s-hub-3", console_.id, undefined, "Hub 3", "waiting_user", { colour: "teal", started_at: minutesAgo(20) });
const hub5 = sessionOf("s-hub-5", console_.id, undefined, "Preview server", "idle", { colour: "pink", started_at: minutesAgo(180), background_tasks: [{ id: "task-console-preview", type: "shell", description: "pnpm dev" }] });
const hub4 = sessionOf("s-hub-4", console_.id, undefined, "Hub 4", "interrupted", { colour: "violet", started_at: minutesAgo(60) });

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
  sessionOf("s-web-3", console_.id, web.id, "Asks which region to use", "waiting_user", { started_at: minutesAgo(40) }),
  // Pinned, so it stays on top whatever its status.
  sessionOf("s-web-4", console_.id, web.id, "Old layout draft", "interrupted", { pinned: true, started_at: minutesAgo(120) }),
  sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "waiting_user", { bound_to: hub2.id }),
  sessionOf("s-api-2", console_.id, api.id, "Write the migration", "idle", { bound_to: hub3.id, started_at: minutesAgo(30) }),
  sessionOf("s-billing-1", console_.id, billing.id, "Reconcile the invoices", "idle"),
  sessionOf("s-admin-1", console_.id, admin.id, "Port the CSV reader", "archived"),
  lead,
  sessionOf("s-web-lead-1", console_.id, web.id, "Chase the flaky test", "working", { bound_to: lead.id, started_at: minutesAgo(5) }),
  sessionOf("s-web-lead-watch", console_.id, web.id, "Watch the gallery build", "idle", { bound_to: lead.id, started_at: minutesAgo(180), background_tasks: [{ id: "task-build", type: "shell", description: "pnpm build --watch" }] }),
  sessionOf("s-web-lead-2", console_.id, web.id, "Measure the first paint", "waiting_user", { bound_to: lead.id }),
  sessionOf("s-api-free", console_.id, api.id, "Plan the reindex", "idle", { started_at: minutesAgo(10) }),
  sessionOf("s-api-free-1", console_.id, api.id, "Check the index sizes", "interrupted", { bound_to: "s-api-free" }),
];

const state = snapshotState({ consoles: [console_], projects: [web, api, billing, admin], sessions });

// Ordering by status switched off: the rows follow when they started, whatever their status.
const offState = { ...state, settings: { ...state.settings!, sidebar_order_by_status: false } };

export const consoleSessionsScenarios: Scenario[] = [
  {
    id: "console-sessions-section",
    group: GROUP,
    category: "normal",
    title: "Console sessions, and project sessions bound, unbound and nested",
    description: "The headings show Console sessions (5) and Projects (4), each counting only its own rows. Agent task titles come before the original session names. Hub 3, the release-plan task under Hub, Preview server, Hub 2 and Hub 4 show Waiting for you, Awaiting instructions, Working in background, Working and Interrupted in that order. The older Preview server stays ahead of the newer Hub 2. Website follows the same order: Update the dependencies, idle with a watcher, precedes the newer Explain the differences. Its pinging hollow green ring differs from the filled accent Working dot, and its hovered glyph says Working in background. Bound sessions carry their owner's colour; unbound ones carry none. Nested sessions stay under their lead without a badge, with Measure the first paint first, then the older Watch the gallery build ahead of the newer Chase the flaky test. The pinned, interrupted Old layout draft stays on top of Website. Projects are ranked by their most pressing session too: Search API and Website, both with a session waiting, come before the idle-only Billing, though Billing comes first by name, and the dimmed Admin console, with only an archived session, ends the list.",
    state,
    steps: [(ui) => ui.hoverRowStatus(ui.session("Update the dependencies"))],
  },
  {
    id: "console-sessions-order-by-status-off",
    group: GROUP,
    category: "normal",
    title: "Console sessions, ordering by status switched off",
    description: "The same sessions as Console sessions, and project sessions bound, unbound and nested, now in the order they started, newest first, with nothing ranked by status: Hub 2 (working) is first among the console sessions, then Hub 3 (waiting), Hub 4 (interrupted), Hub and the older Preview server. Projects are by name, Billing, Search API, Website, with the dimmed Admin console still last. In Search API, Plan the reindex and Write the migration come before the older waiting Add idempotency keys; in Website the pinned Old layout draft stays on top, then Explain the differences, Asks which region to use, the lead and its children, and the oldest Update the dependencies, and in the lead Chase the flaky test comes before Measure the first paint.",
    state: offState,
  },
  {
    id: "console-session-colour-submenu",
    group: GROUP,
    category: "normal",
    title: "Choose a console session's colour",
    description:
      "The Colour submenu opens beside Hub 2's action menu, a grid of sixteen swatches in two rows, one per colour family. Hub 2's current colour carries the check mark and a ring in its own colour; " +
      "no colour name is written out. Copy session ID stays directly after Rename in the parent menu, with the ID on a second line beneath its label; clicking either line copies the ID.",
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
