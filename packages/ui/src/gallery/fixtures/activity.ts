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
// These consoles expose each avatar status glyph and the priority between neighboring statuses.
const staging = consoleOf("c-staging", "Staging");
const preview = projectOf("p-preview", staging.id, "Preview site");
const build = consoleOf("c-build", "Build");
const release = projectOf("p-release", build.id, "Release");
const ready = consoleOf("c-ready", "Ready");
const starting = consoleOf("c-starting", "Starting");
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
  sessionOf("s-preview-2", staging.id, preview.id, "Update the preview", "working"),
  sessionOf("s-release-1", build.id, release.id, "Prepare the release", "working"),
  sessionOf("s-release-2", build.id, release.id, "Launch the release check", "starting"),
  sessionOf("s-ready-1", ready.id, undefined, "Await the next instruction", "idle"),
  sessionOf("s-ready-2", ready.id, undefined, "Compile the bundle", "working"),
  sessionOf("s-starting-1", starting.id, undefined, "Launch the agent", "starting"),
];

const state = snapshotState({ consoles: [console_, ready, staging, build, starting], projects: [archived, empty, website, api, mobile, billing, docs, preview, release], sessions });

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
    description: "Each collapsed row's marker shows the most pressing activity beneath it: a waving hand for Website (waiting beats working), a pinging accent dot for Mobile app and Search API, a speech bubble for Billing and none for Docs. Project order follows session status: Website, Billing, Mobile app, Search API, then Docs. Mobile app's only session is idle with a dev server still running, so its background work sorts after Billing's Awaiting instructions and before Search API's Working, even though Mobile app and Search API share the working marker. The rail keeps the original inverted waiting hand on yellow and idle speech bubble on green, with 14px circular badges separated from the avatars by the original cutouts. Main shows the waving hand and Ready the speech bubble despite a working session. Staging shows a hollow center on a green background-work badge despite a working session; Build shows a filled center on a blue working badge despite a starting session. Both work indicators breathe gently over 2.4 seconds without expanding and stay still with reduced motion. Avatar priority is Waiting for you, Awaiting instructions, Working in background, then Working. Starting has only a launching session and no avatar badge; starting sessions keep their usual position alongside idle sessions in the sidebar. Hover a marker to read its tooltip. Docs, whose only session is interrupted, stays undimmed. The pinned Archived only and No sessions yet projects sort after the active ones, dimmed together. Expand from the Projects heading: only the five active projects open, and those two stay folded.",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.collapseAll")),
      (ui) => ui.wait(500),
    ],
  },
];
