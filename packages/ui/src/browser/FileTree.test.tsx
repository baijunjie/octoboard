// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";

import { FileTree } from "./FileTree";
import { rowKey, type TreeNode } from "./tree";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom has no ResizeObserver, which the rows' names (FadeOverflow) need.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

afterEach(() => document.body.replaceChildren());

const fileNode = (path: string, selected: boolean): TreeNode => ({
  type: "file",
  key: rowKey(path),
  path,
  entry: { name: path, kind: "file", size: 1, version: "v1", target: null },
  selected,
});

it("tints the selected file's row and names it as selected", () => {
  const container = document.body.appendChild(document.createElement("div"));
  act(() =>
    createRoot(container).render(
      <FileTree
        nodes={[fileNode("a.ts", false), fileNode("b.ts", true)]}
        expanded={new Set()}
        label="Files"
        onExpandedChange={() => {}}
        onOpenFile={() => {}}
        onRetry={() => {}}
        treeRef={() => {}}
      />,
    ),
  );
  const row = (name: string) => container.querySelector(`[role=row][data-key="${rowKey(name)}"]`)!;
  expect(row("b.ts").hasAttribute("data-current")).toBe(true);
  expect(row("b.ts").getAttribute("aria-label")).toBe("b.ts, selected");
  expect(row("a.ts").hasAttribute("data-current")).toBe(false);
});

// react-aria's own label for the chevron is its translation of "Expand" ("扩展" in zh-CN), which is
// not the word the tooltip uses; the label comes from the app's catalog, and react-aria's
// aria-labelledby (the chevron, then the row) appends the folder's name.
it("labels a directory's chevron from the catalog, by whether the directory is open", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const directory: TreeNode = {
    type: "directory",
    key: rowKey("src"),
    path: "src",
    entry: { name: "src", kind: "directory", size: 0, version: "v1", target: null },
    children: [],
  };
  const render = (expanded: Set<string>) =>
    act(() =>
      root.render(
        <FileTree
          nodes={[directory]}
          expanded={expanded}
          label="Files"
          onExpandedChange={() => {}}
          onOpenFile={() => {}}
          onRetry={() => {}}
          treeRef={() => {}}
        />,
      ),
    );
  const root = createRoot(container);
  render(new Set());
  const chevron = () => container.querySelector("[slot=chevron]")!;
  expect(chevron().getAttribute("aria-label")).toBe("Expand");
  expect(chevron().getAttribute("aria-labelledby")).toBe(`${chevron().id} ${container.querySelector("[role=row]")!.id}`);
  render(new Set([rowKey("src")]));
  expect(chevron().getAttribute("aria-label")).toBe("Collapse");
  expect(chevron().getAttribute("aria-labelledby")).toBe(`${chevron().id} ${container.querySelector("[role=row]")!.id}`);
});
