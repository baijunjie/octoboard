import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState, switchingAccount } from "../../fixtures/builders";

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
const finished = consoleOf("c-finished", "Finished");
const delegating = consoleOf("c-delegating", "Delegating");
const watching = { background_tasks: [{ id: "task-watch", type: "shell", description: "pnpm dev" }] };

// One project per activity a collapsed row can show. Website has sessions in all three states and
// shows the most pressing; Mobile app has one session, idle with a watcher still running, which counts as
// background work; Search API is working; interrupted-only Docs stays undimmed without an activity marker.
// Pinned projects with no unarchived sessions still follow every unarchived project.
const sessions = [
  sessionOf("s-web-1", console_.id, website.id, "Fix the summary layout", "working"),
  sessionOf("s-web-2", console_.id, website.id, "Update the dependencies", "idle"),
  sessionOf("s-web-3", console_.id, website.id, "Asks which region to use", "waiting_user"),
  sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "working"),
  sessionOf("s-api-2", console_.id, api.id, "Plan the reindex", "interrupted"),
  sessionOf("s-mobile-1", console_.id, mobile.id, "Serve the preview", "idle", watching),
  sessionOf("s-bill-1", console_.id, billing.id, "Reconcile March", "idle", { has_conversation: false }),
  sessionOf("s-docs-1", console_.id, docs.id, "Old draft", "interrupted"),
  sessionOf("s-archived-1", console_.id, archived.id, "Completed draft", "archived"),
  sessionOf("s-preview-1", staging.id, preview.id, "Serve the preview", "idle", watching),
  sessionOf("s-preview-2", staging.id, preview.id, "Update the preview", "working"),
  sessionOf("s-release-1", build.id, release.id, "Prepare the release", "working"),
  sessionOf("s-release-2", build.id, release.id, "Launch the release check", "starting"),
  sessionOf("s-ready-1", ready.id, undefined, "Await the next instruction", "idle", { has_conversation: false }),
  sessionOf("s-ready-2", ready.id, undefined, "Compile the bundle", "working"),
  sessionOf("s-starting-1", starting.id, undefined, "Launch the agent", "starting"),
  sessionOf("s-finished-1", finished.id, undefined, "Summarize the release", "idle"),
  sessionOf("s-finished-2", finished.id, undefined, "Compile the notes", "working"),
  sessionOf("s-delegating-1", delegating.id, undefined, "Coordinate the migration", "idle"),
  sessionOf("s-delegating-2", delegating.id, undefined, "Port the schema", "working", { bound_to: "s-delegating-1" }),
];

const state = snapshotState({ consoles: [console_, ready, staging, build, starting, finished, delegating], projects: [archived, empty, website, api, mobile, billing, docs, preview, release], sessions });

// One session per status the sidebar draws, each titled by its status and started in the order they sort in.
// Switching account and Starting sort where they will land: the first has had a conversation, so with Done;
// the second launches with a prompt, so with Working (older than it, to follow it).
const glyphConsole = consoleOf("c-glyphs", "Main");
const glyphProject = projectOf("p-glyphs", glyphConsole.id, "Status glyphs");
const glyphSessions = [
  sessionOf("s-glyph-waiting", glyphConsole.id, glyphProject.id, "Waiting for you", "waiting_user", { started_at: minutesAgo(10) }),
  sessionOf("s-glyph-done", glyphConsole.id, glyphProject.id, "Done", "idle", { started_at: minutesAgo(20) }),
  sessionOf("s-glyph-switching", glyphConsole.id, glyphProject.id, "Switching account", "idle", { started_at: minutesAgo(30) }),
  sessionOf("s-glyph-idle", glyphConsole.id, glyphProject.id, "Awaiting instructions", "idle", { started_at: minutesAgo(40), has_conversation: false }),
  sessionOf("s-glyph-delegated", glyphConsole.id, glyphProject.id, "Child sessions working", "idle", { started_at: minutesAgo(50) }),
  sessionOf("s-glyph-child", glyphConsole.id, glyphProject.id, "Measure the first paint", "working", { started_at: minutesAgo(5), bound_to: "s-glyph-delegated" }),
  sessionOf("s-glyph-background", glyphConsole.id, glyphProject.id, "Working in background", "idle", { started_at: minutesAgo(60), background_tasks: watching.background_tasks }),
  sessionOf("s-glyph-working", glyphConsole.id, glyphProject.id, "Working", "working", { started_at: minutesAgo(70) }),
  sessionOf("s-glyph-starting", glyphConsole.id, glyphProject.id, "Starting", "starting", { started_at: minutesAgo(80), prompt_on_launch: true }),
  sessionOf("s-glyph-interrupted", glyphConsole.id, glyphProject.id, "Interrupted", "interrupted", { started_at: minutesAgo(90) }),
];
const glyphState = switchingAccount(
  snapshotState({ consoles: [glyphConsole], projects: [glyphProject], sessions: glyphSessions }),
  ["s-glyph-switching"],
);

export const activityScenarios: Scenario[] = [
  {
    id: "project-default-folds",
    group: "Projects",
    category: "normal",
    title: "Projects open only when they have unarchived sessions",
    description: "Website, Billing, Mobile app and Search API open with their sessions visible, ranked by their most pressing session: Website has one waiting for you (above its Done and Working ones), Billing one awaiting instructions, Mobile app one working in background and Search API one working. Docs follows and also opens because its interrupted session is unarchived. The pinned Archived only and No sessions yet projects start folded and follow the expanded projects. Toggle a project to change its fold state.",
    state,
  },
  {
    id: "session-status-glyphs",
    group: GROUP,
    category: "normal",
    title: "Every session status glyph, side by side",
    description: "One session per status, titled by its status and in the order the sidebar ranks them: Waiting for you (orange waving hand), Done (green check mark), Awaiting instructions (green speech bubble), Child sessions working (pinging green ring with a dot in its centre, the working session bound to it nested below), Working in background (pinging hollow green ring), Working (solid blue dot) and Interrupted (grey pause). Switching account and Starting, both a grey turning arc, sort with the status they will reach: Switching account, which has had a conversation, follows Done; Starting, launching with a prompt, follows Working. Hover a glyph for its label.",
    state: glyphState,
  },
  {
    id: "collapsed-project-activity",
    group: GROUP,
    category: "normal",
    title: "Collapsed project rows, by what is going on beneath them",
    description: "Each collapsed row's marker shows the most pressing activity beneath it: a waving hand for Website (waiting beats working), a pinging hollow green ring for Mobile app, a pinging accent dot for Search API, a speech bubble for Billing and none for Docs. Project order follows session status: Website, Billing, Mobile app, Search API, then Docs. Mobile app's only session is idle with a dev server still running, so its background work sorts after Billing's Awaiting instructions and before Search API's Working, and its marker is the green hollow ring where Search API's is the blue dot. The rail keeps the original inverted waiting hand on yellow and Awaiting instructions speech bubble on green, with 14px circular badges separated from the avatars by the original cutouts. Main shows the waving hand, Finished the check mark on green and Ready the speech bubble, each despite a working session; Delegating shows a ring around a dot on green despite its working child. Staging shows a hollow center on a green background-work badge despite a working session; Build shows a filled center on a blue working badge despite a starting session. Both work indicators breathe gently over 2.4 seconds without expanding and stay still with reduced motion. Avatar priority is Waiting for you, Done, Awaiting instructions, Child sessions working, Working in background, then Working. Starting has only a launching session and no avatar badge; a starting session sorts in the sidebar where it will land, here with Done. Hover a marker to read its tooltip. Docs, whose only session is interrupted, stays undimmed. The pinned Archived only and No sessions yet projects sort after the active ones, dimmed together. Expand from the Projects heading: only the five active projects open, and those two stay folded.",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.collapseAll")),
      (ui) => ui.wait(500),
    ],
  },
];
