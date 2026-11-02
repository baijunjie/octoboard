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
const watching = { background_tasks: [{ id: "task-watch", type: "shell", description: "pnpm dev" }] };

// Main's projects, one per status a collapsed row can mark: Website has sessions in several states and shows
// the most pressing; Mobile app has one session, idle with a watcher still running, which counts as
// background work; Search API is working; interrupted-only Docs is marked with the grey pause. Each of the
// others is topped by its own status over a working session, or alone where it is itself the status.
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
];

const state = snapshotState({ consoles: [console_], projects: [archived, empty, website, api, mobile, billing, docs], sessions });

// `state` with every collapsed-project status and a console for each possible rail summary input.
// Project rows retain their full status vocabulary; rail badges show only waiting, paused, done and idle.
// Consoles containing only other display statuses remain visible without a badge. Working sessions
// beside eligible ones demonstrate that the rail chooses its top status after filtering.
const analytics = projectOf("p-analytics", console_.id, "Analytics");
const infrastructure = projectOf("p-infrastructure", console_.id, "Infrastructure");
const designSystem = projectOf("p-design-system", console_.id, "Design system");
const pipeline = projectOf("p-pipeline", console_.id, "Data pipeline");
const onboarding = projectOf("p-onboarding", console_.id, "Onboarding");
const payments = projectOf("p-payments", console_.id, "Payments");
const staging = consoleOf("c-staging", "Staging");
const preview = projectOf("p-preview", staging.id, "Preview site");
const build = consoleOf("c-build", "Build");
const release = projectOf("p-release", build.id, "Release");
const ready = consoleOf("c-ready", "Ready");
const starting = consoleOf("c-starting", "Starting");
const switching = consoleOf("c-switching", "Switching");
const interrupted = consoleOf("c-interrupted", "Interrupted");
const finished = consoleOf("c-finished", "Finished");
const dormant = consoleOf("c-dormant", "Dormant");
const paused = consoleOf("c-paused", "Paused");
const delegating = consoleOf("c-delegating", "Delegating");
const markerSessions = [
  ...sessions,
  sessionOf("s-analytics-1", console_.id, analytics.id, "Watch the background agents", "dormant"),
  sessionOf("s-analytics-2", console_.id, analytics.id, "Compile the report", "working"),
  sessionOf("s-infra-1", console_.id, infrastructure.id, "Retry the failed build", "idle", { paused: true }),
  sessionOf("s-infra-2", console_.id, infrastructure.id, "Rotate the keys", "working"),
  sessionOf("s-design-1", console_.id, designSystem.id, "Summarize the release", "idle"),
  sessionOf("s-design-2", console_.id, designSystem.id, "Compile the notes", "working"),
  sessionOf("s-pipeline-1", console_.id, pipeline.id, "Coordinate the migration", "idle"),
  sessionOf("s-pipeline-2", console_.id, pipeline.id, "Port the schema", "working", { bound_to: "s-pipeline-1" }),
  sessionOf("s-onboarding-1", console_.id, onboarding.id, "Launch the agent", "starting", { prompt_on_launch: true }),
  sessionOf("s-payments-1", console_.id, payments.id, "Update the card form", "idle"),
  sessionOf("s-preview-1", staging.id, preview.id, "Serve the preview", "idle", watching),
  sessionOf("s-preview-2", staging.id, preview.id, "Update the preview", "working"),
  sessionOf("s-release-1", build.id, release.id, "Prepare the release", "working"),
  sessionOf("s-release-2", build.id, release.id, "Launch the release check", "starting", { prompt_on_launch: true }),
  sessionOf("s-ready-1", ready.id, undefined, "Await the next instruction", "idle", { has_conversation: false }),
  sessionOf("s-ready-2", ready.id, undefined, "Compile the bundle", "working"),
  sessionOf("s-starting-1", starting.id, undefined, "Launch the agent", "starting"),
  sessionOf("s-switching-1", switching.id, undefined, "Move to the other account", "idle"),
  sessionOf("s-interrupted-1", interrupted.id, undefined, "Old draft", "interrupted"),
  sessionOf("s-finished-1", finished.id, undefined, "Summarize the release", "idle"),
  sessionOf("s-finished-2", finished.id, undefined, "Compile the notes", "working"),
  sessionOf("s-dormant-1", dormant.id, undefined, "Watch the background agents", "dormant"),
  sessionOf("s-dormant-2", dormant.id, undefined, "Compile the report", "working"),
  sessionOf("s-paused-1", paused.id, undefined, "Retry the failed build", "idle", { paused: true }),
  sessionOf("s-paused-2", paused.id, undefined, "Compile the report", "working"),
  sessionOf("s-delegating-1", delegating.id, undefined, "Coordinate the migration", "idle"),
  sessionOf("s-delegating-2", delegating.id, undefined, "Port the schema", "working", { bound_to: "s-delegating-1" }),
];
const markerState = switchingAccount(
  snapshotState({
    consoles: [console_, dormant, paused, finished, ready, delegating, staging, build, starting, switching, interrupted],
    projects: [archived, empty, website, api, mobile, billing, docs, analytics, infrastructure, designSystem, pipeline, onboarding, payments, preview, release],
    sessions: markerSessions,
  }),
  ["s-payments-1", "s-switching-1"],
);

// One session per status the sidebar draws, each titled by its status and started in the order they sort in.
// Switching account and Starting sort where they will land: the first has had a conversation, so with Done;
// the second launches with a prompt, so with Working (older than it, to follow it).
const glyphConsole = consoleOf("c-glyphs", "Main");
const glyphProject = projectOf("p-glyphs", glyphConsole.id, "Status glyphs");
const glyphSessions = [
  sessionOf("s-glyph-waiting", glyphConsole.id, glyphProject.id, "Waiting for you", "waiting_user", { started_at: minutesAgo(10) }),
  sessionOf("s-glyph-dormant", glyphConsole.id, glyphProject.id, "Dormant", "dormant", { started_at: minutesAgo(15) }),
  sessionOf("s-glyph-paused", glyphConsole.id, glyphProject.id, "Paused", "idle", { started_at: minutesAgo(20), paused: true }),
  sessionOf("s-glyph-done", glyphConsole.id, glyphProject.id, "Done", "idle", { started_at: minutesAgo(25) }),
  sessionOf("s-glyph-switching", glyphConsole.id, glyphProject.id, "Switching account", "idle", { started_at: minutesAgo(30) }),
  sessionOf("s-glyph-idle", glyphConsole.id, glyphProject.id, "Awaiting instructions", "idle", { started_at: minutesAgo(40), has_conversation: false }),
  sessionOf("s-glyph-delegated", glyphConsole.id, glyphProject.id, "Child sessions working", "idle", { started_at: minutesAgo(50) }),
  sessionOf("s-glyph-child", glyphConsole.id, glyphProject.id, "Watch the first paint build", "idle", { started_at: minutesAgo(5), bound_to: "s-glyph-delegated", background_tasks: watching.background_tasks }),
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
    description: "One session per status, titled by its status and in the order the sidebar ranks them: Waiting for you (orange waving hand), Dormant (orange zz, Zs rising from the bottom left to the top right and shrinking to a speck as they go, a still large and small Z with reduced motion on, between the hand and the green glyphs), Paused (green circled pause sign, ranked above Done), Done (green circled check mark), Awaiting instructions (green speech bubble), Child sessions working (pinging green ring with a dot in its centre; its only child, nested below, is idle with a watcher still running and shows Working in background, which is enough to read as delegated rather than Done), Working in background (pinging hollow green ring), Working (solid blue dot) and Interrupted (grey pause). Switching account and Starting, both a grey turning arc, sort with the status they will reach: Switching account, which has had a conversation, follows Done; Starting, launching with a prompt, follows Working. Hover a glyph for its label.",
    state: glyphState,
  },
  {
    id: "collapsed-project-activity",
    group: GROUP,
    category: "normal",
    title: "Collapsed project rows and rail avatars, by their top status",
    description: "Collapsed project rows show every status, while rail avatars summarize only Waiting for you, Paused, Done and Awaiting instructions. Rows carry the compact glyph, and each row's accessible name ends with its status: Website a waving hand (Waiting for you beats its Done and Working sessions), Analytics the orange zz (Dormant), Infrastructure a green circled pause sign (Paused), Design system a green circled check mark (Done), Billing a green speech bubble (Awaiting instructions), Data pipeline a pinging green ring around a dot (Child sessions working), Mobile app a pinging hollow green ring (Working in background: its only session is idle with a dev server still running), Search API a pinging blue dot (Working), Onboarding and Payments a grey turning arc (Starting; Switching account) and Docs a grey pause (Interrupted). The sidebar sorts Payments and Onboarding where they will land, with Done and Working. Hover a marker to read its tooltip. The rail shows only Main with the inverted waving hand on orange, Paused with a bare pause sign, Finished with a check mark and Ready with a speech bubble on green. These 14px badges have cutouts separating them from their avatars, and each eligible status wins over the working session in the same console. Dormant, Delegating, Staging (background work), Build (Working and Starting), Starting, Switching and Interrupted remain in the rail with no badge. Hover Main, Paused, Finished or Ready: the tooltip puts the console name before a soft status badge containing only localized status text, orange for Waiting for you and green for Paused, Done and Awaiting instructions. Each avatar still names its status once for assistive technology, with no repeated tooltip description. Avatars without an eligible summary keep a name-only tooltip. Dormant raises no hand, so the rail’s waiting count leaves it out. The pinned Archived only and No sessions yet projects sort after the active ones, dimmed together. Expand from the Projects heading: only the eleven active projects open, and those two stay folded.",
    state: markerState,
    steps: [
      (ui) => ui.press(ui.t("sidebar.collapseAll")),
      (ui) => ui.wait(500),
    ],
  },
];
