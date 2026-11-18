// @vitest-environment jsdom
import React, { act, useState } from "react";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { FIRST_WRITE_DELAY_MS } from "../../components/announce/StatusAnnouncer";
import { isolate } from "../../i18n/isolate";
import { ChangeList } from "./ChangeList";
import { changeRowOffset } from "./changeListLayout";
import { change, changeViewFixture, committedChange, mountRows, sizedElements } from "../browserTestSupport";
import type { ChangeLayout } from "./changeLayout";
import type { ChangeViewState } from "./changeView";
import { changeItems, commitItems, commitKey, commitScope, directoryKey, sectionKey } from "./changes";
import { HEADING_HEIGHT } from "./changeSectionHeading";
import { ROW_HEIGHT } from "../treeRow";
import { settleFrames } from "../../components/dialog/dialogTestSupport";
import { fakeTimers, runOutTimers } from "../../test/clock";
import { focusAtRemoval } from "../../test/focusAtRemoval";
import { useFocusHandoff } from "../../components/focus/useFocusHandoff";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => fakeTimers());

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
 * it back both run after the renders that take the rows away (`keepHeadingFocus`). */
const settle = () => settleFrames(2);

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
  const statusText = (container: HTMLElement) => container.querySelector("[role=status]")?.textContent;
  const typed = render({ filter: "zzz" });
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  expect(statusText(typed.container)).toBe("No matching changes");
  const empty = render({ filter: "" }, []);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  expect(statusText(empty.container)).toBe("");
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

it("follows the count of committed changes with the count of their commits while grouped by commit", () => {
  const committedText = (grouped: boolean, items: typeof byCommit) =>
    heading(mountRows(<ChangeList items={items} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={changeViewFixture({ byCommit: { value: grouped, onChange: () => {} } })} />), "Committed").textContent;
  expect(committedText(true, byCommit)).toBe("Committed3 (2)");
  expect(committedText(false, byCommit)).toBe("Committed3");
  // Grouped, with no commit listed yet (loading, or failed), there is no "(0)".
  expect(committedText(true, items)).toBe("Committed0");
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

it("groups from the Committed heading and folds its commits together without changing folder or section folds", async () => {
  const commits = commitItems([
    { commit: "3333333333", parent: "2222222222", subject: "Parser", changes: [committedChange("src/parse.ts")] },
    { commit: "2222222222", parent: "1111111111", subject: "Lexer", changes: [committedChange("src/lex.ts")] },
  ]);
  const untouched = directoryKey(commitScope("3333333333"), "src");
  const view = changeViewFixture();
  function Host() {
    const [grouped, setGrouped] = useState(false);
    const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set([untouched]));
    return <ChangeList items={grouped ? commits : changeItems([committedChange("src/parse.ts")])} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={{ ...view, layout: "tree", collapsed, onCollapsedChange: setCollapsed, byCommit: { value: grouped, onChange: setGrouped } }} />;
  }
  const container = mountRows(<Host />);
  const button = (label: string) => container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
  expect(button("Group by commit").closest("[role=row]")).toBe(heading(container, "Committed").closest("[role=row]"));
  expect(button("Collapse all commits")).toBeNull();
  act(() => button("Group by commit").click());
  await settle();
  const band = heading(container, "Committed");
  const firstCommit = container.querySelector<HTMLElement>(`[data-key="${CSS.escape(commitKey("3333333333"))}"]`)!;
  act(() => firstCommit.focus());
  press(firstCommit, "ArrowUp");
  expect(document.activeElement).toBe(band);
  pressOn("ArrowRight");
  expect(document.activeElement).toBe(button("Collapse all commits"));
  pressOn("ArrowRight");
  expect(document.activeElement).toBe(button("Show commits together"));
  pressOn("ArrowLeft");
  expect(document.activeElement).toBe(button("Collapse all commits"));
  pressOn("Enter");
  await settle();
  expect(document.activeElement).toBe(button("Expand all commits"));
  for (const hash of ["3333333333", "2222222222"]) expect(container.querySelector(`[data-key="${CSS.escape(commitKey(hash))}"]`)?.getAttribute("aria-expanded")).toBe("false");

  act(() => button("Expand all commits").click());
  expect(container.textContent).not.toContain("parse.ts");
  expect(container.textContent).toContain("lex.ts");
  expect(container.querySelector(`[data-key="${CSS.escape(untouched)}"]`)?.getAttribute("aria-expanded")).toBe("false");
  expect(band.getAttribute("aria-expanded")).toBe("true");
  act(() => button("Show commits together").click());
  expect(button("Expand all commits")).toBeNull();
  expect(button("Collapse all commits")).toBeNull();
});

describe("Collapse all commits and focus", () => {
  const commits = commitItems([
    { commit: "3333333333", parent: "2222222222", subject: "Parser", changes: [committedChange("src/nested/deep/parse.ts"), committedChange("src/top.ts"), committedChange("root.ts")] },
    { commit: "2222222222", parent: "1111111111", subject: "Lexer", changes: [committedChange("src/lex.ts")] },
  ]);
  const listed = [...changeItems([change("staged", "src/staged.ts")]), ...commits];
  const keyOf = (path: string) => listed.find((item) => item.path === path)!.key;

  /** With `handoff`, the list sits in a region that hands focus on to a control beside it when the element that held it
   * is removed, as the git view does with its branch selector (`useFocusHandoff`). `changes` replaces the changes listed. */
  function mountGrouped(layout: ChangeLayout, { handoff = false, changes = listed } = {}) {
    const view = changeViewFixture();
    function Host() {
      const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
      const selector = React.useRef<HTMLButtonElement>(null);
      const region = useFocusHandoff(() => selector.current?.focus());
      const list = <ChangeList items={changes} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={{ ...view, layout, collapsed, onCollapsedChange: setCollapsed, byCommit: { value: true, onChange: () => {} } }} />;
      if (!handoff) return list;
      return (
        <>
          <button ref={selector} data-testid="selector">Branch</button>
          <div onFocus={region.onFocus} onBlur={region.onBlur}>{list}</div>
        </>
      );
    }
    const container = mountRows(<Host />);
    const row = (key: string) => container.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`);
    const button = (label: string) => container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
    // The press of the pointer, which takes no focus, with the frames the fold's focus move needs.
    const fold = async () => {
      act(() => button("Collapse all commits")!.click());
      await settle();
    };
    const unfold = async () => {
      act(() => button("Expand all commits")!.click());
      await settle();
    };
    return { container, row, button, fold, unfold };
  }

  it.each(["tree", "flat"] as const)("hands focus to the commit row when the fold takes a focused row away, in the %s layout", async (layout) => {
    const { row, button, fold, unfold } = mountGrouped(layout);
    const parser = commitKey("3333333333");
    const lexer = commitKey("2222222222");

    // A change under a commit goes with the fold: focus moves to that commit's row.
    act(() => row(keyOf("src/nested/deep/parse.ts"))!.focus());
    await fold();
    expect(row(keyOf("src/nested/deep/parse.ts"))).toBeNull();
    expect(document.activeElement).toBe(row(parser));
    // Expanding never moves focus.
    await unfold();
    expect(document.activeElement).toBe(row(parser));

    // The commit's other change moves it to the same commit, the other commit's to its own.
    act(() => row(keyOf(layout === "tree" ? "src/top.ts" : "root.ts"))!.focus());
    await fold();
    expect(document.activeElement).toBe(row(parser));
    await unfold();
    act(() => row(keyOf("src/lex.ts"))!.focus());
    await fold();
    expect(document.activeElement).toBe(row(lexer));
    await unfold();

    // A commit row itself stays where it is.
    act(() => row(lexer)!.focus());
    await fold();
    expect(document.activeElement).toBe(row(lexer));
    await unfold();

    // A row of another section is not under a commit, and stays on screen with focus on it.
    act(() => row(keyOf("src/staged.ts"))!.focus());
    await fold();
    expect(document.activeElement).toBe(row(keyOf("src/staged.ts")));
    await unfold();

    // Focus on the button that folds, outside the rows, is not taken.
    act(() => button("Collapse all commits")!.focus());
    await fold();
    expect(document.activeElement).toBe(button("Expand all commits"));
  });

  it.each(["tree", "flat"] as const)("has focus on the commit row before the fold removes the focused row, in the %s layout", async (layout) => {
    const { container, row, button } = mountGrouped(layout);
    const target = row(keyOf(layout === "tree" ? "src/nested/deep/parse.ts" : "root.ts"))!;
    act(() => target.focus());
    const focusWhenRemoved = focusAtRemoval(target);
    // The press takes no focus; what matters is where focus is as the row goes, with no frame or timer in between.
    act(() => button("Collapse all commits")!.click());
    expect(focusWhenRemoved()).toBe(container.querySelector(`[data-key="${CSS.escape(commitKey("3333333333"))}"]`));
    await settle();
    expect(document.activeElement).toBe(row(commitKey("3333333333")));
  });

  it.each(["tree", "flat"] as const)("keeps focus in the list when a region around it hands focus on from a removed element, in the %s layout", async (layout) => {
    const { container, row, fold } = mountGrouped(layout, { handoff: true });
    const selector = container.querySelector<HTMLElement>("[data-testid=selector]")!;
    // Removing the focused row blurs nothing here, as in WebKit: focus falls to `<body>` unannounced.
    act(() => row(keyOf("src/lex.ts"))!.focus());
    await fold();
    expect(row(keyOf("src/lex.ts"))).toBeNull();
    expect(document.activeElement).toBe(row(commitKey("2222222222")));
    expect(document.activeElement).not.toBe(selector);
  });

  it.each(["tree", "flat"] as const)("brings a commit row the list has not rendered into view and focuses it before the fold, in the %s layout", (layout) => {
    // Two commits of thirty changes each, in a list a hundred pixels tall: scrolled to the end, the second commit's row
    // is far above what the virtualizer renders.
    const many = (hash: string, name: string) => ({ commit: hash.repeat(10), parent: null, subject: name, changes: Array.from({ length: 30 }, (_, i) => committedChange(`${name}/f${i}.ts`)) });
    const long = commitItems([many("4", "first"), many("5", "second")]);
    sizedElements({ width: 300, height: 100 });
    const { container, row, button } = mountGrouped(layout, { handoff: true, changes: long });
    const list = container.querySelector<HTMLElement>("[role=treegrid]")!;
    const second = commitKey("5555555555");
    const last = long.at(-1)!.key;
    // jsdom lays nothing out, so the scroll goes past the end and the virtualizer keeps it to its content.
    act(() => {
      list.scrollTop = 100 * ROW_HEIGHT;
      list.dispatchEvent(new Event("scroll"));
    });
    act(() => row(last)!.focus());
    expect(row(second)).toBeNull();
    act(() => button("Collapse all commits")!.click());
    // Focus is on the commit row as the fold renders, with no frame or timer in between.
    expect(document.activeElement).toBe(row(second));
    runOutTimers();
    expect(document.activeElement).toBe(row(second));
    // Folded, the commit's row is the list's third line: scrolled back up into view, clear of the sticky heading.
    expect(changeRowOffset(long, second, layout, new Set([commitKey("4444444444"), second]))).toBe(HEADING_HEIGHT + ROW_HEIGHT);
    expect(list.scrollTop).toBeLessThanOrEqual(ROW_HEIGHT);
  });

  it("hands focus from a folder row under a commit, at any depth, to that commit's row in the tree layout", async () => {
    const { row, fold, unfold } = mountGrouped("tree");
    for (const path of ["src", "src/nested/deep"]) {
      const folder = row(directoryKey(commitScope("3333333333"), path));
      expect(folder).not.toBeNull();
      act(() => folder!.focus());
      await fold();
      expect(document.activeElement).toBe(row(commitKey("3333333333")));
      await unfold();
    }
  });

  it("leaves focus outside the list where it is", async () => {
    const { row, fold } = mountGrouped("tree");
    const outside = document.body.appendChild(document.createElement("button"));
    onTestFinished(() => outside.remove());
    act(() => outside.focus());
    await fold();
    expect(row(commitKey("3333333333"))!.getAttribute("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(outside);
  });

  it("folds and unfolds every commit and nothing else, whatever focus does", async () => {
    const { row, fold, unfold } = mountGrouped("tree");
    act(() => row(keyOf("src/lex.ts"))!.focus());
    await fold();
    for (const hash of ["3333333333", "2222222222"]) expect(row(commitKey(hash))!.getAttribute("aria-expanded")).toBe("false");
    // The section and the other sections' rows are as they were.
    expect(row(keyOf("src/staged.ts"))).not.toBeNull();
    await unfold();
    for (const hash of ["3333333333", "2222222222"]) expect(row(commitKey(hash))!.getAttribute("aria-expanded")).toBe("true");
    expect(row(keyOf("src/lex.ts"))).not.toBeNull();
  });
});
