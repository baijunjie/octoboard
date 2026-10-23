// The checks for a scope, in the order they run: format, lint and type checks first, tests after, so
// the cheap failures come first. The full set is `pnpm format:check` (ESLint and rustfmt), `pnpm
// typecheck`, the web locale check, `pnpm -r test` and `pnpm test:rust`; a package's checks are its
// type check, tests and ESLint on its changed files (rustfmt and `cargo test -p` for the daemon).
// `heavy` marks the steps that queue for a machine-wide slot (UI tests, Rust builds and tests).
import { existsSync } from "node:fs";
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
  if (scope.mode === "unread") return [];
  const [pkg] = scope.packages;
  const rewrite = eslintFiles(files).filter((file) => belongsTo(pkg, file));
  const steps = rewrite.length === 0 ? [] : [{
    label: "format (eslint layout)",
    cmd: ["pnpm", "exec", "eslint", "--fix", "--fix-type", "layout", "--no-warn-ignored", ...rewrite],
  }];
  if (pkg === "daemon") steps.push({ label: "format (rustfmt)", cmd: ["cargo", "fmt", "-p", "octoboardd"] });
  return steps;
}

export function checkSteps(scope, files) {
  if (scope.mode === "full") {
    return [
      { label: "format check and lint", cmd: ["pnpm", "format:check"] },
      { label: "type check", cmd: ["pnpm", "typecheck"] },
      { label: "web locales", cmd: ["pnpm", "--filter", "@octoboard/web", "check:locales"] },
      VERIFY_TESTS,
      { label: "package tests", cmd: ["pnpm", "-r", "test"], heavy: true },
      { label: "rust tests", cmd: ["pnpm", "test:rust"], heavy: true },
    ];
  }
  if (scope.mode === "unread") return [];
  const [pkg] = scope.packages;
  const own = files.filter((file) => belongsTo(pkg, file));
  if (pkg === "ui") {
    return [
      ...eslint("lint", own),
      { label: "ui type check", cmd: ["pnpm", "--filter", "@octoboard/ui", "typecheck"] },
      { label: "ui tests", cmd: ["pnpm", "--filter", "@octoboard/ui", "test"], heavy: true },
    ];
  }
  if (pkg === "web") {
    return [
      ...eslint("lint", own),
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
    ...eslint("lint", own),
    VERIFY_TESTS,
  ];
}
