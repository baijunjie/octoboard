import type { SessionUsage } from "../../protocol";
import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Focus mode";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const other = projectOf("p-2", console_.id, "Website");
const third = projectOf("p-3", console_.id, "Docs");

// The switch strip lists console sessions by when they started, in the order they are named here.
const hub1 = sessionOf("s-console", console_.id, undefined, "Hub", "idle", { colour: "teal", started_at: minutesAgo(400) });
const hub2 = sessionOf("s-console-2", console_.id, undefined, "Hub 2", "working", { colour: "pink", started_at: minutesAgo(300) });
const hub3 = sessionOf("s-console-3", console_.id, undefined, "Hub 3", "idle", { colour: "lime", started_at: minutesAgo(200) });
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
  sessionOf("s-console-old", console_.id, undefined, "Archived console session", "archived", { colour: "cyan", started_at: minutesAgo(60 * 25), ended_at: minutesAgo(60 * 24) }),
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
const longHub = sessionOf("s-long-hub", console_.id, undefined, "A console session titled at a length no row fits", "idle", { colour: "cyan", terminal_title: "Review a very long agent-generated task title across the entire release pipeline" });
const redesignHub = sessionOf("s-redesign-hub", console_.id, undefined, "Website redesign coordinator", "idle", { colour: "yellow" });
const searchProject = projectOf("p-search", console_.id, "Search interface service");



// Many console sessions, some with long titles, so the strip has to scroll; started in the
// order they are listed here, which is the strip's order.
const crowd = [
  sessionOf("o-1", console_.id, undefined, "Website redesign", "working", { colour: "teal" }),
  sessionOf("o-2", console_.id, undefined, "A console session titled at a length no chip fits", "idle", { colour: "cyan" }),
  sessionOf("o-3", console_.id, undefined, "Website redesign coordinator", "waiting_user", { colour: "yellow" }),
  sessionOf("o-4", console_.id, undefined, "Release notes", "idle", { colour: "pink" }),
  sessionOf("o-5", console_.id, undefined, "Billing migration", "idle", { colour: "violet" }),
  sessionOf("o-6", console_.id, undefined, "Docs", "idle", { colour: "lime" }),
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
      const session = sessionOf(`s-scroll-unbound-${i}`, console_.id, project.id, `Review search improvement ${i + 1}`, "idle", i === 0 ? { pinned: true } : {});
      return [session.id, session] as const;
    }),
    // Nothing was ever said in it, so its shortcut and menu offer Delete in place of Archive.
    ["s-scroll-unused", sessionOf("s-scroll-unused", console_.id, project.id, "Draft the release notes", "idle", { has_conversation: false })] as const,
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
// ten-root boundary has no hidden children; the eleven-root overflow also has two hidden children.
const previewHub = sessionOf("s-preview-hub", console_.id, undefined, "Hub", "idle", { colour: "teal" });
const previewLead = sessionOf("s-preview-lead", console_.id, emptyProject.id, "Archived lead session", "archived", { bound_to: previewHub.id, ended_at: minutesAgo(8), usage: { input_tokens: 184_320, output_tokens: 16_384, total_tokens: 200_704 } });
function archivePreviewState(count: 10 | 11, includeUnused = false) {
  return snapshotState({
    consoles: [console_],
    projects: [emptyProject],
    sessions: [
      previewHub,
      previewLead,
      ...Array.from({ length: count === 11 ? 2 : 0 }, (_, i) => sessionOf(`s-preview-child-${i}`, console_.id, emptyProject.id, `Session ${i + 1} of the archived lead`, "archived", { bound_to: previewLead.id, agent: i === 1 ? "codex" : "claude", ended_at: minutesAgo(2 + i * 3) })),
      ...Array.from({ length: count - 1 }, (_, i) => {
        const unused = includeUnused && i === 2;
        return sessionOf(`s-preview-${i}`, console_.id, emptyProject.id, unused ? "Unused archived draft" : (i === 0 ? "A completed search investigation with a long archived preview title" : i === 1 ? "A completed search investigation whose long archived title scrolls only when hovering the text" : `Bound archived session ${i + 1}`), "archived", { bound_to: unused ? undefined : previewHub.id, has_conversation: !unused, ended_at: minutesAgo(10 + i * 5), ...(i === 0 ? { usage: { input_tokens: 0, output_tokens: 0, total_tokens: 0 } } : {}) });
      }),
    ],
  });
}
const archivePreviewTen = archivePreviewState(10);
const archivePreviewEleven = archivePreviewState(11);
const projectArchivePreviewTen = archivePreviewState(10, true);
const projectArchivePreviewEleven = archivePreviewState(11, true);

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

// The token count of "Add idempotency keys", as an agent that reports no cache would leave it.
const withoutCache = (usage: SessionUsage): SessionUsage => {
  const { cached_input_tokens: _cached, cache_write_input_tokens: _written, ...counts } = usage;
  return counts;
};
const uncachedUsageState = {
  ...scrollingFocusState,
  sessions: new Map([...scrollingFocusState.sessions].map(([id, session]) => [id, id === "s-1" ? { ...session, usage: session.usage && withoutCache(session.usage) } : session])),
};

// A single recorded token exercises the tooltip heading's singular form.
const singleTokenUsageState = {
  ...scrollingFocusState,
  sessions: new Map([...scrollingFocusState.sessions].map(([id, session]) => [id, id === "s-1" ? { ...session, usage: { input_tokens: 1, output_tokens: 0, total_tokens: 1 } } : session])),
};

const addIdempotencyKeys = (ui: Ui) => ui.session(ui.t("session.displayTitle", { title: "Prevent duplicate API requests", name: "Add idempotency keys" }));

const hierarchyState = {
  ...state,
  projects: new Map([...state.projects!].map(([id, item]) => [id, id === project.id ? { ...item, pinned: true } : item])),
  sessions: new Map([
    ...[...state.sessions!].map(([id, session]) => [id, id === "s-3b" ? { ...session, pinned: true, locked: true } : id === "s-3" ? { ...session, pinned: true } : id === "s-2" ? { ...session, terminal_title: "Review search result accuracy and verify long session row titles scroll only while hovering their text" } : session] as const),
    // Locking also protects an unused session that would otherwise offer immediate deletion.
    ["s-unused", sessionOf("s-unused", console_.id, project.id, "Draft the release notes", "idle", { has_conversation: false, locked: true })] as const,
  ]),
};

const lockedFocusState = {
  ...scrollingFocusState,
  sessions: new Map([...scrollingFocusState.sessions].map(([id, session]) => [id, id === hub1.id || id === "s-1" ? { ...session, locked: true } : session])),
};

export const focusScenarios: Scenario[] = [
  {
    id: "sidebar-session-hierarchy",
    group: "Sidebar sessions",
    category: "normal",
    title: "Project session hierarchy",
    state: hierarchyState,
    sessionRecords: { "s-unused": "absent" },
    preferences: { sidebarConsole: console_.id },
    description: "The console header keeps New console session immediately before its actions menu. Scroll the sidebar: Console sessions stays pinned until Projects pushes it away, then Projects and its controls stay above the fade. Rounded branches connect each project session and its nested sessions; the final branch ends at the last child. Agent task titles and fallback session names appear together. Search API and its bound Add idempotency keys and unbound Write the migration and Profile the slow query are pinned. Binding dots and pins remain visible while action menus hide; hover or focus a row to compare the evenly spaced controls, then unpin a session to see the remaining controls close together. An Archive button precedes the menu only on a session that archives without a confirmation: an idle or interrupted one with no unarchived session bound to it or to one bound to it. Rework the retry loop has it; the pinned, locked Profile the slow query instead keeps a yellow border and lock before its menu without moving in the order; Write the migration and Compare ranking strategies (unarchived sessions are bound under them), the idle console sessions (which have bound sessions too), Add idempotency keys (working) and Review search result accuracy (waiting) do not. Draft the release notes is unused but locked, so its yellow-bordered row has no Delete shortcut. Both locked rows keep a clickable Unlock icon visible before actions, and their menus offer Unlock with Archive or Delete disabled. Long session names scroll only while their text is hovered; hovering row icons or controls leaves them still. Set Modifier to Held above the window to see the console header's and each project row's new-session buttons as the bolt that creates with defaults.",
  },
  {
    id: "focus-project",
    group: GROUP,
    category: "normal",
    title: "Project focus",
    state: scrollingFocusState,
    sessionRecords: { "s-scroll-unused": "absent" },
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    description: "Write the migration has one nested card: a rounded branch crosses the card gap and stops at that child’s centre, with no line below it or beside top-level cards. The branch mirrors in Arabic. Write the migration has recorded zero usage, so its usage value is hidden, as it is for sessions without an agent usage record. The switch beside the Sessions count starts off; hover or focus it for its Show all project sessions tooltip. Colored summary chips and gray count badges sit above archives. Scroll to confirm Sessions stays clear above the fade. At a 200px window height, Archived pushes it away at the top; the preview hides child sessions and retains View all. Idle and interrupted cards show an Archive button beside the menu, the pinned Review search improvement 1 after its pin; Write the migration (an unarchived session is bound under it) has no shortcut; the unused Draft the release notes instead has a Delete shortcut immediately before its menu, deleting immediately and moving any native records to the Trash without confirmation. Rework the result ranking and the working and waiting cards sit under console sessions, so they appear once the switch is on. Compare the selected session card and project header actions buttons at rest with muted glyphs, then hover or open each menu for the shared subtle tint. Set Modifier to Held to see the header's new-session button as the bolt.",
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
    description: "Agent names sit on the left of the metadata line; a compact usage chip and creation age sit on the right, all sharing the same muted text color. A substantial cached total is shown; zero totals and missing native records omit the chip without adding a row. Account names appear only in agent icon tooltips and accessible session labels. The switch beside Sessions is on; hover or focus it for its tooltip. Bound and unbound cards differ subtly; owner badges precede long single-line titles, which fade and scroll on text hover. Hover only the text to scroll it; owner, status and agent marks or menu controls leave both lines still. Bound cards keep status, owner and agent marks in one column; titles without an owner badge start at the card content edge without an empty owner slot. Pin/menu spacing matches the header controls. Nested cards inherit owner colors; hover badges for owner names. Rounded branches connect both the single child under Write the migration and the two children under Rework the result ranking. The vertical rail crosses the card gaps and stops at the last child’s centre; top-level cards have no rail. Arabic mirrors the branches. Pinned cards come first, then Waiting for you, Awaiting instructions, Working and Interrupted; children stay under parents. Scroll to confirm Sessions stays above the fade and Archived pushes it away at a 200px window height. Card and header actions buttons share muted resting glyphs and the shared subtle tint when hovered or open.",
    steps: [
      (ui) => ui.press(ui.session(ui.t("session.displayTitle", { title: "Compare ranking strategies", name: "Rework the result ranking" }))),
    ],
  },
  {
    id: "focus-console-session",
    group: GROUP,
    category: "normal",
    title: "Console session focus",
    state: lockedFocusState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    description: "The focused Hub header and pinned Add idempotency keys card have yellow lock borders and persistent Unlock controls before actions; unlocking keeps their existing order. Bound cards place agent names on the left and a compact usage chip beside creation age on the right, all sharing the same muted text color. Zero and absent usage omit the chip without adding a row; hover the chip for input, output and cache details, or agent icons for account details. Bound cards and archives stay under their owners. The two cards under Rework the result ranking have rounded branches joined by a continuous vertical rail across the card gap. The rail stops at the last child’s centre and does not appear beside top-level cards; Arabic mirrors it. Scroll the long list to confirm Sessions stays clear above the fade and, at a 200px window height, Archived replaces it at the top. The switch strip includes waiting, working and inactive sessions; hover markers for details. The header plus menu, header actions, nested project actions, session card actions and archived session actions use the same muted resting glyphs and the shared subtle tint when hovered or open. Set Modifier to Held to see the header plus menu and each project group's new-session button as the bolt; choosing a project from the menu then creates with defaults.",
    steps: [
      (ui) => ui.press(addIdempotencyKeys(ui)),
    ],
  },
  {
    id: "focus-usage-tooltip",
    group: GROUP,
    category: "normal",
    title: "Token usage tooltip",
    description: "The pointer rests on Add idempotency keys' token count. The tooltip leads with the exact total (plural “tokens” in English), then a divider and a two-column table: muted labels at the start, figures aligned to the end (mirrored under right-to-left), Input and Output followed by Cached input and Cache write.",
    state: scrollingFocusState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    steps: [(ui) => ui.hoverSessionUsage(addIdempotencyKeys(ui))],
  },
  {
    id: "focus-usage-tooltip-single-token",
    group: GROUP,
    category: "normal",
    title: "Token usage tooltip, one token",
    description: "The heading reads “1 token” in English, using the singular form; Chinese retains its usual token label. The table shows one input token and zero output tokens.",
    state: singleTokenUsageState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    steps: [(ui) => ui.hoverSessionUsage(addIdempotencyKeys(ui))],
  },
  {
    id: "focus-usage-tooltip-uncached",
    group: GROUP,
    category: "normal",
    title: "Token usage tooltip without cache",
    description: "The same count for an agent that reports no cache: the table holds only Input and Output, with no empty rows for the cache entries.",
    state: uncachedUsageState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    steps: [(ui) => ui.hoverSessionUsage(addIdempotencyKeys(ui))],
  },
  {
    id: "focus-console-session-long",
    group: GROUP,
    category: "normal",
    title: "Long console session and project names",
    state: state,
    preferences: { sidebarWidth: 200, sidebarConsole: console_.id, sidebarFocus: `consoleSession:${longHub.id}` },
    description: "Long English agent task titles with original session names in the header and cards, alongside long session names and project headings. Hover the clipped console subtitle or session heading to run only that text's marquee after a 600-millisecond initial pause. It pauses at the far end for three seconds, returns to the beginning in about 200 milliseconds, waits three seconds, then starts another run. Moving the pointer away resets it immediately; hovering header controls leaves both still.",
  },
  {
    id: "focus-project-no-sessions",
    group: GROUP,
    category: "normal",
    title: "Project archive preview, exactly ten parents",
    state: projectArchivePreviewTen,
    sessionRecords: { "s-preview-2": "absent" },
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${emptyProject.id}`, projectFocusIncludeConsoleSessions: true },
    description: "The headings count zero live sessions and ten archived parents. All ten parents appear without View all. Compact archive rows omit dates and show a usage chip only for positive totals, matching the muted title color. Hover or focus Unused archived draft to see the Delete shortcut immediately before its menu: it has no conversation, so it deletes and moves any native records to the Trash without confirmation. Used archived rows have only their actions menu, where Delete still requires confirmation. Long preview titles scroll only while the pointer is over their text; icons and row controls do not start them. The Show all project sessions switch is absent even when its saved preference is on. Delete the unused draft or reopen a row, then archive it again: the remaining archive rows slide into place.",
  },
  {
    id: "focus-archived-session-menu",
    group: GROUP,
    category: "normal",
    title: "Archived session actions",
    state: archivePreviewTen,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "Used archived rows have no Delete shortcut because deletion requires confirmation. The menu offers Reopen, Copy session ID and destructive Delete, with a divider before Delete. Archived rows have no lock controls or yellow outlines. Reopen selects the session and moves it from Archived to Sessions.",
    steps: [(ui) => ui.press(ui.t("sidebar.session.actions", { title: previewLead.title }))],
  },
  {
    id: "focus-project-archive-overflow",
    group: GROUP,
    category: "normal",
    title: "Project archive preview, eleven parents with children",
    state: projectArchivePreviewEleven,
    sessionRecords: { "s-preview-2": "absent" },
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${emptyProject.id}` },
    description: "The heading counts thirteen archived sessions: eleven parents and two hidden children. Long preview titles scroll only on text hover. Compact archive rows omit dates and show a usage chip only for positive totals, matching the muted title color. Hover or focus Unused archived draft to see the Delete shortcut immediately before its menu: it has no conversation, so it deletes and moves any native records to the Trash without confirmation. Used archived rows have only their actions menu, where Delete still requires confirmation. Ten parent rows precede View all; children of the archived lead appear only in the full archive. Delete the unused draft to bring the eleventh parent into the preview; reopen another row, then archive it again to watch the remaining rows and View all slide into place. View all remains while archived children are hidden.",
  },
  {
    id: "focus-console-archive-five",
    group: GROUP,
    category: "normal",
    title: "Console archive preview, exactly ten parents",
    state: archivePreviewTen,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "The headings count zero live sessions and ten directly bound archived parents. All ten appear without View all. Compact archive rows omit dates and show a usage chip only for positive totals, matching the muted title color. Used archived rows have no Delete shortcut; their actions menu retains Delete with confirmation and Reopen. Long preview titles scroll only while their text is hovered; icons and controls leave them still. Reopen a row, then archive it again to see the remaining archive rows slide into place.",
  },
  {
    id: "focus-console-archive-overflow",
    group: GROUP,
    category: "normal",
    title: "Console archive preview, eleven parents with children",
    state: archivePreviewEleven,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "The heading counts thirteen bound archived sessions, including two hidden children. Long preview titles scroll only on text hover. Compact archive rows omit dates and show a usage chip only for positive totals, matching the muted title color. Used archived rows have no Delete shortcut; their actions menu retains Delete with confirmation and Reopen. Only ten of eleven parent rows appear before View all; the full archive keeps children under their archived lead. Reopen a parent without children, then archive it again: the remaining archive rows slide as the preview changes.",
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
    description: "Archive a console session from its menu: the remaining markers slide horizontally. Scroll the strip to reveal other sessions without triggering list motion.",
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
