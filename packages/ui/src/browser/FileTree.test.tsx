// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { isolate } from "../i18n/isolate";
import type { PlatformAdapter } from "../platform";
import { FileTree } from "./FileTree";
import { mountRows } from "./browserTestSupport";
import { rowKey, type TreeNode } from "./tree";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const fileNode = (path: string, selected: boolean): TreeNode => ({
  type: "file",
  key: rowKey(path),
  path,
  entry: { name: path, kind: "file", size: 1, version: "v1", target: null },
  selected,
});

function render(
  nodes: TreeNode[],
  { expanded = new Set<string>(), clipboard, onOpenFile = () => {} }: { expanded?: Set<string>; clipboard?: PlatformAdapter["clipboard"]; onOpenFile?: (path: string) => void } = {},
) {
  return mountRows(
    <FileTree nodes={nodes} expanded={expanded} label="Files" onExpandedChange={() => {}} onOpenFile={onOpenFile} onRetry={() => {}} treeRef={() => {}} />,
    { clipboard },
  );
}

const press = (element: HTMLElement, key: string) =>
  act(() => {
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });

it("tints the selected file's row and names it as selected", () => {
  const container = render([fileNode("a.ts", false), fileNode("b.ts", true)]);
  const row = (name: string) => container.querySelector(`[role=row][data-key="${rowKey(name)}"]`)!;
  expect(row("b.ts").hasAttribute("data-current")).toBe(true);
  expect(row("b.ts").getAttribute("aria-label")).toBe(`${isolate("b.ts")}, selected`);
  expect(row("a.ts").hasAttribute("data-current")).toBe(false);
});

// react-aria's own label for the chevron is its translation of "Expand" ("扩展" in zh-CN), which is
// not the word the tooltip uses; the label comes from the app's catalog, and react-aria's
// aria-labelledby (the chevron, then the row) appends the folder's name.
it("labels a directory's chevron from the catalog, by whether the directory is open", () => {
  const directory: TreeNode = {
    type: "directory",
    key: rowKey("src"),
    path: "src",
    entry: { name: "src", kind: "directory", size: 0, version: "v1", target: null },
    children: [],
  };
  let container = render([directory]);
  const chevron = () => container.querySelector("[slot=chevron]")!;
  expect(chevron().getAttribute("aria-label")).toBe("Expand");
  expect(chevron().getAttribute("aria-labelledby")).toBe(`${chevron().id} ${container.querySelector("[role=row]")!.id}`);
  container = render([directory], { expanded: new Set([rowKey("src")]) });
  expect(chevron().getAttribute("aria-label")).toBe("Collapse");
  expect(chevron().getAttribute("aria-labelledby")).toBe(`${chevron().id} ${container.querySelector("[role=row]")!.id}`);
});

it("gives each file row an actions button, named for its file, that the row's arrow keys reach", () => {
  const container = render([fileNode("a.ts", false)], { clipboard: { writeText: async () => {} } });
  const row = container.querySelector<HTMLElement>("[role=row]")!;
  const actions = row.querySelector<HTMLButtonElement>("button[aria-haspopup]")!;
  expect(actions.getAttribute("aria-label")).toBe(`Actions for file ${isolate("a.ts")}`);
  act(() => row.focus());
  press(row, "ArrowRight");
  expect(document.activeElement).toBe(actions);
  press(actions, "ArrowLeft");
  expect(document.activeElement).toBe(row);
});

// What the menu does with the path is `RowActions`' own, pinned in `RowActions.test.tsx`; what the
// tree has to answer for is that reaching the menu is not reaching the file.
it("opens the menu without opening the file, and gives a directory or a status row none", () => {
  const onOpenFile = vi.fn();
  const container = render(
    [
      {
        type: "directory",
        key: rowKey("src"),
        path: "src",
        entry: { name: "src", kind: "directory", size: 0, version: "v1", target: null },
        children: [{ type: "status", key: "status:src", dir: "src", status: "empty" }],
      },
      fileNode("a.ts", false),
    ],
    { clipboard: { writeText: async () => {} }, expanded: new Set([rowKey("src")]), onOpenFile },
  );
  const rows = [...container.querySelectorAll<HTMLElement>("[role=row]")];
  const withButton = rows.filter((row) => row.querySelector("button[aria-haspopup]"));
  expect(withButton).toEqual([rows.find((row) => row.dataset.key === rowKey("a.ts"))]);
  act(() => withButton[0].querySelector<HTMLButtonElement>("button[aria-haspopup]")!.click());
  expect(document.querySelector<HTMLElement>("[role=menuitem]")!.textContent).toBe("Copy path");
  expect(onOpenFile).not.toHaveBeenCalled();
});
