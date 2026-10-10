import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Focus mode";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const other = projectOf("p-2", console_.id, "Website");
const third = projectOf("p-3", console_.id, "Docs");

const hub1 = sessionOf("s-console", console_.id, undefined, "Hub", "idle", { colour: "teal" });
const hub2 = sessionOf("s-console-2", console_.id, undefined, "Hub 2", "working", { colour: "rose" });
const hub3 = sessionOf("s-console-3", console_.id, undefined, "Hub 3", "idle", { colour: "jade" });

// A named account, so a card's agent line reads "Claude Code (Work)".
const settings = {
  auto_sync_repositories: false,
  default_clone_dir: "/Users/dev/Projects",
  accounts: [{ id: "a-work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" }],
};

// The sessions of `project` are bound to one of three console sessions or to none, so the project's
// focus mode has something to leave out; `hub1`'s focus mode has two projects of its own, a bound
// session of another owner and an unbound one to leave out, and archived sessions of its own.
const sessions = [
  hub1,
  hub2,
  hub3,
  sessionOf("s-1", console_.id, project.id, "Add idempotency keys", "working", { pinned: true, bound_to: hub1.id, account_id: "a-work" }),
  sessionOf("s-2", console_.id, project.id, "Fix the rounding of partial results", "waiting_user", { bound_to: hub1.id }),
  sessionOf("s-3", console_.id, project.id, "Write the migration", "idle"),
  sessionOf("s-3b", console_.id, project.id, "Profile the slow query", "interrupted", { account_id: "a-work" }),
  sessionOf("s-3c", console_.id, project.id, "Rework the retry loop", "idle", { bound_to: hub2.id }),
  sessionOf("s-3d", console_.id, project.id, "Document the retry policy", "idle", { bound_to: hub3.id }),
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
];

// Names that test a narrow layout: a long Latin one and a CJK one.
const longHub = sessionOf("s-long-hub", console_.id, undefined, "A console session titled at a length no row fits", "idle", { colour: "azure" });
const cjkHub = sessionOf("s-cjk-hub", console_.id, undefined, "负责网站改版的编排会话", "idle", { colour: "olive" });
const cjkProject = projectOf("p-cjk", console_.id, "搜索接口服务");

const state = snapshotState({ consoles: [console_], projects: [project, other, third], sessions, settings });

// A fourth console session with nothing going on, and one of the others waiting through a session
// bound to it, so the switch strip shows both activities and a chip without one. The strip lists
// console sessions by when they started, so these start in the order they are named.
const hub4 = sessionOf("s-console-4", console_.id, undefined, "Release notes", "idle", { colour: "violet", started_at: minutesAgo(100) });
const stripState = snapshotState({
  consoles: [console_],
  projects: [project, other, third],
  sessions: [
    { ...hub1, started_at: minutesAgo(400) },
    { ...hub2, started_at: minutesAgo(300) },
    { ...hub3, started_at: minutesAgo(200) },
    hub4,
    sessionOf("t-1", console_.id, project.id, "Fix the rounding of partial results", "waiting_user", { bound_to: hub1.id }),
    sessionOf("t-2", console_.id, project.id, "Add idempotency keys", "idle", { bound_to: hub3.id }),
    sessionOf("t-3", console_.id, other.id, "Refresh the landing page", "working", { bound_to: hub3.id }),
    sessionOf("t-4", console_.id, other.id, "Check the dark theme", "working", { bound_to: hub1.id }),
    sessionOf("t-5", console_.id, third.id, "Proofread the guide", "idle", { bound_to: hub4.id }),
    sessionOf("t-6", console_.id, third.id, "An archived console session's session", "archived", { bound_to: hub4.id, ended_at: minutesAgo(60) }),
    sessionOf("s-console-old", console_.id, undefined, "Archived console session", "archived", { colour: "azure", ended_at: minutesAgo(60 * 24) }),
  ],
  settings,
});

// Many console sessions, some with long and CJK titles, so the strip has to scroll; started in the
// order they are listed here, which is the strip's order.
const crowd = [
  sessionOf("o-1", console_.id, undefined, "Website redesign", "working", { colour: "teal" }),
  sessionOf("o-2", console_.id, undefined, "A console session titled at a length no chip fits", "idle", { colour: "azure" }),
  sessionOf("o-3", console_.id, undefined, "负责网站改版的编排会话", "waiting_user", { colour: "olive" }),
  sessionOf("o-4", console_.id, undefined, "Release notes", "idle", { colour: "rose" }),
  sessionOf("o-5", console_.id, undefined, "Billing migration", "idle", { colour: "violet" }),
  sessionOf("o-6", console_.id, undefined, "Docs", "idle", { colour: "jade" }),
  sessionOf("o-7", console_.id, undefined, "Search tuning", "working", { colour: "teal" }),
].map((s, i) => ({ ...s, started_at: minutesAgo(700 - i * 10) }));

// A lead session of `project` with two sessions of its own, which the project's focus mode leaves
// to `hub1`'s and `hub1`'s nests under it, beside an unbound session with one of its own, which the
// project's focus mode does list.
const lead = sessionOf("s-lead", console_.id, project.id, "Rework the result ranking", "idle", { bound_to: hub1.id, account_id: "a-work" });
const teamState = snapshotState({
  consoles: [console_],
  projects: [project, other],
  sessions: [
    hub1,
    hub2,
    lead,
    sessionOf("s-lead-1", console_.id, project.id, "Chase the flaky test", "working", { bound_to: lead.id }),
    sessionOf("s-lead-2", console_.id, project.id, "Measure the first paint", "waiting_user", { bound_to: lead.id }),
    sessionOf("s-free", console_.id, project.id, "Write the migration", "idle"),
    sessionOf("s-free-1", console_.id, project.id, "Check the index sizes", "interrupted", { bound_to: "s-free" }),
    sessionOf("s-lead-archived", console_.id, project.id, "An archived lead session", "archived", { bound_to: hub1.id, ended_at: minutesAgo(90) }),
    sessionOf("s-lead-archived-1", console_.id, project.id, "A session of the archived lead session", "archived", { bound_to: "s-lead-archived", ended_at: minutesAgo(80) }),
    sessionOf("s-other-bound", console_.id, other.id, "Refresh the landing page", "idle", { bound_to: hub2.id }),
  ],
  settings,
});

export const focusScenarios: Scenario[] = [
  {
    id: "focus-project",
    group: GROUP,
    title: "Focus mode on a project",
    description:
      "Only the project's unbound sessions are listed. A sentence says how many others are bound to console " +
      "sessions, with a chip for each of those console sessions that leads to its focus mode; the archive " +
      "still holds bound sessions.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    state,
    steps: [(ui) => ui.press(ui.session("Write the migration"))],
  },
  {
    id: "focus-project-new-session",
    group: GROUP,
    title: "New session from a project's focus mode",
    description: "The dialog offers no owner choice although the console has console sessions: the session is unbound.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    state,
    steps: [(ui) => ui.press(ui.t("sidebar.project.openSession"))],
  },
  {
    id: "focus-console-session",
    group: GROUP,
    title: "Focus mode on a console session",
    description:
      "Only the projects with a session bound to the console session, and within each only those sessions, as " +
      "cards naming their agent and account. Its own archived bound sessions are below.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    state,
    steps: [(ui) => ui.press(ui.session("Add idempotency keys"))],
  },
  {
    id: "focus-console-session-strip",
    group: GROUP,
    title: "Focus mode on a console session, with the switch strip",
    description:
      "Under the header, one chip per console session of the console that is not archived, in the order " +
      "they started: Hub is waiting through a bound session, Hub 2 and Hub 3 (in focus, outlined) are working, " +
      "Release notes has nothing going on, so shows nothing. Pressing a chip enters that console session's focus mode and selects it; with " +
      "`chrome=1` in the URL, ⌃Tab and ⌃⇧Tab do the same going round.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub3.id}` },
    state: stripState,
    steps: [(ui) => ui.press(ui.session("Add idempotency keys"))],
  },
  {
    id: "focus-console-session-strip-overflow",
    group: GROUP,
    title: "The switch strip with more console sessions than fit",
    description:
      "One row that scrolls sideways under an edge fade, with no scrollbar; the current chip, the sixth (Docs), is " +
      "scrolled into view. A title too long for its chip fades out and is the chip's tooltip.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${crowd[5].id}` },
    state: snapshotState({ consoles: [console_], projects: [project], sessions: crowd, settings }),
  },
  {
    id: "focus-console-session-new-session",
    group: GROUP,
    title: "New session from a console session's focus mode",
    description: "The header's menu picks the project; the dialog shows the owner as a fixed line, not a field.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    state,
    steps: [(ui) => ui.press(ui.t("sidebar.focus.newSession")), (ui) => ui.press(third.name)],
  },
  {
    id: "focus-console-session-empty",
    group: GROUP,
    title: "Focus mode on a console session with nothing bound",
    description: "It is the console's only console session, so there is no switch strip.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub2.id}` },
    state: snapshotState({ consoles: [console_], projects: [project], sessions: [hub2], settings }),
  },
  {
    id: "focus-console-session-no-projects",
    group: GROUP,
    title: "Focus mode on a console session in a console with no projects",
    description: "The header's new-session button is disabled, and the view says why and offers to add a project.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub2.id}` },
    state: snapshotState({ consoles: [console_], sessions: [hub2], settings }),
  },
  {
    id: "focus-project-teams",
    group: GROUP,
    title: "A project's focus mode with teams",
    description:
      "An unbound session's own sessions are inset under its card. The lead session and the sessions under it are " +
      "not listed: they are counted in the line and the chip of the console session they report to. The archive " +
      "insets an archived session under its archived owner.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    state: teamState,
  },
  {
    id: "focus-console-session-teams",
    group: GROUP,
    title: "A console session's focus mode with a lead session",
    description:
      "The lead session is one of the console session's cards, with the sessions bound to it inset under it: the " +
      "console session sees the team and deals with the lead session alone. Its archive groups them the same way.",
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub1.id}` },
    state: teamState,
  },
  {
    id: "focus-project-long-names",
    group: GROUP,
    title: "A project's focus mode with long and CJK console session names, narrow",
    description:
      "The chips wrap onto further rows at the narrowest sidebar and cut a name longer than the row; the names " +
      "are a long Latin one and a CJK one.",
    preferences: { sidebarWidth: 200, sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` },
    state: snapshotState({
      consoles: [console_],
      projects: [project],
      sessions: [longHub, cjkHub, sessionOf("s-l1", console_.id, project.id, "Check the build", "idle", { bound_to: longHub.id }), sessionOf("s-l2", console_.id, project.id, "检查构建", "idle", { bound_to: cjkHub.id }), sessionOf("s-l3", console_.id, project.id, "Write the release notes", "idle")],
      settings,
    }),
  },
  {
    id: "focus-console-session-long-names",
    group: GROUP,
    title: "A console session's focus mode with a long title and a CJK project, narrow",
    description: "The pressable header name over the console name, the project group heading and the header's project menu.",
    preferences: { sidebarWidth: 200, sidebarConsole: console_.id, sidebarFocus: `consoleSession:${longHub.id}` },
    state: snapshotState({
      consoles: [consoleOf("c-1", "A console with a rather long name indeed")],
      projects: [cjkProject, projectOf("p-long", console_.id, "A project whose name is far too long for one row")],
      sessions: [longHub, sessionOf("s-l1", console_.id, cjkProject.id, "检查构建并更新依赖项", "idle", { bound_to: longHub.id }), sessionOf("s-l2", console_.id, "p-long", "Check the build", "working", { bound_to: longHub.id })],
      settings,
    }),
    steps: [(ui) => ui.press(ui.t("sidebar.focus.newSession")), (ui) => ui.press(cjkProject.name)],
  },
];
