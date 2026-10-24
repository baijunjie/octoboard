import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { clearFailure, failedFiles, isolatedTestFile, narrowedFiles, readFailure, recordFailure, REPORT_ENV } from "./rerun.mjs";
import { workingTree } from "./tree.mjs";

const X = "packages/ui/src/a/X.test.tsx";
const Y = "packages/ui/src/a/Y.test.tsx";
const SHARED = "packages/ui/src/a/Shared.test.ts";
const SOURCE = "packages/ui/src/a/X.tsx";
const toolchain = ["node v1"];

// A repository holding two test files, a source file, and a test file another test file imports.
function repository(run) {
  const root = mkdtempSync(path.join(os.tmpdir(), "rerun-"));
  const write = (file, content) => {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), content);
  };
  const git = (...args) => {
    try {
      return execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
    } catch {
      return null;
    }
  };
  try {
    git("init", "-q", "-b", "main");
    write(X, "test x");
    write(Y, "import { cases } from \"./Shared.test\";");
    write(SHARED, "export const cases = [];");
    write(SOURCE, "export const x = 1;");
    run({ root, write, git, tree: () => workingTree(root) });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("a changed test file nothing else names narrows the re-run to it and the failed files", () => {
  repository(({ write, git, tree }) => {
    const failure = { tree: tree(), toolchain: toolchain.join("\n"), files: [Y] };
    assert.deepEqual(narrowedFiles(git, failure, failure.tree, toolchain), [Y]);
    write(X, "test x, fixed");
    assert.deepEqual(narrowedFiles(git, failure, tree(), toolchain), [Y, X]);
  });
});

test("any other change, or another toolchain, runs the step in full", () => {
  repository(({ root, write, git, tree }) => {
    const failure = { tree: tree(), toolchain: toolchain.join("\n"), files: [X] };
    assert.equal(narrowedFiles(git, failure, failure.tree, ["node v2"]), null);
    write(SHARED, "export const cases = [1];");
    assert.equal(narrowedFiles(git, failure, tree(), toolchain), null);
    write(SHARED, "export const cases = [];");
    write(SOURCE, "export const x = 2;");
    assert.equal(narrowedFiles(git, failure, tree(), toolchain), null);
    write(SOURCE, "export const x = 1;");
    write("packages/ui/package.json", "{}");
    assert.equal(narrowedFiles(git, failure, tree(), toolchain), null);
    rmSync(path.join(root, "packages/ui/package.json"));
    assert.equal(narrowedFiles(git, { ...failure, tree: "0".repeat(40) }, tree(), toolchain), null);
    assert.equal(narrowedFiles(git, null, tree(), toolchain), null);
  });
});

test("a failed file that is gone is dropped from the re-run", () => {
  repository(({ root, git, tree }) => {
    const failure = { tree: tree(), toolchain: toolchain.join("\n"), files: [X] };
    rmSync(path.join(root, X));
    assert.deepEqual(narrowedFiles(git, failure, tree(), toolchain), []);
  });
});

test("a test file another file names is not isolated, nor is a file that is not a UI test", () => {
  repository(({ git, tree }) => {
    const current = tree();
    assert.equal(isolatedTestFile(git, current, X), true);
    assert.equal(isolatedTestFile(git, current, SHARED), false);
    assert.equal(isolatedTestFile(git, current, SOURCE), false);
    assert.equal(isolatedTestFile(git, current, "apps/web/test/a.test.ts"), false);
  });
});

test("a report counts only when it accounts for every failure", () => {
  const dir = mkdtempSync(path.join(os.tmpdir(), "rerun-report-"));
  try {
    const root = path.join(dir, "repo");
    const report = (content) => {
      writeFileSync(path.join(dir, "report.json"), JSON.stringify(content));
      return failedFiles(path.join(dir, "report.json"), root);
    };
    assert.deepEqual(report({ failed: [path.join(root, X)], complete: true }), [X]);
    assert.equal(report({ failed: [path.join(root, X)], complete: false }), null);
    assert.equal(report({ failed: [], complete: true }), null);
    assert.equal(report({ failed: [path.join(dir, "elsewhere.test.ts")], complete: true }), null);
    assert.equal(failedFiles(path.join(dir, "missing.json"), root), null);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a failure is recorded per worktree and step, and cleared", () => {
  const dir = mkdtempSync(path.join(os.tmpdir(), "rerun-record-"));
  try {
    const step = { label: "ui tests", cmd: ["pnpm", "test"] };
    const failure = { tree: "t", toolchain: "n", files: [X] };
    assert.equal(readFailure(dir, "/a", step), null);
    recordFailure(dir, "/a", step, failure);
    assert.deepEqual(readFailure(dir, "/a", step), failure);
    assert.equal(readFailure(dir, "/b", step), null);
    assert.equal(readFailure(dir, "/a", { ...step, cmd: ["other"] }), null);
    clearFailure(dir, "/a", step);
    assert.equal(readFailure(dir, "/a", step), null);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("the UI's reporter reads the report path from the same variable", () => {
  const reporter = fileURLToPath(new URL("../../packages/ui/src/test/failedFilesReporter.ts", import.meta.url));
  assert.match(readFileSync(reporter, "utf8"), new RegExp(`REPORT_ENV = "${REPORT_ENV}"`));
});
