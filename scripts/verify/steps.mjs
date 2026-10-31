// Read-only checks for affected consumers; tests, builds and Rust lint acquire the machine-wide heavy slot.
import path from "node:path";
import { ALL_PACKAGES } from "./scope.mjs";

const VERIFY_TESTS = { label: "verify tests", cmd: ["node", "--test", "scripts/verify/*.test.mjs"] };
const pnpm = (label, args, extra = {}) => ({ label, cmd: ["pnpm", ...args], ...extra });
// The cache lives under `node_modules` like the root `pnpm lint`'s: a cache file in the repository
// root would be an untracked file, which `workingTree()` counts, so it would fail the run that wrote it.
// Each step keeps its own file, so steps linting different paths do not evict each other's entries.
const lint = (label, targets, ignore = []) => pnpm(label, ["exec", "eslint", "--cache", "--cache-strategy", "content", "--cache-location", `node_modules/.cache/eslint/verify-${label.replaceAll(" ", "-")}`, "--max-warnings", "0", "--no-warn-ignored", ...ignore.flatMap((pattern) => ["--ignore-pattern", pattern]), ...targets]);
const uiTestFiles = (files) => files.length === 0 ? [] : [
  ["pnpm", "--filter", "@octoboard/ui", "test", ...files.map((file) => path.relative("packages/ui", file))],
];

// Light steps come first, heavy ones last, so a cheap failure does not wait for a long suite.
const lightFirst = (steps) => [...steps.filter((step) => !step.heavy), ...steps.filter((step) => step.heavy)];
// Both crates are checked together whatever changed: the test and clippy scripts cover the workspace.
const RUST_STEPS = [
  { label: "rustfmt check", cmd: ["cargo", "fmt", "--all", "--", "--check"] },
  pnpm("rust clippy", ["clippy:rust"], { heavy: true }),
  pnpm("rust tests", ["test:rust"], { heavy: true }),
];

export function checkSteps(scope) {
  if (scope.mode === "release") {
    return lightFirst([
      lint("application lint", ["packages/ui", "apps/desktop/scripts", "scripts/env.mjs"], ["**/src/gallery/**", "**/src/website-preview/**", "**/scripts/gallery*", "**/scripts/wkbench/**"]),
      pnpm("application type check", ["--filter", "@octoboard/ui", "typecheck:app"]),
      pnpm("desktop script tests", ["--filter", "@octoboard/desktop", "test"]),
      RUST_STEPS[0],
      pnpm("application UI tests", ["--filter", "@octoboard/ui", "test:app"], { heavy: true }),
      ...RUST_STEPS.slice(1),
    ]);
  }
  const packages = scope.mode === "full" ? ALL_PACKAGES : scope.packages;
  const steps = [];
  if (packages.includes("verify")) steps.push(lint("verify lint", ["scripts", "eslint.config.mjs"]), VERIFY_TESTS);
  if (packages.includes("ui")) steps.push(
    lint("ui lint", ["packages/ui"]),
    pnpm("ui type check", ["--filter", "@octoboard/ui", "typecheck"]),
    pnpm("ui tests", ["--filter", "@octoboard/ui", "test"], { heavy: true, rerun: uiTestFiles }),
  );
  if (packages.includes("web")) steps.push(
    lint("web lint", ["apps/web"]),
    pnpm("web type check", ["--filter", "@octoboard/web", "typecheck"]),
    pnpm("web locales", ["--filter", "@octoboard/web", "check:locales"]),
    // The SEO test reads generated output, so building the website is part of its verification.
    pnpm("web build", ["--filter", "@octoboard/web", "build"], { heavy: true }),
    pnpm("web tests", ["--filter", "@octoboard/web", "test"], { heavy: true }),
  );
  if (packages.includes("desktop")) steps.push(
    lint("desktop lint", ["apps/desktop/scripts"]),
    pnpm("desktop script tests", ["--filter", "@octoboard/desktop", "test"]),
  );
  if (packages.includes("daemon") || packages.includes("native")) steps.push(...RUST_STEPS);
  if (scope.builds?.includes("ui")) steps.push(pnpm("ui build", ["--filter", "@octoboard/ui", "build"], { heavy: true }));
  return lightFirst(steps);
}
