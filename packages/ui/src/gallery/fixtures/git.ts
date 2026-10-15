import { type Ui } from "../interact";
import type { Scenario } from "../scenario";
import type { FixtureGit } from "./changeFixtures";
import { consoleOf, gitStatusOf, projectOf, sessionOf, snapshotState } from "./builders";
import { RESTORE_FILES, RESTORE_GIT, RESTORED_PROJECT } from "./restore";

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

// A long project name beside a long branch name with both counts non-zero, at the
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
const failureState = snapshotState({
  consoles: [console_],
  projects: [errorProject, unreadableProject],
  sessions: sessions.filter(({ project_id }) => project_id === errorProject.id || project_id === unreadableProject.id),
  gitStatuses: [errorStatus, unreadableStatus],
});

const normalProjects = [...projects, ...squeezeProjects].filter(({ id }) => id !== errorProject.id && id !== unreadableProject.id);
const badgesState = snapshotState({
  consoles: [console_],
  projects: normalProjects,
  sessions: [...sessions, ...squeezeSessions].filter(({ project_id }) => normalProjects.some(({ id }) => id === project_id)),
  gitStatuses: [...gitStatuses, ...squeezeStatuses].filter(({ project }) => normalProjects.some(({ id }) => id === project)),
});
const longFailureState = snapshotState({
  consoles: [console_], projects: [squeezeProject], sessions: [squeezeSession],
  gitStatuses: [{ ...squeezeStatus, error: "fetch: Could not resolve host 'origin'" }],
});
const restoreProject = projectOf("p-restore", console_.id, "Search API");
const restoreState = snapshotState({ consoles: [console_], projects: [restoreProject], gitStatuses: [gitStatusOf(restoreProject.id)] });
const restoreFixture = { state: restoreState, git: RESTORE_GIT, files: RESTORE_FILES };
const openRestore = [...openGitSubmenu(restoreProject.name), (ui: Ui) => ui.press(ui.t("sidebar.project.restoreChanges"))];

const switchBranch = "feature/search-ranking";
const switchGit: FixtureGit = {
  ...RESTORE_GIT,
  branches: [
    { name: "feature/a-long-local-branch-name-to-check-menu-width-and-overflow", commit: "def123456789" },
    { name: switchBranch, commit: RESTORE_GIT.worktrees[0].head! },
    { name: "main", commit: RESTORE_GIT.worktrees[0].head! },
    { name: "機能/日本語ブランチ", commit: "fed123456789" },
  ],
};
const switchFixture = {
  ...restoreFixture,
  git: switchGit,
  projectBranchSwitches: { [restoreProject.id]: { [switchBranch]: {
    git: { ...switchGit, worktrees: switchGit.worktrees.map((worktree) => ({ ...worktree, branch: switchBranch })) },
    files: RESTORE_FILES,
    status: gitStatusOf(restoreProject.id, { branch: switchBranch, upstream: null }),
  } } },
};
const openSwitch = [...openGitSubmenu(restoreProject.name), (ui: Ui) => ui.press(ui.t("sidebar.project.switchBranch")), (ui: Ui) => ui.wait(300)];

export const gitScenarios: Scenario[] = [
  {
    id: "git-badges-errors", group: GROUP, category: "exceptional",
    title: "Fetch and branch-read failures",
    description: "Deliberate fetch and HEAD errors replace the branch glyph with a warning. Hover warnings for their causes.",
    state: failureState, preferences: { sidebarWidth: 200 },
  },
  {
    id: "git-badge-focus-long-error", group: GROUP, category: "exceptional",
    title: "Long branch with a fetch failure in project focus",
    description: "The warning is deliberate: fetch cannot resolve origin. Hover it for the failure; long branch text and counts retain their marquee.",
    state: longFailureState,
    preferences: { sidebarWidth: 200, sidebarConsole: console_.id, sidebarFocus: `project:${squeezeProject.id}` },
  },
  {
    id: "git-project-menu",
    group: "Projects",
    category: "normal",
    title: "Project Git actions",
    description: "Restore file changes is separated from Sync and Push and marked as destructive. Selecting it opens a confirmation before changing files.",
    ...restoreFixture,
    projectRestores: { [restoreProject.id]: RESTORED_PROJECT },
    steps: openGitSubmenu(restoreProject.name),
  },
  {
    id: "git-switch-branch", group: "Projects", category: "normal",
    title: "Local branch menu",
    description: "The third menu lists local branches and checks the current one. Long Latin and Japanese branch names remain readable. Selecting feature/search-ranking switches immediately.",
    ...switchFixture,
    steps: openSwitch,
  },
  {
    id: "git-switch-branch-empty", group: "Projects", category: "normal",
    title: "No local branches",
    ...restoreFixture, git: { ...RESTORE_GIT, branches: [], worktrees: RESTORE_GIT.worktrees.map((worktree) => ({ ...worktree, head: null, branch: null })) },
    state: snapshotState({ consoles: [console_], projects: [restoreProject], gitStatuses: [gitStatusOf(restoreProject.id, { branch: null, upstream: null })] }),
    steps: openSwitch,
  },
  {
    id: "git-switch-branch-loading", group: "Projects", category: "normal",
    title: "Loading local branches",
    ...switchFixture,
    projectBranches: { [restoreProject.id]: "pending" },
    steps: openSwitch,
  },
  {
    id: "git-switch-branch-list-error", group: "Projects", category: "exceptional",
    title: "Local branches could not be read",
    description: "The third menu shows the listing error and a retry action.",
    ...switchFixture,
    projectBranches: { [restoreProject.id]: { code: "git_failed", params: { detail: "fatal: unable to read refs/heads" }, message: "Unable to list local branches" } },
    steps: openSwitch,
  },
  {
    id: "git-switch-branch-error", group: "Projects", category: "exceptional",
    title: "Branch switch failed",
    description: "A refused switch reports Git's reason in an error toast. The branch badge stays on main.",
    ...switchFixture,
    projectBranchSwitches: { [restoreProject.id]: { [switchBranch]: { code: "git_switch_failed", params: { detail: "error: Your local changes to README.md would be overwritten by checkout. Please commit your changes or stash them before you switch branches." }, message: "Git switch failed" } } },
    steps: [...openSwitch, (ui) => ui.press(switchBranch), (ui) => ui.wait(300)],
  },
  {
    id: "git-restore-confirm",
    group: "Projects",
    category: "normal",
    title: "Restore file changes confirmation",
    description: "The named project's warning distinguishes tracked edits and staged new files from untracked and ignored files. Cancel leaves the project intact; confirming restores the fixture's tracked file and retains its notes and ignored cache.",
    ...restoreFixture,
    projectRestores: { [restoreProject.id]: RESTORED_PROJECT },
    steps: openRestore,
  },
  {
    id: "git-restore-error",
    group: "Projects",
    category: "exceptional",
    title: "Restore file changes failed",
    description: "A failed restore stays in the confirmation with an inline error and enabled retry and cancel controls.",
    ...restoreFixture,
    projectRestores: { [restoreProject.id]: { code: "git_restore_failed", params: { detail: "fatal: Unable to create .git/index.lock: File exists" }, message: "Git restore failed" } },
    steps: [...openRestore, (ui) => ui.confirm(), (ui) => ui.wait(300)],
  },
  {
    id: "git-badges",
    group: GROUP,
    category: "normal",
    title: "Branch badges",
    state: badgesState,
    preferences: { sidebarWidth: 200 },
    description: "Remote counts, in-flight work, detached and missing branches, with long Latin and CJK labels. Hover a clipped Latin or CJK project name or branch to scroll only that text; counts move with their branch. Hovering row controls leaves both labels still. Hover badges for their full details; drag the sidebar wider to inspect clipping.",
  },
  {
    id: "git-badge-focus",
    group: GROUP,
    category: "exceptional",
    title: "Unreadable branch in project focus",
    state: failureState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${unreadableProject.id}` },
    description: "The focus header has only a warning glyph; hover it for the unreadable-branch reason.",
  },
  {
    id: "git-badge-focus-long",
    group: GROUP,
    category: "normal",
    title: "Long branch in project focus",
    state: badgesState,
    preferences: { sidebarWidth: 200, sidebarConsole: console_.id, sidebarFocus: `project:${squeezeProject.id}` },
    description: "The long project name and branch fade separately. Hover either text to run only its marquee; hovering the console subtitle or header controls leaves both still. The branch and counts scroll together; hover the branch text for full details.",
  },
  {
    id: "git-push-accepted",
    group: GROUP,
    category: "normal",
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
