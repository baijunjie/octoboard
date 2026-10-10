import type { Agent } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Archive view";

export const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const AGENTS: Agent[] = ["claude", "codex", "grok"];

const archived = Array.from({ length: 70 }, (_, i) =>
  sessionOf(`s-a${i}`, console_.id, project.id, i % 7 === 3 ? `Audit the retry path ${i}` : i % 11 === 5 ? `A very long archived session title that does not fit one row ${i}` : `Archived session ${i}`, "archived", {
    agent: AGENTS[i % 3],
    ended_at: minutesAgo(20 + i * 400),
  }),
);
const consoleSessions = Array.from({ length: 8 }, (_, i) =>
  sessionOf(`s-c${i}`, console_.id, undefined, i === 0 ? "Hub" : `Hub ${i + 1}`, "archived", { ended_at: minutesAgo(60 * (i + 1)) }),
);

const state = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [sessionOf("s-live", console_.id, project.id, "Live session", "idle"), ...archived, ...consoleSessions],
});

// A live console session with some of its own bound sessions since archived, among other sessions
// not bound to it, kept out of `state` above so that mixing it into the shared project archive does
// not perturb the counts the scenarios above assert. Exported for `ArchiveView.test.tsx`, which
// renders the view on these directly.
export const boundConsoleSession = sessionOf("s-bound-console", console_.id, undefined, "Hub", "idle", { colour: "teal" });
export const boundArchived = Array.from({ length: 3 }, (_, i) =>
  sessionOf(`s-bound${i}`, console_.id, project.id, `Bound archived session ${i}`, "archived", {
    bound_to: boundConsoleSession.id,
    ended_at: minutesAgo(5 + i * 5),
  }),
);
// A second owner, an archived session bound to it instead of `boundConsoleSession`, and an
// archived session with no owner at all — so `ArchiveView.test.tsx` can tell a real filter from
// one that happens to pass everything through.
export const otherConsoleSession = sessionOf("s-other-console", console_.id, undefined, "Hub 2", "idle", { colour: "jade" });
export const otherOwnerArchived = sessionOf("s-other-bound", console_.id, project.id, "Archived session bound elsewhere", "archived", {
  bound_to: otherConsoleSession.id,
  ended_at: minutesAgo(5),
});
export const unboundArchived = sessionOf("s-unbound-archived", console_.id, project.id, "Unbound archived session", "archived", {
  ended_at: minutesAgo(5),
});

// An archived lead session bound to `boundConsoleSession` with two archived sessions of its own:
// a console session's archive reaches them, and lists them under their lead session. Exported for
// `ArchiveView.test.tsx`, as the sessions above are.
export const archivedLead = sessionOf("s-lead-archived", console_.id, project.id, "Archived lead session", "archived", {
  bound_to: boundConsoleSession.id,
  ended_at: minutesAgo(8),
});
export const underArchivedLead = Array.from({ length: 2 }, (_, i) =>
  sessionOf(`s-under-lead${i}`, console_.id, project.id, `Session ${i} of the archived lead session`, "archived", {
    bound_to: archivedLead.id,
    ended_at: minutesAgo(2 + i * 3),
  }),
);

export const archiveScenarios: Scenario[] = [
  {
    id: "archive-many",
    group: GROUP,
    title: "A project's archive, many entries",
    description: "Seventy archived sessions, listed thirty at a time as the end scrolls into view. The empty view is not reachable: the menu only offers View all when something is archived.",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: project.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll", { count: archived.length })),
    ],
  },
  {
    id: "archive-console-sessions",
    group: GROUP,
    title: "A console's archived console sessions",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.consoleSessions.actions", { name: console_.name })),
      (ui) => ui.press(ui.t("sidebar.archive.consoleSessions")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll", { count: consoleSessions.length })),
    ],
  },
  {
    id: "archive-bound-sessions",
    group: GROUP,
    title: "A console session's archived bound sessions",
    description: "Reached from the console session's focus mode; the sessions bound elsewhere and the unbound one are not listed.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${boundConsoleSession.id}` },
    state: snapshotState({
      consoles: [console_],
      projects: [project],
      sessions: [boundConsoleSession, otherConsoleSession, otherOwnerArchived, unboundArchived, ...boundArchived],
    }),
    steps: [(ui) => ui.press(ui.t("sidebar.archive.viewAll", { count: boundArchived.length }))],
  },
  {
    id: "archive-bound-sessions-with-a-lead-session",
    group: GROUP,
    title: "A console session's archive with an archived lead session",
    description: "The sessions archived under the lead session are inset below it, in the one list.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${boundConsoleSession.id}` },
    state: snapshotState({
      consoles: [console_],
      projects: [project],
      sessions: [boundConsoleSession, ...boundArchived, archivedLead, ...underArchivedLead],
    }),
    steps: [(ui) => ui.press(ui.t("sidebar.archive.viewAll", { count: boundArchived.length + 1 + underArchivedLead.length }))],
  },
];
