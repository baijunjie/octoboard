import { expect, it } from "vitest";

import type { ChangeEntry, ChangeSide, ContentSource } from "../../protocol";
import {
  changeEvidence,
  changeGroups,
  changeItem,
  changeItems,
  changeTree,
  commitItems,
  commitKey,
  commitScope,
  compareChanges,
  directoryFold,
  directoryKey,
  filterChanges,
  foldRows,
  holderOf,
  sectionKey,
  sections,
  visibleChanges,
  visibleNodes,
  type ChangeItem,
  type ChangeNode,
} from "./changes";
import { neighboursIn, rereadFor, type Evidence, type Shown } from "../viewedItem";

const changeNeighbours = (items: readonly ChangeItem[], current: ChangeItem) => neighboursIn(items, current, (a, b) => a.key === b.key, compareChanges);
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

it("puts the paths in conflict first, then the other sections, and paths within one as the file tree does", () => {
  expect(changeItems(entries).map((item) => `${item.section} ${item.path} ${item.status}`)).toEqual([
    "conflicted z.txt conflicted",
    "staged src/a.ts modified",
    "staged b.ts renamed",
    "unstaged src/a.ts modified",
    "untracked new.txt untracked",
  ]);
});

it("lists the committed section by commit, newest first, with a row of each for a file both changed", () => {
  const commit = (path: string, id: string): ChangeEntry => ({ group: "committed", old: at(path, "o"), new: at(path, id) });
  const items = [
    ...changeItems([entries[1]]),
    ...commitItems([
      { commit: "c3", parent: "c2", subject: "second", changes: [commit("src/a.ts", "c3")] },
      { commit: "c2", parent: "c1", subject: "first", changes: [commit("z.ts", "c2"), commit("src/a.ts", "c2")] },
    ]),
  ];
  expect(sections(items).map((group) => [group.section, group.items.map((item) => item.path)])).toEqual([
    ["unstaged", ["src/a.ts"]],
    ["committed", ["src/a.ts", "src/a.ts", "z.ts"]],
  ]);
  expect(new Set(items.map((item) => item.key)).size).toBe(items.length);
  expect(changeNeighbours(items, items[1])).toMatchObject({ previous: { section: "unstaged" }, next: { commit: { commit: "c2" } } });

  // One row per commit under the section, each holding its own changes in the layout's form.
  const [, committed] = changeGroups(items, "tree");
  expect(committed.nodes.map((node) => [node.type, node.key, node.type === "commit" ? node.count : undefined])).toEqual([
    ["commit", commitKey("c3"), 1],
    ["commit", commitKey("c2"), 2],
  ]);
  expect(outline(committed.nodes)).toEqual(["c3", "  src/", "    src/a.ts", "c2", "  src/", "    src/a.ts", "  z.ts"]);
  expect(outline(changeGroups(items, "flat")[1].nodes)).toEqual(["c3", "  src/a.ts", "c2", "  src/a.ts", "  z.ts"]);
  // The same directory of two commits is two rows with keys of their own.
  const [first, second] = committed.nodes.map((node) => (node as Extract<ChangeNode, { type: "commit" }>).children[0]);
  expect([first.key, second.key]).toEqual([directoryKey(commitScope("c3"), "src"), directoryKey(commitScope("c2"), "src")]);
});

it("leaves the changes under a folded commit or directory off the viewer's walk", () => {
  const commit = (path: string, id: string): ChangeEntry => ({ group: "committed", old: at(path, "o"), new: at(path, id) });
  const items = commitItems([
    { commit: "c3", parent: "c2", subject: "second", changes: [commit("src/a.ts", "c3")] },
    { commit: "c2", parent: "c1", subject: "first", changes: [commit("z.ts", "c2"), commit("src/b.ts", "c2")] },
  ]);
  const paths = (layout: "flat" | "tree", folded: string[]) => visibleChanges(items, layout, new Set(folded)).map((item) => item.path);
  expect(paths("flat", [])).toEqual(["src/a.ts", "src/b.ts", "z.ts"]);
  expect(paths("flat", [commitKey("c3")])).toEqual(["src/b.ts", "z.ts"]);
  expect(paths("tree", [commitKey("c2")])).toEqual(["src/a.ts"]);
  expect(paths("tree", [directoryKey(commitScope("c2"), "src")])).toEqual(["src/a.ts", "z.ts"]);
  expect(paths("tree", [sectionKey("committed")])).toEqual([]);
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
  expect(changeNeighbours(items, items[0]).next?.section).toBe("staged");
  const lastStaged = items[2];
  expect(changeNeighbours(items, lastStaged).next?.section).toBe("unstaged");
  expect(changeNeighbours(items, items[0]).previous).toBeUndefined();
  expect(changeNeighbours(items, items[items.length - 1]).next).toBeUndefined();
  const gone = changeItem({ group: "unstaged", old: at("src/zz.ts"), new: absent });
  expect(changeNeighbours(items, gone)).toMatchObject({ previous: { path: "src/a.ts", section: "unstaged" }, next: { section: "untracked" } });
});

const listed = (version: string): Evidence => ({ state: "listed", version });
it.each<[string, Shown, Evidence, Evidence | undefined, boolean, boolean]>([
  ["the same versions listed again", { state: "read" }, listed("v1"), listed("v1"), true, false],
  ["other versions listed", { state: "read" }, listed("v2"), listed("v1"), true, true],
  ["the change gone from the list", { state: "read" }, { state: "gone" }, listed("v1"), true, true],
  ["a list that says nothing yet", { state: "read" }, { state: "unknown" }, listed("v1"), true, false],
  ["a change that kept moving, at a fresh list", { state: "error", changing: true }, listed("v1"), listed("v1"), true, true],
  ["a change that kept moving, at no fresh list", { state: "error", changing: true }, listed("v1"), listed("v1"), false, false],
  ["a read still out", { state: "loading" }, listed("v2"), listed("v1"), true, false],
])("reads a change again on %s: %s", (_, shown, evidence, previous, fresh, reread) => {
  expect(rereadFor(shown, evidence, previous, fresh, true)).toBe(reread);
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

/** A tree's rows as indented text: directories by their name, commits by their id, changes by their path. */
const outline = (nodes: readonly ChangeNode[], depth = 0): string[] =>
  nodes.flatMap((node) =>
    node.type === "directory"
      ? [`${"  ".repeat(depth)}${node.name}/`, ...outline(node.children, depth + 1)]
      : node.type === "commit"
        ? [`${"  ".repeat(depth)}${node.commit.commit}`, ...outline(node.children, depth + 1)]
        : [`${"  ".repeat(depth)}${node.item.path}`],
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

it("walks the changes on screen: none of a folded section's in either layout, and in the tree none under a collapsed directory", () => {
  const items = changeItems(treeEntries);
  expect(visibleChanges(items, "flat", new Set(["dir:unstaged:src"])).map((item) => item.path)).toEqual(items.map((item) => item.path));
  // A folded section keeps none of its changes, whichever layout draws it, and leaves the other
  // sections' alone.
  const inTwoSections = changeItems([...treeEntries, { group: "staged", old: at("src/a.ts", "head"), new: at("src/a.ts") }]);
  for (const layout of ["flat", "tree"] as const) {
    expect(visibleChanges(inTwoSections, layout, new Set([sectionKey("unstaged")])).map((item) => item.section)).toEqual(["staged"]);
  }
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

const sorted = (keys: readonly string[]) => [...keys].sort();

it("names every directory of the tree at every depth for the header's fold button, and says whether any is open", () => {
  const items = changeItems(treeEntries);
  const all = sorted([directoryKey("unstaged", "src"), directoryKey("unstaged", "src/utils/kickback"), directoryKey("unstaged", "tools/release")]);
  const fold = directoryFold(items, new Set());
  expect(sorted(fold.keys)).toEqual(all);
  expect(fold.anyOpen).toBe(true);
  // Every directory folded: they are all still named, and none is open.
  const folded = directoryFold(items, new Set(all));
  expect(sorted(folded.keys)).toEqual(all);
  expect(folded.anyOpen).toBe(false);
});

it("counts a directory as open only when its own row is on screen, not one left open under a folded directory", () => {
  const items = changeItems(treeEntries);
  const src = directoryKey("unstaged", "src");
  const kickback = directoryKey("unstaged", "src/utils/kickback");
  const release = directoryKey("unstaged", "tools/release");
  // `kickback` is still open, but its row is hidden under the folded `src`.
  expect(directoryFold(items, new Set([src, release])).anyOpen).toBe(false);
  // `src` shows open, though the directory inside it is folded.
  expect(directoryFold(items, new Set([kickback, release])).anyOpen).toBe(true);
  // One open directory is enough.
  expect(directoryFold(items, new Set([src, kickback])).anyOpen).toBe(true);
});

it("has no directory to fold when every change is at the root", () => {
  expect(directoryFold(changeItems(["README.md", "z.ts"].map(unstaged)), new Set())).toMatchObject({ keys: [], anyOpen: false });
  expect(directoryFold([], new Set())).toMatchObject({ keys: [], anyOpen: false });
});

it("keeps the directories of a folded section out of the fold, whatever state they were left in", () => {
  const items = changeItems([...treeEntries, { group: "staged", old: at("lib/a.ts", "head"), new: at("lib/a.ts") }]);
  const staged = directoryKey("staged", "lib");
  const unstagedKeys = [directoryKey("unstaged", "src"), directoryKey("unstaged", "src/utils/kickback"), directoryKey("unstaged", "tools/release")];
  const fold = directoryFold(items, new Set([sectionKey("unstaged")]));
  expect(fold.keys).toEqual([staged]);
  expect(fold.anyOpen).toBe(true);
  // The staged directory folded: nothing on screen is open, though the unstaged ones, hidden, are.
  expect(directoryFold(items, new Set([sectionKey("unstaged"), staged])).anyOpen).toBe(false);
  // Both sections folded: nothing to act on.
  expect(directoryFold(items, new Set([sectionKey("unstaged"), sectionKey("staged")]))).toMatchObject({ keys: [], anyOpen: false });
  expect(sorted(directoryFold(items, new Set()).keys)).toEqual(sorted([staged, ...unstagedKeys]));
});

it("folds the directories of each commit that is not folded away, a commit's directories being its own", () => {
  const commit = (path: string, id: string): ChangeEntry => ({ group: "committed", old: at(path, "o"), new: at(path, id) });
  const items = commitItems([
    { commit: "c3", parent: "c2", subject: "second", changes: [commit("src/a.ts", "c3")] },
    { commit: "c2", parent: "c1", subject: "first", changes: [commit("z.ts", "c2"), commit("src/b.ts", "c2")] },
  ]);
  const c3 = directoryKey(commitScope("c3"), "src");
  const c2 = directoryKey(commitScope("c2"), "src");
  expect(sorted(directoryFold(items, new Set()).keys)).toEqual(sorted([c3, c2]));
  // A folded commit keeps its directories out, and what it hides does not count as open.
  const withoutC3 = directoryFold(items, new Set([commitKey("c3")]));
  expect(withoutC3).toMatchObject({ keys: [c2], anyOpen: true });
  expect(directoryFold(items, new Set([commitKey("c3"), c2])).anyOpen).toBe(false);
  // The commits' own rows are not directories: nothing of the commits' or the section's keys is named.
  expect(directoryFold(items, new Set()).keys.every((key) => key.startsWith("dir:"))).toBe(true);
  expect(directoryFold(items, new Set([sectionKey("committed")]))).toMatchObject({ keys: [], anyOpen: false });
  expect(directoryFold(items, new Set([commitKey("c3"), commitKey("c2")]))).toMatchObject({ keys: [], anyOpen: false });
});

it("folds only the directories the filter leaves, and ignores one it hides", () => {
  const items = changeItems(treeEntries);
  const fold = directoryFold(filterChanges(items, "notes"), new Set());
  expect(fold).toMatchObject({ keys: [directoryKey("unstaged", "tools/release")], anyOpen: true });
  // A hidden directory's state does not count: `src` is folded, but it is not in the list.
  expect(directoryFold(filterChanges(items, "notes"), new Set([directoryKey("unstaged", "src")])).anyOpen).toBe(true);
  expect(directoryFold(filterChanges(items, "zzz"), new Set())).toMatchObject({ keys: [], anyOpen: false });
});

const outermostOf = (items: readonly ChangeItem[], collapsed: ReadonlySet<string> = new Set()) => {
  const { outermost } = directoryFold(items, collapsed);
  const keyOf = (path: string) => items.find((item) => item.path === path)!.key;
  return { outermost, keyOf };
};

it("maps every row under a directory to the outermost directory it sits in, never to the nearer one, and leaves the top-level rows out", () => {
  const items = changeItems(["README.md", "a/x.ts", "a/b/y.ts", "a/b/c/z.ts", "tools/release/notes.md"].map(unstaged));
  const { outermost, keyOf } = outermostOf(items);
  const a = directoryKey("unstaged", "a");
  const release = directoryKey("unstaged", "tools/release");
  expect(Object.fromEntries(outermost)).toEqual({
    [directoryKey("unstaged", "a/b")]: a,
    [directoryKey("unstaged", "a/b/c")]: a,
    [keyOf("a/x.ts")]: a,
    [keyOf("a/b/y.ts")]: a,
    [keyOf("a/b/c/z.ts")]: a,
    [keyOf("tools/release/notes.md")]: release,
  });
  // A top-level directory is its own outermost one, and a change at the root has none.
  expect(outermost.has(a)).toBe(false);
  expect(outermost.has(release)).toBe(false);
  expect(outermost.has(keyOf("README.md"))).toBe(false);
  // A compacted chain is one directory, so the rows under it map to it.
  const compact = outermostOf(changeItems(["x/y/z/f.ts", "x/y/z/g.ts"].map(unstaged)));
  expect(Object.fromEntries(compact.outermost)).toEqual({
    [compact.keyOf("x/y/z/f.ts")]: directoryKey("unstaged", "x/y/z"),
    [compact.keyOf("x/y/z/g.ts")]: directoryKey("unstaged", "x/y/z"),
  });
});

it("keeps the outermost directory of each section and each commit separate, and maps nothing in a folded section or commit", () => {
  const commit = (path: string, id: string): ChangeEntry => ({ group: "committed", old: at(path, "o"), new: at(path, id) });
  const items = [
    ...changeItems([unstaged("src/a/x.ts"), { group: "staged", old: at("src/a/x.ts", "head"), new: at("src/a/x.ts") }]),
    ...commitItems([
      { commit: "c3", parent: "c2", subject: "second", changes: [commit("src/a/y.ts", "c3")] },
      { commit: "c2", parent: "c1", subject: "first", changes: [commit("src/b.ts", "c2")] },
    ]),
  ];
  const byKey = (section: string, commitId?: string) => items.find((item) => item.section === section && (commitId === undefined || item.commit?.commit === commitId))!.key;
  const all = directoryFold(items, new Set()).outermost;
  expect(all.get(byKey("unstaged"))).toBe(directoryKey("unstaged", "src/a"));
  expect(all.get(byKey("staged"))).toBe(directoryKey("staged", "src/a"));
  expect(all.get(byKey("committed", "c3"))).toBe(directoryKey(commitScope("c3"), "src/a"));
  expect(all.get(byKey("committed", "c2"))).toBe(directoryKey(commitScope("c2"), "src"));

  const folded = directoryFold(items, new Set([sectionKey("unstaged"), commitKey("c3")])).outermost;
  expect(folded.has(byKey("unstaged"))).toBe(false);
  expect(folded.has(byKey("committed", "c3"))).toBe(false);
  expect(folded.get(byKey("staged"))).toBe(directoryKey("staged", "src/a"));
  expect(folded.get(byKey("committed", "c2"))).toBe(directoryKey(commitScope("c2"), "src"));
  expect(directoryFold(items, new Set([sectionKey("committed")])).outermost.has(byKey("committed", "c2"))).toBe(false);
});

it("maps only the rows of what the filter leaves", () => {
  const items = changeItems(treeEntries);
  const shown = filterChanges(items, "notes");
  const { outermost } = directoryFold(shown, new Set());
  expect([...outermost]).toEqual([[shown[0].key, directoryKey("unstaged", "tools/release")]]);
  expect(directoryFold([], new Set()).outermost.size).toBe(0);
});

it("returns the folded rows with the keys added or removed and every other row as it was, leaving its input alone", () => {
  const collapsed: ReadonlySet<string> = new Set(["section:staged", "dir:unstaged:a"]);
  const folded = foldRows(collapsed, ["dir:unstaged:a", "dir:unstaged:b", "row:commit:c1"], true);
  expect([...folded].sort()).toEqual(["dir:unstaged:a", "dir:unstaged:b", "row:commit:c1", "section:staged"]);
  const unfolded = foldRows(collapsed, new Set(["dir:unstaged:a", "dir:unstaged:never-folded"]), false);
  expect([...unfolded]).toEqual(["section:staged"]);
  expect([...collapsed].sort()).toEqual(["dir:unstaged:a", "section:staged"]);
  // A new set each time, even where nothing changes.
  const same = foldRows(collapsed, [], true);
  expect(same).not.toBe(collapsed);
  expect([...same].sort()).toEqual([...collapsed].sort());
});

it("names the node that holds a row at any depth beneath it, and nothing for a row that no node holds", () => {
  const commit = (path: string, id: string): ChangeEntry => ({ group: "committed", old: at(path, "o"), new: at(path, id) });
  const items = [
    ...changeItems([entries[1]]),
    ...commitItems([
      { commit: "c3", parent: "c2", subject: "second", changes: [commit("src/nested/deep/b.ts", "c3"), commit("src/top.ts", "c3"), commit("z.ts", "c3")] },
      { commit: "c2", parent: "c1", subject: "first", changes: [commit("src/a.ts", "c2")] },
    ]),
  ];
  const [unstaged, committed] = changeGroups(items, "tree");
  const keyOf = (path: string, scope: "c3" | "c2") => items.find((item) => item.path === path && item.commit?.commit === scope)!.key;
  const c3Directory = directoryKey(commitScope("c3"), "src/nested/deep");
  const c3Src = directoryKey(commitScope("c3"), "src");
  // A change, and a directory, at any depth under a commit are held by that commit.
  expect(holderOf(committed.nodes, keyOf("src/nested/deep/b.ts", "c3"))).toBe(commitKey("c3"));
  expect(holderOf(committed.nodes, keyOf("z.ts", "c3"))).toBe(commitKey("c3"));
  expect(holderOf(committed.nodes, keyOf("src/a.ts", "c2"))).toBe(commitKey("c2"));
  expect(holderOf(committed.nodes, c3Src)).toBe(commitKey("c3"));
  expect(holderOf(committed.nodes, c3Directory)).toBe(commitKey("c3"));
  // The flat layout holds the changes one level beneath the commit.
  expect(holderOf(changeGroups(items, "flat")[1].nodes, keyOf("src/a.ts", "c2"))).toBe(commitKey("c2"));
  // The nodes themselves are not counted: a node is not held by itself, nor by a sibling.
  expect(holderOf(committed.nodes, commitKey("c3"))).toBeUndefined();
  expect(holderOf(committed.nodes, commitKey("c2"))).toBeUndefined();
  // A row of another section, a key that names nothing, and no nodes at all have no holder.
  expect(holderOf(committed.nodes, unstaged.items[0].key)).toBeUndefined();
  expect(holderOf(committed.nodes, "row:commit:nope")).toBeUndefined();
  expect(holderOf([], keyOf("z.ts", "c3"))).toBeUndefined();
  // Directories of a tree that is not by commit hold their own rows the same way.
  const tree = changeTree("unstaged", changeItems(treeEntries));
  const deep = changeItems(treeEntries).find((item) => item.path === "src/utils/kickback/x.ts")!;
  expect(holderOf(tree, deep.key)).toBe(directoryKey("unstaged", "src"));
  expect(holderOf(tree, directoryKey("unstaged", "src/utils/kickback"))).toBe(directoryKey("unstaged", "src"));
  expect(holderOf(tree, directoryKey("unstaged", "src"))).toBeUndefined();
});
