// Re-running only the failed test files of a step. When a step that can be narrowed (`rerun` in
// `steps.mjs`) fails, the UI's test files that failed are recorded with the content they failed on.
// If everything that changed since is a UI test file nothing else imports, every other file of that
// run would pass again as it did, so the next run takes only the failed files and the changed ones,
// and its pass counts as the whole step's pass for the new content. Any other change (source, a
// shared test helper or fixture, setup or configuration, a dependency, the toolchain) runs the step
// in full. A test file counts as imported when another UI file names it, so one pulled in without
// being named (a glob, a directory read) would not be seen. The record lives beside the stamps, one
// per worktree and step.
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { changedFiles } from "./changes.mjs";
import { hash, stepKey } from "./stamp.mjs";

// What `packages/ui/src/test/failedFilesReporter.ts` reads for the path to write its report to.
export const REPORT_ENV = "OCTOBOARD_VERIFY_TEST_REPORT";

const UI_TEST_FILE = /^packages\/ui\/src\/.+\.test\.tsx?$/;

const recordFile = (dir, root, step) => path.join(dir, `failed-${hash(`${root}\0${stepKey(step)}`)}.json`);

export function readFailure(dir, root, step) {
  try {
    return JSON.parse(readFileSync(recordFile(dir, root, step), "utf8"));
  } catch {
    return null;
  }
}

// `failure` is `{ tree, toolchain, files }`: the content's tree object, the toolchain versions, and
// the failed test files relative to `root`.
export function recordFailure(dir, root, step, failure) {
  mkdirSync(dir, { recursive: true });
  writeFileSync(recordFile(dir, root, step), JSON.stringify(failure));
}

export const clearFailure = (dir, root, step) => rmSync(recordFile(dir, root, step), { force: true });

// The failed files of a report the reporter wrote at `reportPath`, relative to `root`, or null when
// the report does not account for every failure: missing, incomplete, naming none, or naming a file
// that is not a UI test file under `packages/ui/src`.
export function failedFiles(reportPath, root) {
  let report;
  try {
    report = JSON.parse(readFileSync(reportPath, "utf8"));
  } catch {
    return null;
  }
  if (!report.complete || !Array.isArray(report.failed) || report.failed.length === 0) return null;
  const files = report.failed.map((file) => path.relative(root, file).split(path.sep).join("/"));
  return files.every((file) => UI_TEST_FILE.test(file)) ? files : null;
}

// Whether `file` is a UI test file that no other file of the UI package in `tree` names, so changing
// it can change no other file's result. `git grep` finding nothing fails like an error does, so a
// file counts as named by others unless the tree can be read.
export function isolatedTestFile(git, tree, file) {
  if (!UI_TEST_FILE.test(file)) return false;
  if (git("cat-file", "-e", tree) == null) return false;
  const stem = path.posix.basename(file).replace(/\.tsx?$/, "");
  const naming = (git("grep", "-l", "-F", "-e", stem, tree, "--", "packages/ui") ?? "").split("\n").filter(Boolean);
  return naming.every((entry) => entry === `${tree}:${file}`);
}

// The test files to re-run `failure`'s step on for the content `tree`, or null when the step has to
// run in full. A file `tree` no longer holds is dropped.
export function narrowedFiles(git, failure, tree, toolchain) {
  if (!failure || failure.toolchain !== toolchain.join("\n")) return null;
  const changed = changedFiles(git, failure.tree, tree);
  if (!changed || !changed.every((file) => isolatedTestFile(git, tree, file))) return null;
  return [...new Set([...failure.files, ...changed])].filter((file) => git("cat-file", "-e", `${tree}:${file}`) != null);
}
