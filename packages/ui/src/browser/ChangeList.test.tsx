// @vitest-environment jsdom
import React, { act, useState } from "react";
import { expect, it, vi } from "vitest";
import { LayoutInfo, ListLayout, Rect } from "react-aria-components";

import { isolate } from "../i18n/isolate";
import { ChangeList, changeRowOffset } from "./ChangeList";
import { change, changeViewFixture, committedChange, mountRows } from "./browserTestSupport";
import type { ChangeLayout } from "./changeLayout";
import type { ChangeViewState } from "./changeView";
import { changeItems, commitItems, commitKey, sectionKey } from "./changes";
import { ChangeListLayout, HEADING_HEIGHT } from "./changeSectionHeading";
import { ROW_HEIGHT } from "./treeRow";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const items = changeItems([change("staged", "src/utils/alpha.ts"), change("staged", "src/beta.ts"), change("unstaged", "src/beta.ts"), change("unstaged", "docs/alpha.md")]);

function render(view: Partial<ChangeViewState> = {}, listed: typeof items = items) {
  const container = mountRows(
    <ChangeList items={listed} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={changeViewFixture(view)} />,
    { clipboard: { writeText: async () => {} } },
  );
  return { container };
}
const rows = (container: HTMLElement) => [...container.querySelectorAll("[role=row][data-key]")].map((row) => row.getAttribute("aria-label") ?? row.textContent);
const rowElements = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>("[role=row][data-key]")];
/** A section's heading, by the name it starts with: the control filling its band. */
const heading = (container: HTMLElement, name: string) =>
  [...container.querySelectorAll<HTMLButtonElement>("[data-section-heading]")].find((band) => band.textContent?.startsWith(name))!;
const press = (element: HTMLElement, key: string) =>
  act(() => {
    element.focus();
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true, cancelable: true }));
  });
/** The same key on whatever holds focus now, for a walk of several steps. */
const pressOn = (key: string) => press(document.activeElement as HTMLElement, key);
/** Lets the frames a fold needs to settle pass: the focus react-aria moves and the heading taking
 * it back both run after the renders that take the rows away (`keepHeadingFocus`, two frames after
 * the press). It counts frames rather than waiting a time, which a stall of the event loop can let
 * expire together with the first frame, before the second is even requested. */
const settle = () =>
  act(async () => {
    for (let frame = 0; frame < 2; frame++) await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  });

it("shows every row when nothing narrows them", () => {
  const { container } = render();
  expect(rows(container)).toEqual([
    `${isolate("alpha.ts")}, modified, ${isolate("src/utils")}`,
    `${isolate("beta.ts")}, modified, ${isolate("src")}`,
    `${isolate("alpha.md")}, modified, ${isolate("docs")}`,
    `${isolate("beta.ts")}, modified, ${isolate("src")}`,
  ]);
  expect(container.textContent).toContain("Staged2");
  expect(container.textContent).toContain("Unstaged2");
});

it("narrows a flat list to the file names that match, each section counting what is left", () => {
  const { container } = render({ filter: "ALPHA" });
  expect(rows(container)).toEqual([`${isolate("alpha.ts")}, modified, ${isolate("src/utils")}`, `${isolate("alpha.md")}, modified, ${isolate("docs")}`]);
  expect(container.textContent).toContain("Staged1");
  expect(container.textContent).toContain("Unstaged1");
});

it("says so when no file name matches", () => {
  const { container } = render({ filter: "zzz" });
  expect(rows(container)).toEqual([]);
  expect(container.textContent).toContain("No matching changes");
});

it("announces that nothing matches, and only when something is typed", () => {
  vi.useFakeTimers();
  try {
    const statusText = (container: HTMLElement) => container.querySelector("[role=status]")?.textContent;
    const typed = render({ filter: "zzz" });
    act(() => void vi.advanceTimersByTime(200));
    expect(statusText(typed.container)).toBe("No matching changes");
    const empty = render({ filter: "" }, []);
    act(() => void vi.advanceTimersByTime(200));
    expect(statusText(empty.container)).toBe("");
  } finally {
    vi.useRealTimers();
  }
});

/** A list that folds its own sections: what the Git mode holds for them is kept here, so a press on
 * a heading really takes that section's rows away. */
function renderFolding(layout: ChangeLayout) {
  const fixture = changeViewFixture();
  function Harness(): React.ReactElement {
    const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
    const view: ChangeViewState = { ...fixture, layout, collapsed, onCollapsedChange: setCollapsed };
    return <ChangeList items={items} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={view} />;
  }
  return mountRows(<Harness />, { clipboard: { writeText: async () => {} } });
}

// A heading is the same control in both layouts, while the rows under it are not (changes alone, and
// under directories), so what it does runs on both.
it.each([
  ["flat", [`${isolate("alpha.md")}, modified, ${isolate("docs")}`, `${isolate("beta.ts")}, modified, ${isolate("src")}`]],
  ["tree", ["docs", `${isolate("alpha.md")}, modified`, "src", `${isolate("beta.ts")}, modified`]],
] as const)("%s: folds a section away from its heading and unfolds it from there, the heading keeping its name and count", async (layout, left) => {
  const container = renderFolding(layout);
  const unfolded = rows(container);
  const band = heading(container, "Staged");
  expect(band.getAttribute("aria-expanded")).toBe("true");
  expect(band.classList.contains("groove-t")).toBe(false);
  expect(heading(container, "Unstaged").classList.contains("groove-t")).toBe(true);
  act(() => band.click());
  expect(rows(container)).toEqual(left);
  expect(band.textContent).toBe("Staged2");
  expect(band.getAttribute("aria-expanded")).toBe("false");
  expect(band.classList.contains("groove-t")).toBe(false);
  expect(heading(container, "Unstaged").classList.contains("groove-t")).toBe(false);
  act(() => band.click());
  expect(rows(container)).toEqual(unfolded);
  expect(heading(container, "Unstaged").classList.contains("groove-t")).toBe(true);

  // A press takes no focus itself, and a row of the section it folds away hands focus to the
  // heading, which stands for it.
  act(() => rowElements(container)[0].focus());
  act(() => band.click());
  await settle();
  expect(document.activeElement).toBe(band);
});

it.each(["flat", "tree"] as const)("%s: reaches a section's heading with Up and Down, and folds the section from there without losing it", async (layout) => {
  const container = renderFolding(layout);
  const unfolded = rows(container);
  const first = rowElements(container)[0];
  const band = heading(container, "Staged");
  act(() => first.focus());
  press(first, "ArrowUp");
  expect(document.activeElement).toBe(band);
  // The walk moves focus itself, so what it lands on has to show its ring. The band says in text
  // what it holds and what pressing it does, so it carries no tooltip; which state it is in is its
  // `aria-expanded`.
  expect(band.dataset.focusVisible).toBe("true");
  expect(band.getAttribute("aria-expanded")).toBe("true");
  pressOn("ArrowDown");
  expect(document.activeElement).toBe(first);

  pressOn("ArrowUp");
  pressOn("Enter");
  await settle();
  // react-aria answers the row that held its focused key going away by moving focus to the nearest
  // row the list has left; the heading the user pressed takes it back.
  expect(document.activeElement).toBe(band);
  expect(rows(container).length).toBeLessThan(unfolded.length);
  expect(band.getAttribute("aria-expanded")).toBe("false");
  // Down from the folded heading is the next section's, the folded one having no row between them.
  pressOn("ArrowDown");
  expect(document.activeElement).toBe(heading(container, "Unstaged"));
  pressOn("ArrowUp");
  expect(document.activeElement).toBe(band);
  pressOn("Enter");
  await settle();
  expect(document.activeElement).toBe(band);
  expect(rows(container)).toEqual(unfolded);
});

it("gives a heading no tab stop of its own, react-aria moving focus landing anywhere but a row to the list's first row", () => {
  const { container } = render();
  const band = heading(container, "Staged");
  expect(band.tabIndex).toBe(-1);
  act(() => band.focus());
  expect(document.activeElement).toBe(rowElements(container)[0]);
});

it("places a row by what comes before it, a folded section counting its heading alone", () => {
  const folded = new Set([sectionKey("staged")]);
  const [firstStaged, , firstUnstaged] = items;
  expect(changeRowOffset(items, firstStaged.key, "flat", folded)).toBeUndefined();
  // Below the two headings alone, without an inset before them.
  expect(changeRowOffset(items, firstUnstaged.key, "flat", folded)).toBe(2 * HEADING_HEIGHT);
  expect(changeRowOffset(items, firstUnstaged.key, "flat", new Set())).toBe(2 * HEADING_HEIGHT + 2 * ROW_HEIGHT);
});

it("pins render copies within sections without changing the geometry used for row navigation", () => {
  const section = new LayoutInfo("section", "staged", new Rect(0, 62, 300, 510));
  const header = new LayoutInfo("header", "header-staged", new Rect(0, 62, 300, HEADING_HEIGHT));
  const row = new LayoutInfo("item", "row", new Rect(0, 92, 300, ROW_HEIGHT));
  const visible = vi.spyOn(ListLayout.prototype, "getVisibleLayoutInfos").mockReturnValue([section, header, row]);
  try {
    const rendered = new ChangeListLayout().getVisibleLayoutInfos(new Rect(0, 100, 300, 200));
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

const byCommit = [
  ...items,
  ...commitItems([
    { commit: "3333333333", parent: "2222222222", subject: "Rework the parser", changes: [committedChange("src/parse.ts")] },
    { commit: "2222222222", parent: "1111111111", subject: "Add the lexer", changes: [committedChange("src/lex.ts"), committedChange("docs/lex.md")] },
  ]),
];

it("narrows the commits too: one with no match has no row, and every count follows what is left", () => {
  const container = mountRows(
    <ChangeList items={byCommit} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={changeViewFixture({ filter: "lex" })} />,
    { clipboard: { writeText: async () => {} } },
  );
  const commits = [...container.querySelectorAll("[role=row]")].filter((row) => row.getAttribute("data-key")?.startsWith("row:commit:")).map((row) => row.textContent);
  expect(commits).toEqual(["2222222Add the lexer2"]);
  expect(container.textContent).toContain("Committed2");
});

it("places a row below the commit rows before it, a folded commit contributing its own row alone", () => {
  const lex = byCommit.find((item) => item.path === "src/lex.ts")!;
  // Under three headings and the four uncommitted rows come the newer commit's row and its change,
  // the older commit's row and its two changes, `src/lex.ts` the second.
  const above = 3 * HEADING_HEIGHT + 4 * ROW_HEIGHT;
  expect(changeRowOffset(byCommit, lex.key, "flat", new Set())).toBe(above + 4 * ROW_HEIGHT);
  expect(changeRowOffset(byCommit, lex.key, "flat", new Set([commitKey("3333333333")]))).toBe(above + 3 * ROW_HEIGHT);
  expect(changeRowOffset(byCommit, lex.key, "flat", new Set([commitKey("2222222222")]))).toBeUndefined();
});
