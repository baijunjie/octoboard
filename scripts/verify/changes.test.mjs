import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { changedFiles, resolveBase } from "./changes.mjs";
import { workingTree } from "./tree.mjs";

function repository(run) {
  const root = mkdtempSync(path.join(os.tmpdir(), "changes-"));
  try {
    const sh = (...args) => {
      try {
        return execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
      } catch {
        return null;
      }
    };
    sh("init", "-q", "-b", "main");
    writeFileSync(path.join(root, "a.txt"), "a");
    sh("add", ".");
    sh("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "x");
    run(root, sh);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("a branch is judged from its recorded fork point, and the target branch has none", () => {
  repository((root, git) => {
    assert.deepEqual(resolveBase(git), { reason: "no recorded fork point", noForkPoint: true });
    const fork = git("rev-parse", "HEAD");
    git("switch", "-q", "-c", "feat/x");
    assert.deepEqual(resolveBase(git), { reason: "no recorded fork point", noForkPoint: true });
    git("config", "branch.feat/x.forkPoint", fork);
    assert.deepEqual(resolveBase(git), { commit: fork });
    git("config", "branch.feat/x.forkPoint", "nonexistent");
    assert.match(resolveBase(git).reason, /not readable/);
    assert.deepEqual(resolveBase(git, fork), { commit: fork });
  });
});

test("a base that is not an ancestor of HEAD cannot be judged", () => {
  repository((root, git) => {
    git("switch", "-q", "-c", "other");
    writeFileSync(path.join(root, "b.txt"), "b");
    git("add", ".");
    git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "y");
    const other = git("rev-parse", "HEAD");
    git("switch", "-q", "main");
    assert.match(resolveBase(git, other).reason, /not an ancestor/);
  });
});

test("the changed files are those between the base and the working tree, uncommitted ones included", () => {
  repository((root, git) => {
    const base = git("rev-parse", "HEAD");
    assert.deepEqual(changedFiles(git, base, workingTree(root)), []);
    writeFileSync(path.join(root, "a.txt"), "changed");
    writeFileSync(path.join(root, "new.txt"), "new");
    assert.deepEqual(changedFiles(git, base, workingTree(root)).sort(), ["a.txt", "new.txt"]);
    assert.equal(changedFiles(git, "nonexistent", workingTree(root)), null);
  });
});
