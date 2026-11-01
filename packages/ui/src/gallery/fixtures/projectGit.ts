import { GIT_STDERR } from "./structuredErrors";
// The gallery's "Project pane: Git" scenarios over the sample repository the fixture daemon answers a
// project's Git mode with (`sampleGit.ts`): the branch selector, the change list in its sections, whole or
// by commit, a diff of each kind opened from it, and the states a source or a list can end in. Every change
// here is built by the factory for its kind (`changeFixtures.ts`).
import type { BranchInfo, WorktreeInfo } from "../../protocol";
import { describing, type Ui } from "../../fixtures/interact";
import type { Preferences } from "../../fixtures/preferences";
import type { Scenario } from "../scenario";
import type { FixtureChange, FixtureCommit, FixtureGit, Versions } from "../../fixtures/changeFixtures";
import type { FixtureError } from "../../fixtures/sampleFiles";
import { BRANCHES, FEATURE_TIP, MAIN_TIP, SAMPLE_GIT, SERVER_ON_DISK, WORKTREES, at, index, live, onFeature, onMain } from "../../fixtures/sampleGit";
import { SAMPLE, sessionOf, snapshotState } from "../../fixtures/builders";
import { addition, textChange, untracked } from "../../fixtures/changeFixtures";

const GROUP = "Project pane: Git";

/** Long enough for the branch list, which the app reads again every ten seconds, to be read again. */
const BRANCH_LIST_REREAD_MS = 10_500;

/** A repository `git init` just made, a file staged and another not yet added: its worktree is on
 * `main`, which has no commit yet, so the branch list is empty and the staged file is compared with
 * the empty tree. */
const FRESH_GIT: FixtureGit = {
  worktrees: [{ id: "wt-fresh", root: "/Users/dev/code/search-api", main: true, head: null, branch: "main", scope_present: true }],
  changes: {
    "wt-fresh": [
      addition("staged", "README.md", index("e5f6a7b", "wt-fresh"), "# Search API\n"),
      untracked("src/index.ts", live("v1"), "export const buildIndex = () => new Map();\n"),
    ],
  },
  branches: [],
};
/** The tip of a branch that shares no commit with the others, as a `gh-pages` branch made from an empty tree. */
const ORPHAN_TIP = "3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b";

/** `main` has two commits `origin/main` does not: `UPSTREAM_TIP` is where the remote-tracking branch
 * is, the commit they forked at. Where `main` has never been pushed, `UPSTREAM_TIP` is the first
 * commit of its history. */
const UPSTREAM_TIP = "7b6a5f4e3d2c1b0a9f8e7d6c5b4a39281706f5e4";
const UNPUSHED_FIRST = "1c0b9a8f7e6d5c4b3a291807f6e5d4c3b2a19087";

const PAGE_SIZE: Versions = {
  old: "export const PAGE_SIZE = 10;\n",
  new: "export const PAGE_SIZE = 25;\n",
  patch: [
    "diff --git a/src/search.ts b/src/search.ts",
    "index c1d2e3f..d2e3f4a 100644",
    "--- a/src/search.ts",
    "+++ b/src/search.ts",
    "@@ -1 +1 @@",
    "-export const PAGE_SIZE = 10;",
    "+export const PAGE_SIZE = 25;",
    "",
  ].join("\n"),
};
const OVERVIEW_BODY = "# Search API overview\n";
const CHANGELOG_BODY = "# Changelog\n\n- Results are listed 25 to a page.\n";

/** What `main` has committed since `origin/main`: the Committed section of its Changes view. */
const UNPUSHED_COMPARISON: FixtureChange[] = [
  textChange("committed", "src/search.ts", at(UPSTREAM_TIP)("c1d2e3f"), onMain("d2e3f4a"), PAGE_SIZE),
  addition("committed", "CHANGELOG.md", onMain("e3f4a5b"), CHANGELOG_BODY),
];

const UNPUSHED_COMMITS: FixtureCommit[] = [
  {
    commit: MAIN_TIP,
    parent: UNPUSHED_FIRST,
    subject: "Add a changelog",
    changes: [addition("committed", "CHANGELOG.md", onMain("e3f4a5b"), CHANGELOG_BODY)],
  },
  {
    commit: UNPUSHED_FIRST,
    parent: UPSTREAM_TIP,
    subject: "Show 25 results to a page",
    changes: [textChange("committed", "src/search.ts", at(UPSTREAM_TIP)("c1d2e3f"), at(UNPUSHED_FIRST)("d2e3f4a"), PAGE_SIZE)],
  },
];

/** One uncommitted change in the main working copy, so that a Committed section starts within sight. */
const MAIN_FEW_CHANGES: FixtureChange[] = [textChange("unstaged", "src/server.ts", index("5d6e7f8"), live("v2"), SERVER_ON_DISK)];

/** What `main` has committed in all, from nothing: every file at its tip is an addition. */
const WHOLE_COMPARISON: FixtureChange[] = [
  // The file `MAIN_FEW_CHANGES` edits is tracked at the tip, so it is among the additions.
  addition("committed", "src/server.ts", onMain("5d6e7f8"), SERVER_ON_DISK.old),
  addition("committed", "src/search.ts", onMain("d2e3f4a"), PAGE_SIZE.new),
  addition("committed", "docs/overview.md", onMain("f4a5b6c"), OVERVIEW_BODY),
  addition("committed", "CHANGELOG.md", onMain("e3f4a5b"), CHANGELOG_BODY),
];

/** The two commits above and the first one, which has no parent. */
const WHOLE_COMMITS: FixtureCommit[] = [
  ...UNPUSHED_COMMITS,
  {
    commit: UPSTREAM_TIP,
    parent: null,
    subject: "Start the search API",
    changes: [
      addition("committed", "src/server.ts", at(UPSTREAM_TIP)("5d6e7f8"), SERVER_ON_DISK.old),
      addition("committed", "src/search.ts", at(UPSTREAM_TIP)("c1d2e3f"), PAGE_SIZE.old),
      addition("committed", "docs/overview.md", at(UPSTREAM_TIP)("f4a5b6c"), OVERVIEW_BODY),
    ],
  },
];

/** The sample repository's branches and a dozen more, past what a selector's list shows without
 * scrolling (ten rows of its height). The first of the others starts with right-to-left letters and
 * is long enough to be cut, within the rows in sight: a name is read left to right whatever it starts
 * with. */
const CROWDED_BRANCHES: BranchInfo[] = [
  ...BRANCHES,
  ...[
    "مراجعة/feature-ranking-pipeline-boosts-applied-after-deduplication-and-the-final-sort",
    "feature/saved-searches",
    "feature/query-suggestions",
    "feature/result-pagination",
    "fix/stale-index-after-rename",
    "fix/timeout-on-large-corpus",
    "chore/bump-toolchain",
    "chore/remove-unused-flags",
    "docs/ranking-overview",
    "refactor/split-pipeline-stages",
    "release/2.3",
    "release/2.5",
    "hotfix/escape-quotes",
  ].map((name, i): BranchInfo => ({ name, commit: `${(i + 1).toString(16).padStart(2, "0")}${"c0ffee".repeat(6)}ab` })),
];

/** `main`, checked out where the project's folder is, is two commits ahead of `origin/main`, which
 * the last fetch left at `UPSTREAM_TIP`: its Committed section lists what is not pushed. */
export const UNPUSHED_GIT: FixtureGit = {
  ...SAMPLE_GIT,
  changes: { ...SAMPLE_GIT.changes, "wt-main": MAIN_FEW_CHANGES },
  branches: BRANCHES.map((branch) => (branch.name === "main" ? { ...branch, upstream: { name: "origin/main", commit: UPSTREAM_TIP } } : branch)),
  comparisons: { ...SAMPLE_GIT.comparisons, "origin/main..main": UNPUSHED_COMPARISON },
  commits: { ...SAMPLE_GIT.commits, "origin/main..main": { commits: UNPUSHED_COMMITS } },
};

const RESTORED_GIT: FixtureGit = {
  ...UNPUSHED_GIT,
  changes: { ...UNPUSHED_GIT.changes, "wt-main": [] },
  comparisons: { ...UNPUSHED_GIT.comparisons, "origin/main..main": [] },
  commits: {
    ...UNPUSHED_GIT.commits,
    "origin/main..main": {
      commits: [
        {
          commit: MAIN_TIP,
          parent: UNPUSHED_FIRST,
          subject: "Restore 10 results to a page",
          changes: [textChange("committed", "src/search.ts", at(UNPUSHED_FIRST)("d2e3f4a"), onMain("c1d2e3f"), {
            old: PAGE_SIZE.new,
            new: PAGE_SIZE.old,
            patch: PAGE_SIZE.patch.replace("c1d2e3f..d2e3f4a", "d2e3f4a..c1d2e3f").replace("-export const PAGE_SIZE = 10;\n+export const PAGE_SIZE = 25;", "-export const PAGE_SIZE = 25;\n+export const PAGE_SIZE = 10;"),
          })],
        },
        ...UNPUSHED_COMMITS.slice(1),
      ],
    },
  },
};

/** `main` has never been pushed: no upstream, and no branch of its name on a remote, and the repository
 * has no remote default branch either, so its Committed section is its whole history, compared with
 * the empty tree. */
export const NEVER_PUSHED_GIT: FixtureGit = {
  ...SAMPLE_GIT,
  changes: { ...SAMPLE_GIT.changes, "wt-main": MAIN_FEW_CHANGES },
  branches: BRANCHES.map((branch) => (branch.name === "main" ? { name: branch.name, commit: branch.commit } : branch)),
  comparisons: { ...SAMPLE_GIT.comparisons, "..main": WHOLE_COMPARISON },
  commits: { ...SAMPLE_GIT.commits, "..main": { commits: WHOLE_COMMITS } },
};

/** The branch the main working copy has checked out in `FORKED_GIT`: made from `main`, it has two commits
 * of its own, the ones `main` has in `UNPUSHED_GIT`. */
const PAGE_BRANCH = "feature/page-size";

/** A branch never pushed, checked out where the project's folder is, in a repository whose remote
 * default branch is `origin/main`, which the last fetch left at `UPSTREAM_TIP`, the commit the branch
 * forked at: its Committed section lists what it changed since, not the whole history. `main` has no
 * upstream of its own here, as only the main working copy's branch has one looked up. */
export const FORKED_GIT: FixtureGit = {
  ...SAMPLE_GIT,
  worktrees: [{ ...WORKTREES[0], branch: PAGE_BRANCH }, WORKTREES[1], WORKTREES[2]],
  changes: { ...SAMPLE_GIT.changes, "wt-main": MAIN_FEW_CHANGES },
  branches: [
    // The branches the checked-out branch forked from are at the commit it forked at.
    ...BRANCHES.map(({ upstream: _upstream, ...branch }) => (branch.commit === MAIN_TIP ? { ...branch, commit: UPSTREAM_TIP } : branch)),
    { name: PAGE_BRANCH, commit: MAIN_TIP },
  ],
  defaultBranch: { name: "origin/main", commit: UPSTREAM_TIP },
  comparisons: { ...SAMPLE_GIT.comparisons, [`origin/main..${PAGE_BRANCH}`]: UNPUSHED_COMPARISON },
  commits: { ...SAMPLE_GIT.commits, [`origin/main..${PAGE_BRANCH}`]: { commits: UNPUSHED_COMMITS } },
};

/** How many commits the daemon lists for a branch at most. */
const COMMIT_LIST_LIMIT = 100;

/** `feature/ranking-experiments` as a long-lived branch: a hundred commits, each changing something
 * in this folder, listed newest first and cut there, so the commit before the oldest one listed (its
 * parent, left out) is where the list ends and the fork lies further back still. */
function longHistory(): { commits: FixtureCommit[]; comparison: FixtureChange[] } {
  const verbs = ["Tune", "Split", "Trim", "Rename", "Document", "Harden", "Simplify", "Batch", "Cache", "Log"];
  const nouns = ["scoring", "tokenizer", "stemmer", "synonyms", "paging", "snippets", "filters", "facets", "boosts", "ranking"];
  const hex = (n: number) => (Math.imul(n + 7, 2_654_435_761) >>> 0).toString(16).padStart(8, "0");
  const id = (n: number) => [0, 101, 202, 303, 404].map((offset) => hex(n + offset)).join("");
  const blob = (n: number) => (0x1000000 + n * 2_654_435).toString(16).slice(-7).padStart(7, "0");
  const comparison: FixtureChange[] = [];
  const commits = Array.from({ length: COMMIT_LIST_LIMIT }, (_, n): FixtureCommit => {
    const commit = n === 0 ? FEATURE_TIP : id(n);
    const files = [`src/rules/rule-${n}.ts`, ...(n % 4 === 0 ? [`docs/rules/rule-${n}.md`] : [])];
    const made = (side: ReturnType<typeof at>) => files.map((path, i) => {
      const body = path.endsWith(".ts") ? `export const rule${n} = ${n};\n` : `# Rule ${n}\n`;
      return addition("committed", path, side(blob(n * 2 + i)), body);
    });
    comparison.push(...made(onFeature));
    return {
      commit,
      parent: n === COMMIT_LIST_LIMIT - 1 ? CUT_PARENT : id(n + 1),
      // A hundred different subjects: the verb follows the commit's last digit, the noun both digits.
      subject: `${verbs[n % 10]} the ${nouns[(Math.floor(n / 10) + 3 * (n % 10)) % 10]}`,
      changes: made(at(commit)),
    };
  });
  return { commits, comparison };
}
/** The commit before the oldest one `longHistory` lists, which the list leaves out. */
const CUT_PARENT = "7c1e".padEnd(40, "5");

/** The side worktree's branch has made more commits since it forked than the daemon lists, so the
 * newest hundred are and the list says the others are not. */
export const LONG_HISTORY_GIT: FixtureGit = (() => {
  const { commits, comparison } = longHistory();
  return {
    ...SAMPLE_GIT,
    comparisons: { ...SAMPLE_GIT.comparisons, "main..feature/ranking-experiments": comparison },
    commits: { ...SAMPLE_GIT.commits, "main..feature/ranking-experiments": { commits, cut: true } },
  };
})();

/** `git` with `worktrees`, in which only the branch the main working copy has checked out carries
 * an upstream: the daemon looks one up for that branch alone. */
const withWorktrees = (git: FixtureGit, worktrees: WorktreeInfo[]): FixtureGit => ({
  ...git,
  worktrees,
  branches: git.branches?.map(({ upstream, ...branch }) => (upstream && worktrees.some((w) => w.main && w.branch === branch.name) ? { ...branch, upstream } : branch)),
});

/** The project's folder is a linked worktree on a branch while the main working copy is detached,
 * so the branch has nothing to be compared with. */
const NO_BASE_GIT = withWorktrees(SAMPLE_GIT, [WORKTREES[1], { ...WORKTREES[0], branch: null }, WORKTREES[2]]);

/** The project's own worktree is detached, so no branch is chosen. */
const DETACHED_GIT = withWorktrees(SAMPLE_GIT, [{ ...WORKTREES[0], branch: null }, WORKTREES[1], WORKTREES[2]]);

/** Every branch of the repository deleted while its only worktree is detached: nothing is checked
 * out and there is no branch to choose. */
const BRANCHLESS_GIT: FixtureGit = { worktrees: [{ ...WORKTREES[0], branch: null }], changes: { "wt-main": [] }, branches: [] };

/** The side worktree has no copy of the project's folder, as a sparse checkout leaves it; with `cutShort`, the daemon
 * also stopped listing changes early. The project is a subfolder of the repository, so its place in the side worktree is
 * a path of its own below that worktree's root. */
const sparse = (cutShort = false): FixtureGit => ({
  ...SAMPLE_GIT,
  ...(cutShort && { cutShort }),
  scope: "apps/web",
  worktrees: [WORKTREES[0], { ...WORKTREES[1], scope_present: false, scope_cause: "missing" }, WORKTREES[2]],
  changes: { ...SAMPLE_GIT.changes, "wt-side": [] },
});

/** The side worktree has `gh-pages` checked out, which shares no commit with `main`. */
const orphaned: FixtureGit = {
  ...SAMPLE_GIT,
  worktrees: [WORKTREES[0], { ...WORKTREES[1], head: ORPHAN_TIP, branch: "gh-pages" }, WORKTREES[2]],
  branches: BRANCHES.map((branch) => (branch.name === "feature/ranking-experiments" ? { ...branch, name: "gh-pages", commit: ORPHAN_TIP } : branch)),
  comparisons: {},
  commits: {},
  forks: { "main..gh-pages": null },
};

const { console: console_, web, sessions } = SAMPLE;
/** The session responsible for `feature/ranking-experiments`, checked out in the side worktree. The
 * other Git scenarios select `sessions[1]`, which is responsible for no branch, so that the selector
 * starts at the branch the project's folder has checked out. */
const rankingSession = sessionOf("s-ranking", console_.id, web.id, "Tune the ranking weights", "idle");
/** The session responsible for `chore/update-dependencies`, which no worktree has checked out. */
const dependenciesSession = sessions[2];
const state = snapshotState({ consoles: [console_], projects: [web, SAMPLE.api], sessions: [...sessions, rankingSession] });

/** The task each of the two live responsible sessions is working on, for the scenario that shows
 * the branch badges' tooltips: the badge names a session as the sidebar does, the task beside the
 * name. The other Git scenarios leave the sessions without one, which is also a state the badge
 * has to name (the name alone). */
export const RANKING_TASK = "Compare the ranking weights";
const DEPENDENCIES_TASK = "Upgrade the outdated packages";
const withTasks = new Map([
  [rankingSession.id, RANKING_TASK],
  [dependenciesSession.id, DEPENDENCIES_TASK],
]);
const stateWithTasks = snapshotState({
  consoles: [console_],
  projects: [web, SAMPLE.api],
  sessions: [...sessions, rankingSession].map((session) => (withTasks.has(session.id) ? { ...session, terminal_title: withTasks.get(session.id) } : session)),
});

/** The Git mode is where the project's browser opens, optionally with what is committed grouped by
 * commit, as a user's own choice would leave it. */
const gitMode = (options: { byCommit?: boolean; layout?: "tree" } = {}): Preferences => ({
  projectBrowsers: { [web.id]: { mode: "git", ...(options.byCommit && { byCommit: true }) } },
  ...(options.layout && { changeLayout: options.layout }),
});
const preferences = gitMode();

/** Selects a session of the project, whose browser then shows the Changes tab. */
const selecting = (session: { title: string }) => [
  (ui: Ui) => ui.press(ui.session(session.title)),
  (ui: Ui) => ui.showPanel("files"),
  (ui: Ui) => ui.wait(600),
];
const openGit = selecting(sessions[1]);

/** Successful review targets do not carry hidden conflicts or mocked failures in their navigation. */
function normalRepository(repository: FixtureGit): FixtureGit {
  const normal = (changes: FixtureChange[]) => changes.filter(({ entry, error, expand }) => entry.group !== "conflicted" && !error && !expand?.error && !expand?.movedOn);
  return {
    ...repository,
    changes: Object.fromEntries(Object.entries(repository.changes).map(([id, changes]) => [id, Array.isArray(changes) ? normal(changes) : changes])),
    comparisons: repository.comparisons && Object.fromEntries(Object.entries(repository.comparisons).map(([id, changes]) => [id, normal(changes)])),
    commits: repository.commits && Object.fromEntries(Object.entries(repository.commits).map(([id, list]) => [id, "commits" in list ? { ...list, commits: list.commits.map((commit) => ({ ...commit, changes: normal(commit.changes) })) } : list])),
  };
}

/** The row of the commit whose subject is `subject` in the Git mode's change list. */
const commitRow = (subject: string) => describing(`commitRow(${subject})`, (name) => name.includes(subject));

const failedRead: FixtureError = { code: "git_failed", params: { detail: "fatal: unable to read the index" }, message: "git failed: fatal: unable to read the index" };

const reviewScenarios: Scenario[] = [
  {
    id: "git-changes",
    group: GROUP,
    category: "normal",
    title: "The project folder's branch",
    description: "The branch selector starts on main, the branch checked out where the project's folder is, and the line above the list says it forked from origin/main at the very commit it is at, so nothing is unpushed and Committed has no rows. Staged, unstaged and untracked changes share the list, with server.ts selected and its rounded highlight spaced like Files rows. Collapse a section to watch the following rows slide into place, and right-click a row for Copy path and the Open in submenu, which groups File manager and Default application. The header reads the project name, the Files and Changes tabs, then the icon buttons and Close panel. Choose the Docked width (1394px, the narrowest docked window) and press Home on the pane's focused resize handle for the pane's 320px minimum to inspect the header: Files and Changes stay fully visible beside Close panel, the project name gives up its space first, and when the remaining icon controls no longer fit they fade at the clipped edge and scroll horizontally without moving the tabs or Close panel.",
    state,
    preferences,
    steps: [...openGit, (ui) => ui.press(ui.changeRow("server.ts", "modified")), (ui) => ui.wait(600), (ui) => ui.key("Escape")],
  },
  {
    id: "git-changes-conflicts",
    group: GROUP,
    category: "exceptional",
    title: "Conflicted changes and read failures",
    description: "Conflicted, staged, unstaged and untracked changes share the list. package.json is selected in Conflicted; open it for conflict markers. This repository deliberately includes read and expansion failures.",
    state,
    preferences,
    git: SAMPLE_GIT,
    steps: [...openGit, (ui) => ui.press(ui.changeRow("package.json", "conflicted")), (ui) => ui.wait(600), (ui) => ui.key("Escape")],
  },
  {
    id: "git-session-branch",
    group: GROUP,
    category: "normal",
    title: "The selected session's branch",
    description:
      "The selected session is responsible for feature/ranking-experiments, which the side worktree has checked out, so the selector starts on it. One header groups the branch selector, the worktree named by its folder with a button to copy its full path, then the fork and current commits. The detail lines share the same spacing, with no divider between them; one bottom border separates the whole header from the changes. The uncommitted change (README.md) comes from this worktree, and Committed lists everything the branch changed since the fork. The committed ranking.ts row's menu keeps Copy path beside Open in, whose File manager and Default application actions use the current file in the side worktree.",
    state,
    preferences,
    steps: [...selecting(rankingSession), (ui) => ui.contextMenu(ui.changeRow("ranking.ts", "modified")), (ui) => ui.press(ui.t("common.openIn"))],
  },
  {
    id: "git-changes-unpushed",
    group: GROUP,
    category: "normal",
    title: "The project folder's branch with unpushed commits",
    description:
      "main is two commits ahead of origin/main: the line above the list says it forked from origin/main at the commit that remote-tracking branch is at, and Committed lists what main changed since, which is what has not been pushed. Uncommitted changes are listed above it. Group by commit sits at the right of the Committed heading; with grouping off, there is no bulk expansion button.",
    state,
    preferences,
    git: UNPUSHED_GIT,
    steps: openGit,
  },
  {
    id: "git-changes-commits-cancel-out",
    group: GROUP,
    category: "normal",
    title: "Commits whose changes cancel out",
    description:
      "main has two unpushed commits: one changes the page size from 10 to 25, and the next restores 10. There are no uncommitted changes and the combined diff is empty. The Committed heading still shows a zero count and Group by commit; turn it on to see both commits and their opposite changes.",
    state,
    preferences,
    git: RESTORED_GIT,
    steps: openGit,
  },
  {
    id: "git-changes-folded-sections",
    group: GROUP,
    category: "normal",
    title: "Adjacent folded change sections",
    description: "Unstaged and Committed are folded to adjacent bands: the first keeps its top groove and the boundary between them has one shared groove. Reopen Unstaged to see Committed regain its top groove below the file row.",
    state,
    preferences,
    steps: [
      ...selecting(rankingSession),
      (ui) => ui.press(`${ui.t("git.group.committed")}9`),
      (ui) => ui.press(`${ui.t("git.group.unstaged")}1`),
      (ui) => ui.wait(300),
    ],
  },
  {
    id: "git-changes-never-pushed",
    group: GROUP,
    category: "normal",
    title: "A branch that has never been pushed",
    description:
      "main is checked out where the project's folder is, and neither it nor the repository has a remote branch to compare it with, not even a remote default branch: the line above the list says it has no remote branch, so all of its commits are listed, with no fork named. Committed lists every file at its tip as added. A branch forked from the remote default branch names that fork in the same line.",
    state,
    preferences,
    git: NEVER_PUSHED_GIT,
    steps: openGit,
  },
  {
    id: "git-changes-never-pushed-by-commit",
    group: GROUP,
    category: "normal",
    title: "All the commits of a branch never pushed",
    description: "With Group by commit on, the Committed band holds a row for every commit main has, newest first, down to the first one, which is listed like the others and adds the files it started with.",
    state,
    preferences: gitMode({ byCommit: true }),
    git: NEVER_PUSHED_GIT,
    steps: openGit,
  },
  {
    id: "git-changes-by-commit",
    group: GROUP,
    category: "normal",
    title: "A branch's commits listed one by one",
    description:
      "Group by commit is on: Show commits together sits at the right of the Committed heading, with Collapse all commits immediately to its left. Both stay beside the heading when it pins above the scrolling list. The Committed band holds one row per commit, newest first, each with its chevron, short hash (muted monospace text, no chip), subject and change count, and its changes one level in, their content starting a chevron's width past the hash. The whole history is listed, so nothing is said under the list. Open a change to see it against its commit's parent.",
    state,
    preferences: gitMode({ byCommit: true }),
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-by-commit-collapsed",
    group: GROUP,
    category: "normal",
    title: "All commit rows collapsed",
    description:
      "Every commit is folded to its hash, subject and change count. The Committed section stays open, with Expand all commits immediately to the left of Show commits together; use it to restore the changes under every displayed commit.",
    state,
    preferences: gitMode({ byCommit: true }),
    steps: [...selecting(rankingSession), (ui) => ui.press(ui.t("git.commits.collapseAll")), (ui) => ui.wait(300)],
  },
  {
    id: "git-changes-by-commit-tree",
    group: GROUP,
    category: "normal",
    title: "Commits grouped by folder",
    description:
      "Group by commit and Group by folder are both on: each commit holds its own folders, which start open. Cache query results is folded to its row, which still counts its two changes. The Committed heading offers Collapse all commits beside Show commits together. Its bulk control folds only commits; folder folds are kept when the commits reopen. Expand or collapse a commit or folder to watch its children fade in and settle down from just above their rows, or fade out toward the heading, while the following rows slide into place. Collapse all commits and Expand all commits use the same motion for every displayed commit.",
    state,
    preferences: gitMode({ byCommit: true, layout: "tree" }),
    steps: [...selecting(rankingSession), (ui) => ui.press(commitRow("Cache query results")), (ui) => ui.wait(300)],
  },
  {
    id: "git-changes-by-commit-filter",
    group: GROUP,
    category: "normal",
    title: "Commits narrowed by the filter",
    description:
      "The filter keeps the changes whose file name contains ranking. A commit with none left has no row, and the Committed count and each commit's count follow what is shown. Collapse all commits in the Committed heading controls only the commits left by this filter.",
    state,
    preferences: gitMode({ byCommit: true }),
    steps: [...selecting(rankingSession), (ui) => ui.press(ui.t("git.filter.show")), (ui) => ui.type("ranking")],
  },
  {
    id: "git-changes-commits-capped",
    group: GROUP,
    category: "normal",
    title: "A branch with more commits than are listed",
    description:
      "Group by commit is on over a long history: the newest hundred commits each have a row, and a warning triangle stands at the end of the Committed heading's row (its popover is in the next scenario); no sentence is added under the list.",
    state,
    preferences: gitMode({ byCommit: true }),
    git: LONG_HISTORY_GIT,
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-commits-capped-hover",
    group: GROUP,
    category: "normal",
    title: "Why only the newest commits are listed, from a resting mouse",
    description:
      "The mouse rests on the warning triangle on the Committed heading: its popover opens with no click and no focus taken, saying that only the newest commits are listed, with their number. The commit rows stay listed beneath.",
    state,
    preferences: gitMode({ byCommit: true }),
    git: LONG_HISTORY_GIT,
    steps: [...selecting(rankingSession), (ui) => ui.hoverStatusMark(ui.t("git.changes.cut.committed"))],
  },
  {
    id: "git-session-unchecked",
    group: GROUP,
    category: "normal",
    title: "A branch checked out nowhere",
    description:
      "The selected session is responsible for chore/update-dependencies, which no worktree has checked out. The header groups the branch selector and fork comparison without a worktree row or a notice explaining the absence of uncommitted changes. Its bottom border remains above the committed changes. Main has moved on since the fork, so the fork commit is not main's tip. The committed package.json row still offers Copy path and Open in: File manager and Default application use the current file in the project directory.",
    state,
    preferences,
    steps: [...selecting(dependenciesSession), (ui) => ui.contextMenu(ui.changeRow("package.json", "modified")), (ui) => ui.press(ui.t("common.openIn"))],
  },
  {
    id: "git-changes-scope-missing",
    group: GROUP,
    category: "normal",
    title: "A branch whose worktree lacks the project's folder",
    description: "The side worktree has feature/ranking-experiments checked out without the project's folder. One header groups the branch selector, the worktree name with its Copy path button and a warning triangle at the end of that line, and the fork comparison. The detail lines share the same spacing, and the committed changes are still listed below the header. The triangle's popover is in the next scenario.",
    state,
    preferences,
    git: sparse(),
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-scope-missing-hover",
    group: GROUP,
    category: "exceptional",
    title: "Why the project's folder is unavailable in a worktree, from a resting mouse",
    description: "The mouse rests on the warning triangle at the end of the worktree name's line: its popover opens with no click and no focus taken, saying the project's folder is not in this worktree (the branch may lack it, or it may have been moved, renamed or deleted) and to restore it or check out a branch that has it. A project folder reached through a link, a file where it should be, or one that cannot be read or is off limits puts its own sentence in the same popover.",
    state,
    preferences,
    git: sparse(),
    steps: [...selecting(rankingSession), (ui) => ui.hoverStatusMark(ui.t("git.changes.scope.mark"))],
  },
  {
    id: "git-changes-scope-and-cut-hover",
    group: GROUP,
    category: "exceptional",
    title: "A worktree both cut short and without the project's folder, from a resting mouse",
    description: "The side worktree lacks the project's folder and its change list was also cut short, so the Worktree line carries one warning triangle for both, not two. The mouse rests on it: its popover opens with no click and no focus taken, giving the cut reason with its remedy first, then why the project's folder is unavailable with its remedy.",
    state,
    preferences,
    git: sparse(true),
    steps: [...selecting(rankingSession), (ui) => ui.hoverStatusMark(ui.t("git.changes.cutAndScope"))],
  },
  {
    id: "git-changes-scope-missing-terminal",
    group: GROUP,
    category: "exceptional",
    title: "A terminal started in a worktree that lacks the project's folder",
    description: "The Changes tab shows the side worktree, which has no copy of the project's folder, and Show terminal panel is pressed: the shell would start there, so the error alert opens, titled \"Could not create the terminal\" and saying the folder /Users/dev/code/search-api-experiments/apps/web cannot be read, the path in the sentence. Its remedy says the folder may have been moved, renamed or deleted, or be missing from the branch checked out there, and to restore it at that path and try again; the system's reason is behind Error details, closed. A folder reached through a link in its path, or one whose access is denied, is refused with the same alert, worded for its cause with its own remedy, and the same closed Error details.",
    state,
    preferences,
    git: sparse(),
    steps: [...selecting(rankingSession), (ui) => ui.press(ui.t("titleBar.terminal.show")), (ui) => ui.wait(400)],
  },
  {
    id: "git-changes-listing-commits",
    group: GROUP,
    category: "normal",
    title: "Listing a branch's commits",
    description: "With Group by commit on, the commits are still being listed: a line above the list says so beneath the line naming the fork, while the uncommitted change is already listed. The Committed heading stays visible with a zero count and Show commits together; Expand all commits is disabled until commit rows are available.",
    state,
    preferences: gitMode({ byCommit: true }),
    git: { ...SAMPLE_GIT, commits: { ...SAMPLE_GIT.commits, "main..feature/ranking-experiments": { pending: true } } },
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-comparing",
    group: GROUP,
    category: "normal",
    title: "Comparing with the base branch",
    description: "The comparison is still being made: the line above the list says what it is being compared with, while the uncommitted change from the side worktree is already listed.",
    state,
    preferences,
    git: {
      ...SAMPLE_GIT,
      comparing: "pending",
      worktrees: SAMPLE_GIT.worktrees.map((worktree) => worktree.main ? { ...worktree, branch: "main/release-coordination-مرحبا-发布" } : worktree),
      branches: SAMPLE_GIT.branches!.map((branch) => branch.name === "main" ? { ...branch, name: "main/release-coordination-مرحبا-发布" } : branch),
    },
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-sticky",
    group: GROUP,
    category: "normal",
    title: "A section heading pinned above flat changes",
    description: "The list starts partway through Staged: its heading stays against the top of the list while rows scroll beneath it. The top fade begins below the pinned band; scrolling farther lets the next section replace it.",
    state,
    preferences,
    steps: [...openGit, (ui) => ui.scrollCollection(ui.t("git.changes.list.label"), 180)],
  },
  {
    id: "git-changes-tree-folder-menu",
    group: GROUP,
    category: "normal",
    title: "A grouping folder’s actions menu",
    description: "In the folder-grouped list a folder row carries the same actions button as a change row, at the end of the row: here it is pressed on the first src folder, under Staged, and the menu shows Copy path and Open in, whose submenu is open with File manager and Terminal and no Default application. Commit rows have no actions.",
    state,
    preferences: gitMode({ layout: "tree" }),
    steps: [...openGit, (ui) => ui.press(ui.t("browser.directory.actions", { name: "src" })), (ui) => ui.press(ui.t("common.openIn"))],
  },
  {
    id: "git-changes-tree-sticky",
    group: GROUP,
    category: "normal",
    title: "A section heading pinned above grouped changes",
    description: "As in the flat list, the folder-grouped list starts partway through Staged and its heading stays against the top of the list while directories and files scroll beneath it; scrolling farther lets the next section replace it.",
    state,
    preferences: gitMode({ layout: "tree" }),
    steps: [...openGit, (ui) => ui.scrollCollection(ui.t("git.changes.list.label"), 180)],
  },
  {
    id: "git-changes-cut-short",
    group: GROUP,
    category: "exceptional",
    title: "A change list cut short",
    description: "The daemon stopped listing early. The source block's Worktree line (the side worktree's folder name and its copy button) ends on a warning triangle named \"Not every uncommitted change is listed\"; there is no Alert above the Staged and Unstaged sections, which start right below the block. The Committed heading still has its own warning triangle at the end of its row. The popovers are in the next scenarios.",
    state,
    preferences,
    git: normalRepository({ ...SAMPLE_GIT, cutShort: true }),
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-cut-short-own",
    group: GROUP,
    category: "exceptional",
    title: "The project's own working copy cut short",
    description: "The project's own working copy normally shows no Worktree line; here its uncommitted changes are cut short, so the source block has one, only because of that: the folder name, its copy button and the warning triangle at the end.",
    state,
    preferences,
    git: normalRepository({ ...SAMPLE_GIT, cutShort: true }),
    steps: openGit,
  },
  {
    id: "git-changes-cut-short-uncommitted-hover",
    group: GROUP,
    category: "exceptional",
    title: "Why the uncommitted changes are cut short, from a resting mouse",
    description: "The mouse rests on the warning triangle at the end of the Worktree line: its popover opens with no click and no focus taken, saying the worktree has more uncommitted changes than the list can show, and what to do about it, with .gitignore as an inline code chip. The Committed heading's triangle opens a different sentence, in the next scenario.",
    state,
    preferences,
    git: normalRepository({ ...SAMPLE_GIT, cutShort: true }),
    steps: [...selecting(rankingSession), (ui) => ui.hoverStatusMark(ui.t("git.changes.cut.uncommitted"))],
  },
  {
    id: "git-changes-cut-short-committed-hover",
    group: GROUP,
    category: "exceptional",
    title: "Why the committed changes are cut short, from a resting mouse",
    description: "The mouse rests on the warning triangle at the end of the Committed heading's row: its popover opens with no click and no focus taken, saying not every change is shown. The Worktree line's triangle stays closed.",
    state,
    preferences,
    git: normalRepository({ ...SAMPLE_GIT, cutShort: true }),
    steps: [...selecting(rankingSession), (ui) => ui.hoverStatusMark(ui.t("git.changes.cut.committed"))],
  },
  {
    id: "git-changes-tree",
    group: GROUP,
    category: "normal",
    title: "Changes grouped by folder",
    description: "Directories start expanded and config.ts is selected inside src, its highlight spaced like the flat list's while file labels follow the folder indentation. Expand or collapse a folder or section to watch its children slide and fade in or out with the same timing as the session menu, while following rows slide into place.",
    state,
    preferences: gitMode({ layout: "tree" }),
    steps: [
      ...openGit,
      (ui) => ui.press(ui.changeRow("config.ts", "renamed")),
      (ui) => ui.wait(600),
      (ui) => ui.key("Escape"),
    ],
  },
  {
    id: "git-clean",
    group: GROUP,
    category: "normal",
    title: "A branch with no changes to show",
    description: "The branch information header keeps its bottom border above the empty change list. Selecting release/2.4 removes the worktree row while preserving the comparison and the header's bottom border.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, changes: { ...SAMPLE_GIT.changes, "wt-main": [] } },
    steps: openGit,
  },
  {
    id: "git-changes-no-base",
    group: GROUP,
    category: "normal",
    title: "A main working copy on no branch",
    description: "The project's folder is a linked worktree on feature/ranking-experiments, and the main working copy is detached: the line above the list says the main working copy is not on a branch, so the branch's commits cannot be compared, and only its uncommitted change is listed.",
    state,
    preferences,
    git: NO_BASE_GIT,
    steps: openGit,
  },
  {
    id: "git-changes-detached",
    group: GROUP,
    category: "normal",
    title: "No branch chosen",
    description: "The worktree holding the project's folder is detached, so no branch is chosen by default: the panel asks for one.",
    state,
    preferences,
    git: DETACHED_GIT,
    steps: openGit,
  },
  {
    id: "git-not-a-repository",
    group: GROUP,
    category: "normal",
    title: "A project in no repository",
    state,
    preferences,
    git: null,
    steps: openGit,
  },
  {
    id: "git-compare-no-branches",
    group: GROUP,
    category: "normal",
    title: "A repository with no branches",
    description: "With no branch to choose, the selector is disabled and the panel says so.",
    state,
    preferences,
    git: BRANCHLESS_GIT,
    steps: openGit,
  },
  {
    id: "git-changes-unborn",
    group: GROUP,
    category: "normal",
    title: "A branch with no commit yet",
    description:
      "main is checked out where the project's folder is, but has no commit yet, so the branch list is empty: the line above the list says there are no commits, and the staged and untracked files are listed.",
    state,
    preferences,
    git: FRESH_GIT,
    steps: openGit,
  },
  {
    id: "git-compare-many-branches",
    group: GROUP,
    category: "normal",
    title: "More branches than the selector shows",
    description:
      "The Branch selector open over a repository with more branches than its list holds: the list scrolls and its rows fade at the bottom edge, where more are waiting, while the popover around it keeps its edges. The row that opens with Arabic letters is laid out left to right like the others: cut at its start and fading at its left edge, it shows its end and not its Arabic opening, as the long local branch does.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, branches: CROWDED_BRANCHES },
    steps: [...openGit, (ui) => ui.press("main"), (ui) => ui.wait(400)],
  },
  {
    id: "git-compare-origins",
    group: GROUP,
    category: "normal",
    title: "Responsible-session badges on branches",
    description:
      "Each branch row's ring names the session working on it: filled in the accent colour for the selected session, plain for another session, and named as no longer existing for a removed one. Both live sessions have a task, so hover their rings: the tooltip names each as its sidebar row does, the task beside the session name.",
    state: stateWithTasks,
    preferences,
    steps: [
      // The row is named as the sidebar names a session with a task, which is also what the badge says.
      (ui) => ui.press(ui.session(ui.t("session.displayTitle", { title: RANKING_TASK, name: rankingSession.title }))),
      (ui) => ui.showPanel("files"),
      (ui) => ui.wait(600),
      (ui) => ui.press("feature/ranking-experiments"),
      (ui) => ui.wait(400),
    ],
  },
  {
    id: "git-compare-deleted",
    group: GROUP,
    category: "normal",
    title: "A branch deleted after it was compared",
    description:
      "The selected session's branch, chore/update-dependencies, was deleted after the comparison was made: the selector is back on main, the branch checked out where the project's folder is, and main's changes are listed with no error alert.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, gone: ["chore/update-dependencies"] },
    steps: [...selecting(dependenciesSession), (ui) => ui.wait(BRANCH_LIST_REREAD_MS)],
  },
  {
    id: "git-branch-gone-worktree",
    group: GROUP,
    category: "exceptional",
    title: "A worktree still naming a deleted branch",
    description: "The worktree inspection still names a long mixed-script branch that the complete branch list no longer contains. Its clipped selector keeps a separate \"No longer exists\" status badge, with no mark beside the selector; hover the name for the rich tooltip, or open the selector to inspect the surviving raw branch choices.",
    state,
    preferences,
    git: {
      ...SAMPLE_GIT,
      worktrees: SAMPLE_GIT.worktrees.map((worktree) => worktree.main ? { ...worktree, branch: "feature/deleted-release-coordination-and-compatibility-review-مرحبا-发布" } : worktree),
      branches: SAMPLE_GIT.branches!.filter((branch) => branch.name !== "main"),
    },
    steps: openGit,
  },
  {
    id: "git-compare-gone",
    group: GROUP,
    category: "exceptional",
    title: "A chosen branch that no longer exists",
    description: "The comparison failure takes the list's place in the empty-state shape: a danger glyph, the heading and the reason, with Try again as the primary button and no details surface, since there is no raw text. Narrow shows the same failure in the docked Files card.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, missing: ["chore/update-dependencies"] },
    steps: selecting(dependenciesSession),
  },
  {
    id: "git-compare-gone-base",
    group: GROUP,
    category: "exceptional",
    title: "The base branch deleted after the comparison was made",
    description: "chore/update-dependencies is compared with main, and main was deleted afterwards. The comparison on screen is the one that was made, so the changes stay listed, and a warning mark sized to the line's text and centred on it sits in the \"Forked from main\" line right after main; there is no loose line, alert or mark beside the Branch select about it.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, goneAfter: { "main..chore/update-dependencies": ["main"] } },
    steps: [...selecting(dependenciesSession), (ui) => ui.wait(BRANCH_LIST_REREAD_MS)],
  },
  {
    id: "git-compare-gone-base-details",
    group: GROUP,
    category: "exceptional",
    title: "The base branch deleted after the comparison was made, mark opened",
    description: "The text-sized warning mark right after main in the \"Forked from main\" line is hovered, and its popover reads \"main no longer exists. The comparison still uses the commits it was made at.\", with main as a code chip.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, goneAfter: { "main..chore/update-dependencies": ["main"] } },
    steps: [...selecting(dependenciesSession), (ui) => ui.wait(BRANCH_LIST_REREAD_MS), (ui) => ui.hoverStatusMark(ui.t("git.branch.deletedMark", { branch: "main" }))],
  },
  {
    id: "git-changes-branches-failed",
    group: GROUP,
    category: "exceptional",
    title: "The branch list could not be read",
    description: "The changes are listed, and a danger glyph beside the Branch select, named for the failure, opens a popover with the reason, then Git's diagnostic, then Try again at its trailing edge.",
    state,
    preferences,
    projectBranches: { [web.id]: { code: "git_failed", params: { detail: "fatal: unable to read refs/heads" }, message: "Unable to list local branches" } },
    steps: openGit,
  },
  {
    id: "git-compare-no-common-commit",
    group: GROUP,
    category: "exceptional",
    title: "A branch with no commit in common with main",
    description: "gh-pages is checked out in the side worktree: its uncommitted change is listed, and a danger glyph on the Committed heading opens a popover saying main and gh-pages have no commit in common, with no Try again, since asking again gives the same answer. A shallow clone words it as missing history in the same popover.",
    state,
    preferences,
    git: orphaned,
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-listing-failed",
    group: GROUP,
    category: "exceptional",
    title: "The uncommitted changes could not be read",
    description: "The side worktree's listing fails: a danger Alert stands at the top of the change list, titled \"Could not list the changes in this worktree\", with a sentence, a Details button and Try again, and the committed changes are still listed below it. The Alert is about all the uncommitted changes rather than one section, so no heading carries a mark for it. Its Details are in the next scenario.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, changes: { ...SAMPLE_GIT.changes, "wt-side": failedRead } },
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-listing-failed-reason",
    group: GROUP,
    category: "exceptional",
    title: "The reason the uncommitted changes could not be read, in the Alert's Details",
    description: "Details on the danger Alert above the change list is open: its popover holds Git's diagnostic as raw text, with focus inside it.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, changes: { ...SAMPLE_GIT.changes, "wt-side": failedRead } },
    steps: [...selecting(rankingSession), (ui) => ui.press(ui.t("error.details"))],
  },
  {
    id: "git-changes-failed",
    group: GROUP,
    category: "exceptional",
    title: "No changes could be listed",
    description: "The project folder's own worktree fails to list, with nothing else to show: the failure takes the list's place in the empty-state shape: a danger glyph, the heading and the reason, with Git's diagnostic on a surface of its own under the reason and Try again as the primary button below it.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, changes: { ...SAMPLE_GIT.changes, "wt-main": failedRead } },
    steps: openGit,
  },
  {
    id: "git-commits-failed",
    group: GROUP,
    category: "exceptional",
    title: "The commits could not be listed",
    description: "With Group by commit on, the commit listing fails: the Committed heading stays visible with a zero count, Show commits together and a disabled Expand all commits button. Its danger glyph opens a popover with the reason, then Git's diagnostic, then Try again at its trailing edge, and the uncommitted change is still listed.",
    state,
    preferences: gitMode({ byCommit: true }),
    git: { ...SAMPLE_GIT, commits: { ...SAMPLE_GIT.commits, "main..feature/ranking-experiments": { error: failedRead } } },
    steps: selecting(rankingSession),
  },
  {
    id: "git-unreadable",
    group: GROUP,
    category: "exceptional",
    title: "A repository Git cannot read",
    description: "The standalone failure takes the empty-state shape: a danger glyph, the heading and the summary, with Git's own diagnostic as selectable code on a surface of its own under the summary and no Try again. Narrow shows the same failure in the docked Files card.",
    state,
    preferences,
    git: { worktrees: [], changes: {}, gitError: GIT_STDERR },
    steps: openGit,
  },
  {
    id: "git-change-viewer",
    group: "File viewer",
    category: "normal",
    title: "Change viewer with collapsed lines",
    state,
    preferences,
    description: "Green additions and red removals after the path match the counts in the change list. The staged tag and long expansion gaps accompany the diff; other change subjects remain available through navigation. File manager and Default application end the header row, as the change is in a working copy.",
    steps: [
      ...openGit,
      (ui) => ui.press(ui.changeRow("pipeline.ts", "modified")),
      (ui) => ui.wait(800),
    ],
  },
  {
    id: "git-whole-history-change-viewer",
    group: "File viewer",
    category: "normal",
    title: "A change from a branch's whole history in the viewer",
    description: "The subtitle names the branch and the commit it is at, with all of its history, because neither the branch nor the repository has a remote branch to say where it forked from. A committed change has no File manager or Default application buttons.",
    state,
    preferences,
    git: NEVER_PUSHED_GIT,
    steps: [...openGit, (ui) => ui.press(ui.changeRow("overview.md", "added")), (ui) => ui.wait(800)],
  },
  {
    id: "git-commit-change-viewer",
    group: "File viewer",
    category: "normal",
    title: "A commit's change in the viewer",
    description: "The subtitle names the commit and the parent the change is compared with. Next moves to the Import the ranking notes commit, which has no parent: its subtitle says so. A committed change has no File manager or Default application buttons.",
    state,
    preferences: gitMode({ byCommit: true }),
    steps: [
      ...selecting(rankingSession),
      (ui) => ui.press(ui.changeRow("ranking.ts", "modified")),
      (ui) => ui.wait(800),
    ],
  },
  {
    id: "git-changes-filter",
    group: GROUP,
    category: "normal",
    title: "Filtered changes",
    state,
    preferences,
    steps: [
      ...openGit,
      (ui) => ui.press(ui.t("git.filter.show")),
      (ui) => ui.type("server"),
    ],
  },
  {
    id: "git-changes-filter-placeholder",
    group: GROUP,
    category: "normal",
    title: "Changes with an empty filter field",
    description: "The focused, empty filter shows its placeholder above the unchanged list; moving focus away closes the empty field.",
    state,
    preferences,
    steps: [
      ...openGit,
      (ui) => ui.press(ui.t("git.filter.show")),
    ],
  },
  {
    id: "git-changes-filter-empty",
    group: GROUP,
    category: "normal",
    title: "No matching changes",
    state,
    preferences,
    steps: [
      ...openGit,
      (ui) => ui.press(ui.t("git.filter.show")),
      (ui) => ui.type("no-such-file"),
    ],
  },
];

export const projectGitScenarios: Scenario[] = reviewScenarios.map((scenario) => scenario.category === "normal" && scenario.git !== null
  ? { ...scenario, git: normalRepository(scenario.git ?? SAMPLE_GIT) }
  : scenario);
