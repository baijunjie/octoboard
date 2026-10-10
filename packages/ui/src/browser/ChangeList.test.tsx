// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import type { ChangeEntry } from "../protocol";
import { ChangeList, type ChangeListView } from "./ChangeList";
import { changeItems } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom has no ResizeObserver, which the rows' names (FadeOverflow) need.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

afterEach(() => document.body.replaceChildren());

const change = (group: "staged" | "unstaged", path: string): ChangeEntry => ({
  group,
  old: { state: "present", path, kind: "file", source: { kind: "index", worktree: "w", blob: "b" } },
  new: { state: "present", path, kind: "file", source: { kind: "live", root_id: "r", version: "v" } },
});
const items = changeItems([change("staged", "src/utils/alpha.ts"), change("staged", "src/beta.ts"), change("unstaged", "src/beta.ts"), change("unstaged", "docs/alpha.md")]);

function render(view: Partial<ChangeListView> = {}) {
  const container = document.body.appendChild(document.createElement("div"));
  const onFilterChange = vi.fn();
  const full: ChangeListView = { layout: "flat", collapsed: new Set(), onCollapsedChange: () => {}, filter: "", onFilterChange, ...view };
  act(() => createRoot(container).render(<ChangeList items={items} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={full} />));
  return { container, onFilterChange };
}
const rows = (container: HTMLElement) => [...container.querySelectorAll("[role=row], [role=option]")].map((row) => row.getAttribute("aria-label") ?? row.textContent);
const field = (container: HTMLElement) => container.querySelector<HTMLInputElement>("input")!;

it("shows every row, and the field above them, when nothing is typed", () => {
  const { container } = render();
  expect(field(container)).not.toBeNull();
  expect(field(container).getAttribute("aria-label")).toBe("File name contains");
  expect(rows(container)).toHaveLength(4);
  expect(container.textContent).toContain("Staged2");
  expect(container.textContent).toContain("Unstaged2");
});

it("narrows a flat list to the file names that match, each section counting what is left", () => {
  const { container } = render({ filter: "ALPHA" });
  expect(rows(container)).toEqual(["alpha.ts, modified, src/utils", "alpha.md, modified, docs"]);
  expect(container.textContent).toContain("Staged1");
  expect(container.textContent).toContain("Unstaged1");
});

it("narrows the tree the same way: each section's heading, then only the directories that lead to a match and the matches", () => {
  const { container } = render({ layout: "tree", filter: "alpha" });
  expect(rows(container)).toEqual(["Staged1", "src/utils", expect.stringContaining("alpha.ts"), "Unstaged1", "docs", expect.stringContaining("alpha.md")]);
});

it("says so when no file name matches, and keeps the field", () => {
  for (const layout of ["flat", "tree"] as const) {
    document.body.replaceChildren();
    const { container } = render({ layout, filter: "zzz" });
    expect(rows(container)).toEqual([]);
    expect(container.textContent).toContain("No matching changes");
    expect(field(container).value).toBe("zzz");
  }
});

it("clears the filter with the field's Clear button", () => {
  const { container, onFilterChange } = render({ filter: "alpha" });
  act(() => container.querySelector<HTMLElement>("button")!.click());
  expect(onFilterChange).toHaveBeenLastCalledWith("");
});

it("keeps the field out of the list, so its keys never reach the list's type-ahead or arrows", () => {
  const { container } = render();
  const input = field(container);
  expect(input.closest("[role=listbox], [role=grid], [role=treegrid]")).toBeNull();
  act(() => input.focus());
  act(() => {
    for (const key of ["b", "ArrowDown"]) input.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  });
  expect(container.querySelector("[data-current]")).toBeNull();
  expect(document.activeElement).toBe(input);
});

it("keeps an input method's composition from anything above the field", () => {
  const { container, onFilterChange } = render({ filter: "alpha" });
  const above = vi.fn();
  container.addEventListener("keydown", above);
  const input = field(container);
  act(() => input.focus());
  act(() => void input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, isComposing: true })));
  act(() => void input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, keyCode: 229 })));
  expect(above).not.toHaveBeenCalled();
  expect(onFilterChange).not.toHaveBeenCalled();
  // Outside a composition Escape clears the field, and goes on up.
  act(() => void input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(above).toHaveBeenCalledOnce();
});

it("announces that nothing matches, and only when something is typed", () => {
  vi.useFakeTimers();
  try {
    const statusText = (container: HTMLElement) => container.querySelector("[role=status]")?.textContent;
    const typed = render({ filter: "zzz" });
    act(() => void vi.advanceTimersByTime(200));
    expect(statusText(typed.container)).toBe("No matching changes");
    document.body.replaceChildren();
    const empty = render({ filter: "" });
    act(() => void vi.advanceTimersByTime(200));
    expect(statusText(empty.container)).toBe("");
  } finally {
    vi.useRealTimers();
  }
});
