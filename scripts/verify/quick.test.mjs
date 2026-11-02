import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CRATE_WIDE_TESTS, moduleFilter, quickSteps, rustChanges, rustTests, UI_DISK_TESTS, UI_DISK_TESTS_COVERED, UI_UNTRACEABLE, uiTests } from "./quick.mjs";
import { planScope } from "./scope.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (file) => readFileSync(path.join(root, file), "utf8");
const walk = (directory) => readdirSync(path.join(root, directory), { withFileTypes: true }).flatMap((entry) => {
  if (entry.name === "node_modules") return [];
  const file = `${directory}/${entry.name}`;
  return entry.isDirectory() ? walk(file) : [file];
});

const quick = (files) => quickSteps(planScope(files), files);
const WIDE = ["layering_tests::", "protocol::", "records::history::"];
const labels = (files) => quick(files).steps.map((step) => step.label);

test("a crate source file maps to its top-level module as a libtest filter", () => {
  assert.equal(moduleFilter("src/coordinator.rs"), "coordinator::");
  assert.equal(moduleFilter("src/processes/ledger/attribute.rs"), "processes::");
  assert.equal(moduleFilter("src/processes/ledger/tests.rs"), "processes::");
  assert.equal(moduleFilter("src/foo/mod.rs"), "foo::");
  assert.equal(moduleFilter("src/foo/bar/baz.rs"), "foo::");
});

test("crate roots, separate targets and files outside the sources are not bound to one module", () => {
  for (const file of ["src/main.rs", "src/lib.rs", "src/bin/tool.rs", "build.rs", "Cargo.toml", "tests/it.rs", "testdata/a.bin"]) {
    assert.equal(moduleFilter(file), null, file);
  }
});

test("Rust changes become filters per crate, or the crate's full tests", () => {
  assert.deepEqual(rustChanges(["apps/daemon/src/coordinator.rs", "apps/daemon/src/processes/ledger/tests.rs", "apps/desktop/src-tauri/src/menu.rs"]), {
    daemon: ["coordinator::", "processes::"], native: ["menu::"],
  });
  assert.deepEqual(rustChanges(["apps/daemon/src/main.rs", "apps/daemon/src/hooks.rs"]), { daemon: "full", native: null });
  assert.deepEqual(rustChanges(["apps/daemon/testdata/a.bin"]), { daemon: "full", native: null });
  assert.deepEqual(rustChanges(["apps/daemon/Cargo.toml"]), { daemon: "full", native: "full" });
  assert.deepEqual(rustChanges(["apps/daemon/README.md", "docs/a.md", "packages/ui/src/App.tsx"]), { daemon: null, native: null });
  // A file the daemon's tests read, and one no rule knows.
  assert.deepEqual(rustChanges(["packages/ui/src/i18n/languages.ts"]), { daemon: "full", native: null });
  assert.deepEqual(rustChanges(["unknown/file.txt"]), { daemon: "full", native: "full" });
});

test("the Rust test run is skipped, narrowed to filters or one crate, or kept in full", () => {
  assert.deepEqual(rustTests(["packages/ui/src/App.tsx"]), { mode: "skip" });
  assert.deepEqual(rustTests(["apps/daemon/src/coordinator.rs", "apps/desktop/src-tauri/src/menu.rs", "apps/daemon/src/hooks.rs"]), {
    mode: "related", args: ["--", "coordinator::", "hooks::", "menu::", ...WIDE].sort(),
  });
  assert.deepEqual(rustTests(["apps/daemon/src/main.rs"]), { mode: "related", args: ["-p", "octoboardd"] });
  assert.deepEqual(rustTests(["apps/desktop/src-tauri/src/lib.rs"]), { mode: "related", args: ["-p", "octoboard"] });
  assert.equal(rustTests(["apps/daemon/src/main.rs", "apps/desktop/src-tauri/src/menu.rs"]).mode, "full");
  assert.equal(rustTests(["Cargo.lock"]).mode, "full");
});

test("UI changes are traced by their source and test files", () => {
  assert.deepEqual(uiTests(["packages/ui/src/a/X.tsx", "packages/ui/src/a/X.test.tsx", "packages/ui/README.md"]), {
    mode: "related", files: ["src/a/X.tsx", "src/a/X.test.tsx", ...UI_DISK_TESTS],
  });
  assert.deepEqual(uiTests(["apps/daemon/src/main.rs", "docs/a.md"]), { mode: "skip" });
  assert.deepEqual(uiTests(["scripts/verify/lock.mjs"]), { mode: "related", files: ["../../scripts/verify/lock.mjs", ...UI_DISK_TESTS] });
});

test("UI changes that imports do not show run the full suite", () => {
  for (const file of [
    "packages/ui/vite.config.ts", "packages/ui/src/test/setup.ts", "packages/ui/src/test/failedFilesReporter.ts", "packages/ui/package.json",
    "packages/ui/tsconfig.json", "packages/ui/src/data.txt", "pnpm-lock.yaml", "config/app.json", "LICENSE",
    "apps/desktop/src-tauri/Info.plist", "unknown/file.txt",
  ]) {
    assert.equal(uiTests(["packages/ui/src/a/X.tsx", file]).mode, "full", file);
  }
});

test("a deleted UI file is not passed to vitest, and nothing is run when only deletions remain", () => {
  const exists = (file) => file !== "packages/ui/src/a/Gone.tsx";
  assert.deepEqual(uiTests(["packages/ui/src/a/Gone.tsx", "packages/ui/src/a/X.tsx"], exists), { mode: "related", files: ["src/a/X.tsx", ...UI_DISK_TESTS] });
  assert.deepEqual(uiTests(["packages/ui/src/a/Gone.tsx"], exists), { mode: "skip" });
});

test("the quick plan drops builds and the full suites, keeping every other step of the normal plan", () => {
  const files = ["packages/ui/src/a/X.tsx", "apps/web/app/app.vue", "apps/daemon/src/coordinator.rs", "apps/desktop/scripts/bundle.mjs", "packages/ui/vite.config.ts"];
  assert.deepEqual(labels(files), [
    "ui lint", "ui type check", "web lint", "web type check", "web locales", "desktop lint", "desktop script tests", "rustfmt check", "rust clippy", "ui tests",
    "rust related tests",
  ]);
});

test("narrowed test steps are heavy, last and never stamped; a fallback keeps the normal stamped suite", () => {
  const narrowed = quick(["packages/ui/src/a/X.tsx", "apps/daemon/src/coordinator.rs"]).steps;
  assert.deepEqual(narrowed.map((step) => step.label), ["ui lint", "ui type check", "rustfmt check", "rust clippy", "ui related tests", "rust related tests"]);
  const related = narrowed.filter((step) => /related/.test(step.label));
  assert.ok(related.every((step) => step.heavy && step.stamp === false));
  assert.deepEqual(related[0].cmd, ["pnpm", "--filter", "@octoboard/ui", "exec", "vitest", "related", "--run", "src/a/X.tsx", ...UI_DISK_TESTS]);
  assert.deepEqual(related[1].cmd, ["node", "scripts/cargo.mjs", "test", "--", "coordinator::", "layering_tests::", "protocol::", "records::history::"]);
  assert.ok(narrowed.filter((step) => !/related/.test(step.label)).every((step) => step.stamp !== false));
  const fallback = quick(["packages/ui/vite.config.ts"]);
  const ui = fallback.steps.find((step) => step.label === "ui tests");
  assert.ok(ui && ui.stamp !== false && typeof ui.rerun === "function");
  assert.match(fallback.notes[0], /^ui tests run in full: packages\/ui\/vite\.config\.ts/);
});

test("without known changed files both suites stay in full, minus the builds", () => {
  const { steps, notes } = quickSteps({ mode: "full", packages: [] }, null);
  assert.deepEqual(steps.map((step) => step.label), [
    "verify lint", "verify tests", "ui lint", "ui type check", "web lint", "web type check", "web locales", "desktop lint",
    "desktop script tests", "rustfmt check", "rust clippy", "ui tests", "rust tests",
  ]);
  assert.equal(notes.length, 2);
  assert.match(quickSteps({ mode: "full", reason: "base origin/x is not readable", packages: [] }, null).notes[0], /: base origin\/x is not readable$/);
});

test("changes to what the sweep test spawns select the sweep test", () => {
  for (const file of ["packages/ui/scripts/gallery-sweep.mjs", "packages/ui/scripts/test-fixtures/gallery-sweep-loader.mjs", "scripts/verify/lock.mjs", "packages/ui/src/style.css"]) {
    const decision = uiTests([file]);
    assert.equal(decision.mode, "related", file);
    assert.ok(decision.files.includes("scripts/gallery-sweep.test.mjs"), file);
  }
});

test("every path the quick lists name exists", () => {
  for (const test of [...UI_DISK_TESTS, ...UI_DISK_TESTS_COVERED]) assert.ok(existsSync(path.join(root, "packages/ui", test)), test);
  for (const file of UI_UNTRACEABLE) assert.ok(existsSync(path.join(root, file)), file);
  for (const test of CRATE_WIDE_TESTS.daemon) for (const file of test.covers) assert.ok(existsSync(path.join(root, "apps/daemon", file)), file);
});

test("every crate-wide filter is the path of a module that defines the test it exists for", () => {
  const crateDirs = { daemon: "apps/daemon", native: "apps/desktop/src-tauri" };
  const sources = (directory) => walk(directory).filter((file) => file.endsWith(".rs"));
  for (const [crate, tests] of Object.entries(CRATE_WIDE_TESTS)) {
    for (const { filter, test: name } of tests) {
      const modulePath = path.posix.join(crateDirs[crate], "src", ...filter.split("::").filter(Boolean));
      // A module is its file, its directory, or both.
      const files = [...(existsSync(path.join(root, modulePath)) ? sources(modulePath) : []), ...[`${modulePath}.rs`].filter((file) => existsSync(path.join(root, file)))];
      assert.notEqual(files.length, 0, `${filter} names no module of the ${crate} crate; the filter has to follow its test`);
      const defined = new RegExp(String.raw`#\[(?:tokio::)?test(?:\([^\]]*\))?\](?:\s*#\[[^\]]*\])*\s*(?:async\s+)?fn ${name}\b`);
      assert.ok(files.some((file) => defined.test(read(file))), `${filter} holds no test named ${name}; the entry has to follow its test`);
    }
  }
});

test("the files the UI's vitest configuration names are all untraceable, and nothing else is listed", () => {
  const config = read("packages/ui/vite.config.ts");
  const named = new Set(["packages/ui/vite.config.ts"]);
  const inside = (specifier) => path.posix.join("packages/ui", specifier);
  for (const match of config.matchAll(/(?:setupFiles|reporters):[^\n]*/g)) {
    for (const [, specifier] of match[0].matchAll(/"(\.\/[^"]+\.[cm]?[jt]s)"/g)) named.add(inside(specifier));
  }
  for (const [, specifier] of config.matchAll(/^import .*from "(\.\/[^"]+)"/gm)) named.add(inside(specifier));
  for (const [, specifier] of config.matchAll(/^import .*from "(\.\.\/[^"]+)"/gm)) {
    const file = path.posix.join("packages/ui", specifier);
    // What it imports from outside the package is selected by a rule of `scope.mjs` (the full suite).
    assert.ok(uiTests(["packages/ui/src/a/X.tsx", file]).mode === "full" || existsSync(path.join(root, file)) === false, file);
  }
  assert.deepEqual([...named].sort(), [...UI_UNTRACEABLE].sort());
});

test("every UI test that uses the file system is named by the lists", () => {
  const reads = /node:fs|node:child_process|\bfrom "(?:fs|child_process)(?:\/promises)?"|readFileSync|readdirSync|\bspawn\(|execFile/;
  const named = new Set([...UI_DISK_TESTS, ...UI_DISK_TESTS_COVERED].map((file) => `packages/ui/${file}`));
  const tests = [...walk("packages/ui/src"), ...walk("packages/ui/scripts")].filter((file) => /\.test\.(?:[tj]sx?|mjs)$/.test(file));
  assert.ok(tests.length > 100);
  const unnamed = tests.filter((file) => reads.test(read(file)) && !named.has(file));
  assert.deepEqual(unnamed, [], "name these in UI_DISK_TESTS (they read UI sources) or UI_DISK_TESTS_COVERED (they do not)");
});

test("the daemon's crate-wide tests cover every file with a whole-crate check", () => {
  const covered = new Set(CRATE_WIDE_TESTS.daemon.flatMap((test) => test.covers));
  const sources = walk("apps/daemon/src").filter((file) => file.endsWith(".rs")).map((file) => file.replace("apps/daemon/", ""));
  // Types deriving `TS` are checked together by the wire-type contract; a source scan reads the crate's `src`.
  const wholeCrate = sources.filter((file) => {
    const text = read(`apps/daemon/${file}`);
    return /derive\([^)]*\bTS\b/.test(text) || (text.includes("CARGO_MANIFEST_DIR") && /\.join\("src"\)/.test(text));
  });
  assert.ok(wholeCrate.length >= 8);
  assert.deepEqual(wholeCrate.filter((file) => !covered.has(file)), [], "name these in CRATE_WIDE_TESTS");
  for (const file of covered) assert.ok(wholeCrate.includes(file), `${file} no longer has a whole-crate check`);
});

test("the desktop crate has no whole-crate test the module filters would skip", () => {
  assert.deepEqual(CRATE_WIDE_TESTS.native, []);
  for (const file of walk("apps/desktop/src-tauri/src").filter((name) => name.endsWith(".rs"))) {
    assert.doesNotMatch(read(file), /derive\([^)]*\bTS\b|CARGO_MANIFEST_DIR|read_dir\(.*src/, file);
  }
});

test("a change that selects no UI or Rust tests adds no test step", () => {
  assert.deepEqual(labels(["apps/desktop/scripts/bundle.mjs"]), ["desktop lint", "desktop script tests"]);
});
