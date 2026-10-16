import { describe, expect, it } from "vitest";

import { buildFileResults, resultDirectories } from "./filesView";
import { rowKey } from "./tree";

describe("file result layouts", () => {
  const paths = ["z.ts", "src/item10.ts", "src/deep/caf%C3%A9.ts", "src/item2.ts", "a.ts", "src/item2.ts"];

  it("sorts and deduplicates flat paths while preserving wire names and selection", () => {
    const { tree, expanded } = buildFileResults(paths, "src/deep/caf%C3%A9.ts", "flat", new Set(["src"]));
    expect(tree.files).toEqual(["src/deep/caf%C3%A9.ts", "src/item2.ts", "src/item10.ts", "a.ts", "z.ts"]);
    expect(tree.rows).toEqual(tree.files.map(rowKey));
    expect(tree.nodes[0]).toMatchObject({ type: "file", selected: true, path: "src/deep/caf%C3%A9.ts", entry: { name: "src/deep/caf%C3%A9.ts" } });
    expect(tree.wanted).toEqual([]);
    expect(expanded.size).toBe(0);
  });

  it("expands known directories by default with directory names and stable keys", () => {
    const { tree, expanded } = buildFileResults(paths, "src/item2.ts", "tree", new Set());
    expect(tree.files).toEqual(["src/deep/caf%C3%A9.ts", "src/item2.ts", "src/item10.ts", "a.ts", "z.ts"]);
    expect(tree.rows).toEqual(["src", "src/deep", ...tree.files].map(rowKey));
    expect(tree.nodes[0]).toMatchObject({ type: "directory", key: rowKey("src"), entry: { name: "src" }, children: [{ type: "directory", path: "src/deep" }, { type: "file", selected: true, entry: { name: "item2.ts" } }, { type: "file" }] });
    expect([...expanded]).toEqual([rowKey("src"), rowKey("src/deep")]);
    expect(resultDirectories(paths)).toEqual(["src", "src/deep"]);
    expect(tree.wanted).toEqual([]);
  });

  it("excludes collapsed descendants from rows and navigation without losing nested memory", () => {
    const collapsed = new Set(["src/deep"]);
    const { tree } = buildFileResults(paths, undefined, "tree", collapsed);
    expect(tree.files).toEqual(["src/item2.ts", "src/item10.ts", "a.ts", "z.ts"]);
    expect(tree.rows).toEqual(["src", "src/deep", ...tree.files].map(rowKey));
    expect(tree.nodes[0]).toMatchObject({ children: [{ children: [] }, {}, {}] });
    collapsed.add("src");
    const closed = buildFileResults(paths, undefined, "tree", collapsed);
    expect(closed.tree.files).toEqual(["a.ts", "z.ts"]);
    expect(closed.expanded.size).toBe(0);
    expect(buildFileResults(paths, undefined, "tree", new Set(["src"])).expanded.has(rowKey("src/deep"))).toBe(true);
  });
});
