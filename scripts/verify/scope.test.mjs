import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { classify, DESKTOP_CONTRACT, MENU_KEYS_FILE, planScope, SWEEP_SHARED, WEB_IMPORTS_FROM_UI, WEB_PREVIEW_INPUTS, WIRE_PROTOCOL } from "./scope.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

const mode = (files, options) => planScope(files, options).mode;

test("documentation and agent files are read by no check", () => {
  assert.equal(mode(["docs/plans/x/README.md", "CLAUDE.md", ".claude/skills/a/SKILL.md", "packages/ui/README.md"]), "unread");
  assert.equal(mode([]), "unread");
});

test("a change inside one package runs that package's checks", () => {
  assert.deepEqual(planScope(["packages/ui/src/App.tsx", "docs/a.md"]).packages, ["ui"]);
  assert.deepEqual(planScope(["apps/web/app/app.vue"]).packages, ["web"]);
  assert.deepEqual(planScope(["apps/daemon/src/main.rs"]).packages, ["daemon"]);
  assert.deepEqual(planScope(["scripts/verify/scope.mjs"]).packages, ["verify"]);
});

test("several packages, shared layers and unrecognised paths run the full set", () => {
  assert.equal(mode(["packages/ui/src/App.tsx", "apps/daemon/src/main.rs"]), "full");
  for (const file of ["Cargo.lock", "pnpm-lock.yaml", "rust-toolchain.toml", "config/app.json", "eslint.config.mjs", "apps/desktop/src-tauri/src/lib.rs", "scripts/test-rust.mjs", ".github/workflows/quality.yml", "something/new.txt"]) {
    assert.equal(mode([file]), "full", file);
  }
});

test("the cross-package relations count as spanning packages", () => {
  for (const file of [
    "packages/ui/src/i18n/matchLanguage.ts", "packages/ui/src/i18n/languages.ts",
    "packages/ui/vite.preview.config.ts", "packages/ui/website-preview.html", "packages/ui/src/website-preview/a.tsx",
    "apps/daemon/PROTOCOL.md", "apps/daemon/src/protocol.rs", "packages/ui/src/protocol.ts",
    "packages/ui/src/platform/tauri.ts", "packages/ui/src/daemon.ts", "packages/ui/src/lifecycle/useNativeMenuLabels.ts",
    "packages/ui/src/lifecycle/statusItemIcons.ts",
  ]) {
    assert.equal(classify(file), "full", file);
  }
  assert.equal(classify("apps/daemon/README.md"), "unread");
});

test("the module the gallery sweep shares runs the verify and UI checks, not the full set", () => {
  for (const file of SWEEP_SHARED) assert.deepEqual(planScope([file, "scripts/verify/scope.mjs"]).packages, ["verify", "ui"], file);
  assert.equal(mode(["scripts/verify/lock.mjs", "packages/ui/src/App.tsx"]), "full");
});

test("the message catalog counts only when a change touches the menu keys", () => {
  assert.equal(mode([MENU_KEYS_FILE]), "package");
  assert.equal(mode([MENU_KEYS_FILE], { fullFiles: [MENU_KEYS_FILE] }), "full");
});

test("manifests and build, type check and test configuration run the full set", () => {
  for (const file of ["package.json", "packages/ui/package.json", "apps/daemon/Cargo.toml", "apps/daemon/build.rs", "packages/ui/tsconfig.json", "packages/ui/vite.config.ts", "apps/web/nuxt.config.ts"]) {
    assert.equal(mode([file]), "full", file);
  }
});

test("every path the relations name still exists", () => {
  for (const file of [...WIRE_PROTOCOL, ...DESKTOP_CONTRACT, ...SWEEP_SHARED, ...WEB_IMPORTS_FROM_UI, ...WEB_PREVIEW_INPUTS, MENU_KEYS_FILE]) {
    assert.ok(existsSync(path.join(root, file)), file);
  }
});

test("a build configuration change advises a build", () => {
  assert.equal(planScope(["packages/ui/vite.config.ts"]).advice.length, 1);
  assert.equal(planScope(["packages/ui/src/App.tsx"]).advice.length, 0);
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
  for (const file of seen) assert.equal(classify(path.relative(root, file)), "full", file);
  assert.ok(seen.size > 0);
});
