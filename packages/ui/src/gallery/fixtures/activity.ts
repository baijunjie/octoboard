import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Sidebar sessions";

const console_ = consoleOf("c-1", "Main");
const website = projectOf("p-web", console_.id, "Website");
const api = projectOf("p-api", console_.id, "Search API");
const billing = projectOf("p-billing", console_.id, "Billing");
const docs = projectOf("p-docs", console_.id, "Docs");
const archived = projectOf("p-archived", console_.id, "Archived only", { pinned: true });
const empty = projectOf("p-empty", console_.id, "No sessions yet", { pinned: true });

// One project per activity a collapsed row can show. Website has sessions in all three states and
// shows the most pressing; interrupted-only Docs stays undimmed without an activity marker.
// Pinned projects with no unarchived sessions still follow every unarchived project.
const sessions = [
  sessionOf("s-web-1", console_.id, website.id, "Fix the summary layout", "working"),
  sessionOf("s-web-2", console_.id, website.id, "Update the dependencies", "idle"),
  sessionOf("s-web-3", console_.id, website.id, "Asks which region to use", "waiting_user"),
  sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "working"),
  sessionOf("s-api-2", console_.id, api.id, "Plan the reindex", "interrupted"),
  sessionOf("s-bill-1", console_.id, billing.id, "Reconcile March", "idle"),
  sessionOf("s-docs-1", console_.id, docs.id, "Old draft", "interrupted"),
  sessionOf("s-archived-1", console_.id, archived.id, "Completed draft", "archived"),
];

export const activityScenarios: Scenario[] = [
  {
    id: "collapsed-project-activity",
    group: GROUP,
    category: "normal",
    title: "Collapsed project rows, by what is going on beneath them",
    description: "Each collapsed row's marker shows the most pressing activity beneath it: a waving hand for Website (waiting beats working), a pinging dot for Search API, a speech bubble for Billing and none for Docs. Hover a marker to read its tooltip. Docs, whose only session is interrupted, stays undimmed. The pinned Archived only and No sessions yet projects sort after the active ones, dimmed together. Expand from the Projects heading: only the four active projects open, and those two stay folded.",
    state: snapshotState({ consoles: [console_], projects: [archived, empty, website, api, billing, docs], sessions }),
    steps: [
      (ui) => ui.press(ui.t("sidebar.collapseAll")),
      (ui) => ui.wait(500),
    ],
  },
];
