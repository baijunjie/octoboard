// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { ChangeList, type ChangeListView } from "./ChangeList";
import { change, mountRows } from "./changeListTestSupport";
import { changeItems } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const items = changeItems([change("staged", "src/utils/alpha.ts"), change("staged", "src/beta.ts"), change("unstaged", "src/beta.ts"), change("unstaged", "docs/alpha.md")]);

function render(view: Partial<ChangeListView> = {}) {
  const onFilterChange = vi.fn();
  const full: ChangeListView = { layout: "flat", collapsed: new Set(), onCollapsedChange: () => {}, filter: "", onFilterChange, ...view };
  const container = mountRows(<ChangeList items={items} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={full} />, {
    clipboard: { writeText: async () => {} },
  });
  return { container, onFilterChange };
}
const rows = (container: HTMLElement) => [...container.querySelectorAll("[role=row][data-key]")].map((row) => row.getAttribute("aria-label") ?? row.textContent);
const field = (container: HTMLElement) => container.querySelector<HTMLInputElement>("input")!;

it("shows every row, and the field above them, when nothing is typed", () => {
  const { container } = render();
  expect(field(container)).not.toBeNull();
  expect(field(container).getAttribute("aria-label")).toBe("File name contains");
  expect(rows(container)).toEqual(["alpha.ts, modified, src/utils", "beta.ts, modified, src", "alpha.md, modified, docs", "beta.ts, modified, src"]);
  expect(container.textContent).toContain("Staged2");
  expect(container.textContent).toContain("Unstaged2");
});

it("narrows a flat list to the file names that match, each section counting what is left", () => {
  const { container } = render({ filter: "ALPHA" });
  expect(rows(container)).toEqual(["alpha.ts, modified, src/utils", "alpha.md, modified, docs"]);
  expect(container.textContent).toContain("Staged1");
  expect(container.textContent).toContain("Unstaged1");
});

it("narrows the tree the same way: only the directories that lead to a match and the matches", () => {
  const { container } = render({ layout: "tree", filter: "alpha" });
  expect(rows(container)).toEqual(["src/utils", "alpha.ts, modified", "docs", "alpha.md, modified"]);
  expect(container.textContent).toContain("Staged1");
  expect(container.textContent).toContain("Unstaged1");
});

it("says so when no file name matches, and keeps the field", () => {
  for (const layout of ["flat", "tree"] as const) {
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
  expect(input.closest("[role=grid], [role=treegrid]")).toBeNull();
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
    const empty = render({ filter: "" });
    act(() => void vi.advanceTimersByTime(200));
    expect(statusText(empty.container)).toBe("");
  } finally {
    vi.useRealTimers();
  }
});
