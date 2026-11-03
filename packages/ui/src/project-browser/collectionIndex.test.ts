import { describe, expect, it } from "vitest";

import { collectionIndex, emptyCollectionIndex } from "./collectionIndex";

interface Node {
  key: string;
  children?: Node[];
}

const tree: Node[] = [
  { key: "a", children: [{ key: "a1" }, { key: "a2", children: [{ key: "a2x" }] }] },
  { key: "b", children: [{ key: "b1" }] },
  { key: "c" },
];
const childrenOf = (node: Node) => node.children ?? [];

describe("collectionIndex", () => {
  it("indexes every node, and the keys on screen under the open ones", () => {
    const open = new Set(["a"]);
    const index = collectionIndex(tree, childrenOf, (node) => open.has(node.key));
    expect([...index.byKey.keys()]).toEqual(["a", "a1", "a2", "a2x", "b", "b1", "c"]);
    expect([...index.visibleKeys]).toEqual(["a", "a1", "a2", "b", "c"]);
    expect([...index.folds.open]).toEqual(["a"]);
    expect(Object.fromEntries(index.folds.parents)).toEqual({ a1: "a", a2: "a" });
  });

  it("shows nothing under a node that is open beneath one that is closed", () => {
    const open = new Set(["a2"]);
    const index = collectionIndex(tree, childrenOf, (node) => open.has(node.key));
    expect([...index.visibleKeys]).toEqual(["a", "b", "c"]);
    expect(index.folds.open.size).toBe(0);
    expect(index.byKey.has("a2x")).toBe(true);
  });

  it("adds the rows of each section to one index, under the section's own row", () => {
    const into = emptyCollectionIndex<Node>();
    const open = (node: Node) => node.key === "a";
    for (const [heading, nodes] of [["heading:one", [tree[0]]], ["heading:two", [tree[2]]]] as const) {
      into.visibleKeys.add(heading);
      into.folds.open.add(heading);
      collectionIndex(nodes, childrenOf, open, { parent: heading, into });
    }
    expect([...into.visibleKeys]).toEqual(["heading:one", "a", "a1", "a2", "heading:two", "c"]);
    expect(Object.fromEntries(into.folds.parents)).toEqual({ a: "heading:one", a1: "a", a2: "a", c: "heading:two" });
    expect([...into.folds.open]).toEqual(["heading:one", "a", "heading:two"]);
  });
});
