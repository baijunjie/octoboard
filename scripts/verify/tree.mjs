// The content of a working copy as git sees it, for keying a stamp: a tree object holding the tracked
// files as they are now, plus the untracked ones that are not ignored.
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, rmSync, statSync, utimesSync } from "node:fs";
import os from "node:os";
import path from "node:path";

function git(gitArgs, env = process.env, cwd) {
  const result = spawnSync("git", gitArgs, { encoding: "utf8", env, cwd, stdio: ["ignore", "pipe", "pipe"] });
  return result.status === 0 ? result.stdout.trim() : null;
}

// The tree of the working copy at `root` as it is now, uncommitted and untracked (not ignored) files
// included, taken through a scratch copy of the index so the real one is left alone. Returns null when
// git fails.
export function workingTree(root) {
  const scratch = mkdtempSync(path.join(os.tmpdir(), "verify-index-"));
  try {
    const indexFile = path.join(scratch, "index");
    const realIndex = path.resolve(root, git(["rev-parse", "--git-path", "index"], process.env, root));
    if (existsSync(realIndex)) {
      try {
        const stamp = statSync(realIndex);
        copyFileSync(realIndex, indexFile);
        // Git's racy-clean check compares cached file times with the index's own mtime. A newer
        // copy can hide same-size edits; rounding down keeps that check conservative.
        utimesSync(indexFile, stamp.atime, Math.floor(stamp.mtimeMs / 1000));
      } catch {
        return null;
      }
    }
    const env = { ...process.env, GIT_INDEX_FILE: indexFile };
    const read = git(["add", "-A", "--", "."], env, root) !== null;
    return read ? git(["write-tree"], env, root) : null;
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}
