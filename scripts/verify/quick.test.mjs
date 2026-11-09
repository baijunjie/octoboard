import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CRATE_WIDE_TESTS, CRATES, moduleFilter, quickSteps, RUST_MARKDOWN_READERS, rustChanges, rustTests, UI_DISK_TESTS, UI_DISK_TESTS_COVERED, UI_READERS, UI_UNTRACEABLE, uiTests } from "./quick.mjs";
import { planScope } from "./scope.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (file) => readFileSync(path.join(root, file), "utf8");
const tracked = (prefix) => execFileSync("git", ["ls-files", "-z", "--", prefix], { cwd: root, encoding: "utf8" }).split("\0").filter((file) => file && existsSync(path.join(root, file)));
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
  assert.deepEqual(rustTests(["apps/daemon/src/main.rs"]), { mode: "related", args: ["--bin", "octoboardd"] });
  assert.deepEqual(rustTests(["apps/desktop/src-tauri/src/lib.rs"]), { mode: "related", args: ["--lib", "--bin", "octoboard"] });
  assert.deepEqual(rustTests(["apps/daemon/src/coordinator.rs"]), { mode: "related", args: ["--bin", "octoboardd", "--", ...["coordinator::", ...WIDE].sort()] });
  assert.deepEqual(rustTests(["apps/desktop/src-tauri/src/menu.rs"]), { mode: "related", args: ["--lib", "--bin", "octoboard", "--", "menu::"] });
  assert.equal(rustTests(["apps/daemon/src/main.rs", "apps/desktop/src-tauri/src/menu.rs"]).mode, "full");
  assert.equal(rustTests(["apps/desktop/src-tauri/src/lib.rs", "apps/daemon/src/coordinator.rs"]).mode, "full");
  // The result is the caller's to change, never the crate's own list.
  assert.notEqual(rustTests(["apps/daemon/src/main.rs"]).args, CRATES.daemon.targets);
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
    "packages/ui/tsconfig.json", "packages/ui/src/data.txt", "pnpm-lock.yaml", "config/app.json", "unknown/file.txt",
  ]) {
    assert.equal(uiTests(["packages/ui/src/a/X.tsx", file]).mode, "full", file);
  }
});

test("a file outside the package that UI tests read runs those tests, and the protocol sources only through the generated types", () => {
  assert.deepEqual(uiTests(["apps/daemon/src/protocol/codes.rs"]), { mode: "related", files: ["src/daemonCodes.test.ts", ...UI_DISK_TESTS] });
  assert.deepEqual(uiTests(["apps/daemon/src/providers/claude.rs"]), {
    mode: "related", files: ["src/daemonCodes.test.ts", "src/fixtures/fixtureProviders.test.ts", ...UI_DISK_TESTS],
  });
  assert.deepEqual(uiTests(["LICENSE"]), { mode: "related", files: ["src/appConfig.test.ts", ...UI_DISK_TESTS] });
  assert.deepEqual(uiTests(["apps/desktop/src-tauri/Info.template.plist"]), { mode: "related", files: ["src/i18n/bundleLocalizations.test.ts", ...UI_DISK_TESTS] });
  assert.deepEqual(uiTests(["apps/daemon/src/protocol/requests.rs"]), { mode: "skip" });
  assert.deepEqual(uiTests(["apps/daemon/src/protocol/requests.rs", "packages/ui/src/protocol.generated.ts"]), {
    mode: "related", files: ["src/protocol.generated.ts", ...UI_DISK_TESTS],
  });
});

test("Markdown a crate reads runs the reading module's tests, and nothing of the UI", () => {
  assert.deepEqual(rustTests(["apps/daemon/PROTOCOL.md"]), { mode: "related", args: ["--bin", "octoboardd", "--", ...["session_tools::", ...WIDE].sort()] });
  assert.deepEqual(rustTests(["apps/daemon/src/adapter/codex/compatibility.md"]), { mode: "related", args: ["--bin", "octoboardd", "--", ...["adapter::", ...WIDE].sort()] });
  assert.deepEqual(uiTests(["apps/daemon/PROTOCOL.md"]), { mode: "skip" });
  assert.deepEqual(labels(["apps/daemon/PROTOCOL.md"]), ["identity check", "verify lint", "verify tests", "rustfmt check", "rust clippy", "rust related tests"]);
});

test("a deleted UI file is not passed to vitest, and nothing is run when only deletions remain", () => {
  const exists = (file) => file !== "packages/ui/src/a/Gone.tsx";
  assert.deepEqual(uiTests(["packages/ui/src/a/Gone.tsx", "packages/ui/src/a/X.tsx"], exists), { mode: "related", files: ["src/a/X.tsx", ...UI_DISK_TESTS] });
  assert.deepEqual(uiTests(["packages/ui/src/a/Gone.tsx"], exists), { mode: "skip" });
});

test("the quick plan drops builds and the full suites, keeping every other step of the normal plan", () => {
  const files = ["packages/ui/src/a/X.tsx", "apps/web/app/app.vue", "apps/daemon/src/coordinator.rs", "apps/desktop/scripts/bundle.mjs", "packages/ui/vite.config.ts"];
  assert.deepEqual(labels(files), [
    "identity check", "ui lint", "ui type check", "web lint", "web type check", "web locales", "desktop lint", "desktop script tests", "rustfmt check", "rust clippy", "ui tests",
    "rust related tests",
  ]);
});

test("narrowed test steps are heavy, last and never stamped; a fallback keeps the normal stamped suite", () => {
  const narrowed = quick(["packages/ui/src/a/X.tsx", "apps/daemon/src/coordinator.rs"]).steps;
  assert.deepEqual(narrowed.map((step) => step.label), ["identity check", "ui lint", "ui type check", "rustfmt check", "rust clippy", "ui related tests", "rust related tests"]);
  const related = narrowed.filter((step) => /related/.test(step.label));
  assert.ok(related.every((step) => step.heavy && step.stamp === false));
  assert.deepEqual(related[0].cmd, ["pnpm", "--filter", "@octoboard/ui", "exec", "vitest", "related", "--run", "src/a/X.tsx", ...UI_DISK_TESTS]);
  assert.deepEqual(related[1].cmd, ["node", "scripts/cargo.mjs", "test", "--bin", "octoboardd", "--", "coordinator::", "layering_tests::", "protocol::", "records::history::"]);
  assert.ok(narrowed.filter((step) => !/related/.test(step.label)).every((step) => step.stamp !== false));
  const fallback = quick(["packages/ui/vite.config.ts"]);
  const ui = fallback.steps.find((step) => step.label === "ui tests");
  assert.ok(ui && ui.stamp !== false && typeof ui.rerun === "function");
  assert.match(fallback.notes[0], /^ui tests run in full: packages\/ui\/vite\.config\.ts/);
});

test("without known changed files both suites stay in full, minus the builds", () => {
  const { steps, notes } = quickSteps({ mode: "full", packages: [] }, null);
  assert.deepEqual(steps.map((step) => step.label), [
    "identity check", "verify lint", "verify tests", "ui lint", "ui type check", "web lint", "web type check", "web locales", "desktop lint",
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
  assert.deepEqual(labels(["apps/desktop/scripts/bundle.mjs"]), ["identity check", "desktop lint", "desktop script tests"]);
});

test("every Markdown file a crate reads with include_str is mapped to the readers' modules", () => {
  const crateDirs = { daemon: "apps/daemon/", native: "apps/desktop/src-tauri/" };
  const found = {};
  for (const [crate, dir] of Object.entries(crateDirs)) {
    for (const file of tracked(dir).filter((name) => name.endsWith(".rs"))) {
      for (const [, target] of read(file).matchAll(/include_str!\(\s*"([^"]+\.md)"/g)) {
        const entry = (found[path.posix.normalize(path.posix.join(path.posix.dirname(file), target))] ??= { crate, filters: [] });
        const filter = moduleFilter(file.slice(dir.length));
        if (!entry.filters.includes(filter)) entry.filters.push(filter);
      }
    }
  }
  assert.ok(found["apps/daemon/PROTOCOL.md"], "the scan no longer finds the daemon's read of its protocol document; its pattern is stale");
  assert.deepEqual(RUST_MARKDOWN_READERS, found);
});

test("a Markdown file read from a file bound to no module runs its crate in full", () => {
  assert.deepEqual(rustChanges(["test/x.md"], { "test/x.md": { crate: "daemon", filters: [null] } }), { daemon: "full", native: null });
});

// What `vitest related` cannot trace: a UI file reading a file outside the package from disk, or importing one. Each
// such read must leave the reader in the run its target selects: the whole suite, the reading test, or (for an import
// the bundler follows) the target itself.
test("every file outside the package that a UI file reads or imports selects the reader", () => {
  for (const { tests } of UI_READERS) for (const test of tests) assert.ok(UI_DISK_TESTS_COVERED.includes(test), test);
  const patterns = [/new URL\(\s*[`"']((?:\.\.\/)+[^`"'$]+)[`"']/g, /(?:from|import)\s*\(?\s*["']((?:\.\.\/)+[^"'?]*)(?:\?[^"']*)?["']/g];
  const reads = [];
  for (const file of tracked("packages/ui/").filter((name) => /\.(?:[cm]?[jt]sx?)$/.test(name))) {
    const text = read(file);
    for (const [index, pattern] of patterns.entries()) {
      for (const [, target] of text.matchAll(pattern)) {
        const joined = path.posix.normalize(path.posix.join(path.posix.dirname(file), target));
        // An import names a module without its extension, or a directory by its index.
        const candidates = index === 1 ? ["", ".ts", ".tsx", ".mjs", "/index.ts", "/index.tsx"].map((suffix) => joined + suffix) : [joined];
        reads.push([file, candidates.find((candidate) => existsSync(path.join(root, candidate)) && statSync(path.join(root, candidate)).isFile()) ?? joined]);
      }
    }
  }
  // A read through a helper that builds the path from the repository root. `scope.test.mjs` cannot see it, so its
  // target has to give the UI its tests as well.
  const helperReads = [...read("packages/ui/src/appConfig.test.ts").matchAll(/repository\("([^"]+)"\)/g)].map(([, target]) => target);
  for (const target of helperReads) {
    assert.ok(planScope([target]).packages.includes("ui"), `packages/ui/src/appConfig.test.ts reads ${target}, which does not select the UI`);
    reads.push(["packages/ui/src/appConfig.test.ts", target]);
  }
  assert.ok(reads.some(([file, target]) => file === "packages/ui/src/daemonCodes.test.ts" && target === "apps/daemon/src/protocol/codes.rs"), "the scan's patterns are stale");
  assert.ok(reads.some(([, target]) => target === "LICENSE"), "the helper scan is stale");
  for (const [file, target] of reads) {
    if (target.startsWith("packages/ui/") || target.startsWith("..") || !planScope([target]).packages.includes("ui")) continue;
    const decision = uiTests([target]);
    const relative = (name) => path.posix.relative("packages/ui", name);
    assert.ok(
      decision.mode === "full" || (decision.mode === "related" && (decision.files.includes(relative(file)) || decision.files.includes(relative(target)))),
      `${file} reads ${target}, whose quick run leaves it out`,
    );
  }
});

// A target the selection misses would never run in a quick run, so each crate's test targets are held to what the
// manifest and the source tree declare.
test("the targets a crate is narrowed to are all of its test targets", () => {
  const manifest = (crate) => read(`${CRATES[crate].dir}Cargo.toml`);
  const has = (file) => existsSync(path.join(root, file));
  for (const crate of Object.keys(CRATES)) {
    const { dir } = CRATES[crate];
    for (const kind of ["test", "bench", "example"]) assert.doesNotMatch(manifest(crate), new RegExp(`^\\[\\[${kind}\\]\\]`, "m"), `${dir}: [[${kind}]]`);
    for (const extra of ["tests", "src/bin", "examples", "benches"]) assert.ok(!has(`${dir}${extra}`), `${dir}${extra}`);
  }
  // One `[[bin]]` table, named `octoboardd` wherever the key sits in it.
  const bins = manifest("daemon").split(/^(?=\[)/m).filter((table) => table.startsWith("[[bin]]"));
  assert.deepEqual(bins.map((table) => table.match(/^name = "([^"]+)"/m)?.[1]), ["octoboardd"]);
  assert.ok(!has(`${CRATES.daemon.dir}src/lib.rs`) && !/^\[lib\]/m.test(manifest("daemon")), "the daemon gained a library, which `--bin` does not select");
  assert.deepEqual(CRATES.daemon.targets, ["--bin", "octoboardd"]);
  // The desktop crate: its library and the default binary of `src/main.rs`, named after the package.
  assert.match(manifest("native"), /^\[package\]\s*\nname = "octoboard"/m);
  assert.doesNotMatch(manifest("native"), /^\[\[bin\]\]/m);
  assert.ok(has(`${CRATES.native.dir}src/main.rs`) && has(`${CRATES.native.dir}src/lib.rs`));
  assert.deepEqual(CRATES.native.targets, ["--lib", "--bin", "octoboard"]);
  // `--lib` leaves out doctests, which the full suite runs, and cargo does not combine `--doc` with `--lib` or `--bin`:
  // a first doctest in the library needs a change to how the crate is narrowed, not to `CRATES` alone. The check is
  // conservative: every fenced block in a line doc comment counts as a doctest unless its opening fence is exactly
  // `text` or `ignore`, so does a line indented by four spaces that opens a doc comment or follows a blank doc line (an
  // indented code block), and `#[doc]` attributes and block doc comments, which it does not parse, are refused.
  const doctest = "counts as a doctest, which a quick run narrowed to the desktop crate's targets would not run";
  for (const file of tracked(`${CRATES.native.dir}src/`).filter((name) => name.endsWith(".rs"))) {
    const text = read(file);
    // The fence that opened the current block, and the previous doc line (null outside a doc comment).
    let open = null;
    let previous = null;
    for (const line of text.split("\n")) {
      const doc = line.match(/^\s*\/\/[/!] ?(.*)$/)?.[1] ?? null;
      const indented = !open && (previous === null || previous.trim() === "") && doc !== null && /^ {4,}\S/.test(doc);
      assert.ok(!indented, `${file}: the indented block at "${line.trim()}" ${doctest}`);
      previous = doc;
      const fence = line.match(/^\s*\/\/[/!]\s*(```+|~~~+)\s*([\w,-]*)/);
      if (!fence) continue;
      if (!open) {
        assert.match(fence[2], /^(?:text|ignore)$/, `${file}: the block opening with "${line.trim()}" ${doctest}`);
        open = fence[1];
      } else if (fence[1][0] === open[0] && fence[1].length >= open.length) open = null;
    }
    assert.doesNotMatch(text, /#!?\[doc\s*=|^\s*\/\*[*!]/m, `${file}: a #[doc] attribute or a block doc comment may hold a doctest; it is not checked`);
  }
});
