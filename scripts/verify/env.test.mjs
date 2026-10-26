import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { localValue } from "../env.mjs";

// `scripts/env.mjs` has no package of its own; its tests sit here so the verify tests' glob runs them.

const NAME = "OCTOBOARD_TEST_LOCAL_VALUE";

function sandbox(run) {
  const dir = realpathSync(mkdtempSync(path.join(os.tmpdir(), "env-")));
  const saved = process.env[NAME];
  delete process.env[NAME];
  try {
    run(dir);
  } finally {
    if (saved === undefined) delete process.env[NAME];
    else process.env[NAME] = saved;
    rmSync(dir, { recursive: true, force: true });
  }
}

function git(cwd, ...args) {
  const result = spawnSync("git", ["-c", "user.name=t", "-c", "user.email=t@t", ...args], { cwd, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
}

// A repository with one commit and a linked worktree next to it.
function repoWithWorktree(dir) {
  const main = path.join(dir, "main");
  const linked = path.join(dir, "linked");
  mkdirSync(main);
  git(main, "init", "-q");
  git(main, "commit", "-q", "--allow-empty", "-m", "init");
  git(main, "worktree", "add", "-q", linked);
  return { main, linked };
}

test("a real environment variable wins over the file", () => {
  sandbox((dir) => {
    writeFileSync(path.join(dir, ".env.local"), `${NAME}=file\n`);
    process.env[NAME] = "real";
    assert.equal(localValue(NAME, dir), "real");
  });
});

test("an empty real environment variable falls through to the file", () => {
  sandbox((dir) => {
    writeFileSync(path.join(dir, ".env.local"), `${NAME}="file"\n`);
    process.env[NAME] = "";
    assert.equal(localValue(NAME, dir), "file");
  });
});

test("an empty file value counts as unset", () => {
  sandbox((dir) => {
    writeFileSync(path.join(dir, ".env.local"), `${NAME}=\n`);
    assert.equal(localValue(NAME, dir), undefined);
  });
});

test("a linked worktree reads the main working tree's file when its own lacks the value", () => {
  sandbox((dir) => {
    const { main, linked } = repoWithWorktree(dir);
    writeFileSync(path.join(main, ".env.local"), `${NAME}=from-main\n`);
    assert.equal(localValue(NAME, linked), "from-main");
    writeFileSync(path.join(linked, ".env.local"), `${NAME}=own\n`);
    assert.equal(localValue(NAME, linked), "own");
    assert.equal(localValue(NAME, main), "from-main");
  });
});

test("a directory outside any repository gives undefined", () => {
  sandbox((dir) => {
    assert.equal(localValue(NAME, dir), undefined);
    assert.equal(localValue(NAME, path.join(dir, "missing")), undefined);
  });
});

test("a bare repository has no main working tree", () => {
  sandbox((dir) => {
    git(dir, "init", "-q", "--bare", "bare.git");
    assert.equal(localValue(NAME, path.join(dir, "bare.git")), undefined);
  });
});
