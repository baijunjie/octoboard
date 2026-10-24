import type { Agent } from "../../protocol";
import type { Scenario } from "../scenario";
import { branchItemOf, consoleOf, contextOf, minutesAgo, projectOf, sessionOf, snapshotState, worktreeItemOf } from "./builders";

const GROUP = "Archive view";

export const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const AGENTS: Agent[] = ["claude", "codex", "grok"];

const archived = Array.from({ length: 70 }, (_, i) =>
  sessionOf(`s-a${i}`, console_.id, project.id, i % 7 === 3 ? `Audit the retry path ${i}` : i % 11 === 5 ? `A very long archived session title that does not fit one row ${i}` : `Archived session ${i}`, "archived", {
    agent: AGENTS[i % 3],
    ...(i === 0 ? { terminal_title: "Explain the differences between the three galleries" } : i === 1 ? { terminal_title: "Review a very long archived agent task title and preserve its original session name" } : {}),
    started_at: minutesAgo(120 + i * 400),
    ended_at: minutesAgo(20 + i * 400),
    ...(i === 0 ? { context: contextOf(43, 1_000_000) } : i === 1 ? { context: { tokens: 0 } } : {}),
  }),
);
const consoleSessions = Array.from({ length: 8 }, (_, i) =>
  sessionOf(`s-c${i}`, console_.id, undefined, i === 0 ? "Hub" : `Hub ${i + 1}`, "archived", {
    ...(i === 0 ? { terminal_title: "Coordinate the search service release" } : i === 1 ? { terminal_title: "Review release readiness across projects" } : {}),
    started_at: minutesAgo(60 * (i + 2)),
    ended_at: minutesAgo(60 * (i + 1)),
    ...(i === 0 ? { context: { tokens: 192_345 } } : {}),
  }),
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
export const otherConsoleSession = sessionOf("s-other-console", console_.id, undefined, "Hub 2", "idle", { colour: "lime" });
export const otherOwnerArchived = sessionOf("s-other-bound", console_.id, project.id, "Archived session bound elsewhere", "archived", {
  bound_to: otherConsoleSession.id,
  context: { tokens: 846_209 },
  ended_at: minutesAgo(5),
});
export const unboundArchived = sessionOf("s-unbound-archived", console_.id, project.id, "Unbound archived session", "archived", {
  context: { tokens: 0 },
  ended_at: minutesAgo(5),
});

// An archived lead session bound to `boundConsoleSession` with two archived sessions of its own:
// a console session's archive reaches them, and lists them under their lead session. Exported for
// `ArchiveView.test.tsx`, as the sessions above are.
export const archivedLead = sessionOf("s-lead-archived", console_.id, project.id, "Release lead session", "archived", {
  bound_to: boundConsoleSession.id,
  context: contextOf(65),
  ended_at: minutesAgo(8),
});
export const underArchivedLead = Array.from({ length: 2 }, (_, i) =>
  sessionOf(`s-under-lead${i}`, console_.id, project.id, `Session ${i} of the archived lead session`, "archived", {
    bound_to: archivedLead.id,
    ended_at: minutesAgo(2 + i * 3),
  }),
);

const liveLead = sessionOf("s-live-lead", console_.id, project.id, "Active lead session", "idle", { bound_to: boundConsoleSession.id });
const underLiveLead = sessionOf("s-under-live-lead", console_.id, project.id, "Archived child of active lead", "archived", { bound_to: liveLead.id, ended_at: minutesAgo(1) });

const boundSessions = [
  liveLead,
  underLiveLead,
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
const boundProject = projectOf("p-bound", console_.id, "Bound archive for release coordination across all production services");
const preservedTaskTitles: Record<string, string> = {
  [otherOwnerArchived.id]: "Review deployment retry handling across production environments and regional release pipelines before the next search service rollout",
  [unboundArchived.id]: "Review regional search releases and retry policies, and verify archived task titles remain readable after account migration",
  [boundArchived[0].id]: "Fix search result pagination",
  [boundArchived[1].id]: "Audit retry handling",
  [boundArchived[2].id]: "Verify search index migration",
};
const archiveState = {
  ...state,
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ["claude" as const, "codex" as const], default_clone_dir: "/Users/dev/Projects", accounts: [
    { id: "a-long", agent: "claude" as const, name: "Release coordination across every production environment, regional deployment pipeline and supported customer account for the next scheduled release", config_dir: "/Users/dev/.claude-release" },
    { id: "a-release", agent: "claude" as const, name: "Production release coordination and regression review account for regional deployment pipelines and customer support, for the next scheduled regional release", config_dir: "/Users/dev/.claude-regression" },
  ] },
  projects: new Map([...state.projects!, [boundProject.id, boundProject]]),
  sessions: new Map([
    ...state.sessions!,
    ...Array.from(deleteState.sessions!, ([id, session]) => [id, { ...session, terminal_title: preservedTaskTitles[session.id] ?? session.terminal_title, project_id: session.project_id === undefined ? undefined : boundProject.id, account_id: session.id === otherOwnerArchived.id ? "a-long" : session.id === unboundArchived.id ? "a-release" : session.account_id, title: session.id === otherOwnerArchived.id ? "Archived session bound elsewhere with a long title for the archive menu, covering release coordination across all production environments and regional deployment pipelines" : session.id === unboundArchived.id ? "Coordinate the next regional deployment across all production pipelines and review customer support migration records before the scheduled release" : session.title }] as const),
  ]),
};

// The project menu's "View archive" submenu shows ten at most: older archived sessions, a few
// with a recorded context size, join the project so its preview has more rows than fit.
const longArchiveState = {
  ...archiveState,
  sessions: new Map([
    ...archiveState.sessions,
    ...Array.from({ length: 6 }, (_, i) => {
      const session = sessionOf(`s-older${i}`, console_.id, boundProject.id, `Older archived session ${i + 1}`, "archived", {
        agent: AGENTS[i % 3],
        ended_at: minutesAgo(200 + i * 30),
        ...(i === 0 ? { context: contextOf(91) } : i % 2 === 0 ? { context: { tokens: 12_345 + i * 98_000 } } : {}),
      });
      return [session.id, session] as const;
    }),
  ]),
};

// The console archive with an archived session bound to one of its archived console sessions, which
// "Delete all" takes along.
const deleteAllState = {
  ...archiveState,
  sessions: new Map([
    ...archiveState.sessions,
    [
      "s-under-hub",
      sessionOf("s-under-hub", console_.id, project.id, "Session under the archived hub", "archived", { bound_to: consoleSessions[0].id, ended_at: minutesAgo(70) }),
    ] as const,
  ]),
};

// The console's menu with nothing archived under it.
const emptyConsoleArchiveState = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [sessionOf("s-live", console_.id, project.id, "Live session", "idle")],
});

const deleteBoundSessions: Scenario = {
  id: "archive-delete-bound-sessions",
  group: GROUP,
  category: "normal",
  title: "Delete an archived lead session",
  state: archiveState,
  sessionRecords: Object.fromEntries([...archiveState.sessions.keys()].map((id) => [id, id === underArchivedLead[0].id ? "present" as const : "absent" as const])),
  preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${boundConsoleSession.id}` },
  description: "Native files exist for a child, so the confirmation keeps the agent-records Trash option, unchecked, with a short note. Above it, two plain options with closed Details: \"Also force-remove the worktrees and branches Git keeps (2)\" for an uncommitted worktree and an unmerged branch, and \"Also delete the locked worktrees (4)\", each unchecked. Under the options, one warning titled \"2 archived sessions under it are deleted with it\" holds, inside it, the list of both archived children, with no consequence line yet. The confirming button reads \"Delete\" until an option is ticked.",
  sessionBranches: [
    worktreeItemOf(boundProject, underArchivedLead[0], "feat/pagination", "/Users/dev/worktrees/search-api-pagination"),
    branchItemOf(boundProject, archivedLead, "release/retry-policy"),
    worktreeItemOf(boundProject, underArchivedLead[0], "feat/ranking", "/Users/dev/worktrees/search-api-ranking", "locked"),
    worktreeItemOf(boundProject, underArchivedLead[1], "feat/synonyms", "/Users/dev/worktrees/search-api-synonyms", "locked", true),
    worktreeItemOf(boundProject, underArchivedLead[1], null, "/Users/dev/worktrees/search-api-spike", "locked"),
    worktreeItemOf(boundProject, archivedLead, null, "/Users/dev/worktrees/search-api-profiling", "locked", true),
  ],
  steps: [
    (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
    (ui) => ui.press((name) => name === ui.t("archive.delete", { title: archivedLead.title })),
  ],
};

const deleteAll: Scenario = {
  id: "archive-delete-all",
  group: GROUP,
  category: "normal",
  title: "Delete all archived console sessions",
  description: "The confirmation counts the archived console sessions. Plain options come first: the force-remove option counting the worktree with uncommitted changes and the unmerged branch left by the sessions deleted (Details closed, unchecked), then the unchecked option to also move the agents' own session records to the Trash with its short note. Under them, one warning titled \"1 archived session under a console session in the view is deleted too\", with nothing else inside it yet. The confirming button reads \"Delete all\".",
  sessionBranches: [
    worktreeItemOf(project, deleteAllState.sessions.get("s-under-hub")!, "feat/hub-handoff", "/Users/dev/worktrees/search-api-hub-handoff"),
    branchItemOf(project, deleteAllState.sessions.get("s-under-hub")!, "fix/stale-cache"),
  ],
  state: deleteAllState,
  sessionRecords: { "s-under-hub": "present" },
  steps: [
    (ui) => ui.press(ui.t("sidebar.console.actions", { name: console_.name })),
    (ui) => ui.press(ui.t("sidebar.archive.consoleSessions")),
    (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
    (ui) => ui.press(ui.t("archive.deleteAll")),
  ],
};

export const archiveScenarios: Scenario[] = [
  {
    id: "archive-project-preview",
    group: GROUP,
    category: "normal",
    title: "Project archive menu, ten newest",
    state: longArchiveState,
    description: "The menu lists the ten newest of thirteen archived rows, with their last active task titles, then View all; the three oldest are left to the full archive. Each item ends in a localized relative archive time, including sessions with a recorded context size; its accessible name gives the full archive date and time. The archived child of the active lead is listed on its own, while the two children of the archived lead stay out. Long archived names fade at the menu edge and run as a marquee when their row is hovered.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: boundProject.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
    ],
  },
  {
    id: "archive-project-family",
    group: GROUP,
    category: "normal",
    title: "Project archive with live and archived owners",
    state: archiveState,
    description: "The full archive shows each last active task title beside the original session name, the archived lead's children nested under it and the archived child of the active lead as a standalone row. A compact-size badge follows the agent name, hidden for a zero or absent size, and led by a small ring of the share used where the agent reported a window (the archived lead session, selected, on the selected row's background); hover a badge for its Context window card with the meter at 65% in warning; only dates sit on the right, archived above created. Each row is a button. Hover or Tab to it to compare its background and focus ring with the Reopen and Delete fills. The archived lead is the selected session. Pressing another row closes the archive and shows that session's history. Delete an entry or a nested child to watch the rows slide into place, then close the archive and reopen it through the project menu to review both transitions.",
    steps: [
      // Selecting the archived lead from the menu, then opening the full archive, marks its row.
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: boundProject.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press((name) => name.startsWith(archivedLead.title)),
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: boundProject.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
    ],
  },
  {
    id: "archive-many",
    group: GROUP,
    category: "normal",
    title: "Project archive, many entries",
    state: archiveState,
    description: "Seventy entries are loaded thirty at a time; the newest keep their last active task titles (one of them long) and the first shows a compact-size badge (430K, led by a small accent ring as the agent reported a window; hover it for the card with the meter at 43%, accent) and the second, whose size is zero, no mark. Rows offer Reopen and Delete, without lock controls. Delete an entry: it fades out to the right by a quarter of its own width over 300ms while the remaining rows slide into place, and the reduced-motion preference keeps the change immediate.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: project.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
    ],
  },
  {
    id: "archive-console-menu",
    group: GROUP,
    category: "normal",
    title: "Console menu, archived console sessions",
    description: "The console's menu (the same one opens from its avatar in the rail) lists Add project, Console settings, View archive and Delete console below a separator, with no Export configuration. The submenu lists all eight archived console sessions (it shows ten at most), newest first, with their last active task titles and a localized relative archive time at the end of each row, then View all.",
    state: archiveState,
    steps: [
      (ui) => ui.press(ui.t("sidebar.console.actions", { name: console_.name })),
      (ui) => ui.press(ui.t("sidebar.archive.consoleSessions")),
    ],
  },
  {
    id: "archive-console-menu-empty",
    group: GROUP,
    category: "normal",
    title: "Console menu, no archived console sessions",
    description: "The View archive submenu holds only a disabled empty entry, with no View all.",
    state: emptyConsoleArchiveState,
    steps: [
      (ui) => ui.press(ui.t("sidebar.console.actions", { name: console_.name })),
      (ui) => ui.press(ui.t("sidebar.archive.consoleSessions")),
    ],
  },
  {
    id: "archive-console-sessions",
    group: GROUP,
    category: "normal",
    title: "Archived console sessions",
    description: "Console sessions (2) counts the live sidebar rows; the eight archived console sessions appear in the archive view, retaining their last active task titles beside their original names.",
    state: archiveState,
    steps: [
      (ui) => ui.press(ui.t("sidebar.console.actions", { name: console_.name })),
      (ui) => ui.press(ui.t("sidebar.archive.consoleSessions")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
    ],
  },
  {
    id: "archive-bound-sessions",
    group: GROUP,
    category: "normal",
    title: "Bound archive with nested lead sessions",
    description: "The full console-session archive retains the archived lead and its nested children. The archived child of the active project lead belongs to that project's archive.",
    state: archiveState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${boundConsoleSession.id}` },
    steps: [
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
    ],
  },
  deleteBoundSessions,
  {
    ...deleteBoundSessions,
    id: "archive-delete-bound-sessions-ticked",
    title: "Delete an archived lead session, every option ticked",
    description: "All three options are ticked and the two Details disclosures opened: the force-remove list (the uncommitted worktree and the unmerged branch), the locked worktrees in each wording, and the agent-records Trash option. The dialog scrolls, so look at its bottom: the one warning under the options, titled \"2 archived sessions under it are deleted with it\", holds the list of both archived children first and, under it inside the same Alert, a consequence line for each ticked option, among them that the agent's records of this session and of the sessions deleted with it are moved to the Trash. The confirming button reads \"Delete all\" because options are ticked. The opened lists sit on a light grey panel, and a worktree's branch (icon and name) moves to the next line as a whole rather than breaking at a slash.",
    steps: [
      ...deleteBoundSessions.steps!,
      (ui) => ui.pressEach(ui.t("branchCleanup.details")),
      (ui) => ui.press(ui.t("dialog.sessionBranches.force", { count: 2 })),
      (ui) => ui.press(ui.t("dialog.lockedWorktrees.remove", { count: 4 })),
      (ui) => ui.press(ui.t("dialog.deleteSession.trashRecords")),
    ],
  },
  {
    id: "archive-delete-bound-sessions-no-records",
    group: GROUP,
    category: "normal",
    title: "Delete used archived sessions without native files",
    state: archiveState,
    sessionRecords: Object.fromEntries([...archiveState.sessions.keys()].map((id) => [id, "absent" as const])),
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${boundConsoleSession.id}` },
    description: "The used archived lead still requires confirmation, with one warning titled \"2 archived sessions under it are deleted with it\" holding the list of its two archived children. No native files exist for any of them, so the Trash checkbox and hint are absent.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
      (ui) => ui.press((name) => name === ui.t("archive.delete", { title: archivedLead.title })),
    ],
  },
  {
    ...deleteBoundSessions,
    id: "archive-delete-session-no-bound-ticked",
    title: "Delete an archived session with none under it, options ticked",
    description: "Nothing is deleted along with the session, so the warning has no list: its title is \"Additional consequences\" and the consequence lines for the two ticked options (the force-remove option and the agent-records Trash option, Details closed) sit under it inside the same Alert.",
    sessionBranches: [
      worktreeItemOf(boundProject, underArchivedLead[0], "feat/pagination", "/Users/dev/worktrees/search-api-pagination"),
      branchItemOf(boundProject, underArchivedLead[0], "release/retry-policy"),
    ],
    steps: [
      (ui) => ui.press(ui.t("sidebar.archive.viewAll")),
      (ui) => ui.press((name) => name === ui.t("archive.delete", { title: underArchivedLead[0].title })),
      (ui) => ui.press(ui.t("dialog.sessionBranches.force", { count: 2 })),
      (ui) => ui.press(ui.t("dialog.deleteSession.trashRecords")),
    ],
  },
  deleteAll,
  {
    ...deleteAll,
    id: "archive-delete-all-trash",
    title: "Delete all archived console sessions, Trash option ticked",
    description: "Only the agents' records Trash option is ticked; the force-remove option stays unchecked. The confirming button still reads \"Delete all\", and the warning's title is now followed, inside the same Alert, by a consequence line saying the records go to the Trash.",
    steps: [
      ...deleteAll.steps!,
      (ui) => ui.press(ui.t("dialog.deleteArchived.trashRecords")),
    ],
  },
  {
    id: "archive-emptied",
    group: GROUP,
    category: "normal",
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
