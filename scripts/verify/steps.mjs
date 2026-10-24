// The checks for a scope, in the order they run: format, lint and type checks first, tests after, so
// the cheap failures come first. The full set is `pnpm format:check` (ESLint and rustfmt), `pnpm
// typecheck`, the web locale check, `pnpm -r test` and `pnpm test:rust`; a package's checks are its
// type check, tests and ESLint on its changed files (rustfmt and `cargo test -p` for the daemon), and a
// scope of several packages runs each one's checks.
// `heavy` marks the steps that queue for a machine-wide slot (UI tests, Rust builds and tests).
import { existsSync } from "node:fs";
import path from "node:path";
import { belongsTo } from "./scope.mjs";

const VERIFY_TESTS = { label: "verify tests", cmd: ["node", "--test", "scripts/verify/*.test.mjs"] };

const eslintFiles = (files) => files.filter((file) => /\.(m?[jt]sx?|cjs|cts|vue)$/.test(file) && existsSync(file));

function eslint(label, files) {
  const targets = eslintFiles(files);
  return targets.length === 0 ? [] : [{
    label,
    cmd: ["pnpm", "exec", "eslint", "--max-warnings", "0", "--no-warn-ignored", ...targets],
  }];
}

// Steps that rewrite files, run first and never stamped: the stamp is taken from the tree they leave.
export function formatSteps(scope, files) {
  if (scope.mode === "full") return [{ label: "format", cmd: ["pnpm", "format"] }];
  return scope.packages.flatMap((pkg) => {
    const rewrite = eslintFiles(files).filter((file) => belongsTo(pkg, file));
    const steps = rewrite.length === 0 ? [] : [{
      label: `format (eslint layout, ${pkg})`,
      cmd: ["pnpm", "exec", "eslint", "--fix", "--fix-type", "layout", "--no-warn-ignored", ...rewrite],
    }];
    if (pkg === "daemon") steps.push({ label: "format (rustfmt)", cmd: ["cargo", "fmt", "-p", "octoboardd"] });
    return steps;
  });
}

// The UI's vitest run on just `files` (paths relative to the repository root), for re-running the test
// files of a step that failed (`scripts/verify/rerun.mjs`).
const uiTestFiles = (files) => files.length === 0 ? [] : [
  ["pnpm", "--filter", "@octoboard/ui", "test", ...files.map((file) => path.relative("packages/ui", file))],
];

// `rerun`, on a step whose failures can be narrowed, gives the commands that re-run it on some of the
// UI's test files, and every other test it holds in full after them (the UI's first, so a failure
// there is recorded for the next run to narrow to).
export function checkSteps(scope, files) {
  if (scope.mode === "full") {
    return [
      { label: "format check and lint", cmd: ["pnpm", "format:check"] },
      { label: "type check", cmd: ["pnpm", "typecheck"] },
      { label: "web locales", cmd: ["pnpm", "--filter", "@octoboard/web", "check:locales"] },
      VERIFY_TESTS,
      {
        label: "package tests",
        cmd: ["pnpm", "-r", "test"],
        heavy: true,
        rerun: (testFiles) => [...uiTestFiles(testFiles), ["pnpm", "-r", "--filter", "!@octoboard/ui", "test"]],
      },
      { label: "rust tests", cmd: ["pnpm", "test:rust"], heavy: true },
    ];
  }
  // Light steps come before heavy ones whichever package they are for.
  const steps = scope.packages.flatMap((pkg) => packageSteps(pkg, files.filter((file) => belongsTo(pkg, file))));
  return [...steps.filter((step) => !step.heavy), ...steps.filter((step) => step.heavy)];
}

function packageSteps(pkg, own) {
  if (pkg === "ui") {
    return [
      ...eslint("ui lint", own),
      { label: "ui type check", cmd: ["pnpm", "--filter", "@octoboard/ui", "typecheck"] },
      { label: "ui tests", cmd: ["pnpm", "--filter", "@octoboard/ui", "test"], heavy: true, rerun: uiTestFiles },
    ];
  }
  if (pkg === "web") {
    return [
      ...eslint("web lint", own),
      { label: "web type check", cmd: ["pnpm", "--filter", "@octoboard/web", "typecheck"] },
      { label: "web locales", cmd: ["pnpm", "--filter", "@octoboard/web", "check:locales"] },
      { label: "web tests", cmd: ["pnpm", "--filter", "@octoboard/web", "test"] },
    ];
  }
  if (pkg === "daemon") {
    return [
      { label: "rustfmt check", cmd: ["cargo", "fmt", "-p", "octoboardd", "--", "--check"] },
      { label: "daemon tests", cmd: ["cargo", "test", "-p", "octoboardd"], heavy: true },
    ];
  }
  // The `verify` package.
  return [
    ...eslint("verify lint", own),
    VERIFY_TESTS,
  ];
}
