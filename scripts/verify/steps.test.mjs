import assert from "node:assert/strict";
import { test } from "node:test";
import { planScope } from "./scope.mjs";
import { checkSteps } from "./steps.mjs";

const steps = (files) => checkSteps(planScope(files));
const labels = (files) => steps(files).map((step) => step.label);

test("affected consumers are combined without unrelated checks, light steps first", () => {
  assert.deepEqual(labels(["scripts/verify/lock.mjs"]), ["verify lint", "verify tests", "ui lint", "ui type check", "ui tests"]);
  const combined = labels(["packages/ui/src/App.tsx", "apps/desktop/scripts/bundle.mjs"]);
  assert.ok(combined.includes("ui tests"));
  assert.ok(combined.includes("desktop script tests"));
  assert.ok(!combined.includes("rust tests"));
  assert.ok(!combined.includes("web type check"));
});

test("any Rust change runs the combined Rust checks, since the tests always cover the workspace", () => {
  const pair = [["cargo", "fmt", "--all", "--", "--check"], ["pnpm", "clippy:rust"], ["pnpm", "test:rust"]];
  for (const file of ["apps/daemon/src/main.rs", "apps/desktop/src-tauri/src/lib.rs", "Cargo.lock"]) {
    assert.deepEqual(steps([file]).filter((step) => /rust/.test(step.label)).map((step) => step.cmd), pair, file);
  }
  assert.deepEqual(labels(["packages/ui/src/App.tsx"]).filter((label) => /rust/.test(label)), []);
});

test("web builds precede output tests", () => {
  const web = labels(["apps/web/app/app.vue"]);
  assert.ok(web.indexOf("web build") < web.indexOf("web tests"));
});

test("--full lists every consumer's steps, light steps before heavy ones", () => {
  const full = checkSteps({ mode: "full" });
  assert.deepEqual(full.map((step) => step.label), [
    "verify lint", "verify tests", "ui lint", "ui type check", "web lint", "web type check", "web locales", "desktop lint",
    "desktop script tests", "rustfmt check", "ui tests", "web build", "web tests", "rust clippy", "rust tests",
  ]);
  const firstHeavy = full.findIndex((step) => step.heavy);
  assert.ok(full.slice(firstHeavy).every((step) => step.heavy));
});

test("every lint step writes its cache outside the working tree, which verify requires to stay clean", () => {
  const plans = [checkSteps({ mode: "full" }), checkSteps({ mode: "release" })].flat().filter((step) => step.cmd.includes("eslint"));
  assert.ok(plans.length > 0);
  for (const step of plans) {
    const at = step.cmd.indexOf("--cache-location");
    assert.ok(step.cmd.includes("--cache") && at > 0, step.label);
    assert.match(step.cmd[at + 1], /^node_modules\//, step.label);
  }
});

test("UI reruns remain narrowed and release checks exclude website and gallery suites", () => {
  const ui = steps(["packages/ui/src/App.tsx"]).find((step) => step.label === "ui tests");
  assert.deepEqual(ui.rerun(["packages/ui/src/a/X.test.tsx"]), [["pnpm", "--filter", "@octoboard/ui", "test", "src/a/X.test.tsx"]]);
  const release = checkSteps({ mode: "release" });
  assert.deepEqual(release.map((step) => step.label), ["application lint", "application type check", "desktop script tests", "rustfmt check", "application UI tests", "rust clippy", "rust tests"]);
  assert.ok(release[0].cmd.includes("**/src/gallery/**"));
  assert.ok(release[1].cmd.includes("typecheck:app"));
  assert.ok(release[4].cmd.includes("test:app"));
  assert.ok(release.slice(4).every((step) => step.heavy));
});

test("the UI build runs only for the scope that names it", () => {
  assert.ok(labels(["packages/ui/vite.config.ts"]).includes("ui build"));
  assert.ok(!labels(["packages/ui/src/App.tsx"]).includes("ui build"));
});
