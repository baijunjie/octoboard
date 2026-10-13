import { describing, type Matcher, type Ui } from "../interact";
import type { Scenario } from "../scenario";
import { consoleOf, gitStatusOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Branch badges";

const console_ = consoleOf("c-1", "Main");

const evenProject = projectOf("p-even", console_.id, "Even with upstream");
const aheadBehindProject = projectOf("p-ahead-behind", console_.id, "Ahead and behind");
const checkingProject = projectOf("p-checking", console_.id, "Checking now");
const syncingProject = projectOf("p-syncing", console_.id, "Fast-forwarding now");
const errorProject = projectOf("p-error", console_.id, "Fetch failed");
const detachedProject = projectOf("p-detached", console_.id, "Detached HEAD");
const noRepoProject = projectOf("p-no-repo", console_.id, "Not a git repository");
const noCommitsProject = projectOf("p-no-commits", console_.id, "Empty repository");
const unreadableProject = projectOf("p-unreadable", console_.id, "Branch unreadable");
const firstCheckProject = projectOf("p-first-check", console_.id, "First check");

const projects = [
  evenProject,
  aheadBehindProject,
  checkingProject,
  syncingProject,
  errorProject,
  detachedProject,
  noRepoProject,
  noCommitsProject,
  unreadableProject,
  firstCheckProject,
];

const sessions = projects.map((project, i) => sessionOf(`s-${i}`, console_.id, project.id, "Working on it", "idle"));

const errorStatus = gitStatusOf(errorProject.id, { ahead: 2, behind: 1, error: "fetch: Could not resolve host 'origin'" });
const unreadableStatus = gitStatusOf(unreadableProject.id, { branch: null, upstream: null, error: "HEAD: unable to read the current branch" });

const gitStatuses = [
  gitStatusOf(evenProject.id),
  gitStatusOf(aheadBehindProject.id, { ahead: 3, behind: 2 }),
  gitStatusOf(checkingProject.id, { activity: "checking" }),
  gitStatusOf(syncingProject.id, { activity: "syncing", behind: 2 }),
  errorStatus,
  gitStatusOf(detachedProject.id, { branch: "a1b2c3d", detached: true, upstream: null }),
  gitStatusOf(noRepoProject.id, { repository: false, branch: null, upstream: null }),
  gitStatusOf(noCommitsProject.id, { branch: null, upstream: null }),
  unreadableStatus,
  gitStatusOf(firstCheckProject.id, { branch: null, upstream: null, activity: "checking" }),
];

// The sidebar's project row named for `project`: the row's name begins with the project's own.
const rowOf = (project: { name: string }): Matcher => describing(`projectRow(${project.name})`, (name) => name.startsWith(project.name));

// A long project name beside a long branch name with both counts non-zero and an error, at the
// sidebar's minimum docked width (`layout/paneWidth.ts`'s `PANES.sidebar.min`): the worst case for
// the badge-versus-name squeeze (see the doc comment at `GitBadge.tsx`'s `min-w-0`) and for the
// branch-and-count line's shared fade and marquee. The CJK branch also keeps its beginning on
// the left in an RTL interface; the short branch exposes a fade falling within the counts. The long
// branch ends in CJK, whose tail only the marquee and the tooltip reach, and the tooltip's sentence
// wraps with that mixed text in it.
const squeezeProject = projectOf(
  "p-squeeze",
  console_.id,
  "A project with a genuinely long name that does not fit a narrow sidebar",
);
const squeezeSession = sessionOf("s-squeeze", console_.id, squeezeProject.id, "Working on it", "idle");
const squeezeStatus = gitStatusOf(squeezeProject.id, {
  branch: "feature/ABC-1234-a-genuinely-long-branch-name-支持多语言的超长分支名称不会在窄栏里完整显示",
  ahead: 12,
  behind: 345,
  error: "fetch: Could not resolve host 'origin'",
});
const cjkProject = projectOf("p-cjk", console_.id, "日本語のプロジェクト名も長くなってサイドバーに収まらない");
const cjkStatus = gitStatusOf(cjkProject.id, {
  branch: "機能/日本語ブランチの表示確認",
  ahead: 12,
  behind: 345,
});
const countsProject = projectOf("p-counts", console_.id, "Counts at the fade");
const countsStatus = gitStatusOf(countsProject.id, { branch: "main", ahead: 12345, behind: 67890 });
const squeezeProjects = [squeezeProject, cjkProject, countsProject];
const squeezeSessions = [
  squeezeSession,
  sessionOf("s-cjk", console_.id, cjkProject.id, "Working on it", "idle"),
  sessionOf("s-counts", console_.id, countsProject.id, "Working on it", "idle"),
];
const squeezeStatuses = [squeezeStatus, cjkStatus, countsStatus];

/** One project, so the open menu is the subject rather than one row of the badge list. */
function projectMenuState(project: ReturnType<typeof projectOf>, status: ReturnType<typeof gitStatusOf>) {
  return snapshotState({
    consoles: [console_],
    projects: [project],
    sessions: [sessionOf(`s-menu-${project.id}`, console_.id, project.id, "Working on it", "idle")],
    gitStatuses: [status],
  });
}

const openProjectMenu = (name: string) => (ui: Ui) => ui.press(ui.t("sidebar.project.actions", { name }));

const openGitSubmenu = (name: string) => [
  openProjectMenu(name),
  (ui: Ui) => ui.press(ui.t("sidebar.project.git")),
  (ui: Ui) => ui.wait(300),
];
const badgesState = snapshotState({ consoles: [console_], projects, sessions, gitStatuses });
const squeezeState = snapshotState({
  consoles: [console_],
  projects: squeezeProjects,
  sessions: squeezeSessions,
  gitStatuses: squeezeStatuses,
});
const focusOn = (project: { id: string }) => ({ sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` }) as const;

export const gitScenarios: Scenario[] = [
  {
    id: "git-badges",
    group: GROUP,
    title: "Branch badges",
    description:
      "Even with the remote, ahead and behind, a check and a fast-forward in flight, a fetch that failed, a " +
      "detached HEAD, a directory that is not a git repository, an empty repository with no commit yet, a branch " +
      "that could not be read, and a first check that has found nothing yet. The last two are a glyph alone. " +
      "Rest the pointer on any badge to read its tooltip; the empty repository's says so in text and has none.",
    state: badgesState,
  },
  {
    id: "git-badge-tooltip",
    group: GROUP,
    title: "Branch badge's tooltip",
    description:
      "The pointer rests on the branch and counts of the badge whose last fetch failed: one tooltip says the " +
      "branch, both counts and the error together.",
    state: snapshotState({
      consoles: [console_],
      projects: [errorProject],
      sessions: [sessionOf("s-error", console_.id, errorProject.id, "Working on it", "idle")],
      gitStatuses: [errorStatus],
    }),
    steps: [(ui) => ui.hoverGitBadge(errorStatus, "line", rowOf(errorProject))],
  },
  {
    id: "git-badge-focus-tooltip",
    group: GROUP,
    title: "Branch badge's tooltip in focus mode",
    description:
      "The header's badge for a branch that could not be read is the warning glyph alone, and the pointer rests " +
      "on it: its tooltip is the only place the reason is written.",
    preferences: focusOn(unreadableProject),
    state: badgesState,
    steps: [(ui) => ui.hoverGitBadge(unreadableStatus, "glyph")],
  },
  {
    id: "git-badge-squeeze-sidebar",
    group: GROUP,
    title: "Branch badge at the sidebar's minimum width",
    description:
      "Latin and Japanese branches keep their beginnings visible and fade toward the right at 200px, even in Arabic. " +
      "Hover each row: branch and counts scroll together, while the activity glyph stays visible. " +
      "The short main branch puts its large counts at the fading edge; drag the sidebar wider to reveal the shared line.",
    preferences: { sidebarWidth: 200 },
    state: squeezeState,
  },
  {
    id: "git-badge-squeeze-sidebar-tooltip",
    group: GROUP,
    title: "Branch badge's tooltip at the sidebar's minimum width",
    description:
      "The pointer rests on the glyph of the long branch with a failed fetch, whose text has given way: the " +
      "tooltip still reads the whole branch, both counts and the error, wrapped.",
    preferences: { sidebarWidth: 200 },
    state: squeezeState,
    steps: [(ui) => ui.hoverGitBadge(squeezeStatus, "glyph", rowOf(squeezeProject))],
  },
  {
    id: "git-badge-squeeze-focus",
    group: GROUP,
    title: "Branch badge at the sidebar's minimum width, in focus mode",
    description:
      "The Japanese branch starts on the left in the focus header, including in Arabic. Hover the badge to read " +
      "the branch and both counts as one scrolling line. Drag the sidebar wider to move the fading edge through " +
      "the branch and counts; the activity glyph stays visible.",
    preferences: { sidebarWidth: 200, ...focusOn(cjkProject) },
    state: squeezeState,
  },
  {
    id: "git-badge-squeeze-focus-tooltip",
    group: GROUP,
    title: "Branch badge's tooltip at the sidebar's minimum width, in focus mode",
    description:
      "The pointer rests on the cut text of the long branch with a failed fetch in the header, which is narrower " +
      "than a row: the tooltip hangs over the project's name and still reads the whole branch, both counts and " +
      "the error.",
    preferences: { sidebarWidth: 200, ...focusOn(squeezeProject) },
    state: squeezeState,
    steps: [(ui) => ui.hoverGitBadge(squeezeStatus, "line")],
  },
  {
    id: "git-project-menu",
    group: GROUP,
    title: "Project menu on a git repository",
    description: "Open terminal, then Git, which opens onto Sync repository and Push current branch.",
    state: projectMenuState(evenProject, gitStatusOf(evenProject.id)),
    steps: openGitSubmenu(evenProject.name),
  },
  {
    id: "git-project-menu-not-repository",
    group: GROUP,
    title: "Project menu where the project is not a repository",
    description:
      "Open terminal is offered and Git is not. A project that has not reported a status yet, and one that is a subdirectory of a repository, show this same menu.",
    state: projectMenuState(noRepoProject, gitStatusOf(noRepoProject.id, { repository: false, branch: null, upstream: null })),
    steps: [openProjectMenu(noRepoProject.name), (ui) => ui.wait(300)],
  },
  {
    id: "git-push-accepted",
    group: GROUP,
    title: "Push of the current branch accepted",
    description: "The notice the push raises once it is accepted: Pushed the current branch.",
    state: projectMenuState(evenProject, gitStatusOf(evenProject.id)),
    steps: [...openGitSubmenu(evenProject.name), (ui) => ui.press(ui.t("sidebar.project.pushBranch")), (ui) => ui.wait(300)],
  },
];
