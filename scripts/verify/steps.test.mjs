import assert from "node:assert/strict";
import { test } from "node:test";
import { planScope } from "./scope.mjs";
import { checkSteps, formatSteps } from "./steps.mjs";

const labels = (files) => checkSteps(planScope(files), files).map((step) => step.label);

test("a scope of several packages runs each one's checks, light ones first", () => {
  assert.deepEqual(labels(["scripts/verify/lock.mjs"]), ["verify lint", "verify tests", "ui type check", "ui tests"]);
  assert.deepEqual(formatSteps(planScope(["scripts/verify/lock.mjs"]), ["scripts/verify/lock.mjs"]).map((step) => step.cmd.at(-1)), ["scripts/verify/lock.mjs"]);
});

test("the UI test steps re-run on chosen files, and the full set's on every other package in full", () => {
  const ui = checkSteps(planScope(["packages/ui/src/App.tsx"]), []).find((step) => step.label === "ui tests");
  assert.deepEqual(ui.rerun(["packages/ui/src/a/X.test.tsx"]), [["pnpm", "--filter", "@octoboard/ui", "test", "src/a/X.test.tsx"]]);
  const all = checkSteps(planScope(["package.json"]), []).find((step) => step.label === "package tests");
  assert.deepEqual(all.rerun([]), [["pnpm", "-r", "--filter", "!@octoboard/ui", "test"]]);
  assert.equal(checkSteps(planScope(["package.json"]), []).filter((step) => step.rerun).length, 1);
});
