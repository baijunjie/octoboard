// The sample repository the fixture daemon answers a project's Git mode with (`fixtureDaemon.ts`),
// and the gallery's "Project Git" scenarios over it: the worktree selector, the change list in its
// sections, a diff of each kind opened from it, and the states a source or a list can end in. Every
// change here is built by the factory for its kind (`changeFixtures.ts`).
import type { BranchInfo, ContentSource, FileContent, WorktreeInfo } from "../../protocol";
import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import type { FixtureChange, FixtureGit, Versions } from "./changeFixtures";
import type { FixtureError } from "./projectFiles";
import { SAMPLE, snapshotState } from "./builders";
import {
  EXPAND_DELAY_MS,
  binaryChange,
  conflicted,
  deletion,
  editedSource,
  expandable,
  fromOutsideTheProject,
  hashedOnDisk,
  nested,
  refused,
  renamedTo,
  renamedUnchanged,
  textChange,
  typeChange,
  unchanged,
  untracked,
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
  nested("staged", "nest", "nest/leaf.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE, 97),
  renamedUnchanged("staged", "nest2/leaf.ts", "nest2", commit(PIPELINE_IDS.new), index(PIPELINE_IDS.new), { content: PIPELINE.new, delay: EXPAND_DELAY_MS }),
  expandable("staged", "src/pipeline.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE),
  expandable("staged", "src/tail.ts", commit(PIPELINE_IDS.old), index("2d3e4f5"), PIPELINE_TAIL),
  expandable("staged", "src/stages.ts", commit("5e6f7a8"), index("7a8b9c0"), SHORT_GAP),
  expandable("staged", "src/huge.ts", commit("1d2e3f4"), index("4f5a6b7"), HUGE),
  expandable("staged", "src/wide.ts", commit("9a8b7c6"), index("6c7d8e9"), WIDE),
  expandable("staged", "src/limits.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE, {
    code: "limit_exceeded",
    params: { limit: "file_bytes", size: "5242880", max: "4194304" },
    message: "the file is larger than the 4194304-byte limit",
  }),
  expandable("staged", "src/moved.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE, {
    code: "source_changed",
    params: {},
    message: "the file changed while it was being read",
  }),
  expandable("staged", "src/flaky.ts", commit(PIPELINE_IDS.old), index(PIPELINE_IDS.new), PIPELINE, UNREADABLE),
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
];

const SIDE_CHANGES: FixtureChange[] = [textChange("unstaged", "README.md", index("aa11bb2"), live("v6"), README_EDIT)];

const MAIN_TIP = "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39";
const FEATURE_TIP = "9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807";
const LONG_BRANCH = "feature/rework-the-ranking-pipeline-so-that-boosts-are-applied-after-deduplication";

const BRANCHES: BranchInfo[] = [
  { name: "feature/ranking-experiments", commit: FEATURE_TIP },
  { name: LONG_BRANCH, commit: "5b4a3f2e1d0c9b8a7f6e5d4c3b2a1f0e9d8c7b6a" },
  { name: "main", commit: MAIN_TIP },
  { name: "release/2.4", commit: MAIN_TIP },
];

const at = (commit: string) => (blob: string) => ({ kind: "commit", commit, branch: null, blob }) as const;
const onMain = at(MAIN_TIP);
const onFeature = at(FEATURE_TIP);

/** The changes from `main` to `feature/ranking-experiments`. */
const FEATURE_COMPARISON: FixtureChange[] = [
  textChange("committed", "src/ranking.ts", onMain("3c4d5e6"), onFeature("7f8a9b0"), RANKING_EDIT),
  renamedTo("committed", "src/conf.ts", "src/config.ts", onMain("0f1e2d3"), onFeature("4c5b6a7"), CONF_RENAME),
  fromOutsideTheProject("committed", "shared/search-index.ts", "src/index.ts", onFeature("2e3d4c5"), INDEX_BODY),
  expandable("committed", "src/pipeline.ts", onMain(PIPELINE_IDS.old), onFeature(PIPELINE_IDS.new), PIPELINE),
  deletion("committed", "docs/old-notes.md", onMain("7c6b5a4"), DELETED_NOTES),
  binaryChange("committed", "docs/logo.png", onMain("6a5b4c3"), onFeature("8b7c6d5"), LOGO_BODIES),
  typeChange("committed", "current", onMain("0d1c2b3"), onFeature("8d2f1a0"), CURRENT_AS_FILE, "releases/2.4"),
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
  comparisons: { "main..feature/ranking-experiments": FEATURE_COMPARISON },
};

const { console: console_, web, api, sessions } = SAMPLE;
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });
const openGit = [
  (ui: Ui) => ui.press(ui.session("Fix the summary layout")),
  (ui: Ui) => ui.wait(400),
  (ui: Ui) => ui.press(ui.t("browser.mode.git")),
  (ui: Ui) => ui.wait(400),
];
const openChange = (name: RegExp) => [...openGit, (ui: Ui) => ui.press(name), (ui: Ui) => ui.wait(800)];
/** Opens Git mode in a narrow window, where the project pane is a drawer. */
const openGitInDrawer = [
  (ui: Ui) => ui.press(ui.t("titleBar.sidebar.show")),
  (ui: Ui) => ui.press(ui.session("Fix the summary layout")),
  (ui: Ui) => ui.press(ui.t("titleBar.projectPane.show")),
  (ui: Ui) => ui.wait(500),
  (ui: Ui) => ui.press(ui.t("browser.mode.git")),
  (ui: Ui) => ui.wait(400),
];
/** Types `text` in the change list's filter field, leaving focus in it. */
const filterBy = (text: string) => [
  (ui: Ui) => ui.focus(ui.t("git.filter.label")),
  (ui: Ui) => ui.type(text),
  (ui: Ui) => ui.wait(300),
];

/** Shows the comparison of branch `left` with branch `right`, choosing each in its selector: From
 * is the first one still to choose, then To. */
const compare = (left: string, right: string) => [
  ...openGit,
  (ui: Ui) => ui.press(ui.t("git.view.compare")),
  (ui: Ui) => ui.wait(300),
  (ui: Ui) => ui.press(ui.t("git.compare.choose")),
  (ui: Ui) => ui.press((name) => name.startsWith(left) && name.length === left.length + 7),
  (ui: Ui) => ui.wait(300),
  (ui: Ui) => ui.press(ui.t("git.compare.choose")),
  (ui: Ui) => ui.press((name) => name.startsWith(right) && name.length === right.length + 7),
  (ui: Ui) => ui.wait(600),
];

export const projectGitScenarios: Scenario[] = [
  {
    id: "git-changes",
    group: GROUP,
    title: "A worktree's uncommitted changes",
    description:
      "The Git mode: the worktree selector, then the changes in sections — staged, conflicted, unstaged, untracked. src/server.ts is in both Staged and Unstaged, as two changes. Press one to open its diff; Left and Right move through the list across sections.",
    width: 1440,
    state,
    steps: openGit,
  },
  { id: "git-diff", group: GROUP, title: "A staged change's diff", width: 1440, state, steps: openChange(/^server\.ts, modified, src$/) },
  { id: "git-rename-outside", group: GROUP, title: "A rename from outside the project", width: 1440, state, steps: openChange(/^index\.ts, renamed/) },
  { id: "git-type-change", group: GROUP, title: "A file that became a symbolic link", width: 1440, state, steps: openChange(/^current, type changed/) },
  { id: "git-nested-into", group: GROUP, title: "A file renamed into a path below itself", width: 1440, state, steps: openChange(/^leaf\.ts, renamed/) },
  { id: "git-nested-out", group: GROUP, title: "A file renamed out of a path below itself", width: 1440, state, steps: openChange(/^nest2, renamed/) },
  {
    id: "git-unchanged-content",
    group: GROUP,
    title: "A change whose two sides hold the same content",
    description:
      "src/same.ts has an empty patch, so there is no diff to draw; its content is read as a diff's collapsed lines are and shown whole, under a notice saying nothing changed.",
    width: 1440,
    state,
    steps: openChange(/^same\.ts, modified/),
  },
  {
    id: "git-unchanged-unreadable",
    group: GROUP,
    title: "A rename with no change whose content cannot be read",
    description: "The read of src/new-name.ts fails, so the viewer falls back to saying the rename changed nothing, with the reason above it.",
    width: 1440,
    state,
    steps: openChange(/^new-name\.ts, renamed/),
  },
  {
    id: "git-expand",
    group: GROUP,
    title: "A change with collapsed lines to expand",
    description:
      "src/pipeline.ts has three hunks with unmodified lines between them. Each expansion of a separator reveals 20 lines, the third reveals the rest of the gap, and Show whole file opens everything.",
    width: 1440,
    state,
    steps: openChange(/^pipeline\.ts, modified/),
  },
  {
    id: "git-expand-short-gap",
    group: GROUP,
    title: "A change with a run short enough to reveal at once",
    description:
      "src/stages.ts has runs of unmodified lines of 6, 3, 9, 27 and 7 lines. The 27-line run between the third and fourth hunks offers the pair of controls; each of the others is no more than one expansion reveals, or lies before the first hunk or after the last, and offers one control, which shows all of it.",
    width: 1440,
    state,
    steps: openChange(/^stages\.ts, modified/),
  },
  {
    id: "git-expand-over-budget",
    group: GROUP,
    title: "A change whose files are past the viewer's limit",
    description:
      "src/huge.ts holds more lines than the viewer renders, though its patch is small: expanding a separator keeps the lines collapsed and says the file is too large to show in full.",
    width: 1440,
    state,
    steps: openChange(/^huge\.ts, modified/),
  },
  {
    id: "git-expand-wide",
    group: GROUP,
    title: "A change with collapsed lines wider than the window",
    description:
      "src/wide.ts, unwrapped, has lines far wider than the dialog. Show whole file stays in view at the end of the bar while the diff is at its left edge, as the label and the chevrons do. A last step switches to the split layout, where each side scrolls on its own and the control sits at the inner edge of the left half.",
    width: 760,
    state,
    steps: [...openGitInDrawer, (ui: Ui) => ui.press(/^wide\.ts, modified/), (ui: Ui) => ui.wait(800), (ui: Ui) => ui.press(ui.t("viewer.layout.split")), (ui: Ui) => ui.wait(400)],
  },
  {
    id: "git-expand-narrow",
    group: GROUP,
    title: "A change with collapsed lines in a narrow window",
    description:
      "src/wide.ts, unwrapped and unified, in a window too narrow for the longest separator label beside Show whole file: the label ends in an ellipsis before the control, which stays clear of it and reachable.",
    width: 460,
    state,
    steps: [...openGitInDrawer, (ui: Ui) => ui.press(/^wide\.ts, modified/), (ui: Ui) => ui.wait(800)],
  },
  {
    id: "git-expand-too-large",
    group: GROUP,
    title: "A change whose files are too large to expand",
    description: "Expanding a separator finds the file past the limit: the lines stay collapsed, with a note saying the daemon does not provide the whole file.",
    width: 1440,
    state,
    steps: openChange(/^limits\.ts, modified/),
  },
  {
    id: "git-expand-moved",
    group: GROUP,
    title: "A change that moved on before it was expanded",
    description: "Expanding a separator finds the file changed since its patch was read: the patch stays as it is, with a note.",
    width: 1440,
    state,
    steps: openChange(/^moved\.ts, modified/),
  },
  {
    id: "git-expand-failed",
    group: GROUP,
    title: "A change whose lines could not be read",
    description: "Expanding a separator fails to read the file; the separator says so and can be tried again.",
    width: 1440,
    state,
    steps: openChange(/^flaky\.ts, modified/),
  },
  { id: "git-image", group: GROUP, title: "An image change", width: 1440, state, steps: openChange(/^logo\.png, modified/) },
  { id: "git-too-large", group: GROUP, title: "A change whose patch is too large", width: 1440, state, steps: openChange(/^fixtures\.bin, modified/) },
  { id: "git-untracked", group: GROUP, title: "An untracked file", width: 1440, state, steps: openChange(/^log\.ts, untracked/) },
  { id: "git-conflict", group: GROUP, title: "A file in conflict", width: 1440, state, steps: openChange(/^package\.json, in conflict/) },
  {
    id: "git-worktrees",
    group: GROUP,
    title: "Choosing a worktree",
    width: 1440,
    state,
    steps: [...openGit, (ui) => ui.press(/^main/), (ui) => ui.wait(400)],
  },
  {
    id: "git-worktree-gone",
    group: GROUP,
    title: "A worktree that has gone",
    width: 1440,
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
  { id: "git-not-a-repository", group: GROUP, title: "A project in no repository", width: 1440, state, git: null, steps: openGit },
  {
    id: "git-unreadable",
    group: GROUP,
    title: "A repository Git cannot read",
    width: 1440,
    state,
    git: { worktrees: [], changes: {}, gitError: "fatal: detected dubious ownership in repository at '/Users/dev/code/search-api'" },
    steps: openGit,
  },
  {
    id: "git-changes-tree",
    group: GROUP,
    title: "A worktree's changes as a tree",
    description:
      "The same sections with their changes grouped by folder; tools/release is a chain of single-child folders shown as one row. Arrow keys move, Left and Right fold and unfold a folder, and the viewer's Left and Right skip the changes of a folded one.",
    width: 1440,
    state,
    preferences: { changeLayout: "tree" },
    steps: openGit,
  },
  {
    id: "git-changes-filter",
    group: GROUP,
    title: "A worktree's changes filtered by file name",
    description:
      "\"server\" typed in the field above the list: only the changes whose file name contains it remain, in their sections, and the sections' counts follow.",
    width: 1440,
    state,
    steps: [...openGit, ...filterBy("server")],
  },
  {
    id: "git-changes-filter-tree",
    group: GROUP,
    title: "A worktree's changes as a tree, filtered by file name",
    description: "\"notes\" typed in the field: the tree keeps only the folders that lead to a match.",
    width: 1440,
    state,
    preferences: { changeLayout: "tree" },
    steps: [...openGit, ...filterBy("notes")],
  },
  {
    id: "git-changes-filter-empty",
    group: GROUP,
    title: "A filter no file name matches",
    description: "The list says no change matches; the field stays, so the text can be changed or cleared.",
    width: 1440,
    state,
    steps: [...openGit, ...filterBy("no-such-file")],
  },
  {
    id: "git-compare-filter",
    group: GROUP,
    title: "A comparison's changes filtered by file name",
    width: 1440,
    state,
    steps: [...compare("main", "feature/ranking-experiments"), ...filterBy("rank")],
  },
  {
    id: "git-compare",
    group: GROUP,
    title: "Comparing two branches",
    description:
      "The Git mode's Compare view: From main To feature/ranking-experiments, the commits compared, and the changed files. Press one to open its diff, read from the two commits.",
    width: 1440,
    state,
    steps: compare("main", "feature/ranking-experiments"),
  },
  {
    id: "git-compare-tree",
    group: GROUP,
    title: "Comparing two branches, as a tree",
    width: 1440,
    state,
    preferences: { changeLayout: "tree" },
    steps: compare("main", "feature/ranking-experiments"),
  },
  {
    id: "git-compare-diff",
    group: GROUP,
    title: "A change between two branches",
    width: 1440,
    state,
    steps: [...compare("main", "feature/ranking-experiments"), (ui) => ui.press(/^ranking\.ts, modified/), (ui) => ui.wait(800)],
  },
  {
    id: "git-compare-type-change",
    group: GROUP,
    title: "A file that became a symbolic link, between two branches",
    width: 1440,
    state,
    steps: [...compare("main", "feature/ranking-experiments"), (ui) => ui.press(/^current, type changed/), (ui) => ui.wait(800)],
  },
  {
    id: "git-compare-expand",
    group: GROUP,
    title: "A change between two branches, expanded",
    description: "src/pipeline.ts between the two commits: its collapsed lines are read from the same two commits.",
    width: 1440,
    state,
    steps: [...compare("main", "feature/ranking-experiments"), (ui) => ui.press(/^pipeline\.ts, modified/), (ui) => ui.wait(800)],
  },
  {
    id: "git-compare-same",
    group: GROUP,
    title: "Two branches at the same commit",
    width: 1440,
    state,
    steps: compare("main", "release/2.4"),
  },
  {
    id: "git-compare-moved",
    group: GROUP,
    title: "A compared branch that has moved since",
    description: "The branch list read after the comparison says feature/ranking-experiments is at another commit now; the comparison stays on the commits it was made at until Refresh.",
    width: 1440,
    state,
    git: { ...SAMPLE_GIT, moved: { "feature/ranking-experiments": "1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e" } },
    steps: [...compare("main", "feature/ranking-experiments"), (ui) => ui.wait(10_500)],
  },
  {
    id: "git-compare-gone",
    group: GROUP,
    title: "A compared branch that no longer exists",
    width: 1440,
    state,
    git: { ...SAMPLE_GIT, missing: ["release/2.4"] },
    steps: compare("main", "release/2.4"),
  },
  {
    id: "git-compare-narrow",
    group: GROUP,
    title: "Narrow window, comparing a long-named branch",
    width: 800,
    state,
    steps: [
      ...openGitInDrawer,
      (ui) => ui.press(ui.t("git.view.compare")),
      (ui) => ui.press(ui.t("git.compare.choose")),
      (ui) => ui.press((name) => name.startsWith("main") && name.length === 11),
      (ui) => ui.press(ui.t("git.compare.choose")),
      (ui) => ui.press((name) => name.startsWith(LONG_BRANCH)),
      (ui) => ui.wait(600),
    ],
  },
  {
    id: "git-narrow-filter-escape",
    group: GROUP,
    title: "Narrow window, Escape in the change filter",
    description: "\"server\" typed in the filter field, then Escape: the field clears and the drawer stays open.",
    width: 800,
    state,
    steps: [...openGitInDrawer, ...filterBy("server"), (ui) => ui.key("Escape")],
  },
  {
    id: "git-narrow-filter-escape-twice",
    group: GROUP,
    title: "Narrow window, a second Escape in the emptied change filter",
    description: "Escape clears the filter, and Escape again, the field being empty, closes the drawer.",
    width: 800,
    state,
    steps: [...openGitInDrawer, ...filterBy("server"), (ui) => ui.key("Escape"), (ui) => ui.key("Escape")],
  },
  {
    id: "git-narrow",
    group: GROUP,
    title: "Narrow window, Git mode in the drawer",
    width: 800,
    state,
    steps: openGitInDrawer,
  },
  {
    id: "git-narrow-tree",
    group: GROUP,
    title: "Narrow window, changes as a tree in the drawer",
    width: 800,
    state,
    preferences: { changeLayout: "tree" },
    steps: openGitInDrawer,
  },
];
