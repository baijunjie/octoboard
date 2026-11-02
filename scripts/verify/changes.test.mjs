import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { changedFiles, chooseBase, requirePushedTree, resolveBase, resolveLocalTargetBase, remoteTargetBase, resolveRemoteTargetBase } from "./changes.mjs";
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

test("a branch with no upstream is measured from its merge base with the remote's target", () => {
  repository((root, git) => {
    const remote = mkdtempSync(path.join(os.tmpdir(), "changes-remote-"));
    try {
      git("clone", "-q", "--bare", ".", remote);
      git("remote", "add", "origin", remote);
      git("switch", "-q", "-c", "topic");
      const forked = git("rev-parse", "HEAD");
      commit(root, git, "b.txt", "b");
      assert.equal(resolveRemoteTargetBase(git).commit, forked);
      git("config", "branch.topic.targetBranch", "missing");
      assert.match(resolveRemoteTargetBase(git).reason, /cannot fetch origin refs\/heads\/missing/);
      git("remote", "remove", "origin");
      assert.match(resolveRemoteTargetBase(git).reason, /no remote/);
    } finally {
      rmSync(remote, { recursive: true, force: true });
    }
  });
});

test("the remote target base honours the named branch's targetBranch, a URL remote and a moved target tip", () => {
  repository((root, git) => {
    const remote = mkdtempSync(path.join(os.tmpdir(), "changes-remote-"));
    try {
      git("clone", "-q", "--bare", ".", remote);
      const forked = git("rev-parse", "HEAD");
      git("branch", "release", forked);
      git("push", "-q", remote, "release");
      git("switch", "-q", "-c", "topic");
      const head = commit(root, git, "b.txt", "b");
      // No target configured: the remote's HEAD. The remote may be a URL, and `branch` need not be checked out.
      assert.deepEqual(remoteTargetBase(git, { branch: "topic", remote, head }), { commit: forked, via: `${remote} HEAD` });
      // The target's tip moves on after the fork: the base is still the fork point.
      git("switch", "-q", "main");
      commit(root, git, "c.txt", "c");
      git("push", "-q", remote, "main");
      git("config", "branch.topic.targetBranch", "release");
      assert.deepEqual(remoteTargetBase(git, { branch: "topic", remote, head }), { commit: forked, via: `${remote} refs/heads/release` });
      git("config", "branch.topic.targetBranch", "main");
      assert.equal(remoteTargetBase(git, { branch: "topic", remote, head }).commit, forked);
      git("config", "branch.topic.targetBranch", "missing");
      assert.match(remoteTargetBase(git, { branch: "topic", remote, head }).reason, /cannot fetch .* refs\/heads\/missing/);
    } finally {
      rmSync(remote, { recursive: true, force: true });
    }
  });
});

// A repository whose main has unpushed commits ahead of the remote's, with a topic branch forked from that main.
function localAhead(run) {
  repository((root, git) => {
    const remote = mkdtempSync(path.join(os.tmpdir(), "changes-remote-"));
    try {
      git("clone", "-q", "--bare", ".", remote);
      git("remote", "add", "origin", remote);
      git("fetch", "-q", "origin");
      git("remote", "set-head", "origin", "main");
      commit(root, git, "unpushed.txt", "local main only");
      const forked = git("rev-parse", "HEAD");
      git("switch", "-q", "-c", "topic");
      commit(root, git, "b.txt", "b");
      run(root, git, forked);
    } finally {
      rmSync(remote, { recursive: true, force: true });
    }
  });
}

test("a quick base is the merge base with the configured local target, not counting its unpushed commits", () => {
  localAhead((root, git, forked) => {
    git("branch", "release", forked);
    git("config", "branch.topic.targetBranch", "release");
    const base = resolveLocalTargetBase(git);
    assert.deepEqual(base, { commit: forked, via: "local branch release" });
    assert.deepEqual(changedFiles(git, base.commit, workingTree(root)), ["b.txt"]);
  });
});

test("without a target branch the quick base is the local branch the remote's HEAD names", () => {
  localAhead((root, git, forked) => {
    assert.deepEqual(resolveLocalTargetBase(git), { commit: forked, via: "local branch main" });
    // A local branch named like the remote-tracking one does not confuse the remote's HEAD.
    git("branch", "origin/main", git("rev-list", "--max-parents=0", "HEAD"));
    assert.deepEqual(resolveLocalTargetBase(git), { commit: forked, via: "local branch main" });
  });
});

test("a configured target missing locally is not replaced by the remote's HEAD", () => {
  localAhead((root, git) => {
    git("config", "branch.topic.targetBranch", "missing");
    assert.equal(resolveLocalTargetBase(git), null);
    assert.match(chooseBase(git, { quick: true }).reason, /cannot fetch origin refs\/heads\/missing/);
  });
});

test("with no local target the quick base is left to the remote", () => {
  repository((root, git) => {
    git("switch", "-q", "-c", "topic");
    assert.equal(resolveLocalTargetBase(git), null);
  });
  localAhead((root, git) => {
    git("remote", "set-head", "origin", "--delete");
    assert.equal(resolveLocalTargetBase(git), null);
    git("switch", "-q", "--detach");
    assert.equal(resolveLocalTargetBase(git), null);
  });
});

test("on the target branch itself the local base is skipped so the remote base measures the unpushed commits", () => {
  localAhead((root, git) => {
    git("switch", "-q", "main");
    assert.equal(resolveLocalTargetBase(git), null);
    const base = resolveRemoteTargetBase(git);
    assert.equal(base.commit, git("rev-parse", "origin/main"));
    assert.deepEqual(changedFiles(git, base.commit, workingTree(root)), ["unpushed.txt"]);
  });
});

test("a local branch tracked as upstream is the local target", () => {
  localAhead((root, git, forked) => {
    git("branch", "release", forked);
    git("config", "branch.topic.remote", ".");
    git("config", "branch.topic.merge", "refs/heads/release");
    assert.deepEqual(resolveLocalTargetBase(git), { commit: forked, via: "local branch release" });
  });
});

test("--base is taken as given, and a quick run keeps a branch's own upstream", () => {
  localAhead((root, git) => {
    const first = git("rev-list", "--max-parents=0", "HEAD");
    assert.deepEqual(chooseBase(git, { quick: true, baseArg: first }), { commit: first });
    git("config", "branch.topic.remote", "origin");
    git("config", "branch.topic.merge", "refs/heads/elsewhere");
    git("push", "-q", "origin", "HEAD:refs/heads/elsewhere");
    assert.equal(chooseBase(git, { quick: true }).commit, git("rev-parse", "HEAD"));
    assert.equal(chooseBase(git, { quick: true }).via, undefined);
  });
});

test("a quick run counts the target's remote branch as no upstream, and a normal run does not", () => {
  localAhead((root, git, forked) => {
    git("config", "branch.topic.remote", "origin");
    git("config", "branch.topic.merge", "refs/heads/main");
    assert.deepEqual(chooseBase(git, { quick: true }), { commit: forked, via: "local branch main" });
    assert.equal(chooseBase(git).commit, git("rev-parse", "origin/main"));
  });
});

test("a quick run goes local, then remote", () => {
  localAhead((root, git) => {
    assert.equal(chooseBase(git, { quick: true }).via, "local branch main");
    git("config", "branch.topic.targetBranch", "missing");
    assert.match(chooseBase(git, { quick: true }).reason, /cannot fetch/);
    git("config", "--unset", "branch.topic.targetBranch");
    git("branch", "-m", "main", "trunk");
    git("remote", "set-head", "origin", "--delete");
    assert.equal(chooseBase(git, { quick: true }).via, "origin HEAD");
  });
});
