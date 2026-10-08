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
