import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import {
  ALL_PACKAGES, DAEMON_READS_UI, DESKTOP_CONTRACT, MENU_KEYS_FILE, matchingRules, planScope, RULES, SWEEP_SHARED, UI_READS_ELSEWHERE,
  UI_READS_NATIVE, VERIFY_READS_UI, WEB_IMPORTS_FROM_UI, WEB_PREVIEW_INPUTS, WIRE_PROTOCOL,
} from "./scope.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

const mode = (files, options) => planScope(files, options).mode;
const JS = ["verify", "ui", "web", "desktop"];
const RUST = ["daemon", "native"];

// One case per rule (a rule with several matchers has one per matcher worth distinguishing): the file
// and the packages and builds it selects.
const CASES = [
  ["scripts/verify/scope.mjs", ["verify"]],
  ["scripts/pre-push.mjs", ["verify"]],
  ["packages/ui/src/App.tsx", ["ui"]],
  ["apps/web/app/app.vue", ["web"]],
  ["apps/web/package.json", ["web"]],
  ["apps/web/nuxt.config.ts", ["web"]],
  ["apps/daemon/src/main.rs", ["daemon"]],
  ["apps/desktop/src-tauri/src/lib.rs", ["native"]],
  ["apps/desktop/scripts/bundle.mjs", ["desktop"]],
  ["apps/desktop/package.json", ["desktop", "native"]],
  ["Cargo.toml", RUST],
  ["Cargo.lock", RUST, ["ui"]],
  ["apps/daemon/Cargo.toml", RUST],
  ["apps/desktop/src-tauri/Cargo.toml", RUST],
  ["rust-toolchain.toml", RUST],
  ["rustfmt.toml", RUST],
  [".cargo/config.toml", RUST],
  ["pnpm-lock.yaml", JS, ["ui"]],
  ["pnpm-workspace.yaml", JS],
  ["patches/a.patch", JS],
  ["eslint.config.mjs", JS],
  ["package.json", ALL_PACKAGES],
  ["scripts/env.mjs", ["verify", "ui", "desktop"], ["ui"]],
  ["scripts/env.d.mts", ["verify", "ui", "desktop"]],
  ["scripts/test-rust.mjs", ["verify", "daemon", "native"]],
  ["scripts/clippy-rust.mjs", ["verify", "daemon", "native"]],
  [".githooks/hook.sh", ["verify"]],
  [".gitignore", ["verify"]],
  ["config/app.json", ALL_PACKAGES, ["ui"]],
  ["docs/plans/x/README.md", []],
  ["docs/images/a.png", []],
  [".claude/skills/a/SKILL.md", []],
  [".agents/a", []],
  [".codex/a", []],
  ["assets/branding/logo.svg", []],
  [".github/workflows/quality.yml", []],
  ["CLAUDE.md", []],
  ["AGENTS.md", []],
  ["packages/ui/src/i18n/matchLanguage.ts", ["verify", "ui", "web"]],
  ["packages/ui/src/website-preview/main.tsx", ["verify", "ui", "web"]],
  ["apps/daemon/PROTOCOL.md", ["verify", "ui", "daemon", "native"]],
  ["packages/ui/src/platform/tauri.ts", ["verify", "ui", "native"]],
  ["scripts/verify/lock.mjs", ["verify", "ui"]],
  ["packages/ui/src/i18n/languages.ts", ["verify", "ui", "web", "daemon"]],
  ["apps/desktop/src-tauri/Info.plist", ["verify", "ui", "native"]],
  ["LICENSE", ["verify", "ui"]],
  ["apps/web/site.config.ts", ["verify", "ui", "web"]],
  ["packages/ui/src/test/failedFilesReporter.ts", ["verify", "ui"]],
  ["apps/daemon/src/adapter/claude_compatibility.md", ["daemon"]],
  ["packages/ui/vite.config.ts", ["ui"], ["ui"]],
  ["packages/ui/index.html", ["ui"], ["ui"]],
  ["packages/ui/scripts/generate-licenses.mjs", ["ui"], ["ui"]],
  ["packages/ui/tsconfig.app.json", ["ui"], ["ui"]],
  ["packages/ui/package.json", ["ui"], ["ui"]],
  // Unknown inputs select everything; Markdown no rule claims is read by nothing.
  ["something/new.txt", ALL_PACKAGES],
  ["apps/web/app/README.md", []],
  ["apps/daemon/README.md", []],
  ["packages/ui/src/nested/NOTES.md", []],
  ["scripts/NOTES.md", []],
  // Not UI build inputs.
  ["packages/ui/scripts/gallery-sweep.mjs", ["ui"], []],
  ["packages/ui/scripts/wkbench/run.mjs", ["ui"], []],
  ["packages/ui/gallery.html", ["ui"], []],
];

test("every case selects the packages and builds its rule names", () => {
  for (const [file, packages, builds = []] of CASES) {
    const scope = planScope([file]);
    assert.deepEqual(scope.packages, packages, file);
    assert.deepEqual(scope.builds, builds, file);
    assert.equal(scope.mode, packages.length === 0 ? "unread" : "package", file);
  }
});

test("every rule has a case, so a new rule cannot go untested", () => {
  const exercised = new Set(CASES.flatMap(([file]) => matchingRules(file)));
  for (const rule of RULES) {
    assert.ok(exercised.has(rule), `no case for the rule: ${rule.why}`);
    assert.ok(rule.why, "a rule states its premise");
  }
});

test("a rule overrides the Markdown fallback, and a Markdown file no rule names selects nothing", () => {
  assert.equal(mode(["docs/plans/x/README.md", "CLAUDE.md", ".claude/skills/a/SKILL.md", "packages/ui/README.md"]), "unread");
  assert.equal(mode([]), "unread");
  assert.deepEqual(planScope(["apps/daemon/PROTOCOL.md"]).packages, ["verify", "ui", "daemon", "native"]);
  assert.deepEqual(planScope(["packages/ui/src/protocol.generated.ts"]).packages, ["verify", "ui", "daemon", "native"]);
});

test("several changed packages select a union, in package order, without unrelated consumers", () => {
  assert.deepEqual(planScope(["apps/daemon/src/main.rs", "packages/ui/src/App.tsx", "docs/a.md"]).packages, ["ui", "daemon"]);
  assert.deepEqual(planScope(["apps/web/app/app.vue", "apps/desktop/scripts/bundle.mjs"]).packages, ["web", "desktop"]);
});

test("the menu keys select the shell only when the caller reports a change to them", () => {
  assert.deepEqual(planScope([MENU_KEYS_FILE]).packages, ["ui"]);
  assert.deepEqual(planScope([MENU_KEYS_FILE], { fullFiles: [MENU_KEYS_FILE] }).packages, ["verify", "ui", "native"]);
});

test("every file named in a relation list selects verify, so these tests run when it changes", () => {
  const lists = [WEB_IMPORTS_FROM_UI, WEB_PREVIEW_INPUTS, DESKTOP_CONTRACT, SWEEP_SHARED, WIRE_PROTOCOL, DAEMON_READS_UI, UI_READS_NATIVE, UI_READS_ELSEWHERE, VERIFY_READS_UI];
  for (const file of lists.flat()) assert.ok(planScope([file]).packages.includes("verify"), file);
});

test("every path the relations name still exists", () => {
  for (const file of [MENU_KEYS_FILE, ...[WEB_IMPORTS_FROM_UI, WEB_PREVIEW_INPUTS, DESKTOP_CONTRACT, SWEEP_SHARED, WIRE_PROTOCOL, DAEMON_READS_UI, UI_READS_NATIVE, UI_READS_ELSEWHERE, VERIFY_READS_UI].flat()]) {
    assert.ok(existsSync(path.join(root, file)), file);
  }
});

// The relation lists are kept by hand: this finds reads that leave a package root, so a new one fails
// here instead of going unnoticed. Rust `include_str!`/`include_bytes!` and `new URL("../../..")` reads
// in sources and tests are resolved against the reading file and must land in the reader's own package
// or in a path the table names.
test("cross-package file reads are all named by a rule", () => {
  const tracked = execFileSync("git", ["ls-files", "-z", "--", "apps", "packages"], { cwd: root, encoding: "utf8" }).split("\0").filter(Boolean);
  const packageOf = (file) => file.split("/").slice(0, file.startsWith("apps/desktop/") ? 3 : 2).join("/");
  const reads = [];
  for (const file of tracked) {
    if (!/\.(rs|ts|tsx|mjs)$/.test(file) || !existsSync(path.join(root, file))) continue;
    const text = readFileSync(path.join(root, file), "utf8");
    const patterns = file.endsWith(".rs")
      ? [/include_(?:str|bytes)!\(\s*"([^"]+)"/g]
      : [/new URL\(\s*[`"']([^`"']*\.\.[^`"']*)[`"']/g];
    for (const pattern of patterns) {
      for (const [, target] of text.matchAll(pattern)) {
        const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(file), target.replace(/^\$\{[^}]*\}/, "")));
        // Only files are reads: a directory is an output (the preview build writes into the website).
        if (!resolved.startsWith("..") && packageOf(resolved) !== packageOf(file) && existsSync(path.join(root, resolved)) && statSync(path.join(root, resolved)).isFile()) reads.push([file, resolved]);
      }
    }
  }
  for (const [file, target] of reads) {
    // The reader's own package must already be selected by the file being read, so changing it re-runs the reader.
    const reader = planScope([target]).packages;
    const own = planScope([file]).packages;
    assert.ok(own.every((name) => reader.includes(name)) || own.length === 0, `${file} reads ${target}, which does not select ${own.join(", ")}`);
  }
  assert.ok(reads.length > 0, "the scan found no cross-package read; its patterns are stale");
});

// `WEB_IMPORTS_FROM_UI` is kept by hand: this finds what `apps/web` imports from `packages/ui/src`,
// and what that imports in turn, so a new import fails here instead of going unnoticed.
test("everything apps/web imports from the UI package is counted as a cross-package relation", () => {
  const seen = new Set();
  const visit = (file) => {
    for (const match of readFileSync(file, "utf8").matchAll(/(?:from|import)\s*\(?\s*["'](\.[^"']*)["']/g)) {
      const base = path.resolve(path.dirname(file), match[1]);
      const resolved = [".ts", ".tsx", ""].map((ext) => base + ext).find((candidate) => existsSync(candidate) && statSync(candidate).isFile());
      if (resolved && resolved.includes(`${path.sep}packages${path.sep}ui${path.sep}src${path.sep}`) && !seen.has(resolved)) {
        seen.add(resolved);
        visit(resolved);
      }
    }
  };
  const webFiles = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if ([".nuxt", ".output", "node_modules", "public"].includes(entry.name)) continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (/\.(ts|vue|mjs)$/.test(entry.name)) webFiles.push(full);
    }
  };
  walk(path.join(root, "apps/web"));
  webFiles.forEach(visit);
  for (const file of seen) assert.ok(planScope([path.relative(root, file)]).packages.includes("web"), file);
  assert.ok(seen.size > 0);
});
