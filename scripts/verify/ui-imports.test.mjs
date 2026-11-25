import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { importsFromUi } from "./ui-imports.mjs";

// A tree of `files` (repository-relative path to content) that `run` is given the root of.
function tree(files, run) {
  const root = mkdtempSync(path.join(os.tmpdir(), "ui-imports-"));
  try {
    const write = (file, content) => {
      mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
      writeFileSync(path.join(root, file), content);
    };
    for (const [file, content] of Object.entries(files)) write(file, content);
    run(root, write);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

const MANIFEST = JSON.stringify({ name: "@octoboard/ui", exports: { "./agent-marks": "./src/providers/agentMarks.ts", "./languages": "./src/i18n/languages.ts" } });

// A website that shows the agents' marks, as the real one does: one import through the `exports` map, and the marks
// importing each provider's presentation module, which imports its types.
const WEBSITE = {
  "packages/ui/package.json": MANIFEST,
  "packages/ui/src/providers/agentMarks.ts": 'import { first } from "./first";\nexport const AGENT_MARKS = { first: first.mark };\n',
  "packages/ui/src/providers/first.ts": 'import type { ProviderPresentation } from "./types";\nexport const first: ProviderPresentation = { mark: "a" };\n',
  "packages/ui/src/providers/types.ts": "export type ProviderPresentation = { mark: string };\n",
  "packages/ui/src/providers/registry.ts": 'import { first } from "./first";\nexport const BUILT_IN = [first];\n',
  "packages/ui/src/i18n/languages.ts": "export const LANGUAGES = [];\n",
  "packages/ui/src/App.tsx": "export const App = null;\n",
  "apps/web/app/components/Agents.vue": '<script setup lang="ts">\nimport { AGENT_MARKS } from "@octoboard/ui/agent-marks";\n</script>\n',
  "apps/web/app/own.ts": 'import { other } from "./other";\n',
  "apps/web/app/other.ts": "export const other = 1;\n",
};

test("an import through the exports map brings in the module and everything it imports in turn, type-only imports included", () => {
  tree(WEBSITE, (root) => {
    assert.deepEqual(importsFromUi(root, "apps/web"), [
      "packages/ui/src/providers/agentMarks.ts",
      "packages/ui/src/providers/first.ts",
      "packages/ui/src/providers/types.ts",
    ]);
  });
});

test("a provider module added to the marks the website imports is found without an edit anywhere else", () => {
  tree(WEBSITE, (root, write) => {
    assert.ok(!importsFromUi(root, "apps/web").includes("packages/ui/src/providers/second.ts"));
    write("packages/ui/src/providers/second.ts", 'import type { ProviderPresentation } from "./types";\nexport const second: ProviderPresentation = { mark: "b" };\n');
    // In the UI's registry alone the website does not reach it.
    write("packages/ui/src/providers/registry.ts", 'import { first } from "./first";\nimport { second } from "./second";\nexport const BUILT_IN = [first, second];\n');
    assert.ok(!importsFromUi(root, "apps/web").includes("packages/ui/src/providers/second.ts"));
    write("packages/ui/src/providers/agentMarks.ts", 'import { first } from "./first";\nimport { second } from "./second";\nexport const AGENT_MARKS = { first: first.mark, second: second.mark };\n');
    assert.ok(importsFromUi(root, "apps/web").includes("packages/ui/src/providers/second.ts"));
  });
});

test("each importer has its own list, and a module nothing of it imports is in none", () => {
  tree({ ...WEBSITE, "apps/docs/src/locales.ts": 'import { LANGUAGES } from "@octoboard/ui/languages";\n' }, (root) => {
    assert.deepEqual(importsFromUi(root, "apps/docs"), ["packages/ui/src/i18n/languages.ts"]);
    const web = importsFromUi(root, "apps/web");
    for (const file of ["packages/ui/src/i18n/languages.ts", "packages/ui/src/App.tsx", "packages/ui/src/providers/registry.ts", "apps/web/app/other.ts"]) {
      assert.ok(!web.includes(file), file);
    }
  });
});

test("re-exports, dynamic imports, directory indexes and relative imports across the package boundary are followed", () => {
  tree({
    "packages/ui/package.json": JSON.stringify({ exports: { "./entry": "./src/entry.ts" } }),
    "packages/ui/src/entry.ts": 'export * from "./reexported";\nexport const lazy = () => import("./lazy");\nimport "./side-effect";\nimport { part } from "./folder";\n',
    "packages/ui/src/reexported.ts": "export const a = 1;\n",
    "packages/ui/src/lazy.tsx": "export const b = 2;\n",
    "packages/ui/src/side-effect.ts": "",
    "packages/ui/src/folder/index.ts": "export const part = 3;\n",
    "packages/ui/src/direct.ts": "export const c = 4;\n",
    "apps/web/a.ts": 'import { a } from "@octoboard/ui/entry";\n',
    "apps/web/b.mjs": 'import { c } from "../../packages/ui/src/direct.ts";\n',
  }, (root) => {
    assert.deepEqual(importsFromUi(root, "apps/web"), [
      "packages/ui/src/direct.ts",
      "packages/ui/src/entry.ts",
      "packages/ui/src/folder/index.ts",
      "packages/ui/src/lazy.tsx",
      "packages/ui/src/reexported.ts",
      "packages/ui/src/side-effect.ts",
    ]);
  });
});

test("build output and dependencies are not read, a cycle ends, and what the exports map does not name is nothing", () => {
  tree({
    "packages/ui/package.json": JSON.stringify({ exports: { "./a": "./src/a.ts", "./built": "./src/built.ts" } }),
    "packages/ui/src/a.ts": 'import "./b";\nimport "react";\n',
    "packages/ui/src/b.ts": 'import "./a";\n',
    "packages/ui/src/built.ts": "",
    // Its own relative imports are its own files, resolved or not: only what lands in the UI package is followed.
    "apps/web/a.ts": 'import "@octoboard/ui/a";\nimport "@octoboard/ui/not-exported";\nimport "@octoboard/ui";\nimport "@octoboard/other/a";\nimport "./style.css";\nimport "./gone";\n',
    "apps/web/.nuxt/x.ts": 'import "@octoboard/ui/built";\n',
    "apps/web/.output/x.mjs": 'import "@octoboard/ui/built";\n',
    "apps/web/node_modules/dep/x.mjs": 'import "@octoboard/ui/built";\n',
    "apps/web/public/x.mjs": 'import "@octoboard/ui/built";\n',
    "apps/web/notes.md": 'import "@octoboard/ui/built";\n',
  }, (root) => {
    assert.deepEqual(importsFromUi(root, "apps/web"), ["packages/ui/src/a.ts", "packages/ui/src/b.ts"]);
  });
});

test("an importer or a UI package that is not there imports nothing", () => {
  tree({ "apps/web/a.ts": 'import "@octoboard/ui/a";\n' }, (root) => {
    assert.deepEqual(importsFromUi(root, "apps/web"), []);
    assert.deepEqual(importsFromUi(root, "apps/none"), []);
  });
});

test("a bundler query on a relative import names the same file", () => {
  tree({
    "packages/ui/package.json": JSON.stringify({ exports: { ".": "./src/index.ts" } }),
    "packages/ui/src/index.ts": 'import icon from "./icon.svg?inline";\nimport text from "./notes.txt?raw";\n',
    "packages/ui/src/icon.svg": "<svg/>",
    "packages/ui/src/notes.txt": "",
    "apps/web/a.ts": 'import "@octoboard/ui";\n',
  }, (root) => {
    assert.deepEqual(importsFromUi(root, "apps/web"), ["packages/ui/src/icon.svg", "packages/ui/src/index.ts", "packages/ui/src/notes.txt"]);
  });
});

// Each of these would be resolved by the importer's build and missed here, leaving a UI module out of scope with
// nothing to show for it, so each fails naming the specifier and the importer.
test("an exports target that is not a plain path throws, naming the specifier and the importer", () => {
  for (const target of [{ import: "./src/a.ts", types: "./src/a.d.ts" }, "./src/*.ts", null, ["./src/a.ts"]]) {
    tree({
      "packages/ui/package.json": JSON.stringify({ exports: { "./a": target } }),
      "packages/ui/src/a.ts": "",
      "apps/web/app/page.ts": 'import "@octoboard/ui/a";\n',
    }, (root) => {
      assert.throws(() => importsFromUi(root, "apps/web"), (error) => error.message.includes("`@octoboard/ui/a`") && error.message.includes("apps/web/app/page.ts") && error.message.includes("exports"), JSON.stringify(target));
    });
  }
});

test("an import the exports map could only name through a pattern or a condition throws", () => {
  for (const exported of [{ "./*": "./src/*.ts" }, { "./providers/*": "./src/providers/*.ts", "./a": "./src/a.ts" }, { import: "./src/index.ts" }]) {
    tree({
      "packages/ui/package.json": JSON.stringify({ exports: exported }),
      "packages/ui/src/a.ts": "",
      "apps/web/app/page.ts": 'import "@octoboard/ui/providers/first";\n',
    }, (root) => {
      assert.throws(() => importsFromUi(root, "apps/web"), (error) => error.message.includes("`@octoboard/ui/providers/first`") && error.message.includes("apps/web/app/page.ts"), JSON.stringify(exported));
    });
  }
  // A map that is one plain path names the package itself and nothing below it.
  tree({
    "packages/ui/package.json": JSON.stringify({ exports: "./src/index.ts" }),
    "packages/ui/src/index.ts": "",
    "apps/web/a.ts": 'import "@octoboard/ui";\nimport "@octoboard/ui/below";\n',
  }, (root) => {
    assert.deepEqual(importsFromUi(root, "apps/web"), ["packages/ui/src/index.ts"]);
  });
});

test("an exports target or a relative import in the UI package that resolves to no file throws", () => {
  const cases = [
    // The target's file is not there.
    [{ "packages/ui/package.json": JSON.stringify({ exports: { "./a": "./src/gone.ts" } }), "apps/web/page.ts": 'import "@octoboard/ui/a";\n' }, "`@octoboard/ui/a`", "apps/web/page.ts"],
    // A module the website reaches imports a file that is not there, or one with an extension this does not try.
    [{ "packages/ui/package.json": JSON.stringify({ exports: { "./a": "./src/a.ts" } }), "packages/ui/src/a.ts": 'import "./missing";\n', "apps/web/page.ts": 'import "@octoboard/ui/a";\n' }, "`./missing`", "packages/ui/src/a.ts"],
    [{ "packages/ui/package.json": JSON.stringify({ exports: { "./a": "./src/a.ts" } }), "packages/ui/src/a.ts": 'import Part from "./Part";\n', "packages/ui/src/Part.vue": "", "apps/web/page.ts": 'import "@octoboard/ui/a";\n' }, "`./Part`", "packages/ui/src/a.ts"],
    // The website reaches into the UI package by a relative path that names no file.
    [{ "packages/ui/package.json": "{}", "apps/web/page.ts": 'import "../../packages/ui/src/gone";\n' }, "`../../packages/ui/src/gone`", "apps/web/page.ts"],
  ];
  for (const [files, specifier, importer] of cases) {
    tree(files, (root) => {
      assert.throws(() => importsFromUi(root, "apps/web"), (error) => error.message.includes(specifier) && error.message.includes(importer) && error.message.includes("resolves to no file"), specifier);
    });
  }
});
