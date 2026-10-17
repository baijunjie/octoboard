// The sample repository the fixture daemon answers a project's Git mode with (`fixtureGit.ts`),
// and the gallery's "Project Git" scenarios over it: the worktree selector, the change list in its
// sections, a diff of each kind opened from it, and the states a source or a list can end in. Every
// change here is built by the factory for its kind (`changeFixtures.ts`).
import type { BranchInfo, ContentSource, FileContent, WorktreeInfo } from "../../protocol";
import { describing, type Matcher, type Ui } from "../interact";
import type { Scenario } from "../scenario";
import type { FixtureChange, FixtureGit, Versions } from "./changeFixtures";
import type { FixtureError } from "./projectFiles";
import { SAMPLE, sessionOf, snapshotState } from "./builders";
import {
  EXPAND_DELAY_MS,
  addition,
  binaryChange,
  conflicted,
  deletion,
  editedSource,
  expandable,
  fromOutsideTheProject,
  hashedOnDisk,
  nested,
  nestedUnchanged,
  refused,
  renamedTo,
  renamedUnchanged,
  textChange,
  typeChange,
  unchanged,
  untracked,
  untrackedNotFile,
} from "./changeFixtures";
import { IMAGES } from "./viewerImages";

const GROUP = "Project pane: Git";

const image = (data: string): FileContent => ({ size: Math.floor((data.length * 3) / 4), kind: "binary", media_type: "image/png", text: null, data });
const commit = (blob: string) => ({ kind: "commit", commit: "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39", branch: null, blob }) as const;
const index = (blob: string) => ({ kind: "index", worktree: "wt-main", blob }) as const;
const live = (version: string): ContentSource => ({ kind: "live", root_id: "fixture", version });

/** The lines of the long file the expansion scenarios change: a run of unmodified lines before the
 * first hunk, a long one between the hunks, and none after the last (the viewer takes that bar away). */
const PIPELINE = editedSource("src/pipeline.ts", 120, [10, 70, 117]);

/** The same shape with a run of unmodified lines after the last hunk, which its bar counts. */
const PIPELINE_TAIL = editedSource("src/pipeline.ts", 120, [10, 70, 100]);

/** Runs of both kinds in one file of 80 lines. A run between two hunks no longer than one expansion
 * gets one control revealing all of it, and a longer one gets the pair. The edits at 10, 20, 36 and
 * 70 leave runs of 6 lines before the first hunk, 3, 9 and 27 between the hunks, and 7 after the
 * last: the 27 gets the pair, and every other run, the one before the first hunk and the one after
 * the last included, gets one control. */
const SHORT_GAP = editedSource("src/pipeline.ts", 80, [10, 20, 36, 70]);

/** A file past the viewer's render budget (`RENDER_BUDGETS.diffLines`) whose patch stays small, so
 * its bodies are refused by the viewer itself and not by the daemon. */
const HUGE = editedSource("src/pipeline.ts", 11000, [10, 70, 100]);

/** The same shape as `PIPELINE` with every line far wider than a window, for the diff unwrapped. */
const WIDE = editedSource("src/pipeline.ts", 120, [10, 70, 117], ` // ${"a note that runs on well past the edge of the window ".repeat(8)}`);

/** The ids of the contents `PIPELINE` holds, which every change that places it at a path names its
 * sides by: Git names an object by its content, so the same text is the same id at any path. A
 * source that shares its old text with another (`PIPELINE_TAIL`, which edits other lines of it)
 * shares the id too. */
const PIPELINE_IDS = { old: "a1b2c3d", new: "b2c3d4e" };

/** The small files the sample changes rewrite, each with the patch `git` writes between its two
 * versions. Every one of these changes reaches its file's last line, so its diff is drawn with no
 * bar for lines beyond it. */
const SERVER_STAGING: Versions = {
  old: 'import http from "node:http";\n\nexport const createServer = () => http.createServer();\n',
  new: 'import http from "node:http";\nimport { logRequests } from "./log";\n\nexport const createServer = () => http.createServer(logRequests);\n',
  // Built line by line, so the blank context lines keep the space that starts them.
  patch: [
    "diff --git a/src/server.ts b/src/server.ts",
    "index 1a2b3c4..5d6e7f8 100644",
    "--- a/src/server.ts",
    "+++ b/src/server.ts",
    "@@ -1,3 +1,4 @@",
    ' import http from "node:http";',
    '+import { logRequests } from "./log";',
    " ",
    "-export const createServer = () => http.createServer();",
    "+export const createServer = () => http.createServer(logRequests);",
    "",
  ].join("\n"),
};

/** The same file changed again on disk, over what is staged. */
const SERVER_ON_DISK: Versions = {
  old: SERVER_STAGING.new,
  new: `${SERVER_STAGING.new}\nexport const PORT = 8080;\n`,
  patch: [
    "diff --git a/src/server.ts b/src/server.ts",
    "index 5d6e7f8..2b3c4d5 100644",
    "--- a/src/server.ts",
    "+++ b/src/server.ts",
    '@@ -2,3 +2,5 @@ import http from "node:http";',
    ' import { logRequests } from "./log";',
    " ",
    " export const createServer = () => http.createServer(logRequests);",
    "+",
    "+export const PORT = 8080;",
    "",
  ].join("\n"),
};

/** A rename whose content changed too, so `git` writes its similarity and both paths above a hunk. */
const CONF_RENAME: Versions = {
  old: "export function loadConfig(env: NodeJS.ProcessEnv) {\n  return { port: Number(env.PORT) };\n}\n",
  new: "export function loadConfig(env: NodeJS.ProcessEnv) {\n  return { port: Number(env.PORT ?? 8080) };\n}\n",
  patch: `diff --git a/src/conf.ts b/src/config.ts
similarity index 82%
rename from src/conf.ts
rename to src/config.ts
index 0f1e2d3..4c5b6a7 100644
--- a/src/conf.ts
+++ b/src/config.ts
@@ -1,3 +1,3 @@
 export function loadConfig(env: NodeJS.ProcessEnv) {
-  return { port: Number(env.PORT) };
+  return { port: Number(env.PORT ?? 8080) };
 }
`,
};

const README_EDIT: Versions = {
  old: "# Search API\n",
  new: "# Search API (experimental branch)\n",
  patch:
    "diff --git a/README.md b/README.md\nindex aa11bb2..cc33dd4 100644\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-# Search API\n+# Search API (experimental branch)\n",
};

const RANKING_EDIT: Versions = {
  old: 'import type { Hit } from "./index";\n\nexport const rank = (hits: Hit[]) => hits.sort((a, b) => b.score - a.score);\n',
  new: 'import type { Hit } from "./index";\n\nexport const rank = (hits: Hit[], boost = 1) =>\n  hits.map((hit) => ({ ...hit, score: hit.score * boost })).sort((a, b) => b.score - a.score);\n',
  patch: [
    "diff --git a/src/ranking.ts b/src/ranking.ts",
    "index 3c4d5e6..7f8a9b0 100644",
    "--- a/src/ranking.ts",
    "+++ b/src/ranking.ts",
    "@@ -1,3 +1,4 @@",
    ' import type { Hit } from "./index";',
    " ",
    "-export const rank = (hits: Hit[]) => hits.sort((a, b) => b.score - a.score);",
    "+export const rank = (hits: Hit[], boost = 1) =>",
    "+  hits.map((hit) => ({ ...hit, score: hit.score * boost })).sort((a, b) => b.score - a.score);",
    "",
  ].join("\n"),
};

/** What `current` held while it was a file, before it became a link to the next release. */
const CURRENT_AS_FILE = "releases/2.3\n";
/** What `docs/old-notes.md` held before it was removed. */
const DELETED_NOTES = "# Old notes\nSuperseded by the architecture document.\n";
/** What `src/cache.ts` holds, a file the feature branch adds. */
const CACHE_BODY = "export const cache = new Map<string, unknown>();\n";
/** The body of the file whose other side lies outside the project's scope. */
const INDEX_BODY = "export const buildIndex = (docs: string[]) => new Map(docs.map((d, i) => [i, d]));\n";
/** The images the binary change compares, and the ids `git` hashed each of them to. */
const LOGO_BODIES = { old: image(IMAGES.before_png.data), new: image(IMAGES.after_png.data) };

const UNREADABLE: FixtureError = {
  code: "git_failed",
  params: { detail: "fatal: unable to read the object" },
  message: "git failed: fatal: unable to read the object",
};

const MAIN_CHANGES: FixtureChange[] = [
  textChange("staged", "src/server.ts", commit("1a2b3c4"), index("5d6e7f8"), SERVER_STAGING),
  renamedTo("staged", "src/conf.ts", "src/config.ts", commit("0f1e2d3"), index("4c5b6a7"), CONF_RENAME),
  fromOutsideTheProject("staged", "shared/search-index.ts", "src/index.ts", index("2e3d4c5"), INDEX_BODY),
  typeChange("staged", "current", commit("0d1c2b3"), index("8d2f1a0"), CURRENT_AS_FILE, "releases/2.4"),
  nested("staged", "nest", "nest/leaf.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE),
  nestedUnchanged("staged", "nest2/leaf.ts", "nest2", commit(PIPELINE_IDS.new), index(PIPELINE_IDS.new), { content: PIPELINE.new, delay: EXPAND_DELAY_MS }),
  expandable("staged", "src/pipeline.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE),
  expandable("staged", "src/tail.ts", commit(PIPELINE_IDS.old), index("2d3e4f5"), PIPELINE_TAIL),
  expandable("staged", "src/stages.ts", commit("5e6f7a8"), index("7a8b9c0"), SHORT_GAP),
  expandable("staged", "src/huge.ts", commit("1d2e3f4"), index("4f5a6b7"), HUGE),
  expandable("staged", "src/wide.ts", commit("9a8b7c6"), index("6c7d8e9"), WIDE),
  expandable("staged", "src/limits.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE, {
    error: { code: "limit_exceeded", params: { limit: "file_bytes", size: "5242880", max: "4194304" }, message: "the file is larger than the 4194304-byte limit" },
  }),
  // The one side that moved on under the window: the daemon finds a version other than the one the
  // request names, and refuses before it reads anything.
  expandable("staged", "src/moved.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE, { movedOn: true }),
  expandable("staged", "src/flaky.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE, { error: UNREADABLE }),
  // Unstaged, which is where an empty patch is actually met: the file on disk compares equal to the
  // version staged before it.
  unchanged("unstaged", "src/same.ts", index(PIPELINE_IDS.new), live("v-same"), { content: PIPELINE.new, delay: EXPAND_DELAY_MS }),
  renamedUnchanged("staged", "src/old-name.ts", "src/new-name.ts", commit("1b2c3d4"), index("1b2c3d4"), { error: UNREADABLE }),
  conflicted("package.json", "both_modified"),
  // Its new side is the file on disk, which carries no line count; the old side's settles the run
  // past the last hunk on its own, the lines past a hunk being the same on both sides.
  textChange("unstaged", "src/server.ts", index("5d6e7f8"), live("v2"), SERVER_ON_DISK),
  deletion("unstaged", "docs/old-notes.md", index("7c6b5a4"), DELETED_NOTES),
  binaryChange("unstaged", "docs/logo.png", index("6a5b4c3"), hashedOnDisk(live("v3"), "8b7c6d5"), LOGO_BODIES),
  refused("unstaged", "data/fixtures.bin", index("1f2e3d4"), live("v4"), {
    code: "limit_exceeded",
    params: { limit: "patch_bytes", max: "4194304" },
    message: "the change's patch is larger than the 4194304-byte limit",
  }),
  untracked("src/log.ts", live("v5"), "export const logRequests = () => {};\n"),
  // Two directories that each hold only the next: the tree shows them as one row, `tools/release`.
  untracked("tools/release/notes.md", live("v7"), "# Release notes\n"),
  // Neither has a body to read nor a patch to make: a link's target and a nested repository's
  // content are not Git's to show here.
  untrackedNotFile("src/config.local.ts", "symlink", live("v8")),
  untrackedNotFile("search-client", "submodule", live("v9")),
];

const SIDE_CHANGES: FixtureChange[] = [textChange("unstaged", "README.md", index("aa11bb2"), live("v6"), README_EDIT)];

/** A repository `git init` just made: no commit, so no branch and nothing to compare, and nothing
 * changed in its worktree. */
const FRESH_GIT: FixtureGit = {
  worktrees: [{ id: "wt-fresh", root: "/Users/dev/code/search-api", main: true, head: null, branch: null, scope_present: true }],
  changes: { "wt-fresh": [] },
  branches: [],
};

const MAIN_TIP = "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39";
const FEATURE_TIP = "9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807";
/** The tip of a branch that shares no commit with the others, as a `gh-pages` branch made from an empty tree. */
const ORPHAN_TIP = "3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b";
const LONG_BRANCH = "feature/rework-the-ranking-pipeline-so-that-boosts-are-applied-after-deduplication";

const BRANCHES: BranchInfo[] = [
  {
    name: "feature/ranking-experiments",
    commit: FEATURE_TIP,
    // The responsibility of a live session of the project, which the selected-session scenarios
    // pick.
    origin: { responsible_session: "s-ranking", responsible_title: "Tune the ranking weights", responsible_since: 1_760_000_000_000 },
  },
  {
    name: LONG_BRANCH,
    commit: "5b4a3f2e1d0c9b8a7f6e5d4c3b2a1f0e9d8c7b6a",
    // The responsible session has been removed since, so the recorded title names it.
    origin: { responsible_session: "s-gone", responsible_title: "Review the deduplication pass", responsible_since: 1_759_000_000_000 },
  },
  {
    name: "chore/update-dependencies",
    commit: "7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807e8d9",
    // A live session that is not the selected one: the plain ring. The title recorded when it became
    // responsible has been superseded by the session's own, which is the one the badge shows.
    origin: { responsible_session: "s-web-2", responsible_title: "Bump the packages", responsible_since: 1_758_000_000_000 },
  },
  { name: "main", commit: MAIN_TIP },
  { name: "release/2.4", commit: MAIN_TIP },
];

const at = (commit: string) => (blob: string) => ({ kind: "commit", commit, branch: null, blob }) as const;
const onMain = at(MAIN_TIP);
const onFeature = at(FEATURE_TIP);

/** What `feature/ranking-experiments` changed since it forked from `main` (at `main`'s tip, the
 * fork `forks` falls back to): the comparison a session's branch fills the Compare view with. */
const FEATURE_COMPARISON: FixtureChange[] = [
  textChange("committed", "src/ranking.ts", onMain("3c4d5e6"), onFeature("7f8a9b0"), RANKING_EDIT),
  renamedTo("committed", "src/conf.ts", "src/config.ts", onMain("0f1e2d3"), onFeature("4c5b6a7"), CONF_RENAME),
  fromOutsideTheProject("committed", "shared/search-index.ts", "src/index.ts", onFeature("2e3d4c5"), INDEX_BODY),
  expandable("committed", "src/pipeline.ts", onMain(PIPELINE_IDS.old), onFeature(PIPELINE_IDS.new), PIPELINE),
  addition("committed", "src/cache.ts", onFeature("5a6b7c8"), CACHE_BODY),
  deletion("committed", "docs/old-notes.md", onMain("7c6b5a4"), DELETED_NOTES),
  binaryChange("committed", "docs/logo.png", onMain("6a5b4c3"), onFeature("8b7c6d5"), LOGO_BODIES),
  typeChange("committed", "current", onMain("0d1c2b3"), onFeature("8d2f1a0"), CURRENT_AS_FILE, "releases/2.4"),
];

/** The sample repository's branches and a dozen more, past what a selector's list shows without
 * scrolling (ten rows of its height). None starts with `main`, which `branchOption` finds by prefix. */
const CROWDED_BRANCHES: BranchInfo[] = [
  ...BRANCHES,
  ...[
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

const WORKTREES: WorktreeInfo[] = [
  { id: "wt-main", root: "/Users/dev/code/search-api", main: true, head: "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39", branch: "main", scope_present: true },
  { id: "wt-side", root: "/Users/dev/code/search-api-experiments", main: false, head: "9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807", branch: "feature/ranking-experiments", scope_present: true },
  { id: "wt-detached", root: "/Users/dev/.worktrees/search-api/review", main: false, head: "0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d", branch: null, scope_present: false },
];

/** The repository every gallery project's Git mode shows unless its scenario gives another. */
export const SAMPLE_GIT: FixtureGit = {
  worktrees: WORKTREES,
  changes: { "wt-main": MAIN_CHANGES, "wt-side": SIDE_CHANGES, "wt-detached": [] },
  branches: BRANCHES,
  comparisons: {
    "main..feature/ranking-experiments": FEATURE_COMPARISON,
    // Two branches at different commits whose difference lies wholly outside the project's folder.
    "release/2.4..feature/ranking-experiments": [],
    // `chore/update-dependencies` has commits of its own past the fork, which are not in the list.
    "chore/update-dependencies..feature/ranking-experiments": FEATURE_COMPARISON,
  },
  forks: { "chore/update-dependencies..feature/ranking-experiments": MAIN_TIP },
};

const { console: console_, web, api, sessions } = SAMPLE;
/** The session responsible for `feature/ranking-experiments`, checked out in the
 * side worktree. The other Git scenarios select `sessions[1]`, which is responsible for no branch,
 * so that the selectors start at their defaults. */
const rankingSession = sessionOf("s-ranking", console_.id, web.id, "Tune the ranking weights", "idle");
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions: [...sessions, rankingSession] });
const openGitFor = (session: { title: string }, view: "worktree" | "compare" = "worktree") => [
  (ui: Ui) => ui.press(ui.session(session.title)),
  (ui: Ui) => ui.wait(400),
  (ui: Ui) => ui.press(ui.t(view === "compare" ? "git.view.compare" : "git.view.worktree")),
  (ui: Ui) => ui.wait(400),
];
const openGit = openGitFor(sessions[1]);
const openCompare = openGitFor(sessions[1], "compare");
/** Matches a branch's option in a selector: its name followed by the 7 characters of its short
 * commit id. */
const branchOption = (name: string): Matcher => describing(`branchOption(${name})`, (n) => n.startsWith(name) && n.length === name.length + 7);

/** Opens the Compare view with `main` as From and the To selector open. */
const openToSelector = [
  ...openCompare,
  (ui: Ui) => ui.wait(300),
  (ui: Ui) => ui.press(ui.t("git.compare.choose")),
  (ui: Ui) => ui.press(branchOption("main")),
  (ui: Ui) => ui.wait(300),
  (ui: Ui) => ui.press(ui.t("git.compare.choose")),
  (ui: Ui) => ui.wait(400),
];

/** Shows the comparison of branch `left` with branch `right`, choosing each in its selector: From
 * is the first one still to choose, then To. */
const compare = (left: string, right: string) => [
  ...openCompare,
  (ui: Ui) => ui.wait(300),
  (ui: Ui) => ui.press(ui.t("git.compare.choose")),
  (ui: Ui) => ui.press(branchOption(left)),
  (ui: Ui) => ui.wait(300),
  (ui: Ui) => ui.press(ui.t("git.compare.choose")),
  (ui: Ui) => ui.press(branchOption(right)),
  (ui: Ui) => ui.wait(600),
];

/** Successful review targets do not carry hidden conflicts or mocked failures in their navigation. */
function normalRepository(repository: FixtureGit): FixtureGit {
  const normal = (changes: FixtureChange[]) => changes.filter(({ entry, error, expand }) => entry.group !== "conflicted" && !error && !expand?.error && !expand?.movedOn);
  return {
    ...repository,
    changes: Object.fromEntries(Object.entries(repository.changes).map(([id, changes]) => [id, Array.isArray(changes) ? normal(changes) : changes])),
    comparisons: repository.comparisons && Object.fromEntries(Object.entries(repository.comparisons).map(([id, changes]) => [id, normal(changes)])),
  };
}

const reviewScenarios: Scenario[] = [
  {
    id: "git-changes-conflicts", group: GROUP, category: "exceptional",
    title: "Conflicted changes and read failures",
    description: "Conflicted, staged, unstaged and untracked changes share the list. package.json is selected in Conflicted; open it for conflict markers. This repository deliberately includes read and expansion failures.",
    state, git: SAMPLE_GIT,
    steps: [...openGit, (ui) => ui.press(ui.changeRow("package.json", "conflicted")), (ui) => ui.wait(600), (ui) => ui.key("Escape")],
  },
  {
    id: "git-changes",
    group: GROUP,
    category: "normal",
    title: "A worktree's uncommitted changes",
    description: "Staged, unstaged and untracked changes share the list, with server.ts selected and its rounded highlight spaced like Files rows. Collapse a section to watch the following rows slide into place, and right-click a row for Open, Show in file manager and Copy path in the selected worktree. Choose Docked above the preview to inspect the 320px project pane at the 1168px minimum window width: tabs scroll horizontally when needed.",
    state,
    steps: [...openGit, (ui) => ui.press(ui.changeRow("server.ts", "modified")), (ui) => ui.wait(600), (ui) => ui.key("Escape")],
  },
  {
    id: "git-changes-sticky",
    group: GROUP,
    category: "normal",
    title: "A section heading pinned above flat changes",
    description: "The list starts partway through Staged: its heading stays against the worktree selector's bottom border while rows scroll beneath it. The top fade begins below the pinned band; scrolling farther lets the next section replace it.",
    state,
    steps: [...openGit, (ui) => ui.scrollCollection(ui.t("git.list.label"), 180)],
  },
  {
    id: "git-changes-tree-sticky",
    group: GROUP,
    category: "normal",
    title: "A section heading pinned above grouped changes",
    description: "As in the flat list, the folder-grouped list starts partway through Staged and its heading stays against the worktree selector's bottom border while directories and files scroll beneath it; scrolling farther lets the next section replace it.",
    state,
    preferences: { changeLayout: "tree" },
    steps: [...openGit, (ui) => ui.scrollCollection(ui.t("git.list.label"), 180)],
  },
  {
    id: "git-worktree-gone",
    group: GROUP,
    category: "exceptional",
    title: "A worktree that has gone",
    state,
    git: {
      ...SAMPLE_GIT,
      changes: {
        ...SAMPLE_GIT.changes,
        "wt-main": { code: "worktree_unavailable", params: { worktree: "wt-main" }, message: "worktree wt-main is not a worktree of this repository any more" },
      },
    },
    steps: openGit,
  },
  {
    id: "git-clean",
    group: GROUP,
    category: "normal",
    title: "A worktree with no uncommitted changes",
    state,
    git: { ...SAMPLE_GIT, changes: { ...SAMPLE_GIT.changes, "wt-main": [] } },
    steps: openGit,
  },
  {
    id: "git-not-a-repository",
    group: GROUP,
    category: "normal",
    title: "A project in no repository",
    state,
    git: null,
    steps: openGit,
  },
  {
    id: "git-unreadable",
    group: GROUP,
    category: "exceptional",
    title: "A repository Git cannot read",
    description: "The standalone failure has a danger alert with a heading and Git's own diagnostic as selectable code. Narrow shows the same failure after opening the project pane drawer.",
    state,
    git: { worktrees: [], changes: {}, gitError: "fatal: detected dubious ownership in repository at '/Users/dev/code/search-api'" },
    steps: openGit,
  },
  {
    id: "git-compare-many-branches",
    group: GROUP,
    category: "normal",
    title: "More branches than the selector shows",
    description:
      "The To selector open over a repository with more branches than its list holds: the list scrolls and its rows fade at the bottom edge, where more are waiting, while the popover around it keeps its edges. The Compare tab opens branch selectors that show every branch with no fade.",
    state,
    git: { ...SAMPLE_GIT, branches: CROWDED_BRANCHES },
    steps: openToSelector,
  },
  {
    id: "git-compare-no-branches",
    group: GROUP,
    category: "normal",
    title: "Comparing in a repository with no branches",
    description: "A repository with no commit yet has no branch to choose, so the Compare view says so instead of asking for two.",
    state,
    git: FRESH_GIT,
    steps: [
      ...openCompare,
      (ui: Ui) => ui.wait(400),
    ],
  },
  {
    id: "git-compare-moved",
    group: GROUP,
    category: "normal",
    title: "A compared branch that has moved since",
    description: "The branch list read after the comparison says feature/ranking-experiments is at another commit now; the comparison stays on the commits it was made at until Refresh.",
    state,
    git: { ...SAMPLE_GIT, moved: { "feature/ranking-experiments": "1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e" } },
    steps: [
      ...compare("main", "feature/ranking-experiments"),
      (ui) => ui.wait(10_500),
    ],
  },
  {
    id: "git-compare-moved-and-deleted",
    group: GROUP,
    category: "normal",
    title: "One compared branch moved and the other deleted",
    description: "Two notices under the commits line: main has moved, and feature/ranking-experiments no longer exists. The moved notice does not advise Refresh, which could not compare the deleted branch.",
    state,
    git: { ...SAMPLE_GIT, moved: { main: FEATURE_TIP }, gone: ["feature/ranking-experiments"] },
    steps: [
      ...compare("main", "feature/ranking-experiments"),
      (ui) => ui.wait(10_500),
    ],
  },
  {
    id: "git-compare-gone",
    group: GROUP,
    category: "exceptional",
    title: "A compared branch that no longer exists",
    description: "The comparison failure has a danger alert with a heading and a separate Try again control. Open the project pane drawer when using Narrow.",
    state,
    git: { ...SAMPLE_GIT, missing: ["release/2.4"] },
    steps: compare("main", "release/2.4"),
  },
  {
    id: "git-compare-forked",
    group: GROUP,
    category: "normal",
    title: "A comparison from a fork",
    description: "From has commits of its own after the fork. The header names the fork commit, not From's tip, and the list holds only what To changed since the fork.",
    state: state,
    steps: compare("chore/update-dependencies", "feature/ranking-experiments"),
  },
  {
    id: "git-compare-no-common-commit",
    group: GROUP,
    category: "exceptional",
    title: "Branches with no commit in common",
    description: "The comparison failure names both branches and says there is nothing to compare from.",
    state,
    git: { ...SAMPLE_GIT, branches: [...BRANCHES, { name: "gh-pages", commit: ORPHAN_TIP }], forks: { "main..gh-pages": null } },
    steps: compare("main", "gh-pages"),
  },
  {
    id: "git-compare-no-common-commit-shallow",
    group: GROUP,
    category: "exceptional",
    title: "No common commit found in a shallow clone",
    description: "The comparison failure words it as missing history of a shallow clone rather than unrelated branches.",
    state,
    git: { ...SAMPLE_GIT, branches: [...BRANCHES, { name: "gh-pages", commit: ORPHAN_TIP }], forks: { "main..gh-pages": null }, shallow: true },
    steps: compare("main", "gh-pages"),
  },
  {
    id: "git-compare-origins",
    group: GROUP,
    category: "normal",
    title: "Responsible-session badges on branches",
    state: state,
    description: "Each branch row's ring names the session working on it: filled in the accent colour for the selected session, plain for another session, and named as no longer existing for a removed one. Hover a ring for its tooltip.",
    steps: [
      ...openGitFor(rankingSession, "compare"),
      (ui: Ui) => ui.wait(300),
      (ui: Ui) => ui.press("main"),
      (ui: Ui) => ui.wait(400),
    ],
  },
  {
    id: "git-session-worktree",
    group: GROUP,
    category: "normal",
    title: "The selected session's worktree",
    state: state,
    description: "The selected session is responsible for feature/ranking-experiments, which the side worktree has checked out: the Worktree selector is on that worktree and lists its changes, not the project folder's.",
    steps: openGitFor(rankingSession),
  },
  {
    id: "git-session-compare",
    group: GROUP,
    category: "normal",
    title: "The selected session's branch compared from main",
    state: state,
    description: "From is the branch the main worktree has checked out and To the branch the selected session is responsible for: the list is what that branch changed since it forked from main. Right-clicking a comparison row offers Copy path only, because this historical view has no live file to open or reveal.",
    steps: [
      ...openGitFor(rankingSession, "compare"),
      (ui: Ui) => ui.wait(600),
    ],
  },
  {
    id: "git-changes-tree",
    group: GROUP,
    category: "normal",
    title: "Changes grouped by folder",
    description: "Directories start expanded and config.ts is selected inside src, its highlight spaced like the flat list's while file labels follow the folder indentation.",
    state: state,
    preferences: { changeLayout: "tree" },
    steps: [
      ...openGit,
      (ui) => ui.press(ui.changeRow("config.ts", "renamed")),
      (ui) => ui.wait(600),
      (ui) => ui.key("Escape"),
    ],
  },
  {
    id: "git-change-viewer",
    group: "File viewer",
    category: "normal",
    title: "Change viewer with collapsed lines",
    state: state,
    description: "The staged tag and long expansion gaps accompany the diff; other change subjects remain available through navigation.",
    steps: [
      ...openGit,
      (ui) => ui.press(ui.changeRow("pipeline.ts", "modified")),
      (ui) => ui.wait(800),
    ],
  },
  {
    id: "git-compare-same",
    group: GROUP,
    category: "normal",
    title: "Branches at the same commit",
    state: state,
    steps: [
      ...compare("main", "release/2.4"),
    ],
  },
  {
    id: "git-compare-empty",
    group: GROUP,
    category: "normal",
    title: "No changes inside the project",
    state: state,
    steps: [
      ...compare("release/2.4", "feature/ranking-experiments"),
    ],
  },
  {
    id: "git-changes-filter",
    group: GROUP,
    category: "normal",
    title: "Filtered changes",
    state: state,
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
    state: state,
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
