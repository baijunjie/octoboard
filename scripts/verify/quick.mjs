// The quick plan: the normal plan's lint, type, locale, formatting and script-test steps, with the
// builds and the two full test suites left out and the UI and Rust tests narrowed to the changed
// files (`--quick`). A narrowing that cannot be trusted for a change falls back to the full suite, which
// then keeps the normal step (and so its stamp). The narrowed steps carry `stamp: false`: their pass
// says nothing about the content as a whole, so it is never recorded as the suite's pass.
import path from "node:path";
import { matches, matchingRules, SWEEP_SHARED } from "./scope.mjs";
import { checkSteps } from "./steps.mjs";

const UI_ROOT = "packages/ui/";

// What `vitest related` follows is the import graph, so only these file types can be traced to the
// tests that load them.
const TRACEABLE = /\.(?:[cm]?[jt]sx?|css)$/;

// UI files that shape every test run without being imported by the tests: what `vite.config.ts`
// names for the tests (the setup file and the reporter) and the configuration itself. The
// configuration's own imports are in the list too; `quick.test.mjs` holds it to the configuration.
export const UI_UNTRACEABLE = [
  "packages/ui/vite.config.ts",
  "packages/ui/src/test/setup.ts",
  "packages/ui/src/test/failedFilesReporter.ts",
];

// UI tests that read sources from disk (or run them in another process) instead of importing them,
// which the import graph does not show: a change to what they read does not select them. They are
// added to every narrowed run, so a change to the stylesheet, a source tree they scan, or the gallery
// sweep script, its fixtures or `lock.mjs` (which it loads) is checked by them. `quick.test.mjs`
// fails when a UI test reads files and is named in neither this list nor `UI_DISK_TESTS_COVERED`.
export const UI_DISK_TESTS = [
  "src/style.test.ts",
  "src/components/scroll/scrollFadeCoverage.test.ts",
  "src/components/status/sleepGlyph.test.ts",
  "src/errorModulesCycle.test.ts",
  "src/cssDependencies.test.ts",
  "src/layout/paneWidth.test.tsx",
  "src/components/text/InlineReference.test.tsx",
  "src/components/form/AdvancedSection.test.tsx",
  "scripts/gallery-sweep.test.mjs",
  "scripts/gallery-compare.test.mjs",
];

// UI tests that use the file system without reading UI sources: they read files outside the package
// that a rule of `scope.mjs` names (a change to those runs what `UI_READERS` gives them, or the whole suite), or only a
// scratch directory.
export const UI_DISK_TESTS_COVERED = [
  "src/appConfig.test.ts",
  "src/daemonCodes.test.ts",
  "src/fixtures/fixtureProviders.test.ts",
  "src/i18n/bundleLocalizations.test.ts",
  "scripts/generate-licenses.test.mjs",
];

// Files outside the package that UI tests read, with the tests that read them (paths relative to the package), so a
// change to one runs those tests instead of the full suite. The first entry a file matches decides; an entry is a
// prefix (ending in "/") or an exact path. The rest of the protocol sources reach the UI only through
// `protocol.generated.ts`, which a daemon test holds to them and `vitest related` traces when it changes, so they add
// no test. A file another rule of `scope.mjs` gives to the UI and no entry names runs the full suite.
// `quick.test.mjs` fails when a UI file reads or imports a file outside the package that its run would then skip.
export const UI_READERS = [
  { match: "apps/daemon/src/protocol/codes.rs", tests: ["src/daemonCodes.test.ts"] },
  { match: "apps/daemon/src/protocol/", tests: [] },
  { match: "apps/daemon/src/providers/", tests: ["src/daemonCodes.test.ts", "src/fixtures/fixtureProviders.test.ts"] },
  { match: "apps/desktop/src-tauri/Info.template.plist", tests: ["src/i18n/bundleLocalizations.test.ts"] },
  { match: "LICENSE", tests: ["src/appConfig.test.ts"] },
];

const uiReaders = (file) => UI_READERS.find(({ match }) => matches(match, file))?.tests;

// The UI test run for `files`: `{ mode: "skip" }`, `{ mode: "full", why }` or `{ mode: "related", files }`.
// A file that no longer exists cannot be traced and is left out (what imported it changed too).
// `UI_DISK_TESTS` come with every narrowed run.
export function uiTests(files, exists = () => true) {
  const related = [];
  for (const file of files) {
    if (file.startsWith(UI_ROOT)) {
      if (file.endsWith(".md")) continue;
      if (UI_UNTRACEABLE.includes(file) || !TRACEABLE.test(file)) return { mode: "full", why: `${file} reaches the UI's tests in ways imports do not show` };
      if (exists(file)) related.push(path.posix.relative(UI_ROOT, file));
      continue;
    }
    const rules = matchingRules(file);
    if (rules.length === 0 ? file.endsWith(".md") : !rules.some((rule) => rule.consumers.includes("ui"))) continue;
    const readers = uiReaders(file);
    if (readers) {
      related.push(...readers);
      continue;
    }
    if (!SWEEP_SHARED.includes(file)) return { mode: "full", why: `${file} is read by the UI's tests without being imported` };
    if (exists(file)) related.push(path.posix.relative(UI_ROOT, file));
  }
  if (related.length === 0) return { mode: "skip" };
  return { mode: "related", files: [...new Set([...related, ...UI_DISK_TESTS])] };
}

// The files that declare wire types, which derive `TS` and `JsonSchema` together.
const WIRE_TYPE_FILES = [
  "src/protocol/accounts.rs", "src/protocol/agent_cli.rs", "src/protocol/agents.rs", "src/protocol/browse.rs", "src/protocol/configuration.rs", "src/protocol/events.rs",
  "src/protocol/git_workflow.rs", "src/protocol/history.rs", "src/protocol/host.rs", "src/protocol/mcp_inventory.rs", "src/protocol/models.rs", "src/protocol/page_content/chart.rs",
  "src/protocol/page_content/mod.rs", "src/protocol/path_links.rs", "src/protocol/processes.rs",
  "src/protocol/providers.rs", "src/protocol/quotas.rs", "src/protocol/records.rs", "src/protocol/requests.rs", "src/protocol/session_branches.rs", "src/protocol/settings.rs",
  "src/protocol/terminal.rs", "src/providers/mod.rs",
];

// Tests of one crate that check the whole crate, so a change to any module can fail them: they run
// with every set of module filters for that crate. `quick.test.mjs` fails when a crate has files of
// the kinds they check (`covers`) that are not named here.
// A `filter` is its test's module path and `test` the name of the function the entry exists for; both
// have to follow the test when it moves: a filter that matches no test runs nothing and fails nothing.
export const CRATE_WIDE_TESTS = {
  daemon: [
    { filter: "protocol::", test: "committed_bindings_are_current", why: "the wire-type contract (`protocol/bindings.rs`) covers every type deriving `TS`", covers: WIRE_TYPE_FILES },
    { filter: "protocol::", test: "committed_schema_is_current", why: "the schema contract (`protocol/schema.rs`) covers every type deriving `JsonSchema`", covers: WIRE_TYPE_FILES },
    { filter: "protocol::", test: "the_schema_only_grows_since_the_last_release", why: "the compatibility guard (`protocol/compatibility/`) compares the rendering of every wire type with the last release's snapshot", covers: WIRE_TYPE_FILES },
    { filter: "protocol::", test: "the_bindings_and_the_schema_agree_on_every_property", why: "the cross-check (`protocol/crosscheck.rs`) compares both renderings of every wire type and scans every source file that derives `TS`", covers: [...WIRE_TYPE_FILES, "src/protocol/crosscheck.rs"] },
    { filter: "records::history::", test: "every_opening_octoboard_writes_with_is_matched", why: "scans every source file for `*_OPENING` constants", covers: ["src/records/history.rs"] },
    { filter: "layering_tests::", test: "every_top_level_module_depends_only_downward_or_as_listed", why: "scans every source file's `crate::` paths against the daemon's layer order", covers: ["src/layering_tests/scan.rs"] },
  ],
  native: [],
};

// Markdown a crate reads with `include_str!`, as the module filters of the files that read it (null for a file bound to
// no module, which runs the crate in full), so a change to one runs those modules' tests instead of the crate's.
// `quick.test.mjs` fails when a read is missing or a filter is not its reader's module.
export const RUST_MARKDOWN_READERS = {
  "apps/daemon/PROTOCOL.md": { crate: "daemon", filters: ["session_tools::"] },
  "apps/daemon/src/adapter/claude/compatibility.md": { crate: "daemon", filters: ["adapter::"] },
  "apps/daemon/src/adapter/codex/compatibility.md": { crate: "daemon", filters: ["adapter::"] },
};

// `targets` selects every test target of the crate (the daemon is one binary; the desktop crate a library and its
// binary, with no doctests) while the whole workspace stays selected. Selecting the package with `-p` instead would
// resolve the dependencies' features for that crate alone, which builds a second copy of every dependency whose
// features differ and of the crate itself, beside the one the workspace runs use. `quick.test.mjs` fails when a crate
// gains a test target these do not select.
export const CRATES = {
  daemon: { dir: "apps/daemon/", targets: ["--bin", "octoboardd"] },
  native: { dir: "apps/desktop/src-tauri/", targets: ["--lib", "--bin", "octoboard"] },
};

// The module a crate source file belongs to as a libtest filter, or null when the file is not
// bound to one module: a crate root declares them all, and `src/bin` holds separate targets.
export function moduleFilter(relative) {
  const parts = relative.split("/");
  if (parts[0] !== "src" || !relative.endsWith(".rs") || parts.length < 2) return null;
  const name = parts[1].replace(/\.rs$/, "");
  if (parts.length === 2 && (name === "main" || name === "lib")) return null;
  if (name === "bin" && parts.length > 2) return null;
  return `${name}::`;
}

// What each crate's tests need for `files`: null (nothing), "full" or the list of module filters.
export function rustChanges(files, markdownReaders = RUST_MARKDOWN_READERS) {
  const need = { daemon: null, native: null };
  const add = (crate, change) => {
    if (need[crate] === "full") return;
    if (change === "full") need[crate] = "full";
    else need[crate] = [...new Set([...(need[crate] ?? []), change])];
  };
  for (const file of files) {
    const markdown = markdownReaders[file];
    if (markdown) {
      markdown.filters.forEach((filter) => add(markdown.crate, filter ?? "full"));
      continue;
    }
    const crate = Object.keys(CRATES).find((name) => file.startsWith(CRATES[name].dir));
    const module = crate && file.endsWith(".rs") ? moduleFilter(file.slice(CRATES[crate].dir.length)) : null;
    if (module) {
      add(crate, module);
      continue;
    }
    const rules = matchingRules(file);
    const hit = rules.length === 0 ? (file.endsWith(".md") ? [] : Object.keys(CRATES)) : Object.keys(CRATES).filter((name) => rules.some((rule) => rule.consumers.includes(name)));
    hit.forEach((name) => add(name, "full"));
  }
  return need;
}

// The Rust test run for `files`: `{ mode: "skip" }`, `{ mode: "full", why }` or `{ mode: "related", args }` with
// the arguments for `scripts/cargo.mjs test`. The whole workspace stays selected, as in the full suite, so the
// crates share one feature set and one build; one crate alone is narrowed to its test targets. Module filters of a
// crate come with its `CRATE_WIDE_TESTS`. A crate that needs its full tests next to filters for the other makes the
// whole workspace's tests run, since one `cargo test` cannot apply filters to one crate only.
export function rustTests(files) {
  const need = rustChanges(files);
  const crates = Object.keys(CRATES).filter((name) => need[name] !== null);
  if (crates.length === 0) return { mode: "skip" };
  const full = crates.filter((name) => need[name] === "full");
  const targets = crates.length === 1 ? CRATES[crates[0]].targets : [];
  if (full.length === 1 && crates.length === 1) return { mode: "related", args: [...targets] };
  if (full.length > 0) return { mode: "full", why: "a crate root, a manifest or a file outside the sources changed, which can change any test" };
  const wide = crates.flatMap((name) => CRATE_WIDE_TESTS[name].map((test) => test.filter));
  return { mode: "related", args: [...targets, "--", ...[...new Set([...crates.flatMap((name) => need[name]), ...wide])].sort()] };
}

const BUILDS_AND_SUITES = new Set(["web build", "ui build", "web tests", "ui tests", "rust tests"]);

// The quick steps and the notes explaining each fallback, for `scope` and its changed `files`
// (null when they are not known, which keeps both suites in full).
export function quickSteps(scope, files, exists) {
  const normal = checkSteps(scope);
  const notes = [];
  const unknown = files === null ? (scope.reason ?? "the changed files are not known") : null;
  const replace = (label, decision, narrowed) => {
    const original = normal.find((step) => step.label === label);
    if (!original) return [];
    if (decision.mode === "skip") return [];
    if (decision.mode === "full") {
      notes.push(`${label} run in full: ${decision.why}`);
      return [original];
    }
    return [narrowed(decision)];
  };
  const ui = unknown ? { mode: "full", why: unknown } : uiTests(files, exists);
  const rust = unknown ? { mode: "full", why: unknown } : rustTests(files);
  const tests = [
    ...replace("ui tests", ui, ({ files: related }) => ({
      label: "ui related tests", stamp: false, heavy: true,
      cmd: ["pnpm", "--filter", "@octoboard/ui", "exec", "vitest", "related", "--run", ...related],
    })),
    ...replace("rust tests", rust, ({ args }) => ({ label: "rust related tests", stamp: false, heavy: true, cmd: ["node", "scripts/cargo.mjs", "test", ...args] })),
  ];
  const kept = normal.filter((step) => !BUILDS_AND_SUITES.has(step.label) || tests.includes(step));
  const light = kept.filter((step) => !step.heavy);
  const heavy = [...kept.filter((step) => step.heavy && !tests.includes(step)), ...tests];
  return { steps: [...light, ...heavy], notes };
}
