// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import type { PlatformAdapter } from "../platform";
import { ChangeList } from "./ChangeList";
import { change, mountRows } from "./changeListTestSupport";
import { changeItems } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const toasts = { notice: vi.fn(), error: vi.fn() };
beforeEach(() => vi.clearAllMocks());

const items = changeItems([change("unstaged", "src/alpha.ts"), change("unstaged", "beta.ts")]);

function render(layout: "flat" | "tree", clipboard: PlatformAdapter["clipboard"], onOpen = vi.fn()) {
  const container = mountRows(
    <ChangeList
      items={items}
      selected={undefined}
      label="Changes"
      onOpen={onOpen}
      listRef={() => {}}
      changeView={{ layout, collapsed: new Set(), onCollapsedChange: () => {}, filter: "", onFilterChange: () => {} }}
    />,
    { clipboard, toasts },
  );
  return { container, onOpen };
}
const rowOf = (container: HTMLElement, name: string) =>
  [...container.querySelectorAll<HTMLElement>("[role=row]")].find((row) => row.getAttribute("aria-label")?.startsWith(name))!;
const actionsOf = (row: HTMLElement) => row.querySelector<HTMLButtonElement>("button[aria-haspopup]")!;
const press = (element: HTMLElement, key: string) =>
  act(() => {
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });
const flush = () => act(async () => {});

// The rows differ between the two layouts (a grid list and a tree), so these run on both.
it.each(["flat", "tree"] as const)("%s: gives each change row an actions button, named for its change, that the row's arrow keys reach", (layout) => {
  const { container } = render(layout, { writeText: async () => {} });
  const row = rowOf(container, "alpha.ts");
  expect(actionsOf(row).getAttribute("aria-label")).toBe("Actions for file alpha.ts");
  act(() => row.focus());
  press(row, "ArrowRight");
  expect(document.activeElement).toBe(actionsOf(row));
  press(actionsOf(row), "ArrowLeft");
  expect(document.activeElement).toBe(row);
});

it.each(["flat", "tree"] as const)("%s: still opens the change on a row's Enter", (layout) => {
  const { container, onOpen } = render(layout, { writeText: async () => {} });
  const row = rowOf(container, "alpha.ts");
  act(() => row.focus());
  press(row, "Enter");
  expect(onOpen).toHaveBeenCalledTimes(1);
});

it("copies the change's path from the menu and confirms with a toast, without opening the change", async () => {
  const writeText = vi.fn(async () => {});
  const { container, onOpen } = render("flat", { writeText });
  act(() => actionsOf(rowOf(container, "alpha.ts")).click());
  const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
  expect(item.textContent).toBe("Copy path");
  act(() => item.click());
  await flush();
  expect(writeText).toHaveBeenCalledWith("src/alpha.ts");
  expect(toasts.notice).toHaveBeenCalledWith("Path copied");
  expect(onOpen).not.toHaveBeenCalled();
});

it("says so when the clipboard refuses", async () => {
  const { container } = render("flat", { writeText: async () => Promise.reject(new Error("denied")) });
  act(() => actionsOf(rowOf(container, "beta.ts")).click());
  act(() => document.querySelector<HTMLElement>("[role=menuitem]")!.click());
  await flush();
  expect(toasts.notice).not.toHaveBeenCalled();
  expect(toasts.error).toHaveBeenCalledWith("Could not copy the path");
});

it("draws no button where the platform has no clipboard", () => {
  const { container } = render("flat", undefined);
  expect(container.querySelector("button[aria-haspopup]")).toBeNull();
});
