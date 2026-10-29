import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { changedFiles, requirePushedTree, resolveBase } from "./changes.mjs";
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

const commit = (root, git, file, content) => {
  writeFileSync(path.join(root, file), content);
  git("add", ".");
  git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", file);
  return git("rev-parse", "HEAD");
};

test("manual verification fetches the remote baseline and includes all local unpushed commits", () => {
  repository((root, git) => {
    assert.equal(resolveBase(git).noTarget, true);
    // A ref in this fixture stands in for a remote repository's published main branch.
    const published = git("rev-parse", "HEAD");
    git("update-ref", "refs/heads/published", published);
    git("remote", "add", "origin", root);
    git("config", "branch.main.remote", "origin");
    git("config", "branch.main.merge", "refs/heads/published");
    commit(root, git, "merged.txt", "an earlier local merge");
    git("switch", "-q", "-c", "feat/x");
    git("config", "branch.feat/x.targetBranch", "main");
    commit(root, git, "own.txt", "this branch");
    assert.deepEqual(resolveBase(git), { commit: published });
    assert.deepEqual(changedFiles(git, published, workingTree(root)), ["merged.txt", "own.txt"]);
    const next = git("rev-parse", "HEAD");
    git("update-ref", "refs/heads/published", next);
    assert.deepEqual(resolveBase(git), { commit: next });
  });
});

test("an explicit push base uses the remote tip directly, including a non-ancestor force-push tip", () => {
  repository((root, git) => {
    git("switch", "-q", "-c", "other");
    const remote = commit(root, git, "remote.txt", "remote-only");
    git("switch", "-q", "main");
    commit(root, git, "local.txt", "local-only");
    assert.deepEqual(resolveBase(git, remote), { commit: remote });
    assert.deepEqual(changedFiles(git, remote, workingTree(root)), ["local.txt", "remote.txt"]);
    assert.match(resolveBase(git, "nonexistent").reason, /not readable/);
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

test("push verification rejects another commit and dirty content, even when its checks passed earlier", () => {
  repository((root, git) => {
    const original = git("rev-parse", "HEAD");
    assert.doesNotThrow(() => requirePushedTree(git, original, workingTree(root)));
    writeFileSync(path.join(root, "a.txt"), "dirty");
    assert.throws(() => requirePushedTree(git, original, workingTree(root)), /clean working tree/);
    commit(root, git, "a.txt", "new commit");
    assert.throws(() => requirePushedTree(git, original, workingTree(root)), /pushed commit/);
  });
});
