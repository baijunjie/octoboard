import { AGENT_LABEL } from "../../agents";
import type { Session, SessionUsage } from "../../protocol";
import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { ALL_AGENTS, consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

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
  auto_delete_unresumable_archives: true,
  added_agents: ALL_AGENTS, default_clone_dir: "/Users/dev/Projects",
  accounts: [{ id: "a-work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" }],
};

// The sessions of `project` are bound to one of three console sessions or to none, so the project's
// focus mode has something to leave out; `hub1`'s focus mode has two projects of its own, a bound
// session of another owner and an unbound one to leave out, and archived sessions of its own. `lead`
// is a session of `project` bound to `hub1` that has three sessions of its own, which the project's
// focus mode summarizes by default or shows with hub-colored dots when included; `hub1`'s nests
// them under it. Its archived child stays in project archives in both modes. `s-3`, unbound, has one of its own,
// which the project's focus mode nests under it. `s-console-old` is an archived console session, which the
// switch strip leaves out. For the strip's three kinds of activity: `hub1` waits through `s-2`, `hub2` is
// working itself and `hub3` works through `s-3d` (in `consoleFocusState`, idle with a watcher still running),
// while `hub4` has nothing going on.
const lead = sessionOf("s-lead", console_.id, project.id, "Rework the result ranking", "idle", { bound_to: hub1.id, account_id: "a-work", terminal_title: "Compare ranking strategies", usage: { input_tokens: 2_450_000, output_tokens: 75_320, cached_input_tokens: 2_100_000, total_tokens: 2_525_320 } });
const sessions = [
  hub1,
  hub2,
  hub3,
  hub4,
  sessionOf("s-console-old", console_.id, undefined, "Archived console session", "archived", { colour: "cyan", started_at: minutesAgo(60 * 25), ended_at: minutesAgo(60 * 24) }),
  sessionOf("s-1", console_.id, project.id, "Add idempotency keys", "working", { started_at: minutesAgo(3), pinned: true, bound_to: hub1.id, account_id: "a-work", terminal_title: "Prevent duplicate API requests", usage: { input_tokens: 184_320, output_tokens: 16_384, cached_input_tokens: 147_456, cache_write_input_tokens: 8_192, total_tokens: 200_704 } }),
  sessionOf("s-2", console_.id, project.id, "Fix the rounding of partial results", "waiting_user", { bound_to: hub1.id }),
  sessionOf("s-3", console_.id, project.id, "Write the migration", "idle", { terminal_title: "Plan the database migration", started_at: minutesAgo(180), background_tasks: [{ id: "task-migration", type: "shell", description: "cargo watch -x check" }], usage: { input_tokens: 0, output_tokens: 0, total_tokens: 0 } }),
  sessionOf("s-3a", console_.id, project.id, "Check the index sizes", "interrupted", { bound_to: "s-3" }),
  lead,
  sessionOf("s-lead-1", console_.id, project.id, "Chase the flaky test", "working", { bound_to: lead.id, started_at: minutesAgo(5) }),
  sessionOf("s-lead-watch", console_.id, project.id, "Watch the ranking build", "idle", { bound_to: lead.id, started_at: minutesAgo(180), background_tasks: [{ id: "task-ranking", type: "shell", description: "pnpm build --watch" }] }),
  sessionOf("s-lead-2", console_.id, project.id, "Measure the first paint", "waiting_user", { bound_to: lead.id }),
  sessionOf("s-lead-archived-child", console_.id, project.id, "Archived work of the active lead", "archived", { bound_to: lead.id, ended_at: minutesAgo(70) }),
  sessionOf("s-lead-archived", console_.id, project.id, "An archived lead session", "archived", { bound_to: hub1.id, ended_at: minutesAgo(90) }),
  sessionOf("s-lead-archived-1", console_.id, project.id, "A session of the archived lead session", "archived", { bound_to: "s-lead-archived", ended_at: minutesAgo(80) }),
  sessionOf("s-3b", console_.id, project.id, "Profile the slow query", "interrupted", { account_id: "a-work" }),
  sessionOf("s-3c", console_.id, project.id, "Rework the retry loop", "idle", { bound_to: hub2.id }),
  sessionOf("s-3d", console_.id, project.id, "Document the retry policy", "working", { bound_to: hub3.id, started_at: minutesAgo(5) }),
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
  consoles: [console_, loneConsole, projectlessConsole, crowdedConsole, fittingConsole],
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

// The same workspace under a console whose name no header fits, for the long-names scenario alone.
const longNameState = {
  ...state,
  consoles: new Map([...state.consoles!].map(([id, item]) => [id, id === console_.id ? { ...item, name: "A console with a rather long name indeed" } : item] as const)),
};

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

// What each card's line beside the agent's mark reads in the project's focus mode: the model and
// effort observed for a Claude, a Codex and a Grok session; a Claude model named with a release
// date and no effort known; a Codex session whose records name a model but no effort, which reads
// the level the daemon resolved as the agent's own (`default_effort`); and a session whose launch
// overrides leave both to the agent's default.
// The cards left out follow the project's setting.
const observedLaunches: Record<string, Partial<Session>> = {
  "s-1": { observed_model: "claude-opus-5-5", observed_effort: "high" },
  "s-lead-1": { agent: "codex", observed_model: "gpt-6.1-sol", observed_effort: "xhigh" },
  "s-lead-watch": { agent: "codex", observed_model: "gpt-6.1-sol", default_effort: "medium" },
  "s-3d": { agent: "grok", observed_model: "grok-4.7", observed_effort: "high" },
  "s-3c": { observed_model: "claude-haiku-4-5-20251001", effort: null },
  "s-3a": { model: null, effort: null },
};

// Long bound titles exercise the single-line fade and full-title hover without displacing the owner badge.
const combinedState = {
  ...scrollingFocusState,
  settings: { ...settings, accounts: [...settings.accounts,
    { id: "a-long", agent: "claude" as const, name: "Release coordination across every production environment", config_dir: "/Users/dev/.claude-release" },
    { id: "a-release", agent: "claude" as const, name: "Production release coordination and regression review account", config_dir: "/Users/dev/.claude-regression" },
  ] },
  // The project's own setting is what a card with nothing observed or overridden follows.
  projects: new Map([...scrollingFocusState.projects!].map(([id, item]) => [id, id === project.id ? { ...item, claude_session_model: "sonnet", claude_session_effort: "medium" } : item])),
  sessions: new Map([...scrollingFocusState.sessions].map(([id, session]) => [id, {
    ...session,
    ...observedLaunches[id],
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

// Project focus also shows a newer unbound foreground session behind the older migration watcher.
const runningInBackgroundState = {
  ...scrollingFocusState,
  sessions: new Map([
    ...scrollingFocusState.sessions,
    ["s-query-working", sessionOf("s-query-working", console_.id, project.id, "Optimize the query plan", "working", { started_at: minutesAgo(5) })],
  ]),
};

const addIdempotencyKeys = (ui: Ui) => ui.session(ui.t("session.displayTitle", { title: "Prevent duplicate API requests", name: "Add idempotency keys" }));

const hierarchyState = {
  ...state,
  projects: new Map([...state.projects!].map(([id, item]) => [id, id === project.id ? { ...item, pinned: true } : item])),
  sessions: new Map([
    ...[...state.sessions!].map(([id, session]) => {
      const row = id === "s-3b" ? { ...session, pinned: true, locked: true }
        : id === "s-3" ? { ...session, pinned: true }
          : id === "s-2" ? { ...session, terminal_title: "Review search result accuracy and verify long session row titles scroll only while hovering their text" }
            : session;
      const showsUsage = [hub1.id, longHub.id, "s-lead-1", "s-2"].includes(id);
      return [id, showsUsage ? { ...row, usage: lead.usage } : row] as const;
    }),
    // Locking also protects an unused session that would otherwise offer immediate deletion.
    ["s-unused", sessionOf("s-unused", console_.id, project.id, "Draft the release notes", "idle", { has_conversation: false, locked: true })] as const,
  ]),
};

// Two sessions bound to `hub1` in which nothing was ever said: one the user opened in its focus mode
// and one an agent started with `new_session`. Only the first is deleted outright; the second was
// dispatched by its owner, which expects it to exist, so it is archived. Started most recently, so
// they sit at the top of the idle cards, within sight without scrolling.
const unusedBoundSessions = [
  sessionOf("s-bound-unused-user", console_.id, project.id, "Draft the changelog", "idle", { bound_to: hub1.id, origin: "user", has_conversation: false, started_at: minutesAgo(3) }),
  sessionOf("s-bound-unused-agent", console_.id, project.id, "Start the benchmark run", "idle", { bound_to: hub1.id, origin: "console", has_conversation: false, started_at: minutesAgo(4) }),
];

// No native records exist for the unused bound sessions.
const unusedBoundRecords = Object.fromEntries(unusedBoundSessions.map((session) => [session.id, "absent" as const]));

// `hub1`'s own card in its focus mode: locked, with a token total beside its agent. `s-1` is locked too.
const consoleFocusState = {
  ...scrollingFocusState,
  sessions: new Map([
    ...[...scrollingFocusState.sessions].map(([id, session]) => [
      id,
      // `hub3`'s only bound session is idle with a watcher still running, which counts as work.
      id === "s-3d" ? { ...session, status: "idle" as const, background_tasks: [{ id: "task-watch", type: "shell", description: "cargo watch -x test" }] }
        : id === hub1.id ? { ...session, locked: true, usage: { input_tokens: 1_240_000, output_tokens: 48_000, cached_input_tokens: 980_000, total_tokens: 1_288_000 } }
          : id === "s-1" ? { ...session, locked: true }
            : session,
    ] as const),
    ...unusedBoundSessions.map((session) => [session.id, session] as const),
  ]),
};

export const focusScenarios: Scenario[] = [
  {
    id: "sidebar-session-hierarchy",
    group: "Sidebar sessions",
    category: "normal",
    title: "Project session hierarchy",
    state: hierarchyState,
    sessionRecords: { "s-unused": "absent" },
    preferences: { sidebarWidth: 280, sidebarConsole: console_.id },
    description: "At the sidebar’s 280px minimum width, usage chips follow console, project and nested session titles, staying visible beside long clipped titles; zero and missing totals leave no chip. Hover a chip for its exact total and cache breakdown. The console header keeps New console session immediately before its actions menu. Scroll the sidebar: Console sessions stays pinned until Projects pushes it away, then Projects and its controls stay above the fade. Rounded branches join each project session to its nested sessions and end at the last child. Binding dots and pins stay visible while action menus hide. Hover or focus a row to compare the evenly spaced controls, then unpin a session to see the remaining controls close together. An Archive button precedes the menu only on a session that archives without confirmation: idle or interrupted, with no unarchived session bound under it, like Rework the retry loop. The pinned, locked Profile the slow query shows a persistent Unlock icon without a lock border, and its menu offers Unlock with Archive disabled. The unused but locked Draft the release notes has no Delete shortcut, and its menu offers Unlock with Delete disabled. Set Modifier to Held to see the console header's and each project row's new-session buttons as the bolt that creates with defaults.",
  },
  {
    id: "focus-project",
    group: GROUP,
    category: "normal",
    title: "Project focus",
    state: runningInBackgroundState,
    sessionRecords: { "s-scroll-unused": "absent" },
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    description: "The switch beside the Sessions count starts off. Hover or focus it for its tooltip. The cards of project sessions bound to console sessions (Rework the result ranking and the working and waiting ones) join the list once it is on. Colored summary chips and gray count badges sit above archives. Scroll to confirm Sessions stays clear above the fade. At a 200px window height Archived pushes it away at the top, its preview hiding child sessions and retaining View all. Idle and interrupted cards show an Archive button beside the menu, the pinned Review search improvement 1 after its pin. Write the migration, selected, has none, as an unarchived session is bound under it and its build watcher is still running: its card reads Running in background with a pinging hollow green ring, unlike the filled blue dot of Optimize the query plan and the working cards that join once the switch is on, as does the glyph at the end of the top bar's breadcrumb, whose tooltip says the same. Scroll down to it: it sits below Awaiting instructions and above the newer working Optimize the query plan, followed by the interrupted Profile the slow query. The unused Draft the release notes has a Delete shortcut instead. Compare the selected card's and the project header's actions buttons at rest, with muted glyphs, then hover or open each menu for the subtle tint every actions button shares. Set Modifier to Held to see the header's new-session button as the bolt.",
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
    description: "The metadata line shows the agent's mark, the model and effort on its left, and a compact usage chip and creation age on its right in the same muted colour. Zero totals and missing native records omit the chip without adding a row. The model and effort, not the agent's name, follow the mark: Add idempotency keys reads Opus 5.5 · high, the Codex session under Rework the result ranking gpt-6.1-sol · xhigh, Watch the ranking build (also Codex, its records naming a model but no effort) gpt-6.1-sol · medium rather than Default, Document the retry policy (Grok) grok-4.7 · high, and Rework the retry loop Haiku 4.5 with a Default effort. Write the migration follows the project's own setting, Sonnet · medium, as do the other cards left unchanged; Check the index sizes, launched with the agent's defaults, reads Default model. Bound cards keep status, owner and agent marks in one column, with owner badges before long single-line titles. Hover a badge for the owner's name. Titles scroll only when their text is hovered, and titles without a badge start at the content edge. Nested cards inherit their owner's colour. Rounded branches connect the single child under Write the migration and the three under Rework the result ranking. A vertical rail crosses the card gaps and stops at the last child's centre, with none beside top-level cards. Arabic mirrors the branches. Pinned cards come first, then Waiting for you, Awaiting instructions, Running in background, Working and Interrupted. The older migration watcher precedes the newer Document the retry policy, while pinned Add idempotency keys stays above the unpinned cards. Under Rework the result ranking, the older Watch the ranking build precedes the newer Chase the flaky test.",
    steps: [
      (ui) => ui.press(ui.session(ui.t("session.displayTitle", { title: "Compare ranking strategies", name: "Rework the result ranking" }))),
    ],
  },
  {
    id: "focus-agent-tooltip",
    group: GROUP,
    category: "normal",
    title: "Agent mark tooltip",
    description: "The pointer rests on the Claude Code mark of Add idempotency keys. The tooltip names the agent and, in brackets, the account the session runs under: Claude Code (Work).",
    state: combinedState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}`, projectFocusIncludeConsoleSessions: true },
    steps: [(ui) => ui.hoverSessionAgent(addIdempotencyKeys(ui), ui.t("session.agentAccount", { agent: AGENT_LABEL.claude, account: "Work" }))],
  },
  {
    id: "focus-console-session",
    group: GROUP,
    category: "normal",
    title: "Console session focus",
    state: consoleFocusState,
    sessionRecords: unusedBoundRecords,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    description: "The header holds only the back button, the console name Main as a single-line heading and the plus menu; Hub's title is on the card below, not in the header. Below the switch strip, fixed above the scrolling list, Hub's own card is not selected: its status, a colour dot, its agent, creation time and 1.3M token count, and its persistent Unlock control before the actions menu, whose items have no Pin or Focus mode. The pinned Add idempotency keys card has a persistent Unlock control too, without lock borders; unlocking either keeps their order. Draft the changelog and Start the benchmark run are both bound to Hub and were never used; the first was opened by the user, so its shortcut and menu offer Delete, while the second was started by an agent and keeps Archive. Within Rework the result ranking, the older Watch the ranking build sits after the waiting child and before the newer working Chase the flaky test. The pinned working Add idempotency keys stays above the waiting and background cards. The switch strip includes waiting, working and inactive sessions; hover markers for details. Hub 3's marker shows work although its only bound session, Document the retry policy, is idle with a test watcher still running. Set Modifier to Held to see the header plus menu and each project group's new-session button as the bolt; choosing a project from the menu then creates with defaults.",
    steps: [
      (ui) => ui.press(addIdempotencyKeys(ui)),
    ],
  },
  {
    id: "focus-console-session-selected",
    group: GROUP,
    category: "normal",
    title: "Console session card selected",
    state: consoleFocusState,
    sessionRecords: unusedBoundRecords,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    description: "Pressing Hub's card in the sidebar selects the console session: the card takes the selected highlight, the breadcrumb and the terminal switch to Hub, and no card in the list is selected. Compare it with the unselected card of Console session focus.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
    ],
  },
  {
    id: "focus-console-session-drawer",
    group: GROUP,
    category: "normal",
    title: "Console session focus in the mobile sidebar card",
    width: 390,
    state: consoleFocusState,
    sessionRecords: unusedBoundRecords,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    description: "Below the docked breakpoint the sidebar is a floating card whose header is 40px tall instead of 48px. The back button, the single-line console name Main and the plus menu fit without clipping, and the switch strip and Hub's own card follow it.",
    steps: [
      (ui) => ui.press(ui.t("titleBar.sidebar.show")),
    ],
  },
  {
    id: "focus-project-drawer",
    group: GROUP,
    category: "normal",
    title: "Project focus in the mobile sidebar card",
    width: 390,
    state: scrollingFocusState,
    sessionRecords: { "s-scroll-unused": "absent" },
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    description: "Below the docked breakpoint the sidebar is a floating card whose header is 40px tall instead of 48px. The project header keeps both lines inside it, the console name Main over Search API with the gap between them no larger than the margins above and below, beside the back button, the + button and the actions menu without clipping.",
    steps: [
      (ui) => ui.press(ui.t("titleBar.sidebar.show")),
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
    state: longNameState,
    preferences: { sidebarWidth: 280, sidebarConsole: console_.id, sidebarFocus: `consoleSession:${longHub.id}` },
    description: "A long console name fills the single-line header, while the console session's own card above the list clips its long agent task title and original name; long session names and project headings follow in the list. Hover the clipped console name, the card's title or a session heading to run only that text's marquee: after a 600-millisecond initial pause it rests three seconds at the far end, returns in about 200 milliseconds, waits three seconds and runs again, resetting as soon as the pointer leaves. Hovering header controls leaves the console name still.",
  },
  {
    id: "focus-project-long",
    group: GROUP,
    category: "normal",
    title: "Project focus with long console and project names",
    state: longNameState,
    preferences: { sidebarWidth: 280, sidebarConsole: console_.id, sidebarFocus: `project:${longProject.id}`, projectFocusIncludeConsoleSessions: true },
    description: "The two-line header clips both the long console name above and the long project name below, each fading at its end. Hover either line to run only that line's marquee and see its tooltip; hovering header controls leaves both still.",
  },
  {
    id: "focus-project-no-sessions",
    group: GROUP,
    category: "normal",
    title: "Project archive preview, exactly ten parents",
    state: projectArchivePreviewTen,
    sessionRecords: { "s-preview-2": "absent" },
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${emptyProject.id}`, projectFocusIncludeConsoleSessions: true },
    description: "The headings count zero live sessions and ten archived parents, all shown without View all; the Show all project sessions switch is absent even when its saved preference is on. Compact archive rows omit dates and show a usage chip only for positive totals. Hover or focus Unused archived draft to see its Delete shortcut before the menu: it has no conversation, so it deletes and moves any native records to the Trash without confirmation, while used rows have only their menu, where Delete still asks. Delete the unused draft or reopen a row, then archive it again: the remaining rows slide into place.",
  },
  {
    id: "focus-archived-session-menu",
    group: GROUP,
    category: "normal",
    title: "Archived session actions",
    state: archivePreviewTen,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "The menu offers Reopen, Copy session ID and a destructive Delete after a divider; there is no Hand off. Used archived rows have no Delete shortcut, because deleting needs confirmation. Archived rows have no lock controls. Reopen selects the session and moves it from Archived to Sessions.",
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
    description: "The heading counts thirteen archived sessions: eleven parents and two hidden children. Ten parent rows precede View all, and the archived lead's children appear only in the full archive. Delete the unused draft to bring the eleventh parent into the preview; View all remains while archived children are hidden.",
  },
  {
    id: "focus-console-archive-five",
    group: GROUP,
    category: "normal",
    title: "Console archive preview, exactly ten parents",
    state: archivePreviewTen,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "The headings count zero live sessions and ten directly bound archived parents, all shown without View all. Used archived rows have no Delete shortcut; their menu keeps Reopen and Delete with confirmation.",
  },
  {
    id: "focus-console-archive-overflow",
    group: GROUP,
    category: "normal",
    title: "Console archive preview, eleven parents with children",
    state: archivePreviewEleven,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${previewHub.id}` },
    description: "The heading counts thirteen bound archived sessions, two of them hidden children. Only ten of the eleven parent rows appear before View all; the full archive keeps children under their archived lead.",
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
    id: "focus-project-empty-no-agent-added",
    group: GROUP,
    category: "exceptional",
    title: "Project with no sessions yet, no agent added",
    state: { ...state, settings: { ...settings, added_agents: [] } },
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${freshProject.id}` },
    description: "The sidebar's compact empty row has no action, the header's \"+\" is disabled, and the main area, under a bot glyph, says no agent is added with an Add agent… button instead of offering New session.",
  },
  {
    id: "focus-console-session-empty",
    group: GROUP,
    category: "normal",
    title: "No bound sessions or switch strip",
    state: state,
    preferences: { sidebarConsole: loneConsole.id, sidebarFocus: "consoleSession:s-lone" },
    description: "No bound sessions appears as a quiet row. The header's plus button opens a project chooser to create a bound session. The main area asks for selection; the focused console session's own card, fixed above the list, is how to select it, with no switch strip above it. Only hub, idle with nothing bound under it, shows an Archive shortcut before its actions menu, as its row in the main sidebar list does.",
  },
  {
    id: "focus-console-session-no-projects",
    group: GROUP,
    category: "normal",
    title: "Console session with no projects",
    state: state,
    preferences: { sidebarConsole: projectlessConsole.id, sidebarFocus: "consoleSession:s-projectless" },
    description: "The sidebar offers Add project as a compact row and keeps New session disabled. The main area asks for selection; the focused console session's own card remains selectable above the list.",
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
  {
    id: "focus-console-new-session-owner-gone",
    group: GROUP,
    category: "exceptional",
    title: "New session from console focus, the console session archived meanwhile",
    state: state,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    description: "Reached only when the console session ends under the open dialog (archived from a second window). A danger alert at the top of the dialog body says it is no longer available, so a session cannot be opened for it; the fixed owner line stays and Open is disabled.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.focus.newSession")),
      (ui) => ui.press(third.name),
      (ui) => ui.archiveSession(hub1.id),
    ],
  },
];
