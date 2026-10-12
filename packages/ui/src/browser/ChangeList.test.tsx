// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { ChangeList } from "./ChangeList";
import { change, changeViewFixture, mountRows } from "./browserTestSupport";
import type { ChangeViewState } from "./changeView";
import { changeItems } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const items = changeItems([change("staged", "src/utils/alpha.ts"), change("staged", "src/beta.ts"), change("unstaged", "src/beta.ts"), change("unstaged", "docs/alpha.md")]);

function render(view: Partial<ChangeViewState> = {}) {
  const container = mountRows(
    <ChangeList items={items} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={changeViewFixture(view)} />,
    { clipboard: { writeText: async () => {} } },
  );
  return { container };
}
const rows = (container: HTMLElement) => [...container.querySelectorAll("[role=row][data-key]")].map((row) => row.getAttribute("aria-label") ?? row.textContent);

it("shows every row when nothing narrows them", () => {
  const { container } = render();
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

it("says so when no file name matches", () => {
  for (const layout of ["flat", "tree"] as const) {
    const { container } = render({ layout, filter: "zzz" });
    expect(rows(container)).toEqual([]);
    expect(container.textContent).toContain("No matching changes");
  }
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
