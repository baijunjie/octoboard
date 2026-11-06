// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";
import { LayoutInfo, ListLayout, Rect } from "react-aria-components";

import { mount, pressWithMouse } from "./testSupport";
import { StickySectionLayout, Tree, TreeCollection, TreeItem } from "./Tree";

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
