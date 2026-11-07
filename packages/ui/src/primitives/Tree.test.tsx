// @vitest-environment jsdom
import { act, useEffect, useState } from "react";
import { expect, it, onTestFinished, vi } from "vitest";
import { LayoutInfo, ListLayout, Rect } from "react-aria-components";

import { mount, pressWithMouse } from "./testSupport";
import { StickySectionLayout, Tree, TreeCollection, TreeExpandButton, TreeItem } from "./Tree";

const HEADING_HEIGHT = 30;
const ROW_HEIGHT = 32;

it("pins render copies within sections without changing the geometry used for row navigation", () => {
  const section = new LayoutInfo("section", "staged", new Rect(0, 62, 300, 510));
  const header = new LayoutInfo("header", "header-staged", new Rect(0, 62, 300, HEADING_HEIGHT));
  const row = new LayoutInfo("item", "row", new Rect(0, 92, 300, ROW_HEIGHT));
  const visible = vi.spyOn(ListLayout.prototype, "getVisibleLayoutInfos").mockReturnValue([section, header, row]);
  try {
    const rendered = new StickySectionLayout().getVisibleLayoutInfos(new Rect(0, 100, 300, 200));
    expect(rendered[0].allowOverflow).toBe(true);
    expect(rendered[1]).toMatchObject({ isSticky: true, zIndex: 3, rect: { y: 0 } });
    expect(rendered[2]).toBe(row);
    expect(header.rect.y).toBe(62);
    expect(header.isSticky).toBe(false);
    expect(section.allowOverflow).toBe(false);
  } finally {
    visible.mockRestore();
  }
});

it("tells onAction and onExpandedChange the ids of the rows as strings", () => {
  onTestFinished(() => {
    vi.restoreAllMocks();
  });
  // jsdom has no layout, so the virtualizer would see a scroller with no room for a row.
  for (const property of ["clientWidth", "clientHeight", "offsetWidth", "offsetHeight"] as const) {
    vi.spyOn(HTMLElement.prototype, property, "get").mockReturnValue(300);
  }
  const onAction = vi.fn();
  const onExpandedChange = vi.fn();
  const { container } = mount(
    <Tree label="Files" items={[{ id: "dir" }]} expandedKeys={new Set<string>()} onAction={onAction} onExpandedChange={onExpandedChange} rowHeight={32}>
      {(item) => (
        <TreeItem id={item.id} textValue={item.id} content={item.id}>
          <TreeCollection items={[{ id: "file" }]}>
            {(child) => <TreeItem id={child.id} textValue={child.id} content={child.id} />}
          </TreeCollection>
        </TreeItem>
      )}
    </Tree>,
  );
  const row = container.querySelector<HTMLElement>('[role="row"]')!;
  pressWithMouse(row);
  expect(onAction).toHaveBeenCalledWith("dir");
  act(() => {
    row.focus();
    row.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true }));
  });
  expect(onExpandedChange).toHaveBeenCalledWith(new Set(["dir"]));
});

it("keeps the tree's keyboard navigation while cancelling native scrolling and preserving callers and bubbling", () => {
  onTestFinished(() => { vi.restoreAllMocks(); });
  for (const property of ["clientWidth", "clientHeight", "offsetWidth", "offsetHeight"] as const) {
    vi.spyOn(HTMLElement.prototype, property, "get").mockReturnValue(300);
  }
  const caller = vi.fn();
  const ancestor = vi.fn();
  const items = Array.from({ length: 20 }, (_, index) => ({ id: String(index) }));
  const { container } = mount(
    <div onKeyDown={(event) => ancestor(event.defaultPrevented)}>
      <Tree label="Files" items={items} expandedKeys={new Set<string>()} onExpandedChange={() => {}} onAction={() => {}} rowHeight={ROW_HEIGHT}
        onKeyDown={(event) => caller(event.defaultPrevented)}>
        {(item) => <TreeItem id={item.id} textValue={item.id} content={item.id} />}
      </Tree>
    </div>,
  );
  act(() => container.querySelector<HTMLElement>('[role="row"][data-key="0"]')!.focus());
  for (const shiftKey of [false, true]) {
    for (const key of ["End", "Home", "PageDown", "PageUp"]) {
      const before = Number((document.activeElement as HTMLElement).dataset.key);
      const event = new KeyboardEvent("keydown", { key, shiftKey, bubbles: true, cancelable: true });
      act(() => { document.activeElement!.dispatchEvent(event); });
      const after = Number((document.activeElement as HTMLElement).dataset.key);
      if (key === "End") expect(after).toBe(19);
      else if (key === "Home") expect(after).toBe(0);
      else if (key === "PageDown") expect(after).toBeGreaterThan(before);
      else expect(after).toBeLessThan(before);
      expect(event.defaultPrevented).toBe(true);
      expect(caller).toHaveBeenLastCalledWith(true);
      expect(ancestor).toHaveBeenLastCalledWith(true);
    }
  }
  expect(caller).toHaveBeenCalledTimes(8);
  expect(ancestor).toHaveBeenCalledTimes(8);
});

it("leaves modified shortcuts and editable children's caret keys to their existing handlers", () => {
  onTestFinished(() => { vi.restoreAllMocks(); });
  for (const property of ["clientWidth", "clientHeight", "offsetWidth", "offsetHeight"] as const) {
    vi.spyOn(HTMLElement.prototype, property, "get").mockReturnValue(300);
  }
  const caller = vi.fn();
  const { container } = mount(
    <Tree label="Files" items={[{ id: "one" }, { id: "two" }]} expandedKeys={new Set<string>()} onExpandedChange={() => {}} onAction={() => {}} rowHeight={ROW_HEIGHT}
      onKeyDown={(event) => caller(event.defaultPrevented)}>
      {(item) => <TreeItem id={item.id} textValue={item.id} content={<><input aria-label={`Name ${item.id}`} /><textarea aria-label={`Description ${item.id}`} /><span contentEditable suppressContentEditableWarning>Editable</span></>} />}
    </Tree>,
  );
  const row = container.querySelector<HTMLElement>('[role="row"]')!;
  for (const modifier of ["metaKey", "ctrlKey", "altKey"]) {
    for (const key of ["Home", "End", "PageUp", "PageDown"]) {
      const event = new KeyboardEvent("keydown", { key, [modifier]: true, bubbles: true, cancelable: true });
      act(() => { row.dispatchEvent(event); });
      expect(caller).toHaveBeenLastCalledWith(false);
    }
  }
  for (const editable of row.querySelectorAll("input, textarea, [contenteditable]")) {
    for (const key of ["Home", "End", "PageUp", "PageDown"]) {
      const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      act(() => { editable.dispatchEvent(event); });
      expect(caller).toHaveBeenLastCalledWith(false);
    }
  }
});

it("mounts and focuses the last root row after an expanded directory asynchronously gains four hundred rows", async () => {
  onTestFinished(() => { vi.restoreAllMocks(); });
  // Keep a real, finite viewport: jsdom's default zero geometry otherwise bypasses virtualization in tests.
  for (const property of ["clientWidth", "offsetWidth"] as const) vi.spyOn(HTMLElement.prototype, property, "get").mockReturnValue(300);
  for (const property of ["clientHeight", "offsetHeight"] as const) vi.spyOn(HTMLElement.prototype, property, "get").mockReturnValue(160);
  const roots = [{ id: "dir" }, { id: "README.md" }];
  const files = Array.from({ length: 400 }, (_, index) => ({ id: `dir/file-${String(index).padStart(3, "0")}.ts` }));
  const pending = [{ id: "dir/loading" }];
  let finishListing!: () => void;
  const listing = new Promise<void>((resolve) => { finishListing = resolve; });
  function Files() {
    const [expanded, setExpanded] = useState(new Set<string>());
    const [loaded, setLoaded] = useState(false);
    useEffect(() => {
      if (expanded.has("dir")) void listing.then(() => setLoaded(true));
    }, [expanded]);
    return (
      <Tree label="Files" items={roots} expandedKeys={expanded} onExpandedChange={setExpanded} onAction={() => {}} rowHeight={32} dependencies={[loaded]}>
        {(item) => item.id === "dir"
          ? <TreeItem id={item.id} textValue={item.id} expandable content={<><TreeExpandButton label="Expand directory">Toggle</TreeExpandButton>dir</>}>
            <TreeCollection items={loaded ? files : pending}>
              {(file) => <TreeItem id={file.id} textValue={file.id} content={file.id} />}
            </TreeCollection>
          </TreeItem>
          : <TreeItem id={item.id} textValue={item.id} content={item.id} />}
      </Tree>
    );
  }
  const { container } = mount(<Files />);
  const row = (key: string) => container.querySelector<HTMLElement>(`[role="row"][data-key="${key}"]`);
  const press = (key: string) => {
    act(() => {
      document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
      document.activeElement!.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true, cancelable: true }));
    });
  };
  // README starts in the visible layout, before the new child rows move it beyond the viewport.
  expect(row("README.md")).not.toBeNull();
  act(() => row("dir")!.focus());
  press("ArrowRight");
  expect(row("dir")!.getAttribute("aria-expanded")).toBe("true");
  await act(async () => { finishListing(); await listing; });
  await vi.waitFor(() => expect(row("dir/file-000.ts")).not.toBeNull());
  press("Home");
  expect(document.activeElement).toBe(row("dir"));
  press("End");
  await vi.waitFor(() => {
    const readme = row("README.md");
    expect(readme).not.toBeNull();
    expect(readme!.isConnected).toBe(true);
    expect(document.activeElement).toBe(readme);
  });
  // Restoring the offscreen focus target must not mount the directory's whole listing.
  expect(container.querySelectorAll('[role="row"]').length).toBeLessThan(50);
});
