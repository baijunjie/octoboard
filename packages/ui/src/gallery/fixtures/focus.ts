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

// A named account exposed through agent icon tooltips and accessible session labels.
const settings = {
  auto_pull_repositories: false,
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
const lead = sessionOf("s-lead", console_.id, project.id, "Rework the result ranking", "idle", { bound_to: hub1.id, account_id: "a-work", terminal_title: "Compare ranking strategies", usage: { input_tokens: 2_450_000, output_tokens: 75_320, cached_input_tokens: 2_100_000, total_tokens: 2_525_320 } });
const sessions = [
  hub1,
  hub2,
  hub3,
  hub4,
  sessionOf("s-console-old", console_.id, undefined, "Archived console session", "archived", { colour: "azure", started_at: minutesAgo(60 * 25), ended_at: minutesAgo(60 * 24) }),
  sessionOf("s-1", console_.id, project.id, "Add idempotency keys", "working", { pinned: true, bound_to: hub1.id, account_id: "a-work", terminal_title: "Prevent duplicate API requests", usage: { input_tokens: 184_320, output_tokens: 16_384, cached_input_tokens: 147_456, cache_write_input_tokens: 8_192, total_tokens: 200_704 } }),
  sessionOf("s-2", console_.id, project.id, "Fix the rounding of partial results", "waiting_user", { bound_to: hub1.id }),
  sessionOf("s-3", console_.id, project.id, "Write the migration", "idle", { terminal_title: "Plan the database migration", usage: { input_tokens: 0, output_tokens: 0, total_tokens: 0 } }),
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
    started_at: minutesAgo(60 * 6),
    ended_at: minutesAgo(60 * 5),
    bound_to: hub1.id,
  }),
  sessionOf("s-5", console_.id, project.id, "Last week's spike", "archived", {
    started_at: minutesAgo(60 * 24 * 7),
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
    sessionOf("s-empty-old", console_.id, emptyProject.id, "Older investigation", "archived", { bound_to: hub1.id, started_at: minutesAgo(400), ended_at: minutesAgo(300) }),
    sessionOf("s-lone", loneConsole.id, undefined, "Only hub", "idle"),
    sessionOf("s-projectless", projectlessConsole.id, undefined, "Projectless hub", "idle"),
    sessionOf("s-fitting-a", fittingConsole.id, undefined, "A", "idle"),
    sessionOf("s-fitting-b", fittingConsole.id, undefined, "B", "working"),
    ...crowd.map((session) => ({ ...session, console_id: crowdedConsole.id })),
  ],
  settings,
});

// The existing focus views share enough cards to scroll even with the console-bound cards
// hidden. Keep the hierarchy and empty-state scenarios on their smaller original workspace.
const scrollingFocusState = {
  ...state,
  sessions: new Map([
    ...state.sessions!,
    ...Array.from({ length: 6 }, (_, i) => {
      const session = sessionOf(`s-scroll-unbound-${i}`, console_.id, project.id, `Review search improvement ${i + 1}`, "idle");
      return [session.id, session] as const;
    }),
    ...Array.from({ length: 6 }, (_, i) => {
      const session = sessionOf(`s-scroll-bound-${i}`, console_.id, project.id, `Investigate ranking result ${i + 1}`, "idle", { bound_to: hub1.id });
      return [session.id, session] as const;
    }),
    ...Array.from({ length: 3 }, (_, i) => {
      const session = sessionOf(`s-scroll-archive-${i}`, console_.id, project.id, `Completed search investigation ${i + 1}`, "archived", { bound_to: hub1.id, started_at: minutesAgo(500 + i * 10), ended_at: minutesAgo(400 + i * 10) });
      return [session.id, session] as const;
    }),
  ]),
};

// Archive-only views keep the preview boundary and heading counts visible together. The
// five-root boundary has no hidden children; the six-root overflow also has two hidden children.
const previewHub = sessionOf("s-preview-hub", console_.id, undefined, "Hub", "idle", { colour: "teal" });
const previewLead = sessionOf("s-preview-lead", console_.id, emptyProject.id, "Archived lead session", "archived", { bound_to: previewHub.id, ended_at: minutesAgo(8), usage: { input_tokens: 184_320, output_tokens: 16_384, total_tokens: 200_704 } });
function archivePreviewState(count: 5 | 6) {
  return snapshotState({
    consoles: [console_],
    projects: [emptyProject],
    sessions: [
      previewHub,
      previewLead,
      ...Array.from({ length: count === 6 ? 2 : 0 }, (_, i) => sessionOf(`s-preview-child-${i}`, console_.id, emptyProject.id, `Session ${i + 1} of the archived lead`, "archived", { bound_to: previewLead.id, agent: i === 1 ? "codex" : "claude", ended_at: minutesAgo(2 + i * 3) })),
      ...Array.from({ length: count - 1 }, (_, i) => sessionOf(`s-preview-${i}`, console_.id, emptyProject.id, (i === 0 ? "A completed search investigation with a long archived preview title" : i === 1 ? "A completed search investigation whose long archived title scrolls only when hovering the text" : `Bound archived session ${i + 1}`), "archived", { bound_to: previewHub.id, ended_at: minutesAgo(10 + i * 5), ...(i === 0 ? { usage: { input_tokens: 0, output_tokens: 0, total_tokens: 0 } } : {}) })),
    ],
  });
}
const archivePreviewFive = archivePreviewState(5);
const archivePreviewSix = archivePreviewState(6);

// Long bound titles exercise the single-line fade and full-title hover without displacing the owner badge.
const combinedState = {
  ...scrollingFocusState,
  settings: { ...settings, accounts: [...settings.accounts,
    { id: "a-long", agent: "claude" as const, name: "Release coordination across every production environment", config_dir: "/Users/dev/.claude-release" },
    { id: "a-release", agent: "claude" as const, name: "Production release coordination and regression review account", config_dir: "/Users/dev/.claude-regression" },
  ] },
  sessions: new Map([...scrollingFocusState.sessions].map(([id, session]) => [id, {
    ...session,
    account_id: id === "s-2" ? "a-long" : id === "s-lead-2" ? "a-release" : session.account_id,
    terminal_title: id === "s-2"
      ? "Fix the rounding of partial search results and preserve precision across every supported currency"
      : id === "s-lead-2"
        ? "Review search result first paint timing and verify long regional task titles and console binding marks remain readable"
        : session.terminal_title,
  }])),
};

const hierarchyState = {
  ...state,
  projects: new Map([...state.projects!].map(([id, item]) => [id, id === project.id ? { ...item, pinned: true } : item])),
  sessions: new Map([...state.sessions!].map(([id, session]) => [id, id === "s-3" ? { ...session, pinned: true } : id === "s-2" ? { ...session, terminal_title: "Review search result accuracy and verify long session row titles scroll only while hovering their text" } : session])),
};

export const focusScenarios: Scenario[] = [
  {
    id: "sidebar-session-hierarchy",
    group: "Sidebar sessions",
    category: "normal",
    title: "Project session hierarchy",
    state: hierarchyState,
    preferences: { sidebarConsole: console_.id },
    description: "Rounded branches connect each project session and its nested sessions; the final branch ends at the last child. Agent task titles and fallback session names appear together. Search API and its bound Add idempotency keys and unbound Write the migration are pinned. Binding dots and pins remain visible while action menus hide; hover or focus a row to compare the evenly spaced controls, then unpin either session to see the remaining controls close together. Long session names scroll only while their text is hovered; hovering row icons or controls leaves them still.",
  },
  {
    id: "focus-project",
    group: GROUP,
    category: "normal",
    title: "Project focus",
    state: scrollingFocusState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    description: "Write the migration has recorded zero usage, so its usage value is hidden, as it is for sessions without an agent usage record. The switch beside the Sessions count starts off; hover or focus it for its Show all project sessions tooltip. Colored summary chips and gray count badges sit above archives. Scroll to confirm Sessions stays clear above the fade. At a 200px window height, Archived pushes it away at the top; the preview hides child sessions and retains View all. Compare the selected session card and project header actions buttons at rest with muted glyphs, then hover or open each menu for the shared subtle tint.",
    steps: [
      (ui) => ui.press(ui.session(ui.t("session.displayTitle", { title: "Plan the database migration", name: "Write the migration" }))),
    ],
  },
  {
    id: "focus-project-console-sessions",
    group: GROUP,
    category: "normal",
    title: "Project focus including console sessions",
    state: combinedState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}`, projectFocusIncludeConsoleSessions: true },
    description: "Agent names sit on the left of the metadata line; positive native usage and creation age sit on the right with a dot between them. A substantial cached total is shown; zero totals and missing native records omit usage and its separator without adding a row. Account names appear only in agent icon tooltips and accessible session labels. The switch beside Sessions is on; hover or focus it for its tooltip. Bound and unbound cards differ subtly; owner badges precede long single-line titles, which fade and scroll on text hover. Hover only the text to scroll it; owner, status and agent marks or menu controls leave both lines still. Bound cards keep status, owner and agent marks in one column; titles without an owner badge start at the card content edge without an empty owner slot. Pin/menu spacing matches the header controls. Nested cards inherit owner colors; hover badges for owner names. Pinned cards come first, then Waiting for you, Awaiting instructions, Working and Interrupted; children stay under parents. Scroll to confirm Sessions stays above the fade and Archived pushes it away at a 200px window height. Card and header actions buttons share muted resting glyphs and the shared subtle tint when hovered or open.",
    steps: [
      (ui) => ui.press(ui.session(ui.t("session.displayTitle", { title: "Compare ranking strategies", name: "Rework the result ranking" }))),
    ],
  },
  {
    id: "focus-console-session",
    group: GROUP,
    category: "normal",
    title: "Console session focus",
    state: scrollingFocusState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    description: "Bound cards place agent names on the left and positive native usage beside creation age on the right, with a dot between usage and age. Zero and absent usage omit their separator without adding a row; hover usage for input, output and cache details, or agent icons for account details. Bound cards and archives stay under their owners. Scroll the long list to confirm Sessions stays clear above the fade and, at a 200px window height, Archived replaces it at the top. The switch strip includes waiting, working and inactive sessions; hover markers for details. The header plus menu, header actions, nested project actions, session card actions and archived session actions use the same muted resting glyphs and the shared subtle tint when hovered or open.",
    steps: [
      (ui) => ui.press(ui.session(ui.t("session.displayTitle", { title: "Prevent duplicate API requests", name: "Add idempotency keys" }))),
    ],
  },
  {
    id: "focus-console-session-long",
    group: GROUP,
    category: "normal",
    title: "Long console session and project names",
    state: state,
    preferences: { sidebarWidth: 200, sidebarConsole: console_.id, sidebarFocus: `consoleSession:${longHub.id}` },
    description: "Long English agent task titles with original session names in the header and cards, alongside long session names and project headings. Hover the clipped console subtitle or session heading to run only that text's marquee; hovering header controls leaves both still.",
  },
  {
    id: "focus-project-no-sessions",
    group: GROUP,
    category: "normal",
    title: "Project archive preview, exactly five parents",
    state: archivePreviewFive,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${emptyProject.id}`, projectFocusIncludeConsoleSessions: true },
    description: "The headings count zero live sessions and five archived parents. All five parents appear without View all. Compact archive rows omit dates and show bare usage only for positive totals. Long preview titles scroll only while the pointer is over their text; icons and row controls do not start them. The Show all project sessions switch is absent even when its saved preference is on.",
  },
  {
    id: "focus-project-archive-overflow",
    group: GROUP,
    category: "normal",
    title: "Project archive preview, six parents with children",
    state: archivePreviewSix,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${emptyProject.id}` },
    description: "The heading counts eight archived sessions: six parents and two hidden children. Long preview titles scroll only on text hover. Compact archive rows omit dates and show bare usage only for positive totals. Five parent rows precede View all; children of the archived lead appear only in the full archive.",
  },
  {
    id: "focus-console-archive-five",
    group: GROUP,
    category: "normal",
    title: "Console archive preview, exactly five parents",
    state: archivePreviewFive,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "The headings count zero live sessions and five directly bound archived parents. All five appear without View all. Compact archive rows omit dates and show bare usage only for positive totals. Long preview titles scroll only while their text is hovered; icons and controls leave them still.",
  },
  {
    id: "focus-console-archive-overflow",
    group: GROUP,
    category: "normal",
    title: "Console archive preview, six parents with children",
    state: archivePreviewSix,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "The heading counts eight bound archived sessions, including two hidden children. Long preview titles scroll only on text hover. Compact archive rows omit dates and show bare usage only for positive totals. Only five of six parent rows appear before View all; the full archive keeps children under their archived lead.",
  },
  {
    id: "focus-project-empty",
    group: GROUP,
    category: "normal",
    title: "Project with no sessions yet",
    state,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${freshProject.id}` },
    description: "With no sessions or archive, the Show all project sessions switch is absent. The sidebar keeps one compact clickable empty row and the main area offers New session for the focused project.",
  },
  {
    id: "focus-console-session-empty",
    group: GROUP,
    category: "normal",
    title: "No bound sessions or switch strip",
    state: state,
    preferences: { sidebarConsole: loneConsole.id, sidebarFocus: "consoleSession:s-lone" },
    description: "No bound sessions appears as a quiet row. The header's plus button opens a project chooser to create a bound session. The main area asks for selection; the focused console session remains selectable in the header.",
  },
  {
    id: "focus-console-session-no-projects",
    group: GROUP,
    category: "normal",
    title: "Console session with no projects",
    state: state,
    preferences: { sidebarConsole: projectlessConsole.id, sidebarFocus: "consoleSession:s-projectless" },
    description: "The sidebar offers Add project as a compact row and keeps New session disabled. The main area asks for selection; the focused console session remains selectable in the header.",
  },
  {
    id: "focus-console-session-overflow",
    group: GROUP,
    category: "normal",
    title: "Overflowing console session strip",
    state: state,
    preferences: { sidebarConsole: crowdedConsole.id, sidebarFocus: `consoleSession:${crowd[5].id}` },
  },
  {
    id: "focus-console-session-fit",
    group: GROUP,
    category: "normal",
    title: "Console session strip that fits",
    state: state,
    preferences: { sidebarConsole: fittingConsole.id, sidebarFocus: "consoleSession:s-fitting-a" },
  },
  {
    id: "focus-project-new-session",
    group: GROUP,
    category: "normal",
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
    category: "normal",
    title: "New session from console focus",
    state: state,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${longHub.id}` },
    description: "The chosen owner is a fixed line with a long agent task title and original name; it fades and runs as a marquee on hover.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.focus.newSession")),
      (ui) => ui.press(third.name),
    ],
  },
];
