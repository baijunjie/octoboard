import { type Ui } from "../interact";
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

const openProjectMenu = (name: string) => (ui: Ui) => ui.press(ui.t("sidebar.project.actions", { name }));

/** Enter a project's focus header from the same sidebar setup. */
const focusProject = (name: string) => [openProjectMenu(name), (ui: Ui) => ui.press((name) => name.startsWith(ui.t("sidebar.focus.enter"))), (ui: Ui) => ui.wait(300)];

const openGitSubmenu = (name: string) => [
  openProjectMenu(name),
  (ui: Ui) => ui.press(ui.t("sidebar.project.git")),
  (ui: Ui) => ui.wait(300),
];
const badgesState = snapshotState({
  consoles: [console_],
  projects: [...projects, ...squeezeProjects],
  sessions: [...sessions, ...squeezeSessions],
  gitStatuses: [...gitStatuses, ...squeezeStatuses],
});

export const gitScenarios: Scenario[] = [
  {
    id: "git-badges",
    group: GROUP,
    title: "Branch badges",
    description:
      "Branches include remote counts, checking and fast-forward in flight, failed fetches, detached HEAD, no repository, an empty repository, unreadable branches and the first check. Hover badges for the full branch, counts and error. Open each project's menu for repository-only Git actions; enter project focus mode for the header badge and its tooltip. Long Latin, CJK and large-count branches share the same list; the sidebar starts at 200, so drag wider to see the text and counts scroll together. Even with upstream → Git → Push current branch raises its acceptance notice.",
    state: badgesState,
    preferences: { sidebarWidth: 200 },
    steps: [
      (ui) => ui.hoverGitBadge(errorStatus, "line", (name) => name.startsWith(errorProject.name)),
      ...focusProject(unreadableProject.name),
      (ui) => ui.hoverGitBadge(unreadableStatus, "glyph"),
      (ui) => ui.press(ui.t("sidebar.focus.exit")),
      (ui) => ui.hoverGitBadge(squeezeStatus, "glyph", (name) => name.startsWith(squeezeProject.name)),
      ...focusProject(squeezeProject.name),
      (ui) => ui.hoverGitBadge(squeezeStatus, "line"),
      (ui) => ui.press(ui.t("sidebar.focus.exit")),
      ...openGitSubmenu(evenProject.name),
      (ui) => ui.press(ui.t("sidebar.project.pushBranch")),
      (ui) => ui.wait(300),
    ],
  },
];
