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
const mobile = projectOf("p-mobile", console_.id, "Mobile app");
// A second console whose only session is idle with a watcher still running: work under way, not just a
// session waiting at its prompt.
const staging = consoleOf("c-staging", "Staging");
const preview = projectOf("p-preview", staging.id, "Preview site");
const build = consoleOf("c-build", "Build");
const release = projectOf("p-release", build.id, "Release");
const watching = { background_tasks: [{ id: "task-watch", type: "shell", description: "pnpm dev" }] };

// One project per activity a collapsed row can show. Website has sessions in all three states and
// shows the most pressing; Mobile app has one session, idle with a watcher still running, which counts as
// work; interrupted-only Docs stays undimmed without an activity marker.
// Pinned projects with no unarchived sessions still follow every unarchived project.
const sessions = [
  sessionOf("s-web-1", console_.id, website.id, "Fix the summary layout", "working"),
  sessionOf("s-web-2", console_.id, website.id, "Update the dependencies", "idle"),
  sessionOf("s-web-3", console_.id, website.id, "Asks which region to use", "waiting_user"),
  sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "working"),
  sessionOf("s-api-2", console_.id, api.id, "Plan the reindex", "interrupted"),
  sessionOf("s-mobile-1", console_.id, mobile.id, "Serve the preview", "idle", watching),
  sessionOf("s-bill-1", console_.id, billing.id, "Reconcile March", "idle"),
  sessionOf("s-docs-1", console_.id, docs.id, "Old draft", "interrupted"),
  sessionOf("s-archived-1", console_.id, archived.id, "Completed draft", "archived"),
  sessionOf("s-preview-1", staging.id, preview.id, "Serve the preview", "idle", watching),
  sessionOf("s-release-1", build.id, release.id, "Prepare the release", "working"),
];

const state = snapshotState({ consoles: [console_, staging, build], projects: [archived, empty, website, api, mobile, billing, docs, preview, release], sessions });

export const activityScenarios: Scenario[] = [
  {
    id: "project-default-folds",
    group: "Projects",
    category: "normal",
    title: "Projects open only when they have unarchived sessions",
    description: "Website, Billing, Mobile app and Search API open with their sessions visible, ordered by Waiting for you, Awaiting instructions, Working in background and Working. Docs follows and also opens because its interrupted session is unarchived. The pinned Archived only and No sessions yet projects start folded and follow the expanded projects. Toggle a project to change its fold state.",
    state,
  },
  {
    id: "collapsed-project-activity",
    group: GROUP,
    category: "normal",
    title: "Collapsed project rows, by what is going on beneath them",
    description: "Each collapsed row's marker shows the most pressing activity beneath it: a waving hand for Website (waiting beats working), a pinging accent dot for Mobile app and Search API, a speech bubble for Billing and none for Docs. Project order follows session status: Website, Billing, Mobile app, Search API, then Docs. Mobile app's only session is idle with a dev server still running, so its background work sorts after Billing's Awaiting instructions and before Search API's Working, even though Mobile app and Search API share the working marker. In the rail, Staging (background work only) and Build (a working session) share a green rim around the theme background (light or dark) with a centre and outward ripple in the same accent colour as the working session dot; Main keeps its waiting hand. Hover a marker to read its tooltip. Docs, whose only session is interrupted, stays undimmed. The pinned Archived only and No sessions yet projects sort after the active ones, dimmed together. Expand from the Projects heading: only the five active projects open, and those two stay folded.",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.collapseAll")),
      (ui) => ui.wait(500),
    ],
  },
];
