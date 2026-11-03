import { describe, expect, it } from "vitest";

import type { BrowseEntry } from "../protocol";
import { buildTree, compareFilePaths, fileEvidence, hasOpenFolder, listingSaysDirectory, parseRowKey, rowKey, treeEntries, type DirListing } from "./tree";

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

  it("leaves out the files an operating system leaves in a folder, in any ASCII case", () => {
    const names = treeEntries([file(".DS_Store"), file("._notes.md"), file("Thumbs.db"), file("Desktop.ini"), file(".ds_store"), dir(".Spotlight-V100"), file("des\u212Atop.ini"), file("notes.md"), file(".env")]).map(
      (e) => e.name,
    );
    expect(names).toEqual([".env", "des\u212Atop.ini", "notes.md"]);
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

  it("says a path is a directory only when a loaded listing does", () => {
    expect(listingSaysDirectory(listings, "docs")).toBe(true);
    expect(listingSaysDirectory(listings, "docs/guide.md")).toBe(false);
    expect(listingSaysDirectory(listings, "src/lib")).toBe(true);
    expect(listingSaysDirectory(listings, "src/lib/nested")).toBe(false);
    expect(listingSaysDirectory(listings, "missing")).toBe(false);
  });

  it("shows only expanded directories' rows, with a status row for loading and empty ones", () => {
    const tree = buildTree(listings, new Set(["src", "src/lib", "empty"]), undefined);
    expect(tree.rows).toEqual([
      "path:docs",
      "path:empty",
      "status:empty:empty",
      "path:src",
      "path:src/lib",
      "status:loading:src/lib",
      "path:src/main.ts",
      "path:README.md",
    ]);
    expect(tree.files).toEqual(["src/main.ts", "README.md"]);
    expect(tree.wanted).toEqual(["", "empty", "src", "src/lib"]);
  });

  it("marks an expanded folder whose listing was cut short on the folder itself, with no row beneath it", () => {
    const tree = buildTree(listings, new Set(["src", "src/lib"]), undefined);
    const src = tree.nodes.find((n) => n.type === "directory" && n.path === "src");
    expect(src).toMatchObject({ partial: true });
    expect(src?.type === "directory" && src.children.some((n) => n.type === "status" && n.status !== "loading")).toBe(false);
    // Whole, folded or failed listings carry no mark.
    expect(buildTree(listings, new Set(["docs"]), undefined).nodes.find((n) => n.type === "directory" && n.path === "docs")).not.toHaveProperty("partial");
    expect(buildTree(listings, new Set(), undefined).nodes.find((n) => n.type === "directory" && n.path === "src")).not.toHaveProperty("partial");
    const failed = new Map<string, DirListing>([["", loaded([dir("src")])], ["src", { state: "error", message: "denied" }]]);
    expect(buildTree(failed, new Set(["src"]), undefined).nodes[0]).not.toHaveProperty("partial");
  });

  it("keeps the empty row of an incomplete empty folder, marking the folder as cut short", () => {
    const partial = new Map<string, DirListing>([["", loaded([dir("src")])], ["src", loaded([], false)]]);
    const tree = buildTree(partial, new Set(["src"]), undefined);
    expect(tree.nodes[0]).toMatchObject({ partial: true, children: [{ status: "empty" }] });
    expect(new Set(tree.rows).size).toBe(tree.rows.length);
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

describe("compareFilePaths", () => {
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

describe("hasOpenFolder", () => {
  const listings = new Map<string, DirListing>([
    ["", loaded([dir("src"), dir("locked")])],
    ["src", loaded([dir("lib")])],
    ["src/lib", loaded([file("a.ts")])],
    ["locked", { state: "error", message: "denied" }],
  ]);
  const open = (...paths: string[]) => hasOpenFolder(buildTree(listings, new Set(paths), undefined).nodes, new Set(paths.map((path) => `path:${path}`)));

  it("is true while a top-level folder is drawn open, and false with none expanded", () => {
    expect(open("src")).toBe(true);
    expect(open()).toBe(false);
  });

  it("does not count a folder expanded beneath a folded one, nor one whose listing failed", () => {
    expect(open("src/lib")).toBe(false);
    expect(open("locked")).toBe(false);
    expect(open("locked", "src")).toBe(true);
  });
});

describe("parseRowKey", () => {
  it("reads the path of an entry's row and the folder of a status row back", () => {
    expect(parseRowKey(rowKey("src/a.ts"))).toEqual({ kind: "path", path: "src/a.ts" });
    expect(parseRowKey(rowKey(""))).toEqual({ kind: "path", path: "" });
    expect(parseRowKey("status:loading:src/lib")).toEqual({ kind: "status", status: "loading", dir: "src/lib" });
    expect(parseRowKey("status:empty:a:b")).toEqual({ kind: "status", status: "empty", dir: "a:b" });
  });

  it("finds nothing in a key that is not a row of the tree", () => {
    expect(parseRowKey("collapsed:src")).toBeUndefined();
    expect(parseRowKey("heading:staged")).toBeUndefined();
  });
});
