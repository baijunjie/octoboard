import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Focus mode";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const other = projectOf("p-2", console_.id, "Website");
const third = projectOf("p-3", console_.id, "Docs");

// The switch strip lists console sessions by when they started, in the order they are named here.
const hub1 = sessionOf("s-console", console_.id, undefined, "Hub", "idle", { colour: "teal", started_at: minutesAgo(400) });
const hub2 = sessionOf("s-console-2", console_.id, undefined, "Hub 2", "working", { colour: "rose", started_at: minutesAgo(300) });
const hub3 = sessionOf("s-console-3", console_.id, undefined, "Hub 3", "idle", { colour: "jade", started_at: minutesAgo(200) });
// A fourth console session with nothing going on, even with an idle and an archived session bound to it.
const hub4 = sessionOf("s-console-4", console_.id, undefined, "Release notes", "idle", { colour: "violet", started_at: minutesAgo(100) });

// A named account, so a card's agent line reads "Claude Code (Work)".
const settings = {
  auto_sync_repositories: false,
  default_clone_dir: "/Users/dev/Projects",
  accounts: [{ id: "a-work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" }],
};

// The sessions of `project` are bound to one of three console sessions or to none, so the project's
// focus mode has something to leave out; `hub1`'s focus mode has two projects of its own, a bound
// session of another owner and an unbound one to leave out, and archived sessions of its own. `lead`
// is a session of `project` bound to `hub1` that has two sessions of its own, which the project's
// focus mode summarizes by default or shows with hub-colored dots when included; `hub1`'s nests
// them under it. Its archived child stays in project archives in both modes. `s-3`, unbound, has one of its own,
// which the project's focus mode nests under it. `s-console-old` is an archived console session, which the
// switch strip leaves out. For the strip's three kinds of activity: `hub1` waits through `s-2`, `hub2` is
// working itself and `hub3` works through `s-3d`, while `hub4` has nothing going on.
const lead = sessionOf("s-lead", console_.id, project.id, "Rework the result ranking", "idle", { bound_to: hub1.id, account_id: "a-work", terminal_title: "Compare ranking strategies" });
const sessions = [
  hub1,
  hub2,
  hub3,
  hub4,
  sessionOf("s-console-old", console_.id, undefined, "Archived console session", "archived", { colour: "azure", ended_at: minutesAgo(60 * 24) }),
  sessionOf("s-1", console_.id, project.id, "Add idempotency keys", "working", { pinned: true, bound_to: hub1.id, account_id: "a-work", terminal_title: "Prevent duplicate API requests" }),
  sessionOf("s-2", console_.id, project.id, "Fix the rounding of partial results", "waiting_user", { bound_to: hub1.id }),
  sessionOf("s-3", console_.id, project.id, "Write the migration", "idle", { terminal_title: "Plan the database migration" }),
  sessionOf("s-3a", console_.id, project.id, "Check the index sizes", "interrupted", { bound_to: "s-3" }),
  lead,
  sessionOf("s-lead-1", console_.id, project.id, "Chase the flaky test", "working", { bound_to: lead.id }),
  sessionOf("s-lead-2", console_.id, project.id, "Measure the first paint", "waiting_user", { bound_to: lead.id }),
  sessionOf("s-lead-archived-child", console_.id, project.id, "Archived work of the active lead", "archived", { bound_to: lead.id, ended_at: minutesAgo(70) }),
  sessionOf("s-lead-archived", console_.id, project.id, "An archived lead session", "archived", { bound_to: hub1.id, ended_at: minutesAgo(90) }),
  sessionOf("s-lead-archived-1", console_.id, project.id, "A session of the archived lead session", "archived", { bound_to: "s-lead-archived", ended_at: minutesAgo(80) }),
  sessionOf("s-3b", console_.id, project.id, "Profile the slow query", "interrupted", { account_id: "a-work" }),
  sessionOf("s-3c", console_.id, project.id, "Rework the retry loop", "idle", { bound_to: hub2.id }),
  sessionOf("s-3d", console_.id, project.id, "Document the retry policy", "working", { bound_to: hub3.id }),
  sessionOf("s-4", console_.id, project.id, "Older investigation", "archived", {
    ended_at: minutesAgo(60 * 5),
    bound_to: hub1.id,
  }),
  sessionOf("s-5", console_.id, project.id, "Last week's spike", "archived", {
    ended_at: minutesAgo(60 * 24 * 6),
  }),
  sessionOf("s-6", console_.id, other.id, "Refresh the landing page", "working", { bound_to: hub1.id }),
  sessionOf("s-7", console_.id, other.id, "Compress the hero images", "idle", { bound_to: hub2.id }),
  sessionOf("s-8", console_.id, other.id, "Check the dark theme", "idle"),
  sessionOf("s-9", console_.id, third.id, "Proofread the guide", "idle", { bound_to: hub2.id }),
  sessionOf("s-9a", console_.id, third.id, "Update the changelog", "idle", { bound_to: hub4.id }),
  sessionOf("s-9b", console_.id, third.id, "An archived session of the fourth", "archived", { bound_to: hub4.id, ended_at: minutesAgo(60) }),
];

// Names that test a narrow layout: a long console-session name and a redesign coordinator.
const longHub = sessionOf("s-long-hub", console_.id, undefined, "A console session titled at a length no row fits", "idle", { colour: "azure", terminal_title: "Review a very long agent-generated task title across the entire release pipeline" });
const redesignHub = sessionOf("s-redesign-hub", console_.id, undefined, "Website redesign coordinator", "idle", { colour: "olive" });
const searchProject = projectOf("p-search", console_.id, "Search interface service");



// Many console sessions, some with long titles, so the strip has to scroll; started in the
// order they are listed here, which is the strip's order.
const crowd = [
  sessionOf("o-1", console_.id, undefined, "Website redesign", "working", { colour: "teal" }),
  sessionOf("o-2", console_.id, undefined, "A console session titled at a length no chip fits", "idle", { colour: "azure" }),
  sessionOf("o-3", console_.id, undefined, "Website redesign coordinator", "waiting_user", { colour: "olive" }),
  sessionOf("o-4", console_.id, undefined, "Release notes", "idle", { colour: "rose" }),
  sessionOf("o-5", console_.id, undefined, "Billing migration", "idle", { colour: "violet" }),
  sessionOf("o-6", console_.id, undefined, "Docs", "idle", { colour: "jade" }),
  sessionOf("o-7", console_.id, undefined, "Search tuning", "working", { colour: "teal" }),
].map((s, i) => ({ ...s, started_at: minutesAgo(700 - i * 10) }));

// Empty and overflow cases belong to separate entities in the same workspace.
const loneConsole = consoleOf("c-lone", "One console session");
const projectlessConsole = consoleOf("c-projectless", "No projects");
const crowdedConsole = consoleOf("c-crowded", "Many console sessions");
const fittingConsole = consoleOf("c-fitting", "Two short hubs");
const emptyProject = projectOf("p-empty", console_.id, "Archived only");
const freshProject = projectOf("p-fresh", console_.id, "No sessions yet");
const longProject = projectOf("p-long", console_.id, "A project whose name is far too long for one row");
const state = snapshotState({
  consoles: [{ ...console_, name: "A console with a rather long name indeed" }, loneConsole, projectlessConsole, crowdedConsole, fittingConsole],
  projects: [project, other, third, searchProject, longProject, emptyProject, freshProject, projectOf("p-lone", loneConsole.id, "No bound sessions"), projectOf("p-fitting", fittingConsole.id, "Short strip")],
  sessions: [
    ...sessions, longHub, redesignHub,
    sessionOf("s-long-latin", console_.id, longProject.id, "Check the build", "working", { bound_to: longHub.id, terminal_title: "Review build failures and dependency upgrade compatibility" }),
    sessionOf("s-long-dependencies", console_.id, searchProject.id, "Check the build and update dependencies", "idle", { bound_to: longHub.id, terminal_title: "Explain the differences between the three galleries and check how long task titles are clipped" }),
    sessionOf("s-redesign-bound", console_.id, project.id, "Check the build", "idle", { bound_to: redesignHub.id }),
    sessionOf("s-long-project-bound", console_.id, project.id, "Check long names", "idle", { bound_to: longHub.id }),
    sessionOf("s-empty-old", console_.id, emptyProject.id, "Older investigation", "archived", { bound_to: hub1.id, ended_at: minutesAgo(300) }),
    sessionOf("s-lone", loneConsole.id, undefined, "Only hub", "idle"),
    sessionOf("s-projectless", projectlessConsole.id, undefined, "Projectless hub", "idle"),
    sessionOf("s-fitting-a", fittingConsole.id, undefined, "A", "idle"),
    sessionOf("s-fitting-b", fittingConsole.id, undefined, "B", "working"),
    ...crowd.map((session) => ({ ...session, console_id: crowdedConsole.id })),
  ],
  settings,
});

// Archive-only views keep the preview boundary and heading counts visible together. The
// archived lead contributes three display rows, including its two nested children.
const previewHub = sessionOf("s-preview-hub", console_.id, undefined, "Hub", "idle", { colour: "teal" });
const previewLead = sessionOf("s-preview-lead", console_.id, emptyProject.id, "Archived lead session", "archived", { bound_to: previewHub.id, ended_at: minutesAgo(8) });
function archivePreviewState(count: 5 | 6) {
  return snapshotState({
    consoles: [console_],
    projects: [emptyProject],
    sessions: [
      previewHub,
      previewLead,
      ...Array.from({ length: 2 }, (_, i) => sessionOf(`s-preview-child-${i}`, console_.id, emptyProject.id, `Session ${i + 1} of the archived lead`, "archived", { bound_to: previewLead.id, agent: i === 1 ? "codex" : "claude", ended_at: minutesAgo(2 + i * 3) })),
      ...Array.from({ length: count - 3 }, (_, i) => sessionOf(`s-preview-${i}`, console_.id, emptyProject.id, `Bound archived session ${i + 1}`, "archived", { bound_to: previewHub.id, ended_at: minutesAgo(10 + i * 5) })),
    ],
  });
}
const archivePreviewFive = archivePreviewState(5);
const archivePreviewSix = archivePreviewState(6);

// Long bound titles exercise the single-line fade and full-title hover without displacing the owner badge.
const combinedState = {
  ...state,
  sessions: new Map([...state.sessions!].map(([id, session]) => [id, {
    ...session,
    terminal_title: id === "s-2"
      ? "Fix the rounding of partial search results and preserve precision across every supported currency"
      : id === "s-lead-2"
        ? "检查搜索结果首次绘制时间并确认所有地区的长标题和控制台绑定标记仍然清晰可见"
        : session.terminal_title,
  }])),
};

const hierarchyState = {
  ...state,
  projects: new Map([...state.projects!].map(([id, item]) => [id, id === project.id ? { ...item, pinned: true } : item])),
  sessions: new Map([...state.sessions!].map(([id, session]) => [id, id === "s-3" ? { ...session, pinned: true } : session])),
};

export const focusScenarios: Scenario[] = [
  {
    id: "sidebar-session-hierarchy",
    group: "Sidebar sessions",
    title: "Project session hierarchy",
    state: hierarchyState,
    preferences: { sidebarConsole: console_.id },
    description: "Rounded branches connect each project session and its nested sessions; the final branch ends at the last child. Agent task titles and fallback session names appear together. Search API and its bound Add idempotency keys and unbound Write the migration are pinned. Binding dots and pins remain visible while action menus hide; hover or focus a row to compare the evenly spaced controls, then unpin either session to see the remaining controls close together.",
  },
  {
    id: "focus-project",
    group: GROUP,
    title: "Project focus",
    state: state,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    description: "Live console-bound sessions make the Show all project sessions switch available; it starts off. Colored summary chips with contrasting gray count badges sit above archives, including bound sessions and an archived child of an active lead.",
    steps: [
      (ui) => ui.press(ui.session(ui.t("session.displayTitle", { title: "Plan the database migration", name: "Write the migration" }))),
    ],
  },
  {
    id: "focus-project-console-sessions",
    group: GROUP,
    title: "Project focus including console sessions",
    state: combinedState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}`, projectFocusIncludeConsoleSessions: true },
    description: "The switch is on: bound and unbound sessions share subtly distinct cards. Owner badges precede single-line titles following the reading direction; long Latin and CJK names fade at the edge, with marquee and the full title available on hover. Status glyphs, owner badges and agent marks share one leading column; all three text lines start together, including unbound cards with an empty owner slot. Pin/menu spacing matches the header's plus/menu spacing. Bound nested cards inherit the owner's color. Hover a badge to read its owner. Pinned cards stay first; other siblings sort Waiting for you, Awaiting instructions, Working, Interrupted, with children kept under their parent. Archives remain available below the cards.",
    steps: [
      (ui) => ui.press(ui.session(ui.t("session.displayTitle", { title: "Compare ranking strategies", name: "Rework the result ranking" }))),
    ],
  },
  {
    id: "focus-console-session",
    group: GROUP,
    title: "Console session focus",
    state: state,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    description: "Bound cards and archives stay under their owners. The switch strip includes waiting, working and inactive sessions; hover markers for details.",
    steps: [
      (ui) => ui.press(ui.session(ui.t("session.displayTitle", { title: "Prevent duplicate API requests", name: "Add idempotency keys" }))),
    ],
  },
  {
    id: "focus-console-session-long",
    group: GROUP,
    title: "Long console session and project names",
    state: state,
    preferences: { sidebarWidth: 200, sidebarConsole: console_.id, sidebarFocus: `consoleSession:${longHub.id}` },
    description: "Long English agent task titles with original session names in the header and cards, alongside long session names and project headings.",
  },
  {
    id: "focus-project-no-sessions",
    group: GROUP,
    title: "Project archive preview, exactly five rows",
    state: archivePreviewFive,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${emptyProject.id}`, projectFocusIncludeConsoleSessions: true },
    description: "The headings show zero live sessions and five archived sessions in parentheses. The entire archive fits, so there is no View all row; archived children stay under their lead. The Show all project sessions switch is absent even when its saved preference is on.",
  },
  {
    id: "focus-project-archive-overflow",
    group: GROUP,
    title: "Project archive preview, six rows",
    state: archivePreviewSix,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${emptyProject.id}` },
    description: "The archive heading counts all six sessions in parentheses. Five rows precede View all, which aligns with the session rows and repeats no count.",
  },
  {
    id: "focus-console-archive-five",
    group: GROUP,
    title: "Console archive preview, exactly five rows",
    state: archivePreviewFive,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "The headings show zero live sessions and five archived sessions in parentheses. All five rows fit, including the lead's nested children, so View all is absent.",
  },
  {
    id: "focus-console-archive-overflow",
    group: GROUP,
    title: "Console archive preview, six rows",
    state: archivePreviewSix,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "The archive heading counts all six bound sessions in parentheses. View all follows the five-row preview with the same row spacing and no repeated count.",
  },
  {
    id: "focus-project-empty",
    group: GROUP,
    title: "Project with no sessions yet",
    state,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${freshProject.id}` },
    description: "With no sessions or archive, the Show all project sessions switch is absent. The sidebar keeps one compact clickable empty row and the main area offers New session for the focused project.",
  },
  {
    id: "focus-console-session-empty",
    group: GROUP,
    title: "No bound sessions or switch strip",
    state: state,
    preferences: { sidebarConsole: loneConsole.id, sidebarFocus: "consoleSession:s-lone" },
    description: "No bound sessions appears as a quiet row. The header's plus button opens a project chooser to create a bound session. The main area asks for selection; the focused console session remains selectable in the header.",
  },
  {
    id: "focus-console-session-no-projects",
    group: GROUP,
    title: "Console session with no projects",
    state: state,
    preferences: { sidebarConsole: projectlessConsole.id, sidebarFocus: "consoleSession:s-projectless" },
    description: "The sidebar offers Add project as a compact row and keeps New session disabled. The main area asks for selection; the focused console session remains selectable in the header.",
  },
  {
    id: "focus-console-session-overflow",
    group: GROUP,
    title: "Overflowing console session strip",
    state: state,
    preferences: { sidebarConsole: crowdedConsole.id, sidebarFocus: `consoleSession:${crowd[5].id}` },
  },
  {
    id: "focus-console-session-fit",
    group: GROUP,
    title: "Console session strip that fits",
    state: state,
    preferences: { sidebarConsole: fittingConsole.id, sidebarFocus: "consoleSession:s-fitting-a" },
  },
  {
    id: "focus-project-new-session",
    group: GROUP,
    title: "New session from project focus",
    state: state,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    description: "The dialog offers no owner choice.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
    ],
  },
  {
    id: "focus-console-new-session",
    group: GROUP,
    title: "New session from console focus",
    state: state,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    description: "The chosen owner is a fixed line.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.focus.newSession")),
      (ui) => ui.press(third.name),
    ],
  },
];
