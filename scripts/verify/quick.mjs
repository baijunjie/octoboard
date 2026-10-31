// The quick plan: the normal plan's lint, type, locale, formatting and script-test steps, with the
// builds and the two full test suites left out and the UI and Rust tests narrowed to the changed
// files (`--quick`). A narrowing that cannot be trusted for a change falls back to the full suite, which
// then keeps the normal step (and so its stamp). The narrowed steps carry `stamp: false`: their pass
// says nothing about the content as a whole, so it is never recorded as the suite's pass.
import path from "node:path";
import { matchingRules, SWEEP_SHARED } from "./scope.mjs";
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
  "src/errorModulesCycle.test.ts",
  "src/cssDependencies.test.ts",
  "src/layout/paneWidth.test.tsx",
  "src/components/text/InlineReference.test.tsx",
  "src/components/form/AdvancedSection.test.tsx",
  "src/protocol.contract.test.ts",
  "scripts/gallery-sweep.test.mjs",
];

// UI tests that use the file system without reading UI sources: they read files outside the package
// that a rule of `scope.mjs` names (so a change to those runs the whole suite), or only a scratch directory.
export const UI_DISK_TESTS_COVERED = [
  "src/appConfig.test.ts",
  "src/daemonCodes.test.ts",
  "src/i18n/bundleLocalizations.test.ts",
  "scripts/generate-licenses.test.mjs",
];

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
    if (!SWEEP_SHARED.includes(file)) return { mode: "full", why: `${file} is read by the UI's tests without being imported` };
    if (exists(file)) related.push(path.posix.relative(UI_ROOT, file));
  }
  if (related.length === 0) return { mode: "skip" };
  return { mode: "related", files: [...new Set([...related, ...UI_DISK_TESTS])] };
}

// Tests of one crate that check the whole crate, so a change to any module can fail them: they run
// with every set of module filters for that crate. `quick.test.mjs` fails when a crate has files of
// the kinds they check (`covers`) that are not named here.
// A `filter` is its test's module path and has to follow the test when its module moves: a filter that
// matches no test runs nothing and fails nothing.
export const CRATE_WIDE_TESTS = {
  daemon: [
    { filter: "protocol::", why: "the wire-type contract (`protocol/bindings.rs`) covers every type deriving `TS`", covers: [
      "src/protocol/accounts.rs", "src/protocol/agents.rs", "src/protocol/browse.rs", "src/protocol/events.rs", "src/protocol/history.rs",
      "src/protocol/models.rs", "src/protocol/path_links.rs", "src/protocol/quotas.rs", "src/protocol/records.rs", "src/protocol/requests.rs",
      "src/protocol/session_branches.rs", "src/protocol/settings.rs", "src/protocol/terminal.rs", "src/config_document/export.rs",
      "src/config_document/import/accounts.rs", "src/config_document/import/mod.rs", "src/git_workflow.rs", "src/processes/ledger/trees.rs",
      "src/accounts/shared_config/link.rs",
    ] },
    { filter: "records::history::", why: "scans every source file for `*_OPENING` constants", covers: ["src/records/history.rs"] },
  ],
  native: [],
};

const CRATES = {
  daemon: { dir: "apps/daemon/", pkg: "octoboardd" },
  native: { dir: "apps/desktop/src-tauri/", pkg: "octoboard" },
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
export function rustChanges(files) {
  const need = { daemon: null, native: null };
  const add = (crate, change) => {
    if (need[crate] === "full") return;
    if (change === "full") need[crate] = "full";
    else need[crate] = [...new Set([...(need[crate] ?? []), change])];
  };
  for (const file of files) {
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
// the arguments for `scripts/test-rust.mjs`. The whole workspace is built and run together, as the full
// suite does, so the crates share one feature set. One crate alone runs with `-p`: after a workspace
// build that reuses every dependency (only the desktop crate itself is rebuilt, as its build script
// reruns when the selection changes). Module filters of a crate come with its `CRATE_WIDE_TESTS`. A crate
// that needs its full tests next to filters for the other makes the whole workspace's tests run, since one
// `cargo test` cannot apply filters to one crate only.
export function rustTests(files) {
  const need = rustChanges(files);
  const crates = Object.keys(CRATES).filter((name) => need[name] !== null);
  if (crates.length === 0) return { mode: "skip" };
  const full = crates.filter((name) => need[name] === "full");
  if (full.length === 1 && crates.length === 1) return { mode: "related", args: ["-p", CRATES[full[0]].pkg] };
  if (full.length > 0) return { mode: "full", why: "a crate root, a manifest or a file outside the sources changed, which can change any test" };
  const wide = crates.flatMap((name) => CRATE_WIDE_TESTS[name].map((test) => test.filter));
  return { mode: "related", args: ["--", ...[...new Set([...crates.flatMap((name) => need[name]), ...wide])].sort()] };
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
    ...replace("rust tests", rust, ({ args }) => ({ label: "rust related tests", stamp: false, heavy: true, cmd: ["node", "scripts/test-rust.mjs", ...args] })),
  ];
  const kept = normal.filter((step) => !BUILDS_AND_SUITES.has(step.label) || tests.includes(step));
  const light = kept.filter((step) => !step.heavy);
  const heavy = [...kept.filter((step) => step.heavy && !tests.includes(step)), ...tests];
  return { steps: [...light, ...heavy], notes };
}
