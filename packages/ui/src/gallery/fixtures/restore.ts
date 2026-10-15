import type { FixtureGit } from "./changeFixtures";
import { addition, textChange, untracked } from "./changeFixtures";
import type { FixtureFiles } from "./projectFiles";

const head = "4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39";
const worktrees: FixtureGit["worktrees"] = [{ id: "restore-main", root: "/Users/dev/code/search-api", main: true, head, branch: "main", scope_present: true }];
const staged = { kind: "index", worktree: "restore-main", blob: "b2c3d4e" } as const;
const live = { kind: "live", root_id: "fixture", version: "changed" } as const;
const note = untracked("notes.txt", live, "Keep these notes.\n");
const tracked = (group: "staged" | "unstaged", old: string, next: string) => textChange(group, "README.md",
  group === "staged" ? { kind: "commit", commit: head, branch: null, blob: "a1b2c3d" } : staged,
  group === "staged" ? staged : live,
  { old, new: next, patch: `diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-${old.trimEnd()}\n+${next.trimEnd()}\n` });

export const RESTORE_GIT: FixtureGit = {
  worktrees,
  changes: { "restore-main": [
    tracked("staged", "# Search API\n", "# Staged edit\n"),
    tracked("unstaged", "# Staged edit\n", "# Working edit\n"),
    addition("staged", "new.txt", { kind: "index", worktree: "restore-main", blob: "c3d4e5f" }, "A staged new file.\n"),
    note,
  ] },
};
export const RESTORE_FILES: FixtureFiles = {
  "": { dir: true },
  "README.md": { text: "# Working edit\n" },
  "new.txt": { text: "A staged new file.\n" },
  "notes.txt": { text: "Keep these notes.\n" },
  ".gitignore": { text: "cache/\n" },
  cache: { dir: true },
  "cache/local.txt": { text: "Keep ignored data.\n" },
};
const { "new.txt": _removed, ...restoredFiles } = RESTORE_FILES;
export const RESTORED_PROJECT = {
  git: { worktrees, changes: { "restore-main": [note] } } satisfies FixtureGit,
  files: { ...restoredFiles, "README.md": { text: "# Search API\n" } } satisfies FixtureFiles,
};
