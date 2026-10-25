import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, expect, test } from "vitest";

import { buildDocument, collectNpmPackages, cratesFromMetadata, isTypesPackage, npmLicense, readLicenseTexts, withoutText } from "./generate-licenses.mjs";

const root = mkdtempSync(path.join(os.tmpdir(), "octoboard-licenses-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));

const write = (file, content) => {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, content);
};
const manifest = (directory, fields) => write(path.join(directory, "package.json"), JSON.stringify(fields));

// A pnpm layout: the app links `a`; the store keeps `a` with its own dependency `b` beside it, and `c` is a
// dev dependency of the app that nothing ships.
const app = path.join(root, "app");
const store = path.join(root, "store/.pnpm");
manifest(app, { name: "app", private: true, dependencies: { a: "1" }, devDependencies: { c: "1" } });
manifest(path.join(store, "a@1.0.0/node_modules/a"), { name: "a", version: "1.0.0", license: "MIT", dependencies: { b: "1" }, optionalDependencies: { missing: "1" } });
write(path.join(store, "a@1.0.0/node_modules/a/LICENSE"), "MIT text\r\n\r\n");
manifest(path.join(store, "a@1.0.0/node_modules/b"), { name: "b", version: "1.2.0", licenses: [{ type: "MIT" }, { type: "Apache-2.0" }] });
write(path.join(store, "a@1.0.0/node_modules/b/LICENSE-MIT.md"), "B text");
write(path.join(store, "a@1.0.0/node_modules/b/index.js"), "not a license");
manifest(path.join(store, "c@1.0.0/node_modules/c"), { name: "c", version: "1.0.0", license: "ISC" });
mkdirSync(path.join(app, "node_modules"), { recursive: true });
symlinkSync(path.join(store, "a@1.0.0/node_modules/a"), path.join(app, "node_modules/a"));
symlinkSync(path.join(store, "c@1.0.0/node_modules/c"), path.join(app, "node_modules/c"));

test("the production closure follows pnpm's links, leaves out dev dependencies and optional ones that are not installed", () => {
  const packages = collectNpmPackages(app);
  expect(packages.map((entry) => `${entry.name}@${entry.version}`).sort()).toEqual(["a@1.0.0", "b@1.2.0"]);
  expect(packages.find((entry) => entry.name === "a")).toMatchObject({ ecosystem: "npm", license: "MIT", texts: ["MIT text"] });
  expect(packages.find((entry) => entry.name === "b")).toMatchObject({ license: "MIT OR Apache-2.0", texts: ["B text"] });
});

test("type declarations are not listed, nor is what only they depend on", () => {
  const typed = path.join(root, "typed");
  const typedStore = path.join(root, "typed-store/.pnpm");
  manifest(typed, { name: "typed", private: true, dependencies: { d: "1" } });
  manifest(path.join(typedStore, "d@1.0.0/node_modules/d"), { name: "d", version: "1.0.0", license: "MIT", dependencies: { "@types/d": "1" } });
  manifest(path.join(typedStore, "d@1.0.0/node_modules/@types/d"), { name: "@types/d", version: "1.0.0", license: "MIT", dependencies: { "types-only": "1" } });
  manifest(path.join(typedStore, "d@1.0.0/node_modules/types-only"), { name: "types-only", version: "1.0.0", license: "MIT" });
  mkdirSync(path.join(typed, "node_modules"), { recursive: true });
  symlinkSync(path.join(typedStore, "d@1.0.0/node_modules/d"), path.join(typed, "node_modules/d"));
  expect(collectNpmPackages(typed).map((entry) => entry.name)).toEqual(["d"]);
  expect(isTypesPackage("@types/node")).toBe(true);
  expect(isTypesPackage("@typescript/native")).toBe(false);
});

test("a required dependency that is not installed fails rather than dropping out of the list", () => {
  const broken = path.join(root, "broken");
  manifest(broken, { name: "broken", private: true, dependencies: { nowhere: "1" } });
  expect(() => collectNpmPackages(broken)).toThrow(/nowhere/);
});

test("the license is read from either manifest form, and a named file is read too", () => {
  expect(npmLicense({ license: { type: "BSD-3-Clause" } })).toBe("BSD-3-Clause");
  expect(npmLicense({})).toBeUndefined();
  const directory = path.join(root, "files");
  write(path.join(directory, "COPYING"), "copying");
  write(path.join(directory, "TERMS.txt"), "terms");
  write(path.join(directory, "README.md"), "readme");
  expect(readLicenseTexts(directory)).toEqual(["copying"]);
  expect(readLicenseTexts(directory, "TERMS.txt")).toEqual(["copying", "terms"]);
});

test("Cargo crates are what the members' normal dependencies reach, build and dev dependencies, procedural macros, what only they need, and the members left out", () => {
  const dep = (pkg, kind) => ({ pkg, dep_kinds: [{ kind, target: null }] });
  const metadata = {
    workspace_members: ["app", "daemon"],
    packages: [
      ...["app", "daemon", "normal", "deep", "build", "dev", "shared", "macro-only"].map((id) => ({ id, name: id })),
      { id: "macro", name: "macro", targets: [{ kind: ["proc-macro"] }, { kind: ["example"] }] },
    ],
    resolve: {
      nodes: [
        { id: "app", deps: [dep("normal", null), dep("build", "build"), dep("daemon", null), dep("macro", null)] },
        { id: "daemon", deps: [dep("dev", "dev")] },
        { id: "normal", deps: [dep("deep", null), dep("shared", null)] },
        { id: "macro", deps: [dep("macro-only", null), dep("shared", null)] },
        { id: "macro-only", deps: [] },
        { id: "shared", deps: [] },
        { id: "deep", deps: [] },
        { id: "build", deps: [] },
        { id: "dev", deps: [] },
      ],
    },
  };
  expect(cratesFromMetadata(metadata).map((crate) => crate.id).sort()).toEqual(["deep", "normal", "shared"]);
});

test("a text several packages share is held once, and packages come out in name order", () => {
  const document = buildDocument([
    { name: "z", version: "1.0.0", ecosystem: "npm", license: "MIT", texts: ["shared"] },
    { name: "a", version: "10.0.0", ecosystem: "cargo", license: "MIT", texts: ["shared", "own"] },
    { name: "a", version: "9.0.0", ecosystem: "cargo", license: "MIT", texts: [] },
  ]);
  expect(document.texts).toEqual(["shared", "own"]);
  expect(document.packages.map((entry) => `${entry.name}@${entry.version}`)).toEqual(["a@9.0.0", "a@10.0.0", "z@1.0.0"]);
  expect(document.packages[1].texts).toEqual([0, 1]);
  expect(withoutText(document).map((entry) => entry.version)).toEqual(["9.0.0"]);
});
