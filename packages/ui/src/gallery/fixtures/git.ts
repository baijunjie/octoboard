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
const pullingProject = projectOf("p-pulling", console_.id, "Pulling now");
const errorProject = projectOf("p-error", console_.id, "Fetch failed");
const detachedProject = projectOf("p-detached", console_.id, "Detached HEAD");
const noRepoProject = projectOf("p-no-repo", console_.id, "Not a git repository");
const noCommitsProject = projectOf("p-no-commits", console_.id, "Empty repository");
const unreadableProject = projectOf("p-unreadable", console_.id, "Branch unreadable");
const firstCheckProject = projectOf("p-first-check", console_.id, "First check");
const retryingProject = projectOf("p-retrying", console_.id, "Retrying after a failure");

const projects = [
  evenProject,
  aheadBehindProject,
  checkingProject,
  pullingProject,
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
  gitStatusOf(pullingProject.id, { activity: "pulling", behind: 2 }),
  errorStatus,
  gitStatusOf(detachedProject.id, { branch: "a1b2c3d", detached: true, upstream: null }),
  gitStatusOf(noRepoProject.id, { repository: false, branch: null, upstream: null }),
  gitStatusOf(noCommitsProject.id, { branch: null, upstream: null }),
  unreadableStatus,
  gitStatusOf(firstCheckProject.id, { branch: null, upstream: null, activity: "checking" }),
];

// A long project name beside a long branch name with both counts non-zero, at the
// sidebar's minimum docked width (`layout/paneWidth.ts`'s `PANES.sidebar.min`): the worst case for
// the badge-versus-name squeeze and the branch-only fade and marquee. Branches keep their
// beginning on the left in an RTL interface; the short branch verifies that large counts remain
// visible when no branch text fits. Long branches have a tail only the marquee and the tooltip
// reach. The first long branch is pushing, keeping
// its upward activity spinner distinct from the scrolling ahead-count arrow in the row and focus header.
const squeezeProject = projectOf(
  "p-squeeze",
  console_.id,
  "A project with a genuinely long name that does not fit a narrow sidebar",
);
const squeezeSession = sessionOf("s-squeeze", console_.id, squeezeProject.id, "Working on it", "idle");
const squeezeStatus = gitStatusOf(squeezeProject.id, {
  branch: "feature/ABC-1234-a-genuinely-long-branch-name-a-branch-title-that-will-not-fit-in-a-narrow-sidebar",
  activity: "pushing",
  ahead: 12,
  behind: 345,
});
const regionalProject = projectOf("p-regional", console_.id, "Regional deployment coordination with a project name too long for the sidebar");
const regionalStatus = gitStatusOf(regionalProject.id, {
  branch: "feature/regional-deployment-status-and-tooltip-wrapping-verification",
  ahead: 12,
  behind: 345,
});
const countsProject = projectOf("p-counts", console_.id, "Counts beside a clipped branch");
const countsStatus = gitStatusOf(countsProject.id, { branch: "main", ahead: 12345, behind: 67890 });
const squeezeProjects = [squeezeProject, regionalProject, countsProject];
const squeezeSessions = [
  squeezeSession,
  sessionOf("s-regional", console_.id, regionalProject.id, "Working on it", "idle"),
  sessionOf("s-counts", console_.id, countsProject.id, "Working on it", "idle"),
];
const squeezeStatuses = [squeezeStatus, regionalStatus, countsStatus];

// Short branches and a wider sidebar keep both count arrows visible throughout a transfer.
const pushPreview = projectOf("p-transfer-push", console_.id, "Pushing");
const pullPreview = projectOf("p-transfer-pull", console_.id, "Pulling");
const noCountsPreview = projectOf("p-transfer-no-counts", console_.id, "Pushing with no counts");
const transferState = snapshotState({
  consoles: [console_],
  projects: [pushPreview, pullPreview, noCountsPreview],
  gitStatuses: [
    gitStatusOf(pushPreview.id, { branch: "main", activity: "pushing", ahead: 3, behind: 2 }),
    gitStatusOf(pullPreview.id, { branch: "main", activity: "pulling", behind: 2 }),
    gitStatusOf(noCountsPreview.id, { branch: "main", activity: "pushing" }),
  ],
});

const openProjectMenu = (name: string) => (ui: Ui) => ui.press(ui.t("sidebar.project.actions", { name }));

const openGitSubmenu = (name: string) => [
  openProjectMenu(name),
  (ui: Ui) => ui.press(ui.t("sidebar.project.git")),
  (ui: Ui) => ui.wait(300),
];
// The failure mark gives way to the spinning glyph while the next check runs; the error stays in the status.
const retryingStatus = gitStatusOf(retryingProject.id, { activity: "checking", error: "fetch: Could not resolve host 'origin'" });
const failureProjects = [errorProject, unreadableProject, retryingProject];
const failureState = snapshotState({
  consoles: [console_],
  projects: failureProjects,
  sessions: [
    ...sessions.filter(({ project_id }) => project_id === errorProject.id || project_id === unreadableProject.id),
    sessionOf("s-retrying", console_.id, retryingProject.id, "Working on it", "idle"),
  ],
  gitStatuses: [errorStatus, unreadableStatus, retryingStatus],
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
  gitStatuses: [{ ...squeezeStatus, activity: "idle", error: "fetch: Could not resolve host 'origin'" }],
});
const restoreProject = projectOf("p-restore", console_.id, "Search API مرحبا 東京 UnbrokenProjectNameForRestoreConfirmationWrapping");
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
    { name: "feature/regional-deployment", commit: "fed123456789" },
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
  ...[
    { id: "github", host: "GitHub", remote: "https://github.com/octoboard/search-api" },
    { id: "gitlab", host: "GitLab", remote: "https://gitlab.com/octoboard/search-api" },
    { id: "bitbucket", host: "Bitbucket", remote: "https://bitbucket.org/octoboard/search-api" },
    { id: "custom-host", host: "code.example.org", remote: "https://code.example.org/octoboard/search-api" },
  ].map(({ id, host, remote }): Scenario => ({
    id: `project-open-in-${id}`,
    group: "Projects",
    category: "normal",
    title: `Open a project in ${host}`,
    description: `Open in groups File manager, Terminal and ${host}. The repository destination uses ${id === "custom-host" ? "the hostname and a globe icon" : `the ${host} brand icon`}.`,
    state: snapshotState({
      consoles: [console_], projects: [{ ...restoreProject, remote_url: remote }],
      gitStatuses: [gitStatusOf(restoreProject.id)],
    }),
    steps: [openProjectMenu(restoreProject.name), (ui) => ui.press(ui.t("common.openIn"))],
  })),
  {
    id: "git-transfer-badges", group: GROUP, category: "normal",
    title: "Push and pull count indicators",
    description: "Each transfer keeps a spinning loader on the left. Pushing scrolls only ↑ beside 3 upward, with the opposite ↓ arrow and both numbers still; pulling shows only ↓2, because it has no commits ahead of upstream, and scrolls that arrow downward. The zero-count push shows only the loader and branch.",
    state: transferState,
    preferences: { sidebarWidth: 480 },
  },
  {
    id: "git-pull-focus", group: GROUP, category: "normal",
    title: "Pulling in project focus",
    description: "The focus header keeps the left loader spinning while only the ↓ count arrow scrolls downward; the zero ahead count is omitted.",
    state: transferState,
    preferences: { sidebarWidth: 480, sidebarConsole: console_.id, sidebarFocus: `project:${pullPreview.id}` },
  },
  {
    id: "git-badges-errors", group: GROUP, category: "exceptional",
    title: "Fetch and branch-read failures",
    description: "A deliberate fetch error and a HEAD read error each put a danger-coloured circled \"!\" named Git check failed at the start of the badge, in place of the branch glyph. It is a button of its own: pressing it opens a popover with the reason, the remedy and then Git's own text in a block of its own, held open here for the unreadable branch (the top row). The badge's tooltip (hover the branch or counts) covers only the branch and counts and never mentions the failure; the unreadable-branch row has no branch or counts, so only the mark stands there with no tooltip. The third row's next check is already running after a failure: the spinning glyph takes the mark's place, with no mark and no failure in the row's name, and the mark returns only if that run fails again.",
    state: failureState, preferences: { sidebarWidth: 280 },
    steps: [(ui) => ui.openGitFailure((name) => name.startsWith(unreadableProject.name))],
  },
  {
    id: "git-badge-focus-long-error", group: GROUP, category: "exceptional",
    title: "Long branch with a fetch failure in project focus",
    description: "The failure is deliberate: fetch cannot resolve origin. Press the danger-coloured circled \"!\" named Git check failed at the start of the badge for its popover (reason, remedy, then Git's own text); only the long branch fades and scrolls, while the mark and counts stay visible. Hovering the branch or counts shows the badge's tooltip, which does not mention the failure.",
    state: longFailureState,
    preferences: { sidebarWidth: 280, sidebarConsole: console_.id, sidebarFocus: `project:${squeezeProject.id}` },
  },
  {
    id: "git-project-menu",
    group: "Projects",
    category: "normal",
    title: "Project Git actions",
    description: "Restore file changes is separated from Pull and Push and marked as destructive. Selecting it opens a confirmation before changing files.",
    ...restoreFixture,
    projectRestores: { [restoreProject.id]: RESTORED_PROJECT },
    steps: openGitSubmenu(restoreProject.name),
  },
  {
    id: "git-switch-branch", group: "Projects", category: "normal",
    title: "Local branch menu",
    description: "The third menu lists local branches and checks the current one. Long branch names remain readable. Selecting feature/search-ranking switches immediately.",
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
    description: "The third menu is replaced by a danger Alert in the popover holding the title Could not load the items, Git's own sentence for the failure, a Details button for Git's diagnostic and Try again at the trailing edge, with no menu items.",
    ...switchFixture,
    projectBranches: { [restoreProject.id]: { code: "git_failed", params: { detail: "fatal: unable to read refs/heads" }, message: "Unable to list local branches" } },
    steps: openSwitch,
  },
  {
    id: "git-switch-branch-cut-short", group: "Projects", category: "exceptional",
    title: "Local branches, the list cut off",
    description: "The branch menu lists the four branches the daemon returned, then a last item that is a muted note with no glyph, its text lined up with the branch labels: \"There are more branches than this list can show. The others are not listed.\" It cannot be pressed, though the arrow keys stop on it.",
    ...switchFixture,
    git: { ...switchGit, branchesCut: true },
    steps: openSwitch,
  },
  {
    id: "git-switch-branch-error", group: "Projects", category: "exceptional",
    title: "Branch switch failed",
    description: "A refused switch is reported in the error alert dialog, which requires OK: it says only that Git could not switch branches and what to do, and Git's own message sits only in the Error details disclosure, closed here. The branch badge stays on main.",
    ...switchFixture,
    projectBranchSwitches: { [restoreProject.id]: { [switchBranch]: { code: "git_switch_failed", params: { detail: "error: Your local changes to README.md would be overwritten by checkout. Please commit your changes or stash them before you switch branches." }, message: "Git switch failed" } } },
    steps: [...openSwitch, (ui) => ui.press(switchBranch), (ui) => ui.wait(300)],
  },
  {
    id: "git-restore-confirm",
    group: "Projects",
    category: "normal",
    title: "Restore file changes confirmation",
    description: "The title is the plain Restore file changes and the first sentence names the project as a chip; the message distinguishes tracked edits and staged new files from untracked and ignored files. Cancel leaves the project intact; confirming restores the fixture's tracked file and retains its notes and ignored cache.",
    ...restoreFixture,
    projectRestores: { [restoreProject.id]: RESTORED_PROJECT },
    steps: openRestore,
  },
  {
    id: "git-restore-error",
    group: "Projects",
    category: "exceptional",
    title: "Restore file changes failed",
    description: "A failed restore is the error alert dialog's, raised over the confirmation; it says only that Git could not restore the file changes and what to do, with Git's own message only in its Error details disclosure, closed here. Nothing is written into the confirmation, which stays behind it with retry and cancel enabled once OK is pressed.",
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
    preferences: { sidebarWidth: 280 },
    description: "Remote counts, spinning checks and transfers, detached and missing branches, and long labels. Hover a clipped project name or branch to scroll only that text, while state glyphs, arrows and counts stay visible; hovering row controls leaves both labels still. Hover badges for their full details, and drag the sidebar wider to inspect clipping.",
  },
  {
    id: "git-badge-focus",
    group: GROUP,
    category: "exceptional",
    title: "Unreadable branch in project focus",
    state: failureState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${unreadableProject.id}` },
    description: "The focus header has only the danger-coloured failure mark named Git check failed, with no branch, counts or tooltip; press it for the reason, the remedy and then Git's own text about the unreadable branch.",
  },
  {
    id: "git-badge-focus-long",
    group: GROUP,
    category: "normal",
    title: "Long branch pushing in project focus",
    state: badgesState,
    preferences: { sidebarWidth: 280, sidebarConsole: console_.id, sidebarFocus: `project:${squeezeProject.id}` },
    description: "The left loading glyph spins during pushing, and the always-visible ahead-count arrow scrolls upward; hover the glyph for the pushing status. The long project name and branch fade separately and run their own marquees on hover, while the glyph and both counts stay visible; hovering the console subtitle or header controls leaves both still.",
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
