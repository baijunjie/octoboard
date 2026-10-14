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
// Latin branch has a tail only the marquee and the tooltip reach; the Japanese branch exercises
// wide characters and tooltip wrapping.
const squeezeProject = projectOf(
  "p-squeeze",
  console_.id,
  "A project with a genuinely long name that does not fit a narrow sidebar",
);
const squeezeSession = sessionOf("s-squeeze", console_.id, squeezeProject.id, "Working on it", "idle");
const squeezeStatus = gitStatusOf(squeezeProject.id, {
  branch: "feature/ABC-1234-a-genuinely-long-branch-name-a-branch-title-that-will-not-fit-in-a-narrow-sidebar",
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
    state: badgesState,
    preferences: { sidebarWidth: 200 },
    description: "Remote counts, in-flight work, failures, detached and missing branches, with long Latin and CJK labels. Hover badges for their full details; drag the sidebar wider to inspect clipping.",
  },
  {
    id: "git-badge-focus",
    group: GROUP,
    title: "Unreadable branch in project focus",
    state: badgesState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${unreadableProject.id}` },
    description: "The focus header has only a warning glyph; hover it for the unreadable-branch reason.",
  },
  {
    id: "git-badge-focus-long",
    group: GROUP,
    title: "Long branch in project focus",
    state: badgesState,
    preferences: { sidebarWidth: 200, sidebarConsole: console_.id, sidebarFocus: `project:${squeezeProject.id}` },
    description: "The long branch and counts share the header fade; hover its text or glyph for full details.",
  },
  {
    id: "git-push-accepted",
    group: GROUP,
    title: "Push accepted",
    state: badgesState,
    description: "The acceptance notice appears after pushing the current branch.",
    steps: [
      ...openGitSubmenu(evenProject.name),
      (ui) => ui.press(ui.t("sidebar.project.pushBranch")),
      (ui) => ui.wait(300),
    ],
  },
];
