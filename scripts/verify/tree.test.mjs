import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { workingTree } from "./tree.mjs";

function repository(run) {
  const root = mkdtempSync(path.join(os.tmpdir(), "tree-"));
  try {
    const git = (...args) => execFileSync("git", args, { cwd: root, stdio: "ignore" });
    git("init", "-q");
    mkdirSync(path.join(root, "pkg/ignored"), { recursive: true });
    mkdirSync(path.join(root, "other"));
    writeFileSync(path.join(root, ".gitignore"), "pkg/ignored/\n");
    writeFileSync(path.join(root, "pkg/a.txt"), "a");
    writeFileSync(path.join(root, "other/b.txt"), "b");
    git("add", ".");
    git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "x");
    run(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("uncommitted, untracked and deleted edits each change the tree, and an ignored file does not", () => {
  repository((root) => {
    const id = (entry) => {
      const result = spawnSync("git", ["rev-parse", "--verify", "--quiet", `${workingTree(root)}:${entry}`], { cwd: root, encoding: "utf8" });
      return result.stdout.trim();
    };
    const before = id("pkg");
    writeFileSync(path.join(root, "pkg/ignored/s.txt"), "s");
    assert.equal(id("pkg"), before);
    writeFileSync(path.join(root, "pkg/new.txt"), "n");
    const afterNew = id("pkg");
    assert.notEqual(afterNew, before);
    writeFileSync(path.join(root, "pkg/a.txt"), "changed");
    assert.notEqual(id("pkg"), afterNew);
    rmSync(path.join(root, "other"), { recursive: true });
    assert.equal(id("other"), "");
  });
});

test("same-size edits inside the index timestamp are read without changing the real index", () => {
  repository((root) => {
    const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
    git("config", "core.trustctime", "false");
    git("config", "core.checkStat", "minimal");
    const file = path.join(root, "pkg/a.txt");
    const index = path.join(root, ".git/index");
    const stamp = Math.floor(Date.now() / 1000) - 60;
    utimesSync(file, stamp, stamp);
    git("add", "pkg/a.txt");
    utimesSync(index, stamp, stamp);
    const originalIndex = readFileSync(index);
    const originalMtime = statSync(index).mtimeMs;

    writeFileSync(file, "b");
    utimesSync(file, stamp, stamp);
    const tree = workingTree(root);

    assert.equal(git("show", `${tree}:pkg/a.txt`), "b");
    assert.deepEqual(readFileSync(index), originalIndex);
    assert.equal(statSync(index).mtimeMs, originalMtime);
  });
});
