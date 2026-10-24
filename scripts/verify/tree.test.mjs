import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { idsAt, workingTree } from "./tree.mjs";

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

test("uncommitted and untracked edits change the ids of the paths read, and only those", () => {
  repository((root) => {
    const ids = () => idsAt(root, workingTree(root, { paths: ["pkg", "other"] }), ["pkg", "other"]);
    const before = ids();
    writeFileSync(path.join(root, "pkg/new.txt"), "n");
    const afterNew = ids();
    assert.notEqual(afterNew[0], before[0]);
    assert.equal(afterNew[1], before[1]);
    writeFileSync(path.join(root, "other/b.txt"), "changed");
    assert.notEqual(ids()[1], before[1]);
  });
});

test("an ignored path counts only when it is forced, and a missing one is skipped", () => {
  repository((root) => {
    const ids = (forced) => idsAt(root, workingTree(root, { paths: ["pkg", "missing"], forced }), ["pkg/ignored", "missing"]);
    writeFileSync(path.join(root, "pkg/ignored/s.txt"), "s");
    assert.deepEqual(ids([]), ["", ""]);
    assert.notEqual(ids(["pkg/ignored"])[0], "");
    assert.equal(ids(["pkg/ignored"])[1], "");
    const listed = workingTree(root, { paths: ["pkg", "pkg/ignored"], forced: ["pkg/ignored"] });
    assert.notEqual(idsAt(root, listed, ["pkg/ignored"])[0], "");
  });
});

test("deleting a tracked path changes its id", () => {
  repository((root) => {
    const ids = () => idsAt(root, workingTree(root, { paths: ["pkg", "other"] }), ["other"]);
    const before = ids();
    rmSync(path.join(root, "other"), { recursive: true });
    assert.deepEqual(ids(), [""]);
    assert.notDeepEqual(ids(), before);
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
