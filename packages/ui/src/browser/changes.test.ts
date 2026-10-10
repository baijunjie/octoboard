import { expect, it } from "vitest";

import type { ChangeEntry, ChangeSide, ContentSource } from "../protocol";
import {
  changeEvidence,
  changeGroups,
  changeItem,
  changeItems,
  changeNeighbours,
  changeTree,
  directoryKey,
  filterChanges,
  rereadChange,
  visibleChanges,
  visibleNodes,
  type ChangeEvidence,
  type ChangeNode,
  type ShownChange,
} from "./changes";

const index = (blob: string): ContentSource => ({ kind: "index", worktree: "w", blob });
const at = (path: string, blob = "b1"): ChangeSide => ({ state: "present", path, kind: "file", source: index(blob) });
const absent: ChangeSide = { state: "absent" };

const entries: ChangeEntry[] = [
  { group: "untracked", old: absent, new: at("new.txt") },
  { group: "unstaged", old: at("src/a.ts"), new: at("src/a.ts", "live") },
  { group: "staged", old: at("src/a.ts", "head"), new: at("src/a.ts") },
  { group: "conflicted", path: "z.txt", conflict: "both_modified" },
  { group: "staged", old: { state: "out_of_scope", repository_path: "lib/b.ts" }, new: at("b.ts") },
];

it("orders sections as git status does, and paths within one as the file tree does", () => {
  expect(changeItems(entries).map((item) => `${item.section} ${item.path} ${item.status}`)).toEqual([
    "staged src/a.ts modified",
    "staged b.ts renamed",
    "conflicted z.txt conflicted",
    "unstaged src/a.ts modified",
    "untracked new.txt untracked",
  ]);
});

it("calls a staged change with an absent old side added, and only the untracked group untracked", () => {
  expect(changeItem({ group: "staged", old: absent, new: at("n.txt") }).status).toBe("added");
  expect(changeItem({ group: "untracked", old: absent, new: at("n.txt") }).status).toBe("untracked");
});

it("tells one path's staged and unstaged changes apart", () => {
  const [staged, unstaged] = [changeItem(entries[2]), changeItem(entries[1])];
  expect(staged.path).toBe(unstaged.path);
  expect(staged.key).not.toBe(unstaged.key);
});

it("moves from one section into the next, places a change the list lost where it was, and does not wrap", () => {
  const items = changeItems(entries);
  const lastStaged = items[1];
  expect(changeNeighbours(items, lastStaged).next?.section).toBe("conflicted");
  expect(changeNeighbours(items, items[0]).previous).toBeUndefined();
  expect(changeNeighbours(items, items[items.length - 1]).next).toBeUndefined();
  const gone = changeItem({ group: "unstaged", old: at("src/zz.ts"), new: absent });
  expect(changeNeighbours(items, gone)).toMatchObject({ previous: { path: "src/a.ts", section: "unstaged" }, next: { section: "untracked" } });
});

const listed = (versions: string): ChangeEvidence => ({ state: "listed", versions });
it.each<[string, ShownChange, ChangeEvidence, ChangeEvidence | undefined, boolean, boolean]>([
  ["the same versions listed again", { state: "read" }, listed("v1"), listed("v1"), true, false],
  ["other versions listed", { state: "read" }, listed("v2"), listed("v1"), true, true],
  ["the change gone from the list", { state: "read" }, { state: "gone" }, listed("v1"), true, true],
  ["a list that says nothing yet", { state: "read" }, { state: "unknown" }, listed("v1"), true, false],
  ["a change that kept moving, at a fresh list", { state: "error", changing: true }, listed("v1"), listed("v1"), true, true],
  ["a change that kept moving, at no fresh list", { state: "error", changing: true }, listed("v1"), listed("v1"), false, false],
  ["a read still out", { state: "loading" }, listed("v2"), listed("v1"), true, false],
])("reads a change again on %s: %s", (_, shown, evidence, previous, fresh, reread) => {
  expect(rereadChange(shown, evidence, previous, fresh)).toBe(reread);
});

it("finds a change in a list, gone from a complete one, unknown in a cut or refreshing one", () => {
  const items = changeItems(entries);
  const key = items[0].key;
  expect(changeEvidence({ items, complete: true, refreshing: false }, key)).toEqual(listed(items[0].versions));
  expect(changeEvidence({ items: [], complete: true, refreshing: false }, key)).toEqual({ state: "gone" });
  expect(changeEvidence({ items: [], complete: false, refreshing: false }, key)).toEqual({ state: "unknown" });
  expect(changeEvidence({ items, complete: true, refreshing: true }, key)).toEqual({ state: "unknown" });
});

const unstaged = (path: string): ChangeEntry => ({ group: "unstaged", old: at(path), new: at(path, "live") });
const treeEntries = ["README.md", "src/a.ts", "src/b.ts", "src/utils/kickback/x.ts", "src/utils/kickback/y.ts", "tools/release/notes.md", "z.ts"].map(unstaged);

/** A tree's rows as indented text: directories by their name, changes by their path. */
const outline = (nodes: readonly ChangeNode[], depth = 0): string[] =>
  nodes.flatMap((node) =>
    node.type === "directory" ? [`${"  ".repeat(depth)}${node.name}/`, ...outline(node.children, depth + 1)] : [`${"  ".repeat(depth)}${node.item.path}`],
  );

it("groups a section's changes under directories, directories first, and compacts a chain of single-child directories", () => {
  const items = changeItems(treeEntries);
  expect(outline(changeTree("unstaged", items))).toEqual([
    "src/",
    "  utils/kickback/",
    "    src/utils/kickback/x.ts",
    "    src/utils/kickback/y.ts",
    "  src/a.ts",
    "  src/b.ts",
    "tools/release/",
    "  tools/release/notes.md",
    "README.md",
    "z.ts",
  ]);
});

it("keeps a directory of one directory and a file as two rows, and keys a compacted row by its last directory", () => {
  const items = changeItems(["a/b/c.ts", "a/d.ts"].map(unstaged));
  const [top] = changeTree("unstaged", items);
  expect(top).toMatchObject({ type: "directory", name: "a", key: directoryKey("unstaged", "a") });
  const [inner] = (top as Extract<ChangeNode, { type: "directory" }>).children;
  expect(inner).toMatchObject({ type: "directory", name: "b", key: directoryKey("unstaged", "a/b") });
  expect(outline(changeTree("unstaged", changeItems(["x/y/z/f.ts"].map(unstaged))))).toEqual(["x/y/z/", "  x/y/z/f.ts"]);
});

it("walks the changes on screen: all of them flat, and in the tree none under a collapsed directory", () => {
  const items = changeItems(treeEntries);
  expect(visibleChanges(items, "flat", new Set(["dir:unstaged:src"])).map((item) => item.path)).toEqual(items.map((item) => item.path));
  expect(visibleChanges(items, "tree", new Set()).map((item) => item.path)).toEqual([
    "src/utils/kickback/x.ts",
    "src/utils/kickback/y.ts",
    "src/a.ts",
    "src/b.ts",
    "tools/release/notes.md",
    "README.md",
    "z.ts",
  ]);
  const folded = new Set([directoryKey("unstaged", "src/utils/kickback"), directoryKey("unstaged", "tools/release")]);
  expect(visibleChanges(items, "tree", folded).map((item) => item.path)).toEqual(["src/a.ts", "src/b.ts", "README.md", "z.ts"]);
  const [group] = changeGroups(items, "tree");
  expect(visibleNodes(group.nodes, folded).map((node) => node.type)).toEqual(["directory", "directory", "change", "change", "directory", "change", "change"]);
});

it("moves between the changes on screen in the tree, past a collapsed directory", () => {
  const items = changeItems(treeEntries);
  const folded = new Set([directoryKey("unstaged", "src/utils/kickback")]);
  const shown = visibleChanges(items, "tree", folded);
  const byPath = (path: string) => items.find((item) => item.path === path)!;
  expect(changeNeighbours(shown, byPath("src/a.ts")).previous).toBeUndefined();
  expect(changeNeighbours(shown, byPath("src/b.ts")).next?.path).toBe("tools/release/notes.md");
  // A change that is not among the rows is placed where it would be, so moving on from it reaches the rows around it.
  expect(changeNeighbours(shown, byPath("src/utils/kickback/y.ts"))).toMatchObject({ next: { path: "src/a.ts" } });
});

it("filters by file name alone, ignoring case and the whitespace around the text, and keeps the list for an empty one", () => {
  const items = changeItems(treeEntries);
  expect(filterChanges(items, "")).toBe(items);
  expect(filterChanges(items, "  ")).toBe(items);
  // The directories are not matched: `src` names a folder, not a file.
  expect(filterChanges(items, "src")).toEqual([]);
  expect(filterChanges(items, " .TS ").map((item) => item.path)).toEqual(["src/utils/kickback/x.ts", "src/utils/kickback/y.ts", "src/a.ts", "src/b.ts", "z.ts"]);
  expect(filterChanges(items, "notes").map((item) => item.path)).toEqual(["tools/release/notes.md"]);
});

it("matches a renamed change by its new name and a deleted one by its old name", () => {
  const items = changeItems([
    { group: "unstaged", old: at("old/before.ts"), new: at("new/after.ts", "live") },
    { group: "unstaged", old: at("gone.ts"), new: absent },
  ]);
  expect(filterChanges(items, "after").map((item) => item.path)).toEqual(["new/after.ts"]);
  expect(filterChanges(items, "before")).toEqual([]);
  expect(filterChanges(items, "gone").map((item) => item.path)).toEqual(["gone.ts"]);
});

it("lays a filtered list out as sections that count what is left, flat or as a tree, and moves through those rows", () => {
  const items = changeItems([...treeEntries, { group: "staged", old: at("src/a.ts", "head"), new: at("src/a.ts") }]);
  const shown = filterChanges(items, "a.ts");
  const flat = changeGroups(shown, "flat");
  expect(flat.map((group) => [group.section, group.items.length])).toEqual([["staged", 1], ["unstaged", 1]]);
  const [staged, unstagedGroup] = changeGroups(shown, "tree");
  // Only the directories that lead to a match remain, and the chain through them compacts again.
  expect(outline(staged.nodes)).toEqual(["src/", "  src/a.ts"]);
  expect(outline(unstagedGroup.nodes)).toEqual(["src/", "  src/a.ts"]);
  expect(visibleChanges(shown, "tree", new Set()).map((item) => item.key)).toEqual(shown.map((item) => item.key));
  // A change the filter hides is placed where it would be, so moving on from it reaches the rows
  // around it.
  const hidden = items.find((item) => item.path === "src/b.ts")!;
  expect(changeNeighbours(shown, hidden)).toMatchObject({ previous: { path: "src/a.ts" } });
});
