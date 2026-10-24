import { describe, expect, it } from "vitest";

import { buildFileResults, resultDirectories } from "./filesView";
import { rowKey } from "./tree";

describe("file results as a tree", () => {
  const paths = ["z.ts", "src/item10.ts", "src/deep/caf%C3%A9.ts", "src/item2.ts", "a.ts", "src/item2.ts"];

  it("expands known directories by default with directory names and stable keys", () => {
    const { tree, expanded } = buildFileResults(paths, "src/item2.ts", new Set());
    expect(tree.files).toEqual(["src/deep/caf%C3%A9.ts", "src/item2.ts", "src/item10.ts", "a.ts", "z.ts"]);
    expect(tree.rows).toEqual(["src", "src/deep", ...tree.files].map(rowKey));
    expect(tree.nodes[0]).toMatchObject({ type: "directory", key: rowKey("src"), entry: { name: "src" }, children: [{ type: "directory", path: "src/deep" }, { type: "file", selected: true, entry: { name: "item2.ts" } }, { type: "file" }] });
    expect([...expanded]).toEqual([rowKey("src"), rowKey("src/deep")]);
    expect(resultDirectories(paths)).toEqual(["src", "src/deep"]);
    expect(tree.wanted).toEqual([]);
  });

  it("excludes collapsed descendants from rows and navigation without losing nested memory", () => {
    const collapsed = new Set(["src/deep"]);
    const { tree } = buildFileResults(paths, undefined, collapsed);
    expect(tree.files).toEqual(["src/item2.ts", "src/item10.ts", "a.ts", "z.ts"]);
    expect(tree.rows).toEqual(["src", "src/deep", ...tree.files].map(rowKey));
    expect(tree.nodes[0]).toMatchObject({ children: [{ children: [] }, {}, {}] });
    collapsed.add("src");
    const closed = buildFileResults(paths, undefined, collapsed);
    expect(closed.tree.files).toEqual(["a.ts", "z.ts"]);
    expect(closed.expanded.size).toBe(0);
    expect(buildFileResults(paths, undefined, new Set(["src"])).expanded.has(rowKey("src/deep"))).toBe(true);
  });

  it("marks the result files Git ignores, and a folder only when everything shown in it is ignored", () => {
    const { tree } = buildFileResults(["dist/a.js", "dist/b.js", "src/c.ts", "src/d.js", "e.js"], undefined, new Set(), new Set(["dist/a.js", "dist/b.js", "src/d.js"]));
    expect(tree.nodes).toMatchObject([
      { type: "directory", path: "dist", entry: { ignored: true }, children: [{ entry: { ignored: true } }, { entry: { ignored: true } }] },
      { type: "directory", path: "src", children: [{ path: "src/c.ts" }, { path: "src/d.js", entry: { ignored: true } }] },
      { type: "file", path: "e.js" },
    ]);
    const src = tree.nodes[1];
    expect(src.type === "directory" && src.entry.ignored).toBeUndefined();
    expect(tree.nodes[2].type === "file" && tree.nodes[2].entry?.ignored).toBeUndefined();
  });
});
