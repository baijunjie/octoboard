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
    ...(i === 0 ? { terminal_title: "Explain the differences between the three galleries" } : i === 1 ? { terminal_title: "Review a very long archived agent task title and preserve its original session name" } : {}),
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

const boundSessions = [
  boundConsoleSession,
  otherConsoleSession,
  otherOwnerArchived,
  unboundArchived,
  ...boundArchived,
  archivedLead,
  ...underArchivedLead,
];

// The same sessions, the second of those under the archived lead session now a Codex session on an
// account that has since been removed and recorded no directory: its agent icon falls back to the
// agent's own name.
const deleteState = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: boundSessions.map((session) =>
    session.id === underArchivedLead[1].id ? { ...session, agent: "codex" as const, account_id: "a-removed" } : session,
  ),
});

// A second project keeps the bound archive separate from the seventy-entry project archive.
const boundProject = projectOf("p-bound", console_.id, "Bound archive");
const archiveState = {
  ...state,
  projects: new Map([...state.projects!, [boundProject.id, boundProject]]),
  sessions: new Map([
    ...state.sessions!,
    ...Array.from(deleteState.sessions!, ([id, session]) => [id, { ...session, project_id: session.project_id === undefined ? undefined : boundProject.id }] as const),
  ]),
};

export const archiveScenarios: Scenario[] = [
  {
    id: "archive-many",
    group: GROUP,
    title: "Project archive, many entries",
    state: archiveState,
    description: "Seventy entries are loaded thirty at a time. The newest entries retain English agent task titles beside their original names, including a long title; other entries keep only their session names.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: project.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
    ],
  },
  {
    id: "archive-console-sessions",
    group: GROUP,
    title: "Archived console sessions",
    description: "Console sessions (2) counts the live sidebar rows; the eight archived console sessions appear in the archive view.",
    state: archiveState,
    steps: [
      (ui) => ui.press(ui.t("sidebar.consoleSessions.actions", { name: console_.name })),
      (ui) => ui.press(ui.t("sidebar.archive.consoleSessions")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
    ],
  },
  {
    id: "archive-bound-sessions",
    group: GROUP,
    title: "Bound archive with nested lead sessions",
    state: archiveState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${boundConsoleSession.id}` },
    steps: [
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
    ],
  },
  {
    id: "archive-delete-bound-sessions",
    group: GROUP,
    title: "Delete an archived lead session",
    state: archiveState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${boundConsoleSession.id}` },
    description: "The confirmation includes both children; hover their agent and status marks for account and archive details.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
      (ui) => ui.press(ui.t("archive.delete", { title: archivedLead.title })),
    ],
  },
  {
    id: "archive-emptied",
    group: GROUP,
    title: "Archive emptied while open",
    state: archiveState,
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: project.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
      (ui) => ui.press(ui.t("archive.deleteAll")),
      (ui) => ui.confirm(),
      (ui) => ui.wait(400),
    ],
  },
];
