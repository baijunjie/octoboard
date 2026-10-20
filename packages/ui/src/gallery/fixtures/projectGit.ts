// The sample repository the fixture daemon answers a project's Git mode with (`fixtureGit.ts`),
// and the gallery's "Project pane: Git" scenarios over it: the branch selector, the change list in
// its sections, whole or by commit, a diff of each kind opened from it, and the states a source or a
// list can end in. Every change here is built by the factory for its kind (`changeFixtures.ts`).
import type { BranchInfo, ContentSource, FileContent, WorktreeInfo } from "../../protocol";
import { describing, type Ui } from "../interact";
import type { Preferences, Scenario } from "../scenario";
import type { FixtureChange, FixtureCommit, FixtureGit, Versions } from "./changeFixtures";
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

/** Long enough for the branch list, which the app reads again every ten seconds, to be read again. */
const BRANCH_LIST_REREAD_MS = 10_500;

const image = (data: string): FileContent => ({ size: Math.floor((data.length * 3) / 4), kind: "binary", media_type: "image/png", text: null, data });
const commit = (blob: string) => ({ kind: "commit", commit: "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39", branch: null, blob }) as const;
const index = (blob: string, worktree = "wt-main") => ({ kind: "index", worktree, blob }) as const;
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

const SIDE_CHANGES: FixtureChange[] = [textChange("unstaged", "README.md", index("aa11bb2", "wt-side"), live("v6"), README_EDIT)];

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

const MAIN_TIP = "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39";
const FEATURE_TIP = "9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807";
const CHORE_TIP = "6d5c4b3a2f1e0d9c8b7a6f5e4d3c2b1a0f9e8d7c";
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

const at = (commit: string) => (blob: string) => ({ kind: "commit", commit, branch: null, blob }) as const;

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

const onMain = at(MAIN_TIP);
const onFeature = at(FEATURE_TIP);

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
 * scrolling (ten rows of its height). */
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

/** `main`, checked out where the project's folder is, is two commits ahead of `origin/main`, which
 * the last fetch left at `UPSTREAM_TIP`: its Committed section lists what is not pushed. */
export const UNPUSHED_GIT: FixtureGit = {
  ...SAMPLE_GIT,
  changes: { ...SAMPLE_GIT.changes, "wt-main": MAIN_FEW_CHANGES },
  branches: BRANCHES.map((branch) => (branch.name === "main" ? { ...branch, upstream: { name: "origin/main", commit: UPSTREAM_TIP } } : branch)),
  comparisons: { ...SAMPLE_GIT.comparisons, "origin/main..main": UNPUSHED_COMPARISON },
  commits: { ...SAMPLE_GIT.commits, "origin/main..main": { commits: UNPUSHED_COMMITS } },
};

/** `main` has never been pushed: no upstream, and no branch of its name on a remote, so its Committed
 * section is its whole history, compared with the empty tree. */
export const NEVER_PUSHED_GIT: FixtureGit = {
  ...SAMPLE_GIT,
  changes: { ...SAMPLE_GIT.changes, "wt-main": MAIN_FEW_CHANGES },
  branches: BRANCHES.map((branch) => (branch.name === "main" ? { name: branch.name, commit: branch.commit } : branch)),
  comparisons: { ...SAMPLE_GIT.comparisons, "..main": WHOLE_COMPARISON },
  commits: { ...SAMPLE_GIT.commits, "..main": { commits: WHOLE_COMMITS } },
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

/** The side worktree has no copy of the project's folder, as a sparse checkout leaves it. */
const SPARSE_GIT: FixtureGit = {
  ...SAMPLE_GIT,
  worktrees: [WORKTREES[0], { ...WORKTREES[1], scope_present: false }, WORKTREES[2]],
  changes: { ...SAMPLE_GIT.changes, "wt-side": [] },
};

/** The side worktree has `gh-pages` checked out, which shares no commit with `main`. */
const orphaned = (shallow: boolean): FixtureGit => ({
  ...SAMPLE_GIT,
  worktrees: [WORKTREES[0], { ...WORKTREES[1], head: ORPHAN_TIP, branch: "gh-pages" }, WORKTREES[2]],
  branches: BRANCHES.map((branch) => (branch.name === "feature/ranking-experiments" ? { ...branch, name: "gh-pages", commit: ORPHAN_TIP } : branch)),
  comparisons: {},
  commits: {},
  forks: { "main..gh-pages": null },
  shallow,
});

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
    description: "The branch selector starts on main, the branch checked out where the project's folder is, and the line above the list says it forked from origin/main at the very commit it is at, so nothing is unpushed and Committed has no rows. Staged, unstaged and untracked changes share the list, with server.ts selected and its rounded highlight spaced like Files rows. Collapse a section to watch the following rows slide into place, and right-click a row for Copy path and the Open in submenu, which groups File manager and Default application. The header reads the project name, the Files and Changes tabs, then the icon buttons and Close panel.",
    state,
    preferences,
    steps: [...openGit, (ui) => ui.press(ui.changeRow("server.ts", "modified")), (ui) => ui.wait(600), (ui) => ui.key("Escape")],
  },
  {
    id: "git-changes-narrow-header",
    group: GROUP,
    category: "normal",
    title: "Project pane header at the narrowest docked width",
    width: 1168,
    description: "The 1168px minimum window width leaves the docked project pane its 320px minimum. Files and Changes stay fully visible beside Close panel. The project name gives up its space first; when the remaining icon controls no longer fit, they fade at the clipped edge and scroll horizontally without moving the tabs or Close panel.",
    state,
    preferences,
    steps: [...openGit, (ui) => ui.wait(300)],
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
      "The selected session is responsible for feature/ranking-experiments, which the side worktree has checked out, so the selector starts on it. One header groups the branch selector, the worktree named by its folder with a button to copy its full path, then the fork and current commits. The detail lines share the same spacing, with no divider between them; one bottom border separates the whole header from the changes. The uncommitted change (README.md) comes from this worktree, and Committed lists everything the branch changed since the fork. Right-clicking a committed row offers Copy path only, because that version has no live file to open or reveal.",
    state,
    preferences,
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-unpushed",
    group: GROUP,
    category: "normal",
    title: "The project folder's branch with unpushed commits",
    description:
      "main is two commits ahead of origin/main: the line above the list says it forked from origin/main at the commit that remote-tracking branch is at, and Committed lists what main changed since, which is what has not been pushed. Uncommitted changes are listed above it.",
    state,
    preferences,
    git: UNPUSHED_GIT,
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
      "main is checked out where the project's folder is, and neither it nor a remote has a branch to compare it with: the line above the list says it has no remote branch, so all of its commits are listed, with no fork named. Committed lists every file at its tip as added.",
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
      "Group by commit is on (the header toggle reads Show commits together): the Committed band holds one row per commit, newest first, each with its chevron, short hash, subject and change count, and its changes one level in, their content starting a chevron's width past the hash. The whole history is listed, so nothing is said under the list. Open a change to see it against its commit's parent.",
    state,
    preferences: gitMode({ byCommit: true }),
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-by-commit-tree",
    group: GROUP,
    category: "normal",
    title: "Commits grouped by folder",
    description:
      "Group by commit and Group by folder are both on: each commit holds its own folders, which start open. Cache query results is folded to its row, which still counts its two changes. Expand or collapse a commit or folder to watch its children slide and fade in or out with the same timing as the session menu.",
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
      "The filter keeps the changes whose file name contains ranking. A commit with none left has no row, and the Committed count and each commit's count follow what is shown.",
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
      "Group by commit is on over a long history: the newest hundred commits each have a row, and a note under the list says only that many are listed.",
    state,
    preferences: gitMode({ byCommit: true }),
    git: LONG_HISTORY_GIT,
    steps: selecting(rankingSession),
  },
  {
    id: "git-session-unchecked",
    group: GROUP,
    category: "normal",
    title: "A branch checked out nowhere",
    description:
      "The selected session is responsible for chore/update-dependencies, which no worktree has checked out. The header groups the branch selector and fork comparison without a worktree row or a notice explaining the absence of uncommitted changes. Its bottom border remains above the committed changes. Main has moved on since the fork, so the fork commit is not main's tip.",
    state,
    preferences,
    steps: selecting(dependenciesSession),
  },
  {
    id: "git-changes-scope-missing",
    group: GROUP,
    category: "normal",
    title: "A branch whose worktree lacks the project's folder",
    description: "The side worktree has feature/ranking-experiments checked out without the project's folder. One header groups the branch selector, worktree name and Copy path button, missing-folder warning, and fork comparison. The detail lines share the same spacing, and the committed changes are still listed below the header.",
    state,
    preferences,
    git: SPARSE_GIT,
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-listing-commits",
    group: GROUP,
    category: "normal",
    title: "Listing a branch's commits",
    description: "With Group by commit on, the commits are still being listed: a line above the list says so beneath the line naming the fork, while the uncommitted change is already listed.",
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
    git: { ...SAMPLE_GIT, comparing: "pending" },
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
    title: "A change list with notes above and below it",
    description: "The side worktree has no folder for the project, which a note says above the list, and the daemon stopped listing early, which a note says under it.",
    state,
    preferences,
    git: normalRepository({ ...SPARSE_GIT, cutShort: true }),
    steps: selecting(rankingSession),
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
    description: "The project's folder is a linked worktree on feature/ranking-experiments, and the main working copy is detached: the line above the list says the branch's commits cannot be compared, and only its uncommitted change is listed.",
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
      "The Branch selector open over a repository with more branches than its list holds: the list scrolls and its rows fade at the bottom edge, where more are waiting, while the popover around it keeps its edges.",
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
    id: "git-compare-gone",
    group: GROUP,
    category: "exceptional",
    title: "A chosen branch that no longer exists",
    description: "The comparison failure takes the list's place, with a heading, the reason and a separate Try again control. Narrow shows the same failure in the docked Files card.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, missing: ["chore/update-dependencies"] },
    steps: selecting(dependenciesSession),
  },
  {
    id: "git-compare-no-common-commit",
    group: GROUP,
    category: "exceptional",
    title: "A branch with no commit in common with main",
    description: "gh-pages is checked out in the side worktree: its uncommitted change is listed, and a line above the list says the branches have no commit in common.",
    state,
    preferences,
    git: orphaned(false),
    steps: selecting(rankingSession),
  },
  {
    id: "git-compare-no-common-commit-shallow",
    group: GROUP,
    category: "exceptional",
    title: "No common commit found in a shallow clone",
    description: "The line above the list words it as missing history of a shallow clone rather than unrelated branches.",
    state,
    preferences,
    git: orphaned(true),
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-listing-failed",
    group: GROUP,
    category: "exceptional",
    title: "The uncommitted changes could not be read",
    description: "The side worktree's listing fails: a line above the list says why, and the committed changes are still listed.",
    state,
    preferences,
    git: { ...SAMPLE_GIT, changes: { ...SAMPLE_GIT.changes, "wt-side": failedRead } },
    steps: selecting(rankingSession),
  },
  {
    id: "git-changes-failed",
    group: GROUP,
    category: "exceptional",
    title: "No changes could be listed",
    description: "The project folder's own worktree fails to list, with nothing else to show: the failure takes the list's place, with a heading, the reason and a separate Try again control.",
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
    description: "With Group by commit on, the commit listing fails: a line above the list says why, and the uncommitted change is still listed.",
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
    description: "The standalone failure has a danger alert with a heading and Git's own diagnostic as selectable code. Narrow shows the same failure in the docked Files card.",
    state,
    preferences,
    git: { worktrees: [], changes: {}, gitError: "fatal: detected dubious ownership in repository at '/Users/dev/code/search-api'" },
    steps: openGit,
  },
  {
    id: "git-change-viewer",
    group: "File viewer",
    category: "normal",
    title: "Change viewer with collapsed lines",
    state,
    preferences,
    description: "The staged tag and long expansion gaps accompany the diff; other change subjects remain available through navigation. File manager and Default application end the header row, as the change is in a working copy.",
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
    description: "The subtitle names the branch and the commit it is at, with all of its history, because the branch has no remote branch to say where it forked from. A committed change has no File manager or Default application buttons.",
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
