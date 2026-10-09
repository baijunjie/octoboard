import { describe, expect, it } from "vitest";

import type { BrowseEntry } from "../protocol";
import { buildTree, compareFilePaths, fileEvidence, neighbours, rereadFor, treeEntries, type DirListing, type FileEvidence } from "./tree";

const file = (name: string, version = "v1"): BrowseEntry => ({ name, kind: "file", size: 1, version, target: null });
const dir = (name: string): BrowseEntry => ({ name, kind: "directory", size: null, version: null, target: null });
const link = (name: string, target: BrowseEntry["target"]): BrowseEntry => ({ name, kind: "symlink", size: null, version: null, target });

const loaded = (entries: BrowseEntry[], complete = true): DirListing => ({
  state: "loaded",
  entries: treeEntries(entries),
  complete,
  refreshing: false,
});

describe("treeEntries", () => {
  it("lists directories (and links to them) first, then by name in natural order, without .git", () => {
    const names = treeEntries([file("b.txt"), file("item 10"), dir(".git"), file("Item 2"), dir("src"), link("lib", "directory"), file("a.md")]).map(
      (e) => e.name,
    );
    expect(names).toEqual(["lib", "src", "a.md", "b.txt", "Item 2", "item 10"]);
  });
});

describe("buildTree", () => {
  const listings = new Map<string, DirListing>([
    ["", loaded([dir("src"), dir("docs"), dir("empty"), file("README.md")])],
    ["src", loaded([file("main.ts"), dir("lib")], false)],
    ["src/lib", { state: "loading" }],
    ["docs", loaded([file("guide.md")])],
    ["empty", loaded([])],
  ]);

  it("shows only expanded directories' rows, with a status row for loading, empty and cut listings", () => {
    const tree = buildTree(listings, new Set(["src", "src/lib", "empty"]), undefined);
    expect(tree.rows).toEqual([
      "path:docs",
      "path:empty",
      "status:empty",
      "path:src",
      "path:src/lib",
      "status:src/lib",
      "path:src/main.ts",
      "status:src",
      "path:README.md",
    ]);
    expect(tree.files).toEqual(["src/main.ts", "README.md"]);
    expect(tree.wanted).toEqual(["", "empty", "src", "src/lib"]);
  });

  it("keeps a selection its directory no longer lists, in its place, marked by why", () => {
    const tree = buildTree(listings, new Set(["docs", "src"]), "docs/api.md");
    expect(tree.files).toEqual(["docs/api.md", "docs/guide.md", "src/main.ts", "README.md"]);
    const docs = tree.nodes.find((n) => n.type === "directory" && n.path === "docs");
    expect(docs?.type === "directory" && docs.children[0]).toMatchObject({ path: "docs/api.md", missing: "removed" });
    const cut = buildTree(listings, new Set(["src"]), "src/zz.ts").nodes.find((n) => n.type === "directory" && n.path === "src");
    expect(cut?.type === "directory" && cut.children.find((n) => n.type === "file" && n.path === "src/zz.ts")).toMatchObject({ missing: "unlisted" });
  });
});

describe("neighbours", () => {
  const files = ["a/x.ts", "a/y.ts", "b.ts"];

  it.each([
    ["the first file", "a/x.ts", { previous: undefined, next: "a/y.ts" }],
    ["the last file", "b.ts", { previous: "a/y.ts", next: undefined }],
    ["a file the tree does not show", "a/xa.ts", { previous: "a/x.ts", next: "a/y.ts" }],
  ])("stops at the ends and places %s among the rest", (_, current, expected) => {
    expect(neighbours(files, current)).toEqual(expected);
  });

  it("orders paths as the tree does: a directory's files before the files beside it", () => {
    expect(["b.ts", "a/x.ts", "a/sub/z.ts"].sort(compareFilePaths)).toEqual(["a/sub/z.ts", "a/x.ts", "b.ts"]);
  });
});

describe("fileEvidence", () => {
  const listings = new Map<string, DirListing>([
    ["", loaded([dir("src"), file("README.md", "r1")])],
    ["src", loaded([file("a.ts", "a1")])],
    ["locked", { state: "error", message: "denied" }],
  ]);

  it.each([
    ["a listed file, with its version", "src/a.ts", { state: "listed", version: "a1" }],
    ["a file its directory no longer lists", "src/b.ts", { state: "gone" }],
    ["a file under a directory that is gone", "lib/a.ts", { state: "gone" }],
  ])("finds %s", (_, path, evidence) => {
    expect(fileEvidence(listings, path)).toEqual(evidence);
  });

  it("finds a file under an unreadable directory unreadable", () => {
    const withLocked = new Map(listings).set("", loaded([dir("locked")]));
    expect(fileEvidence(withLocked, "locked/a.ts")).toEqual({ state: "unreadable" });
  });
});

describe("rereadFor", () => {
  const listed = (version: string): FileEvidence => ({ state: "listed", version });
  it.each([
    ["a body listed at its own version", { state: "file", version: "v1" }, listed("v1"), undefined, false],
    ["a body rewritten at the same size", { state: "file", version: "v1" }, listed("v2"), listed("v1"), true],
    ["a body whose file or directory is gone", { state: "file", version: "v1" }, { state: "gone" }, listed("v1"), true],
    ["a body whose directory cannot be listed", { state: "file", version: "v1" }, { state: "unreadable" }, listed("v1"), true],
    ["a failure whose file is listed again", { state: "error" }, listed("v2"), { state: "gone" }, true],
    ["a failure the listings have nothing new about", { state: "error" }, listed("v2"), listed("v2"), false],
    ["a file that kept changing as it was read, at every listing", { state: "error", changing: true }, listed("v2"), listed("v2"), true],
    ["a read still out", { state: "loading" }, { state: "gone" }, undefined, false],
  ] as const)("reads %s again: %s", (_, shown, evidence, previous, expected) => {
    expect(rereadFor(shown, evidence, previous, true)).toBe(expected);
  });

  it("reads a file that kept changing again only at a new listing of its own directory", () => {
    expect(rereadFor({ state: "error", changing: true }, listed("v2"), listed("v2"), false)).toBe(false);
  });
});
