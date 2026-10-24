// The content of a working copy as git sees it, for keying a stamp: a tree object holding the tracked
// files as they are now, plus the untracked ones that are not ignored
// (and the ignored ones asked for).
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";

function git(gitArgs, env = process.env, cwd) {
  const result = spawnSync("git", gitArgs, { encoding: "utf8", env, cwd, stdio: ["ignore", "pipe", "pipe"] });
  return result.status === 0 ? result.stdout.trim() : null;
}

// The tree of the working copy at `root` as it is now, uncommitted and untracked (not ignored) files
// included, taken through a scratch copy of the index so the real one is left alone. `paths` limits
// the working files read to those paths, the rest of the tree being the index's; `forced` are paths
// read even where git ignores them, whether or not `paths` lists them. Both are relative to `root`, and a path that does not exist is
// skipped unless the index holds it. Returns null when git fails.
export function workingTree(root, { paths, forced = [] } = {}) {
  const scratch = mkdtempSync(path.join(os.tmpdir(), "verify-index-"));
  try {
    const indexFile = path.join(scratch, "index");
    const realIndex = path.resolve(root, git(["rev-parse", "--git-path", "index"], process.env, root));
    try {
      copyFileSync(realIndex, indexFile);
    } catch {
      // No index yet: the scratch one starts empty.
    }
    const env = { ...process.env, GIT_INDEX_FILE: indexFile };
    // A path on neither the disk nor the index makes `git add` fail; one only in the index is a deletion to take.
    const known = (list) => list.filter((entry) => existsSync(path.join(root, entry)) || git(["ls-files", "--", entry], env, root));
    const add = (flags, list) => list.length === 0 || git(["add", ...flags, "--", ...list], env, root) !== null;
    const read = add(["-A"], paths ? known(paths.filter((entry) => !forced.includes(entry))) : ["."]) && add(["-A", "-f"], known(forced));
    return read ? git(["write-tree"], env, root) : null;
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

// The object id `tree` holds at each of `paths` (empty for one it does not hold).
export const idsAt = (root, tree, paths) => paths.map((entry) => git(["rev-parse", "--verify", "--quiet", `${tree}:${entry}`], process.env, root) ?? "");
