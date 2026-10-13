import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Sidebar sessions";

const console_ = consoleOf("c-1", "Main");
const website = projectOf("p-web", console_.id, "Website");
const api = projectOf("p-api", console_.id, "Search API");
const billing = projectOf("p-billing", console_.id, "Billing");
const docs = projectOf("p-docs", console_.id, "Docs");

// One project per activity a collapsed row can show. Website has sessions in all three states and
// shows the most pressing; Docs has nothing live, so shows no marker.
const sessions = [
  sessionOf("s-web-1", console_.id, website.id, "Fix the summary layout", "working"),
  sessionOf("s-web-2", console_.id, website.id, "Update the dependencies", "idle"),
  sessionOf("s-web-3", console_.id, website.id, "Asks which region to use", "waiting_user"),
  sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "working"),
  sessionOf("s-api-2", console_.id, api.id, "Plan the reindex", "interrupted"),
  sessionOf("s-bill-1", console_.id, billing.id, "Reconcile March", "idle"),
  sessionOf("s-docs-1", console_.id, docs.id, "Old draft", "interrupted"),
];

export const activityScenarios: Scenario[] = [
  {
    id: "collapsed-project-activity",
    group: GROUP,
    title: "Collapsed project rows, by what is going on beneath them",
    description:
      "Each collapsed row's marker stands for the most pressing activity among its sessions: a waving hand for " +
      "Website (waiting beats working), a pinging dot for Search API, a speech bubble for Billing, and none for " +
      "Docs. The pointer rests on Website's marker, whose tooltip reads A session is waiting for you; rest it " +
      "on the others to read A session is working and A session is running.",
    state: snapshotState({ consoles: [console_], projects: [website, api, billing, docs], sessions }),
    steps: [
      (ui) => ui.press(ui.t("sidebar.collapseAll")),
      (ui) => ui.wait(500),
      (ui) => ui.hoverActivityMarker(ui.t("sidebar.activity.waiting", { name: website.name })),
    ],
  },
];
