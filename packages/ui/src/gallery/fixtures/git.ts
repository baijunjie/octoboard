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

const projects = [
  evenProject,
  aheadBehindProject,
  checkingProject,
  syncingProject,
  errorProject,
  detachedProject,
  noRepoProject,
  noCommitsProject,
];

const sessions = projects.map((project, i) => sessionOf(`s-${i}`, console_.id, project.id, "Working on it", "idle"));

const gitStatuses = [
  gitStatusOf(evenProject.id),
  gitStatusOf(aheadBehindProject.id, { ahead: 3, behind: 2 }),
  gitStatusOf(checkingProject.id, { activity: "checking" }),
  gitStatusOf(syncingProject.id, { activity: "syncing", behind: 2 }),
  gitStatusOf(errorProject.id, { behind: 1, error: "fetch: Could not resolve host 'origin'" }),
  gitStatusOf(detachedProject.id, { branch: "a1b2c3d", detached: true, upstream: null }),
  gitStatusOf(noRepoProject.id, { repository: false, branch: null, upstream: null }),
  gitStatusOf(noCommitsProject.id, { branch: null, upstream: null }),
];

// A long project name beside a long branch name with both counts non-zero and an error, at the
// sidebar's minimum docked width (`layout/paneWidth.ts`'s `PANES.sidebar.min`): the worst case for
// the badge-versus-name squeeze (see the doc comment at `GitBadge.tsx`'s `min-w-0`) and for the
// branch-and-count line's shared fade and marquee. The CJK branch also keeps its beginning on
// the left in an RTL interface; the short branch exposes a fade falling within the counts.
const squeezeProject = projectOf(
  "p-squeeze",
  console_.id,
  "A project with a genuinely long name that does not fit a narrow sidebar",
);
const squeezeSession = sessionOf("s-squeeze", console_.id, squeezeProject.id, "Working on it", "idle");
const squeezeStatus = gitStatusOf(squeezeProject.id, {
  branch: "feature/ABC-1234-a-genuinely-long-branch-name-that-does-not-fit-either",
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

export const gitScenarios: Scenario[] = [
  {
    id: "git-badges",
    group: GROUP,
    title: "Branch badges",
    description:
      "Even with the remote, ahead and behind, a check and a fast-forward in flight, a fetch that failed, a " +
      "detached HEAD, a directory that is not a git repository, and an empty repository with no commit yet.",
    state: snapshotState({ consoles: [console_], projects, sessions, gitStatuses }),
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
    state: snapshotState({
      consoles: [console_],
      projects: squeezeProjects,
      sessions: squeezeSessions,
      gitStatuses: squeezeStatuses,
    }),
  },
  {
    id: "git-badge-squeeze-focus",
    group: GROUP,
    title: "Branch badge at the sidebar's minimum width, in focus mode",
    description:
      "The Japanese branch starts on the left in the focus header, including in Arabic. Hover the badge to read " +
      "the branch and both counts as one scrolling line. Drag the sidebar wider to move the fading edge through " +
      "the branch and counts; the activity glyph stays visible.",
    preferences: { sidebarWidth: 200, sidebarConsole: console_.id, sidebarFocus: `project:${cjkProject.id}` },
    state: snapshotState({
      consoles: [console_],
      projects: squeezeProjects,
      sessions: squeezeSessions,
      gitStatuses: squeezeStatuses,
    }),
  },
];
