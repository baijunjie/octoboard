// Records which steps passed on which content, so verifying the same content again runs nothing.
// A stamp is keyed by the tree of the working copy (the lockfiles are part of it) and the toolchain
// versions, and lists each passed step by its command line. It lives in the repository's common git
// directory, so every worktree sees it: after a fast-forward merge the target branch's tree is the
// tree that passed on the branch. Git-ignored files are not part of the tree, so an ignored input a
// check reads (the gallery's scratch scenarios, a built `apps/web/.output`) is not covered.
import { createHash } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";

const MAX_AGE_MS = 14 * 24 * 60 * 60 * 1000;

export const hash = (text) => createHash("sha256").update(text).digest("hex").slice(0, 32);

// Where the stamps live, given the repository's `git rev-parse --git-common-dir` read from `root`.
export const stampDirectory = (root, commonDir) => path.join(path.resolve(root, commonDir), "verify-stamps");

export const contentKey = (tree, toolchain) => hash(`${tree}\n${toolchain.join("\n")}`);
export const stepKey = (step) => hash(step.cmd.join("\0"));

export function readStamp(dir, key) {
  try {
    return JSON.parse(readFileSync(path.join(dir, `${key}.json`), "utf8"));
  } catch {
    return {};
  }
}

export const recordPass = (dir, key, step) => recordPasses(dir, key, [step]);

export function recordPasses(dir, key, steps) {
  mkdirSync(dir, { recursive: true });
  const stamp = readStamp(dir, key);
  for (const step of steps) stamp[stepKey(step)] = step.label;
  writeFileSync(path.join(dir, `${key}.json`), JSON.stringify(stamp));
  prune(dir);
}

function prune(dir) {
  for (const name of readdirSync(dir)) {
    const file = path.join(dir, name);
    try {
      if (Date.now() - statSync(file).mtimeMs > MAX_AGE_MS) rmSync(file, { force: true });
    } catch {
      // Another worktree pruned it first.
    }
  }
}
