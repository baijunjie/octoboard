// The sample repository the fixture daemon answers a project's Git mode with (`fixtureGit.ts`) unless its
// fixture gives another. Every change here is built by the factory for its kind (`changeFixtures.ts`).
import type { BranchInfo, ContentSource, FileContent, WorktreeInfo } from "../protocol";
import type { FixtureChange, FixtureCommit, FixtureGit, Versions } from "./changeFixtures";
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
import type { FixtureError } from "./sampleFiles";
import { IMAGES } from "./viewerImages";

const image = (data: string): FileContent => ({ size: Math.floor((data.length * 3) / 4), kind: "binary", media_type: "image/png", text: null, data });
export const commit = (blob: string) => ({ kind: "commit", commit: "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39", branch: null, blob }) as const;
export const index = (blob: string, worktree = "wt-main") => ({ kind: "index", worktree, blob }) as const;
export const live = (version: string): ContentSource => ({ kind: "live", root_id: "fixture", version });

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
export const SERVER_ON_DISK: Versions = {
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

const SIDE_CHANGES: FixtureChange[] = [textChange("unstaged", "README.md", index("aa11bb2", "wt-side"), live("v6"), README_EDIT)];

export const MAIN_TIP = "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39";
export const FEATURE_TIP = "9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807";
const CHORE_TIP = "6d5c4b3a2f1e0d9c8b7a6f5e4d3c2b1a0f9e8d7c";
const LONG_BRANCH = "feature/rework-the-ranking-pipeline-so-that-boosts-are-applied-after-deduplication";

export const BRANCHES: BranchInfo[] = [
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
    commit: CHORE_TIP,
    // A live session that is not the selected one in most scenarios: the plain ring. The title
    // recorded when it became responsible has been superseded by the session's own, which is the
    // one the badge shows. No worktree has the branch checked out.
    origin: { responsible_session: "s-web-2", responsible_title: "Bump the packages", responsible_since: 1_758_000_000_000 },
  },
  // Pushed up to its tip, as far as the last fetch knows.
  { name: "main", commit: MAIN_TIP, upstream: { name: "origin/main", commit: MAIN_TIP } },
  { name: "release/2.4", commit: MAIN_TIP },
];

export const at = (commit: string) => (blob: string) => ({ kind: "commit", commit, branch: null, blob }) as const;

/** The commits of `feature/ranking-experiments` since it forked from `main`, newest first. The tip
 * is a merge: its first parent is `boost`, and it took in `root`, the commit of an unrelated
 * history the ranking notes were imported from, which has no parent of its own. */
const RANKING = {
  merge: FEATURE_TIP,
  boost: "a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b8",
  root: "5e4d3c2b1a0f9e8d7c6b5a4f3e2d1c0b9a817060",
  cache: "8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a29180706",
  cleanup: "6f5e4d3c2b1a0f9e8d7c6b5a4f3e2d1c0b9a8170",
};
const NOTES_BODY = "# Ranking notes\nImported from the archived repository.\n";

export const onMain = at(MAIN_TIP);
export const onFeature = at(FEATURE_TIP);

/** What `feature/ranking-experiments` changed since it forked from `main`, the commits' changes
 * together: the Committed section of its Changes view. */
const FEATURE_COMPARISON: FixtureChange[] = [
  textChange("committed", "src/ranking.ts", onMain("3c4d5e6"), onFeature("7f8a9b0"), RANKING_EDIT),
  renamedTo("committed", "src/conf.ts", "src/config.ts", onMain("0f1e2d3"), onFeature("4c5b6a7"), CONF_RENAME),
  fromOutsideTheProject("committed", "shared/search-index.ts", "src/index.ts", onFeature("2e3d4c5"), INDEX_BODY),
  expandable("committed", "src/pipeline.ts", onMain(PIPELINE_IDS.old), onFeature(PIPELINE_IDS.new), PIPELINE),
  addition("committed", "src/cache.ts", onFeature("5a6b7c8"), CACHE_BODY),
  addition("committed", "docs/ranking-notes.md", onFeature("9c0d1e2"), NOTES_BODY),
  deletion("committed", "docs/old-notes.md", onMain("7c6b5a4"), DELETED_NOTES),
  binaryChange("committed", "docs/logo.png", onMain("6a5b4c3"), onFeature("8b7c6d5"), LOGO_BODIES),
  typeChange("committed", "current", onMain("0d1c2b3"), onFeature("8d2f1a0"), CURRENT_AS_FILE, "releases/2.4"),
];

const rankingCommits = (): FixtureCommit[] => {
  const merge = at(RANKING.merge);
  const boost = at(RANKING.boost);
  const cache = at(RANKING.cache);
  const root = at(RANKING.root);
  const cleanup = at(RANKING.cleanup);
  // The oldest commit's parent is the fork itself, the tip of `main`.
  const before = MAIN_TIP;
  return [
    {
      commit: RANKING.merge,
      parent: RANKING.boost,
      merged: RANKING.root,
      subject: "Merge the ranking notes from the archived repository",
      changes: [addition("committed", "docs/ranking-notes.md", merge("9c0d1e2"), NOTES_BODY)],
    },
    {
      commit: RANKING.boost,
      parent: RANKING.cache,
      subject: "Apply boosts after deduplication",
      changes: [
        textChange("committed", "src/ranking.ts", cache("3c4d5e6"), boost("7f8a9b0"), RANKING_EDIT),
        expandable("committed", "src/pipeline.ts", cache(PIPELINE_IDS.old), boost(PIPELINE_IDS.new), PIPELINE),
      ],
    },
    {
      commit: RANKING.root,
      parent: null,
      subject: "Import the ranking notes from the archived repository",
      changes: [addition("committed", "docs/ranking-notes.md", root("9c0d1e2"), NOTES_BODY)],
    },
    {
      commit: RANKING.cache,
      parent: RANKING.cleanup,
      subject: "Cache query results",
      changes: [
        addition("committed", "src/cache.ts", cache("5a6b7c8"), CACHE_BODY),
        fromOutsideTheProject("committed", "shared/search-index.ts", "src/index.ts", cache("2e3d4c5"), INDEX_BODY),
      ],
    },
    {
      commit: RANKING.cleanup,
      parent: before,
      subject: "Move the loader to config.ts and clear out the old files",
      changes: [
        renamedTo("committed", "src/conf.ts", "src/config.ts", at(before)("0f1e2d3"), cleanup("4c5b6a7"), CONF_RENAME),
        deletion("committed", "docs/old-notes.md", at(before)("7c6b5a4"), DELETED_NOTES),
        binaryChange("committed", "docs/logo.png", at(before)("6a5b4c3"), cleanup("8b7c6d5"), LOGO_BODIES),
        typeChange("committed", "current", at(before)("0d1c2b3"), cleanup("8d2f1a0"), CURRENT_AS_FILE, "releases/2.4"),
      ],
    },
  ];
};

/** What `chore/update-dependencies` changed since it forked from `main`, which has moved on since
 * (`CHORE_FORK` is older than main's tip): two commits of its own. */
const CHORE_FORK = "2d1c0b9a8f7e6d5c4b3a291807e8d9f0a1b2c3d4";
const CHORE_FIRST = "3e2d1c0b9a8f7e6d5c4b3a291807e8d9f0a1b2c3";

const PACKAGE_BUMP: Versions = {
  old: '{\n  "name": "search-api",\n  "devDependencies": {\n    "vitest": "^2.0.0"\n  }\n}\n',
  new: '{\n  "name": "search-api",\n  "devDependencies": {\n    "vitest": "^3.1.0"\n  }\n}\n',
  patch: [
    "diff --git a/package.json b/package.json",
    "index 1d3f5a7..2e4a6b8 100644",
    "--- a/package.json",
    "+++ b/package.json",
    "@@ -1,6 +1,6 @@",
    " {",
    '   "name": "search-api",',
    '   "devDependencies": {',
    '-    "vitest": "^2.0.0"',
    '+    "vitest": "^3.1.0"',
    "   }",
    " }",
    "",
  ].join("\n"),
};

const TSCONFIG_TARGET: Versions = {
  old: '{\n  "compilerOptions": {\n    "target": "ES2020",\n    "strict": true\n  }\n}\n',
  new: '{\n  "compilerOptions": {\n    "target": "ES2022",\n    "strict": true\n  }\n}\n',
  patch: [
    "diff --git a/tsconfig.json b/tsconfig.json",
    "index 3f5b7c9..4a6c8d0 100644",
    "--- a/tsconfig.json",
    "+++ b/tsconfig.json",
    "@@ -1,6 +1,6 @@",
    " {",
    '   "compilerOptions": {',
    '-    "target": "ES2020",',
    '+    "target": "ES2022",',
    '     "strict": true',
    "   }",
    " }",
    "",
  ].join("\n"),
};

const CHORE_COMPARISON: FixtureChange[] = [
  textChange("committed", "package.json", at(CHORE_FORK)("1d3f5a7"), at(CHORE_TIP)("2e4a6b8"), PACKAGE_BUMP),
  textChange("committed", "tsconfig.json", at(CHORE_FORK)("3f5b7c9"), at(CHORE_TIP)("4a6c8d0"), TSCONFIG_TARGET),
];

const CHORE_COMMITS: FixtureCommit[] = [
  {
    commit: CHORE_TIP,
    parent: CHORE_FIRST,
    subject: "Bump the test runner",
    changes: [textChange("committed", "package.json", at(CHORE_FIRST)("1d3f5a7"), at(CHORE_TIP)("2e4a6b8"), PACKAGE_BUMP)],
  },
  {
    commit: CHORE_FIRST,
    parent: CHORE_FORK,
    subject: "Raise the TypeScript target",
    changes: [textChange("committed", "tsconfig.json", at(CHORE_FORK)("3f5b7c9"), at(CHORE_FIRST)("4a6c8d0"), TSCONFIG_TARGET)],
  },
];

export const WORKTREES: WorktreeInfo[] = [
  { id: "wt-main", root: "/Users/dev/code/search-api", main: true, head: "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39", branch: "main", scope_present: true },
  { id: "wt-side", root: "/Users/dev/code/search-api-experiments", main: false, head: "9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807", branch: "feature/ranking-experiments", scope_present: true },
  { id: "wt-detached", root: "/Users/dev/.worktrees/search-api/review", main: false, head: "0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d", branch: null, scope_present: false, scope_cause: "missing" },
];

/** The repository every gallery project's Git mode shows unless its scenario gives another: the
 * project's folder is the main working copy, on `main`; `feature/ranking-experiments` is checked
 * out in a side worktree, and `chore/update-dependencies` in none. */
export const SAMPLE_GIT: FixtureGit = {
  worktrees: WORKTREES,
  changes: { "wt-main": MAIN_CHANGES, "wt-side": SIDE_CHANGES, "wt-detached": [] },
  branches: BRANCHES,
  comparisons: {
    "main..feature/ranking-experiments": FEATURE_COMPARISON,
    "main..chore/update-dependencies": CHORE_COMPARISON,
  },
  commits: {
    "main..feature/ranking-experiments": { commits: rankingCommits() },
    "main..chore/update-dependencies": { commits: CHORE_COMMITS },
  },
  forks: { "main..chore/update-dependencies": CHORE_FORK },
};
