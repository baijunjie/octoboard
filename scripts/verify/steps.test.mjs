import assert from "node:assert/strict";
import { test } from "node:test";
import { planScope } from "./scope.mjs";
import { checkSteps } from "./steps.mjs";

const steps = (files) => checkSteps(planScope(files));
const labels = (files) => steps(files).map((step) => step.label);

test("affected consumers are combined without unrelated checks, light steps first", () => {
  assert.deepEqual(labels(["scripts/verify/lock.mjs"]), ["identity check", "provider-neutral core", "verify lint", "verify tests", "ui lint", "ui type check", "docs lint", "docs type check", "ui tests", "docs build", "docs tests"]);
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

test("a docs change checks only the docs: lint of the package, its type check, its build, then its tests as heavy steps", () => {
  const docs = steps(["apps/docs/astro.config.ts"]);
  assert.deepEqual(docs.map((step) => step.label), ["identity check", "provider-neutral core", "docs lint", "docs type check", "docs build", "docs tests"]);
  const [, , lintStep, typecheck, build, tests] = docs;
  assert.ok(lintStep.cmd.includes("eslint") && lintStep.cmd.at(-1) === "apps/docs");
  assert.deepEqual(typecheck.cmd, ["pnpm", "--filter", "@octoboard/docs", "typecheck"]);
  assert.deepEqual(build.cmd, ["pnpm", "--filter", "@octoboard/docs", "build"]);
  assert.deepEqual(tests.cmd, ["pnpm", "--filter", "@octoboard/docs", "test"]);
  assert.ok(build.heavy && tests.heavy && !typecheck.heavy && !lintStep.heavy);
  assert.deepEqual(labels(["apps/docs/README.md"]), []);
});

test("a docs build precedes the docs tests, which read its output", () => {
  const docs = labels(["apps/docs/astro.config.ts"]);
  assert.ok(docs.indexOf("docs build") < docs.indexOf("docs tests"));
});

test("a docs page also checks the website, whose SEO test reads the docs' content directory", () => {
  const selected = labels(["apps/docs/src/content/docs/index.md"]);
  for (const label of ["docs lint", "docs build", "docs tests", "web lint", "web build", "web tests"]) assert.ok(selected.includes(label), label);
});

test("a change to the language list the docs build their locales from checks the docs as well as the UI", () => {
  for (const file of ["packages/ui/src/i18n/languages.ts", "packages/ui/package.json"]) {
    const selected = labels([file]);
    for (const label of ["ui tests", "docs lint", "docs type check", "docs build"]) assert.ok(selected.includes(label), `${file}: ${label}`);
  }
  assert.ok(!labels(["apps/web/app/app.vue"]).includes("docs build"));
});

test("--full lists every consumer's steps, light steps before heavy ones", () => {
  const full = checkSteps({ mode: "full" });
  assert.deepEqual(full.map((step) => step.label), [
    "identity check", "provider-neutral core", "verify lint", "verify tests", "ui lint", "ui type check", "web lint", "web type check", "web locales", "docs lint",
    "docs type check", "desktop lint", "desktop script tests", "rustfmt check", "ui tests", "web build", "web tests", "docs build", "docs tests", "rust clippy",
    "rust tests",
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
  assert.deepEqual(release.map((step) => step.label), ["identity check", "provider-neutral core", "application lint", "application type check", "desktop script tests", "rustfmt check", "application UI tests", "rust clippy", "rust tests", "windows cross-check"]);
  assert.ok(release[2].cmd.includes("**/src/gallery/**"));
  assert.ok(release[2].cmd.includes("**/src/fixtures/**"));
  assert.ok(release[3].cmd.includes("typecheck:app"));
  assert.ok(release[6].cmd.includes("test:app"));
  assert.ok(release.slice(6).every((step) => step.heavy));
});

test("the UI build runs only for the scope that names it", () => {
  assert.ok(labels(["packages/ui/vite.config.ts"]).includes("ui build"));
  assert.ok(!labels(["packages/ui/src/App.tsx"]).includes("ui build"));
});

test("release groups together run every release check with identical commands and no macOS tests moved to Linux", () => {
  const all = checkSteps({ mode: "release" });
  const frontend = checkSteps({ mode: "release", group: "frontend" });
  const native = checkSteps({ mode: "release", group: "native" });
  const windows = checkSteps({ mode: "release", group: "windows" });
  const commands = (items) => [...new Set(items.map((step) => JSON.stringify(step.cmd)))].sort();
  assert.deepEqual(commands([...frontend, ...native, ...windows]), commands(all));
  assert.deepEqual(frontend.map((step) => step.label), ["identity check", "provider-neutral core", "application lint", "application type check", "application UI tests"]);
  assert.deepEqual(native.map((step) => step.label), ["identity check", "provider-neutral core", "desktop script tests", "rustfmt check", "rust clippy", "rust tests"]);
  assert.deepEqual(windows.map((step) => step.label), ["identity check", "provider-neutral core", "windows cross-check"]);
  assert.throws(() => checkSteps({ mode: "release", group: "unknown" }), /Unknown release check group/);
});

test("the complete release plan cross-checks both crates for Windows, a heavy step no hosted group runs", () => {
  const step = checkSteps({ mode: "release" }).find((item) => item.label === "windows cross-check");
  assert.deepEqual(step.cmd, ["pnpm", "check:windows"]);
  assert.equal(step.heavy, true);
  for (const group of ["frontend", "native"]) {
    assert.ok(!checkSteps({ mode: "release", group }).some((item) => item.cmd.includes("check:windows")), group);
  }
  assert.ok(!checkSteps({ mode: "full" }).some((item) => item.cmd.includes("check:windows")));
});
