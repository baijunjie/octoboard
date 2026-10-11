// What the fixture daemon answers a project's Git mode with (`fixtureDaemon.ts`), and the gallery's
// "Project Git" scenarios over it: the worktree selector, the change list in its sections, a diff of
// each kind opened from it, and the states a source or a list can end in.
import type { BranchInfo, ChangeEntry, ChangeSide, ContentSource, FileContent, WorktreeInfo } from "../../protocol";
import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import type { FixtureError } from "./projectFiles";
import { SAMPLE, snapshotState, text } from "./builders";
import { IMAGES } from "./viewerImages";

const GROUP = "Project pane: Git";

/** One change of a fixture worktree: the entry its list carries, and what reading it answers — its
 * patch and its sides' bodies, or an error. */
export interface FixtureChange {
  entry: ChangeEntry;
  patch?: string;
  bodies?: { old?: FileContent; new?: FileContent };
  error?: FixtureError;
  /** What reading its whole bodies answers, for expanding its diff: both sides' text, or an error;
   * with `delay` the answer comes that many milliseconds late. */
  expand?: ({ old: string; new: string } | FixtureError) & { delay?: number };
}

/** A fixture project's repository: its worktrees, the first holding the project, and each one's
 * changes by its id; a worktree with an `error` in place of changes fails to list. With `gitError`,
 * the repository cannot be read at all. `branches` are its local branches; `comparisons` the
 * changes between two of them, keyed `<left>..<right>` (none for a pair not given), and `missing`
 * branches the list has that a comparison no longer finds. `moved` is what the branch list says of
 * a branch once a comparison has been made: a branch moved to another commit since. */
export interface FixtureGit {
  worktrees: WorktreeInfo[];
  changes: Record<string, FixtureChange[] | FixtureError>;
  gitError?: string;
  branches?: BranchInfo[];
  comparisons?: Record<string, FixtureChange[]>;
  missing?: string[];
  moved?: Record<string, string>;
}

const image = (data: string): FileContent => ({ size: Math.floor((data.length * 3) / 4), kind: "binary", media_type: "image/png", text: null, data });
const commit = (blob: string): ContentSource => ({ kind: "commit", commit: "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39", branch: null, blob });
const index = (blob: string): ContentSource => ({ kind: "index", worktree: "wt-main", blob });
const live = (version: string): ContentSource => ({ kind: "live", root_id: "fixture", version });
const file = (path: string, source: ContentSource): ChangeSide => ({ state: "present", path, kind: "file", source });

// Built line by line, so the blank context lines keep the space that starts them.
const SERVER_PATCH = [
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
].join("\n");

const SERVER_UNSTAGED_PATCH = [
  "diff --git a/src/server.ts b/src/server.ts",
  "index 5d6e7f8..9a8b7c6 100644",
  "--- a/src/server.ts",
  "+++ b/src/server.ts",
  '@@ -2,3 +2,5 @@ import http from "node:http";',
  ' import { logRequests } from "./log";',
  " ",
  " export const createServer = () => http.createServer(logRequests);",
  "+",
  "+export const PORT = 8080;",
  "",
].join("\n");

const RENAME_PATCH = `diff --git a/src/conf.ts b/src/config.ts
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
`;

const TYPE_CHANGE_PATCH = `diff --git a/current b/current
deleted file mode 100644
index 3b18e51..0000000
--- a/current
+++ /dev/null
@@ -1 +0,0 @@
-releases/2.3
diff --git a/current b/current
new file mode 120000
index 0000000..8d2f1a0
--- /dev/null
+++ b/current
@@ -0,0 +1 @@
+releases/2.4
\\ No newline at end of file
`;

const DELETED_PATCH = `diff --git a/docs/old-notes.md b/docs/old-notes.md
deleted file mode 100644
index 7c6b5a4..0000000
--- a/docs/old-notes.md
+++ /dev/null
@@ -1,2 +0,0 @@
-# Old notes
-Superseded by the architecture document.
`;

/** A source file of `count` numbered lines before and after the lines `edits` names (1-based) are
 * rewritten, and the unified patch between them as `git diff` writes it with three lines of context:
 * edits at most seven lines apart share a hunk, and the lines between hunks are what the viewer
 * collapses. */
function editedSource(path: string, count: number, edits: number[], tail = ""): { old: string; new: string; patch: string } {
  const before = Array.from({ length: count }, (_, i) => `  stages.push(stage${i + 1}(context)); // step ${i + 1}${tail}`);
  const after = before.map((line, i) => (edits.includes(i + 1) ? `  stages.push(await stage${i + 1}(context)); // step ${i + 1}, awaited${tail}` : line));
  const groups: number[][] = [];
  for (const edit of edits) {
    const last = groups[groups.length - 1];
    if (last && edit - last[last.length - 1] <= 7) last.push(edit);
    else groups.push([edit]);
  }
  const patch = [`diff --git a/${path} b/${path}`, "index 1a2b3c4..5d6e7f8 100644", `--- a/${path}`, `+++ b/${path}`];
  for (const group of groups) {
    const first = Math.max(group[0] - 3, 1);
    const last = Math.min(group[group.length - 1] + 3, count);
    patch.push(`@@ -${first},${last - first + 1} +${first},${last - first + 1} @@`);
    for (let n = first; n <= last; n++) {
      if (edits.includes(n)) patch.push(`-${before[n - 1]}`, `+${after[n - 1]}`);
      else patch.push(` ${before[n - 1]}`);
    }
  }
  return { old: `${before.join("\n")}\n`, new: `${after.join("\n")}\n`, patch: `${patch.join("\n")}\n` };
}

/** The lines of the long file the expansion scenarios change: a run of unmodified lines before the
 * first hunk, a long one between the hunks, and a short one after the last. */
const PIPELINE = editedSource("src/pipeline.ts", 120, [10, 70, 117]);

/** The same shape with every line far wider than a window, for the diff unwrapped. */
const WIDE = editedSource("src/pipeline.ts", 120, [10, 70, 117], ` // ${"a note that runs on well past the edge of the window ".repeat(8)}`);

/** How long reading the bodies of an expandable change takes, long enough to see it loading. */
const EXPAND_DELAY_MS = 600;

/** A change of a file whose diff can be expanded: `source` is what reading its bodies answers
 * (its text, or the `error` given). */
function expandable(
  group: "staged" | "unstaged" | "committed",
  path: string,
  from: ContentSource,
  to: ContentSource,
  source: { old: string; new: string; patch: string },
  error?: FixtureError,
): FixtureChange {
  return {
    entry: { group, old: file(path, from), new: file(path, to) },
    patch: source.patch.replaceAll("src/pipeline.ts", path),
    expand: { ...(error ?? { old: source.old, new: source.new }), delay: EXPAND_DELAY_MS },
  };
}

/** A file renamed into or out of a path below itself, which git writes as a removal and an addition
 * (the one whose path sorts first coming first); both sides are files, so its bodies can be read. */
function nested(from: string, to: string, before: string[], after: string[]): FixtureChange {
  const removal = [`diff --git a/${from} b/${from}`, "deleted file mode 100644", "index 3b18e51..0000000", `--- a/${from}`, "+++ /dev/null", `@@ -1,${before.length} +0,0 @@`, ...before.map((l) => `-${l}`)];
  const addition = [`diff --git a/${to} b/${to}`, "new file mode 100644", "index 0000000..9c8d7e6", "--- /dev/null", `+++ b/${to}`, `@@ -0,0 +1,${after.length} @@`, ...after.map((l) => `+${l}`)];
  const sections = from < to ? [removal, addition] : [addition, removal];
  return {
    entry: { group: "staged", old: file(from, commit("3b18e51")), new: file(to, index("9c8d7e6")) },
    patch: sections.flat().join("\n") + "\n",
    expand: { old: before.join("\n") + "\n", new: after.join("\n") + "\n", delay: EXPAND_DELAY_MS },
  };
}

const MAIN_CHANGES: FixtureChange[] = [
  { entry: { group: "staged", old: file("src/server.ts", commit("1a2b3c4")), new: file("src/server.ts", index("5d6e7f8")) }, patch: SERVER_PATCH },
  { entry: { group: "staged", old: file("src/conf.ts", commit("0f1e2d3")), new: file("src/config.ts", index("4c5b6a7")) }, patch: RENAME_PATCH },
  {
    entry: { group: "staged", old: { state: "out_of_scope", repository_path: "shared/search-index.ts" }, new: file("src/index.ts", index("2e3d4c5")) },
    bodies: { new: text("export const buildIndex = (docs: string[]) => new Map(docs.map((d, i) => [i, d]));\n") },
  },
  {
    entry: {
      group: "staged",
      old: file("current", commit("3b18e51")),
      new: { state: "present", path: "current", kind: "symlink", source: index("8d2f1a0") },
    },
    patch: TYPE_CHANGE_PATCH,
  },
  nested("nest", "nest/leaf.ts", ["const a = 1;", "const b = 2;", "const c = 3;"], ["const a = 1;", "const b = 20;", "const c = 3;"]),
  nested("nest2/leaf.ts", "nest2", ["const a = 1;", "const b = 2;", "const c = 3;"], ["const a = 1;", "const b = 2;", "const c = 3;"]),
  expandable("staged", "src/pipeline.ts", commit("a1b2c3d"), index("b2c3d4e"), PIPELINE),
  expandable("staged", "src/wide.ts", commit("9a8b7c6"), index("6c7d8e9"), WIDE),
  expandable("staged", "src/limits.ts", commit("c3d4e5f"), index("d4e5f6a"), PIPELINE, {
    code: "limit_exceeded",
    params: { limit: "file_bytes", size: "5242880", max: "4194304" },
    message: "the file is larger than the 4194304-byte limit",
  }),
  expandable("staged", "src/moved.ts", commit("e5f6a7b"), index("f6a7b8c"), PIPELINE, {
    code: "source_changed",
    params: {},
    message: "the file changed while it was being read",
  }),
  expandable("staged", "src/flaky.ts", commit("a7b8c9d"), index("b8c9d0e"), PIPELINE, {
    code: "git_failed",
    params: { detail: "fatal: unable to read the object" },
    message: "git failed: fatal: unable to read the object",
  }),
  { entry: { group: "conflicted", path: "package.json", conflict: "both_modified" } },
  { entry: { group: "unstaged", old: file("src/server.ts", index("5d6e7f8")), new: file("src/server.ts", live("v2")) }, patch: SERVER_UNSTAGED_PATCH },
  { entry: { group: "unstaged", old: file("docs/old-notes.md", index("7c6b5a4")), new: { state: "absent" } }, patch: DELETED_PATCH },
  {
    entry: { group: "unstaged", old: file("docs/logo.png", index("6a5b4c3")), new: file("docs/logo.png", live("v3")) },
    patch: "diff --git a/docs/logo.png b/docs/logo.png\nindex 6a5b4c3..0000000 100644\nBinary files a/docs/logo.png and b/docs/logo.png differ\n",
    bodies: { old: image(IMAGES.before_png.data), new: image(IMAGES.after_png.data) },
  },
  {
    entry: { group: "unstaged", old: file("data/fixtures.bin", index("1f2e3d4")), new: file("data/fixtures.bin", live("v4")) },
    error: {
      code: "limit_exceeded",
      params: { limit: "patch_bytes", max: "4194304" },
      message: "the change's patch is larger than the 4194304-byte limit",
    },
  },
  { entry: { group: "untracked", old: { state: "absent" }, new: file("src/log.ts", live("v5")) }, bodies: { new: text("export const logRequests = () => {};\n") } },
  // Two directories that each hold only the next: the tree shows them as one row, `tools/release`.
  { entry: { group: "untracked", old: { state: "absent" }, new: file("tools/release/notes.md", live("v7")) }, bodies: { new: text("# Release notes\n") } },
];

const SIDE_CHANGES: FixtureChange[] = [
  { entry: { group: "unstaged", old: file("README.md", index("aa11bb2")), new: file("README.md", live("v6")) }, patch: "diff --git a/README.md b/README.md\nindex aa11bb2..cc33dd4 100644\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-# Search API\n+# Search API (experimental branch)\n" },
];

const MAIN_TIP = "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39";
const FEATURE_TIP = "9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807";
const LONG_BRANCH = "feature/rework-the-ranking-pipeline-so-that-boosts-are-applied-after-deduplication";

const BRANCHES: BranchInfo[] = [
  { name: "feature/ranking-experiments", commit: FEATURE_TIP },
  { name: LONG_BRANCH, commit: "5b4a3f2e1d0c9b8a7f6e5d4c3b2a1f0e9d8c7b6a" },
  { name: "main", commit: MAIN_TIP },
  { name: "release/2.4", commit: MAIN_TIP },
];

const at = (commit: string) => (blob: string): ContentSource => ({ kind: "commit", commit, branch: null, blob });
const onMain = at(MAIN_TIP);
const onFeature = at(FEATURE_TIP);

const RANKING_PATCH = [
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
].join("\n");

/** The changes from `main` to `feature/ranking-experiments`. */
const FEATURE_COMPARISON: FixtureChange[] = [
  { entry: { group: "committed", old: file("src/ranking.ts", onMain("3c4d5e6")), new: file("src/ranking.ts", onFeature("7f8a9b0")) }, patch: RANKING_PATCH },
  { entry: { group: "committed", old: file("src/conf.ts", onMain("0f1e2d3")), new: file("src/config.ts", onFeature("4c5b6a7")) }, patch: RENAME_PATCH },
  {
    entry: { group: "committed", old: { state: "out_of_scope", repository_path: "shared/search-index.ts" }, new: file("src/index.ts", onFeature("2e3d4c5")) },
    bodies: { new: text("export const buildIndex = (docs: string[]) => new Map(docs.map((d, i) => [i, d]));\n") },
  },
  expandable("committed", "src/pipeline.ts", onMain("a1b2c3d"), onFeature("b2c3d4e"), PIPELINE),
  { entry: { group: "committed", old: file("docs/old-notes.md", onMain("7c6b5a4")), new: { state: "absent" } }, patch: DELETED_PATCH },
  {
    entry: { group: "committed", old: file("docs/logo.png", onMain("6a5b4c3")), new: file("docs/logo.png", onFeature("8b7c6d5")) },
    patch: "diff --git a/docs/logo.png b/docs/logo.png\nindex 6a5b4c3..8b7c6d5 100644\nBinary files a/docs/logo.png and b/docs/logo.png differ\n",
    bodies: { old: image(IMAGES.before_png.data), new: image(IMAGES.after_png.data) },
  },
  {
    entry: {
      group: "committed",
      old: file("current", onMain("3b18e51")),
      new: { state: "present", path: "current", kind: "symlink", source: onFeature("8d2f1a0") },
    },
    patch: TYPE_CHANGE_PATCH,
  },
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
  (ui: Ui) => ui.press(ui.t("rail.projectPane.show")),
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
    id: "git-expand-wide",
    group: GROUP,
    title: "A change with collapsed lines wider than the window",
    description:
      "src/wide.ts, unwrapped, has lines far wider than the dialog. Show whole file stays in view at the end of the bar while the diff is at its left edge, as the label and the chevrons do. A last step switches to the split layout, where each side scrolls on its own and the control sits at the inner edge of the left half.",
    width: 760,
    state,
    steps: [...openGitInDrawer, (ui: Ui) => ui.press(/^wide\.ts, modified/), (ui: Ui) => ui.wait(800), (ui: Ui) => ui.press(/^Split$/), (ui: Ui) => ui.wait(400)],
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
    description: "Expanding a separator finds the file past the limit: the separators keep their form and offer nothing more.",
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
