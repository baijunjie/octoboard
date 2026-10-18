// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { isolate } from "../i18n/isolate";
import { ChangeTree } from "./ChangeTree";
import { HEADING_HEIGHT } from "./changeSectionHeading";
import { ROW_HEIGHT } from "./treeRow";
import { change, committedChange, mountRows } from "./browserTestSupport";
import type { ChangeLayout } from "./changeLayout";
import { changeItems, commitItems, commitKey, commitScope, directoryKey, sectionKey, type ChangeItem } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const items = changeItems([change("staged", "src/utils/kickback/a.ts"), change("staged", "src/b.ts"), change("unstaged", "src/b.ts"), change("unstaged", "top.ts")]);

function render(
  collapsed: ReadonlySet<string>,
  handlers: { onOpen?: () => void; onCollapsedChange?: (next: ReadonlySet<string>) => void } = {},
  rows: ChangeItem[] = items,
  layout: ChangeLayout = "tree",
) {
  return mountRows(
    <ChangeTree
      items={rows}
      selected={items[1].key}
      layout={layout}
      label="Changes"
      onOpen={handlers.onOpen ?? (() => {})}
      listRef={() => {}}
      collapsed={collapsed}
      onCollapsedChange={handlers.onCollapsedChange ?? (() => {})}
    />,
    { clipboard: { writeText: async () => {} } },
  );
}
const labels = (container: HTMLElement) => [...container.querySelectorAll("[role=row]")].map((row) => row.getAttribute("aria-label") ?? row.textContent);

const row = (container: HTMLElement, key: string) => container.querySelector<HTMLElement>(`[role=row][data-key="${CSS.escape(key)}"]`)!;
const press = (element: HTMLElement, key: string) =>
  act(() => {
    element.focus();
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });

it("shows each section with its count and its changes under their directories, a chain of single-child directories as one row", () => {
  const container = render(new Set());
  expect(labels(container)).toEqual([
    "Staged2",
    "src",
    "utils/kickback",
    `${isolate("a.ts")}, modified`,
    `${isolate("b.ts")}, modified, selected`,
    "Unstaged2",
    "src",
    `${isolate("b.ts")}, modified`,
    `${isolate("top.ts")}, modified`,
  ]);
  expect(row(container, directoryKey("staged", "src/utils/kickback")).getAttribute("aria-expanded")).toBe("true");
});

it("drops the rows under a collapsed directory and says so on the directory's row", () => {
  const container = render(new Set([directoryKey("staged", "src/utils/kickback"), directoryKey("unstaged", "src")]));
  expect(labels(container)).toEqual(["Staged2", "src", "utils/kickback", `${isolate("b.ts")}, modified, selected`, "Unstaged2", "src", `${isolate("top.ts")}, modified`]);
  expect(row(container, directoryKey("staged", "src/utils/kickback")).getAttribute("aria-expanded")).toBe("false");
  expect(row(container, directoryKey("unstaged", "src")).getAttribute("aria-expanded")).toBe("false");
});

it("folds a directory on Left, unfolds one on Right and opens a change on Enter, keeping the other view's folded directories", () => {
  const onCollapsedChange = vi.fn();
  const onOpen = vi.fn();
  const other = directoryKey("committed", "docs");
  const container = render(new Set([other, directoryKey("unstaged", "src")]), { onCollapsedChange, onOpen });
  press(row(container, directoryKey("staged", "src/utils/kickback")), "ArrowLeft");
  expect(onCollapsedChange).toHaveBeenLastCalledWith(new Set([other, directoryKey("unstaged", "src"), directoryKey("staged", "src/utils/kickback")]));
  press(row(container, directoryKey("unstaged", "src")), "ArrowRight");
  expect(onCollapsedChange).toHaveBeenLastCalledWith(new Set([other]));
  press(row(container, items[0].key), "Enter");
  expect(onOpen).toHaveBeenCalledWith(items[0]);
});

it("folds a section away from its heading, leaving the directories inside it as they were", () => {
  const onCollapsedChange = vi.fn();
  const container = render(new Set([sectionKey("staged")]), { onCollapsedChange });
  expect(labels(container)).toEqual(["Staged2", "Unstaged2", "src", `${isolate("b.ts")}, modified`, `${isolate("top.ts")}, modified`]);
  // Folding a directory of the section on screen decides that one alone: the folded section's own
  // directories are no rows of this tree, and keep the state they had.
  press(row(container, directoryKey("unstaged", "src")), "ArrowLeft");
  expect(onCollapsedChange).toHaveBeenLastCalledWith(new Set([sectionKey("staged"), directoryKey("unstaged", "src")]));
});

/** Two commits' changes, newest first: the second changed a file in a directory, the first two files. */
const commitRows = commitItems([
  { commit: "3333333333", parent: "2222222222", subject: "Rework the parser", changes: [committedChange("src/parse.ts")] },
  { commit: "2222222222", parent: "1111111111", subject: "Add the lexer", changes: [committedChange("src/lex.ts"), committedChange("src/parse.ts")] },
]);
const asRows = [...items, ...commitRows];

it("nests a row per commit in the committed section, newest first, each counting its changes and holding them one level in", () => {
  for (const layout of ["flat", "tree"] as const) {
    const container = render(new Set(), {}, asRows, layout);
    const rows = [...container.querySelectorAll<HTMLElement>("[role=row]")];
    const commit = rows.filter((row) => row.dataset.key?.startsWith("row:commit:"));
    expect(commit.map((row) => row.textContent)).toEqual(["3333333Rework the parser1", "2222222Add the lexer2"]);
    expect(commit.map((row) => row.getAttribute("aria-expanded"))).toEqual(["true", "true"]);
    // The section's band stays one, counting everything under it.
    expect(rows.filter((row) => row.textContent?.startsWith("Committed")).map((row) => row.textContent)).toEqual(["Committed3"]);
    // A commit is a level under the section, and what it holds one more.
    const level = (row: HTMLElement) => Number(row.getAttribute("aria-level"));
    const changes = rows.filter((row) => row.getAttribute("aria-label")?.includes("modified") && row.dataset.key?.startsWith('["commit:'));
    expect(changes).toHaveLength(3);
    expect(new Set(commit.map(level))).toEqual(new Set([level(commit[0])]));
    for (const row of changes) expect(level(row)).toBeGreaterThan(level(commit[0]));
  }
});

it("folds a commit on Left or Enter, unfolds it on Right, leaving the other commit and the directories as they were", () => {
  const onCollapsedChange = vi.fn();
  const other = directoryKey(commitScope("2222222222"), "src");
  const container = render(new Set([other]), { onCollapsedChange }, asRows, "tree");
  const newer = commitKey("3333333333");
  press(row(container, newer), "ArrowLeft");
  expect(onCollapsedChange).toHaveBeenLastCalledWith(new Set([other, newer]));
  press(row(container, newer), "Enter");
  expect(onCollapsedChange).toHaveBeenLastCalledWith(new Set([other, newer]));
  const folded = render(new Set([newer]), { onCollapsedChange }, asRows, "tree");
  expect(row(folded, newer).getAttribute("aria-expanded")).toBe("false");
  press(row(folded, newer), "ArrowRight");
  expect(onCollapsedChange).toHaveBeenLastCalledWith(new Set());
});

it("leaves a commit as it is on Space, which selects in a list that has no selection, as it does a directory", () => {
  const onCollapsedChange = vi.fn();
  const onOpen = vi.fn();
  const container = render(new Set(), { onCollapsedChange, onOpen }, asRows, "tree");
  press(row(container, commitKey("3333333333")), " ");
  press(row(container, directoryKey("staged", "src/utils/kickback")), " ");
  expect(onCollapsedChange).not.toHaveBeenCalled();
  expect(onOpen).not.toHaveBeenCalled();
});

it("names a commit's row by its short id, its subject and its count, in the order they are drawn", () => {
  const container = render(new Set(), {}, asRows, "tree");
  expect(row(container, commitKey("3333333333")).getAttribute("aria-label")).toBe(`3333333 ${isolate("Rework the parser")} 1`);
  expect(row(container, commitKey("2222222222")).getAttribute("aria-label")).toBe(`2222222 ${isolate("Add the lexer")} 2`);
});

it("drops the rows under a folded commit, and under a folded committed section the commits too, each still counting", () => {
  const newer = commitKey("3333333333");
  const container = render(new Set([newer]), {}, asRows, "flat");
  const names = labels(container);
  expect(names.filter((name) => name.includes("parse.ts"))).toHaveLength(1);
  const sectionFolded = render(new Set([sectionKey("committed")]), {}, asRows, "flat");
  expect(labels(sectionFolded).filter((name) => /^\d{7}/.test(name))).toEqual([]);
  expect(labels(sectionFolded)).toContain("Committed3");
});

/** Commits enough to run well past a short list's visible rectangle, each a row holding two
 * changes, so most of the expanded rows are below the part of the list on screen. */
const longHistory = commitItems(
  Array.from({ length: 8 }, (_, index) => ({
    commit: `${index}`.repeat(10),
    parent: null,
    subject: `Commit ${index}`,
    changes: [committedChange(`src/a${index}.ts`), committedChange(`src/b${index}.ts`)],
  })),
);

/** Gives every element the size `size` holds, as react-aria reads it, so the list virtualizes as it
 * does in a browser rather than rendering every row as it does under test by default. */
function sizedElements(size: { width: number; height: number }) {
  for (const [name, read] of [["clientWidth", () => size.width], ["clientHeight", () => size.height]] as const) {
    Object.defineProperty(HTMLElement.prototype, name, { configurable: true, get: read });
  }
  onTestFinished(() => {
    delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
    delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
  });
}

const scroller = (container: HTMLElement) => container.querySelector<HTMLElement>("[role=treegrid]")!;
const contentHeight = (container: HTMLElement) => parseFloat(scroller(container).querySelector<HTMLElement>(":scope > [role=presentation]")!.style.height);

it("sizes a list longer than its scroller to its rows, commits below the visible part included", () => {
  sizedElements({ width: 300, height: 100 });
  const container = render(new Set(), {}, longHistory, "flat");
  // One heading, and a row for each commit and each of its two changes.
  expect(contentHeight(container)).toBe(HEADING_HEIGHT + 8 * 3 * ROW_HEIGHT);
});

it("keeps a section's heading when the list resizes scrolled past the section's top", () => {
  const size = { width: 300, height: 100 };
  sizedElements(size);
  const container = render(new Set(), {}, longHistory, "flat");
  const list = scroller(container);
  act(() => {
    list.scrollTop = 10 * ROW_HEIGHT;
    list.dispatchEvent(new Event("scroll"));
  });
  size.width = 280;
  act(() => window.dispatchEvent(new Event("resize")));
  expect(labels(container)).toContain("Committed16");
});
