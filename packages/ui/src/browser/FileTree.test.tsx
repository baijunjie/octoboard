// @vitest-environment jsdom
import { act, useState } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { isolate } from "../i18n/isolate";
import type { PlatformAdapter } from "../platform";
import { FileTree } from "./FileTree";
import { mountRows, pressTab } from "./browserTestSupport";
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
  { expanded = new Set<string>(), clipboard, onOpenFile = () => {}, onExpandedChange = () => {}, onRetry = () => {} }: {
    expanded?: Set<string>;
    onRetry?: (dir: string) => void;
    clipboard?: PlatformAdapter["clipboard"];
    onOpenFile?: (path: string) => void;
    onExpandedChange?: (keys: Set<string | number>) => void;
  } = {},
) {
  return mountRows(
    <FileTree nodes={nodes} expanded={expanded} label="Files" onExpandedChange={onExpandedChange} onOpenFile={onOpenFile} onRetry={onRetry} treeRef={() => {}} />,
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

// react-aria's own label for the expand control (in its chevron slot) is its
// translation of "Expand" ("扩展" in zh-CN), which is not the word the tooltip uses; the label comes
// from the app's catalog, and react-aria's aria-labelledby (the control, then the row) appends the
// folder's name.
it("labels a directory's expand control from the catalog, by whether the directory is open", () => {
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

it("keeps shared chevron presses in the tree's expansion and row-focus model", async () => {
  const directory: TreeNode = {
    type: "directory", key: rowKey("src"), path: "src",
    entry: { name: "src", kind: "directory", size: 0, version: "v1", target: null },
    children: [fileNode("src/child.ts", false)],
  };
  function Files() {
    const [expanded, setExpanded] = useState(new Set<string | number>());
    return <FileTree nodes={[directory, fileNode("top.ts", false)]} expanded={new Set([...expanded].map(String))} label="Files" onExpandedChange={setExpanded} onOpenFile={() => {}} onRetry={() => {}} treeRef={() => {}} />;
  }
  const container = mountRows(<Files />, { clipboard: { writeText: async () => {} } });
  const row = (path: string) => container.querySelector<HTMLElement>(`[role=row][data-key="${rowKey(path)}"]`)!;
  const folder = row("src");
  const chevron = folder.querySelector<HTMLButtonElement>("[slot=chevron]")!;
  expect(chevron.classList.contains("app-button--xs")).toBe(true);
  expect(chevron.classList.contains("button--ghost")).toBe(true);
  expect(chevron.classList.contains("button--icon-only")).toBe(true);
  expect(chevron.tabIndex).toBe(-1);
  act(() => folder.focus());
  press(folder, "Enter");
  expect(folder.getAttribute("aria-expanded")).toBe("true");
  expect(document.activeElement).toBe(folder);
  const actions = folder.querySelector<HTMLButtonElement>("button[aria-haspopup]")!;
  press(folder, "ArrowRight");
  expect(document.activeElement).toBe(actions);
  press(actions, "ArrowLeft");
  expect(document.activeElement).toBe(folder);
  press(folder, "ArrowDown");
  expect(document.activeElement).toBe(row("src/child.ts"));
  press(document.activeElement as HTMLElement, "Home");
  expect(document.activeElement).toBe(folder);
  press(folder, "End");
  expect(document.activeElement).toBe(row("top.ts"));
  act(() => row("src/child.ts").focus());
  act(() => chevron.click());
  expect(folder.getAttribute("aria-expanded")).toBe("false");
  expect(row("src/child.ts")).toBeNull();
  await vi.waitFor(() => expect(document.activeElement).toBe(folder));
  expect(chevron.tabIndex).toBe(-1);
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

it("leaves the tree on Option+Tab and Option+Shift+Tab from where Tab and Shift+Tab do, past the rows' Actions buttons", () => {
  const container = render([fileNode("a.ts", false), fileNode("b.ts", false)], { clipboard: { writeText: async () => {} } });
  const tree = container.querySelector<HTMLElement>("[role=treegrid]")!;
  const [first, last] = [...container.querySelectorAll<HTMLElement>("[role=row]")];
  const firstActions = first.querySelector<HTMLElement>("button[aria-haspopup]")!;
  for (const altKey of [false, true]) {
    expect(pressTab(first, { altKey })).toEqual({ unprevented: true, movedTo: last.querySelector("button[aria-haspopup]") });
    expect(pressTab(firstActions, { altKey, shiftKey: true })).toEqual({ unprevented: true, movedTo: tree });
  }
});

// What the menu does with the path is `RowActions`' own, pinned in `RowActions.test.tsx`; what the
// tree has to answer for is that reaching the menu is not reaching the file.
it("opens the menu without opening the file, and gives directories actions while status rows have none", () => {
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
  expect(withButton).toEqual(rows.filter((row) => row.dataset.key !== "status:src"));
  act(() => withButton[0].querySelector<HTMLButtonElement>("button[aria-haspopup]")!.click());
  expect(document.querySelector<HTMLElement>("[role=menuitem]")!.textContent).toBe("Copy path");
  expect(onOpenFile).not.toHaveBeenCalled();
});

it.each([false, true])("copies a directory path without toggling it (expanded: %s)", async (expanded) => {
  const writeText = vi.fn(async () => {});
  const onExpandedChange = vi.fn();
  const onOpenFile = vi.fn();
  const directory: TreeNode = {
    type: "directory",
    key: rowKey("src/caf%C3%A9"),
    path: "src/caf%C3%A9",
    entry: { name: "caf%C3%A9", kind: "directory", size: 0, version: "v1", target: null },
    children: [],
  };
  const container = render([directory], {
    clipboard: { writeText },
    expanded: new Set(expanded ? [directory.key] : []),
    onExpandedChange,
    onOpenFile,
  });
  const row = container.querySelector<HTMLElement>("[role=row]")!;
  const actions = row.querySelector<HTMLButtonElement>("button[aria-haspopup]")!;
  expect(actions.getAttribute("aria-label")).toBe(`Actions for folder ${isolate("café")}`);
  act(() => row.focus());
  press(row, "ArrowRight");
  if (expanded) {
    expect(document.activeElement).toBe(actions);
    press(actions, "ArrowLeft");
    expect(document.activeElement).toBe(row);
  } else {
    expect(onExpandedChange).toHaveBeenCalledWith(new Set([directory.key]));
    onExpandedChange.mockClear();
  }
  act(() => actions.click());
  act(() => document.querySelector<HTMLElement>("[role=menuitem]")!.click());
  await act(async () => {});
  expect(writeText).toHaveBeenCalledWith("src/café");
  expect(onExpandedChange).not.toHaveBeenCalled();
  expect(onOpenFile).not.toHaveBeenCalled();
});


it.each(["file", "directory"] as const)("opens a %s row's context menu without activating or expanding it", (kind) => {
  const onOpenFile = vi.fn();
  const onExpandedChange = vi.fn();
  const node: TreeNode = kind === "file" ? fileNode("a.ts", false) : {
    type: "directory", key: rowKey("src"), path: "src",
    entry: { name: "src", kind: "directory", size: 0, version: "v1", target: null }, children: [],
  };
  const container = render([node], { clipboard: { writeText: async () => {} }, onOpenFile, onExpandedChange });
  const row = container.querySelector<HTMLElement>("[role=row]")!;
  act(() => row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 30, clientY: 40 })));
  expect(document.querySelector<HTMLElement>("[role=menuitem]")!.textContent).toBe("Copy path");
  expect(onOpenFile).not.toHaveBeenCalled();
  expect(onExpandedChange).not.toHaveBeenCalled();
});


it("folds a directory's rows in and out without presence, and animates rows a listing adds or removes", async () => {
  const animate = vi.fn(() => ({ cancel: vi.fn(), effect: { getComputedTiming: () => ({ progress: 1 }) }, onfinish: null, oncancel: null }));
  const previous = HTMLElement.prototype.animate;
  HTMLElement.prototype.animate = animate as unknown as typeof HTMLElement.prototype.animate;
  onTestFinished(() => { HTMLElement.prototype.animate = previous; });
  const directory = (children: TreeNode[]): TreeNode => ({
    type: "directory", key: rowKey("src"), path: "src",
    entry: { name: "src", kind: "directory", size: 0, version: "v1", target: null },
    children,
  });
  let list: (children: TreeNode[]) => void = () => {};
  // A stable ref, as the browser hands it: a new one each render would restart the motion's watch.
  const treeRef = () => {};
  function Folder() {
    const [expanded, setExpanded] = useState(new Set<string | number>());
    const [children, setChildren] = useState<TreeNode[]>([fileNode("src/child.ts", false)]);
    list = (next) => act(() => setChildren(next));
    const open = new Set([...expanded].map(String));
    return <FileTree nodes={[directory(open.has(rowKey("src")) ? children : [])]} expanded={open} label="Files" onExpandedChange={setExpanded} onOpenFile={() => {}} onRetry={() => {}} treeRef={treeRef} />;
  }
  const container = mountRows(<Folder />);
  await act(async () => {});
  const presences = () => animate.mock.calls.filter((call) => (call as unknown as [Keyframe[]])[0][0].opacity !== undefined);
  const toggle = () => act(() => container.querySelector<HTMLElement>("[slot=chevron]")!.click());
  toggle();
  await act(async () => {});
  expect(container.querySelector(`[data-list-motion="${rowKey("src/child.ts")}"]`)).not.toBeNull();
  expect(presences()).toHaveLength(0);
  list([fileNode("src/child.ts", false), fileNode("src/added.ts", false)]);
  await act(async () => {});
  expect(presences().map((call) => (call as unknown as [Keyframe[]])[0][0].opacity)).toEqual([0]);
  list([fileNode("src/added.ts", false)]);
  await act(async () => {});
  const ghost = container.querySelector<HTMLElement>("[data-list-motion-ghost]")!;
  expect(ghost.textContent).toContain("child.ts");
  expect(ghost.hasAttribute("inert")).toBe(true);
  animate.mockClear();
  toggle();
  await act(async () => {});
  expect(container.querySelector(`[data-list-motion="${rowKey("src/added.ts")}"]`)).toBeNull();
  expect(presences()).toHaveLength(0);
  expect(container.querySelector("[data-list-motion-ghost]")?.textContent ?? "").not.toContain("added.ts");
});

it("marks a folder that could not be listed with a named glyph and keeps the reason in its popover", () => {
  const failed: TreeNode = { type: "status", key: "status:error:src", dir: "src", status: "error", message: "permission denied" };
  const container = render([failed]);
  const row = container.querySelector("[role=row]")!;
  const mark = row.querySelector("button[aria-label]")!;
  expect(mark.getAttribute("aria-label")).toBe("Could not list this folder");
  // No sentence of error text in the row: the reason is not drawn until the mark is pressed.
  expect(row.textContent).not.toContain("permission denied");
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) mark.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
  });
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("permission denied");
});

it("opens a failed folder's reason from the mark without listing the folder again, and reaches the mark with ArrowRight", () => {
  const failed: TreeNode = { type: "status", key: "status:error:src", dir: "src", status: "error", message: "permission denied" };
  const onRetry = vi.fn();
  const container = render([failed], { onRetry });
  const row = container.querySelector<HTMLElement>("[role=row]")!;
  const mark = row.querySelector<HTMLButtonElement>("button[aria-label]")!;
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup"]) {
      const event = new MouseEvent(type, { bubbles: true, button: 0 });
      if (type.startsWith("pointer")) Object.defineProperty(event, "pointerType", { value: "mouse" });
      mark.dispatchEvent(event);
    }
    mark.dispatchEvent(new MouseEvent("click", { bubbles: true, button: 0, detail: 1 }));
  });
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  expect(onRetry).not.toHaveBeenCalled();
  // A tab stop inside the tree row as well as a stop of the row's arrow-key walk.
  expect(mark.getAttribute("tabindex")).toBe("0");
  act(() => row.focus());
  press(row, "ArrowRight");
  expect(document.activeElement).toBe(mark);
});

/** Opens the Actions menu of `row` from the keyboard (Right to its button, then Enter) and returns
 * the button and the menu's first item, which holds focus. */
function openActionsFromKeyboard(row: HTMLElement) {
  const actions = row.querySelector<HTMLButtonElement>("button[aria-haspopup]")!;
  act(() => row.focus());
  press(row, "ArrowRight");
  expect(document.activeElement).toBe(actions);
  press(actions, "Enter");
  const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
  expect(document.activeElement).toBe(item);
  return { actions, item };
}

/** Lets the menu's popover go and the focus moves after it run: the return to the Actions button two
 * animation frames on, and react-aria's own moves, which wait for renders in between. */
async function settle() {
  for (let i = 0; i < 5; i++) await act(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); });
}

it.each(["Escape", "Enter"])("returns focus to a row's Actions button when its keyboard-opened menu closes on %s", async (key) => {
  const writeText = vi.fn(async () => {});
  const container = render([fileNode("a.ts", false)], { clipboard: { writeText } });
  const { actions, item } = openActionsFromKeyboard(container.querySelector<HTMLElement>("[role=row]")!);
  press(item, key);
  await vi.waitFor(() => expect(document.activeElement).toBe(actions));
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(writeText).toHaveBeenCalledTimes(key === "Enter" ? 1 : 0);
});

it("does not pull focus to a removed row's Actions button as a keyboard-opened menu closes", async () => {
  let removeFirst = () => {};
  function Files() {
    const [nodes, setNodes] = useState([fileNode("a.ts", false), fileNode("b.ts", false)]);
    removeFirst = () => setNodes([fileNode("b.ts", false)]);
    return <FileTree nodes={nodes} expanded={new Set()} label="Files" onExpandedChange={() => {}} onOpenFile={() => {}} onRetry={() => {}} treeRef={() => {}} />;
  }
  const container = mountRows(<Files />, { clipboard: { writeText: async () => {} } });
  const { item } = openActionsFromKeyboard(container.querySelector<HTMLElement>(`[role=row][data-key="${rowKey("a.ts")}"]`)!);
  act(() => removeFirst());
  press(item, "Escape");
  await settle();
  expect(document.querySelector("[role=menu]")).toBeNull();
  // The menu went with its row and focus with the menu; the list, which did not hold focus, leaves it.
  expect(document.activeElement).toBe(document.body);
});

it("leaves focus where it was put once a keyboard-opened menu closed", async () => {
  const container = render([fileNode("a.ts", false)], { clipboard: { writeText: async () => {} } });
  const outside = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => outside.remove());
  const { item } = openActionsFromKeyboard(container.querySelector<HTMLElement>("[role=row]")!);
  press(item, "Escape");
  act(() => outside.focus());
  await settle();
  expect(document.activeElement).toBe(outside);
});

it("leaves focus on the row when a pointer press on it dismisses a keyboard-opened menu", async () => {
  const container = render([fileNode("a.ts", false)], { clipboard: { writeText: async () => {} } });
  const row = container.querySelector<HTMLElement>("[role=row]")!;
  openActionsFromKeyboard(row);
  const cell = row.querySelector<HTMLElement>("[role=gridcell]")!;
  act(() => {
    cell.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, pointerId: 1, pointerType: "mouse" }));
    cell.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, pointerId: 1, pointerType: "mouse" }));
    cell.click();
  });
  await settle();
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(document.activeElement).toBe(row);
});

it("gives focus back to the row when a menu opened with the pointer from a focused row closes", async () => {
  const container = render([fileNode("a.ts", false)], { clipboard: { writeText: async () => {} } });
  const row = container.querySelector<HTMLElement>("[role=row]")!;
  const actions = row.querySelector<HTMLButtonElement>("button[aria-haspopup]")!;
  act(() => row.focus());
  act(() => {
    actions.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    actions.click();
  });
  const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
  act(() => item.focus());
  press(item, "Escape");
  await settle();
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(document.activeElement).toBe(row);
});
