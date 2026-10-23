// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { enter, leave, openTooltip, withFakeTimers } from "../components/tooltipTestSupport";
import { isolate } from "../i18n/isolate";
import type { ChangeEntry, ChangeSide } from "../protocol";
import type { StatusKey } from "../viewer/statusMarks";
import { change, changeViewFixture, mountRows, rowOf } from "./browserTestSupport";
import { ChangeList } from "./ChangeList";
import { changeItems } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const index = { kind: "index", worktree: "w", blob: "b" } as const;
const side = (path: string, kind: "file" | "symlink" = "file"): ChangeSide => ({ state: "present", path, kind, source: index });
const absent: ChangeSide = { state: "absent" };

/** One change of every status a row can show, each under a folder of its own so the flat list has a
 * detail to hover and each file in a different place, with the word the tooltip should say. */
const cases: { status: StatusKey; letter: string; word: string; name: string; entry: ChangeEntry }[] = [
  { status: "added", letter: "A", word: "Added", name: "added.ts", entry: { group: "staged", old: absent, new: side("d/added.ts") } },
  { status: "untracked", letter: "U", word: "Untracked", name: "untracked.ts", entry: { group: "untracked", old: absent, new: side("d/untracked.ts") } },
  { status: "deleted", letter: "D", word: "Deleted", name: "deleted.ts", entry: { group: "unstaged", old: side("d/deleted.ts"), new: absent } },
  { status: "modified", letter: "M", word: "Modified", name: "modified.ts", entry: change("unstaged", "d/modified.ts") },
  { status: "renamed", letter: "R", word: "Renamed", name: "renamed.ts", entry: { group: "staged", old: side("d/was.ts"), new: side("d/renamed.ts") } },
  { status: "typeChanged", letter: "T", word: "Type changed", name: "typed.ts", entry: { group: "unstaged", old: side("d/typed.ts"), new: side("d/typed.ts", "symlink") } },
  { status: "conflicted", letter: "C", word: "Conflicted", name: "conflicted.ts", entry: { group: "conflicted", path: "d/conflicted.ts", conflict: "both_modified" } },
];

function render(layout: "flat" | "tree", entries: ChangeEntry[] = cases.map((c) => c.entry)) {
  const settle = withFakeTimers();
  const container = mountRows(
    <ChangeList
      items={changeItems(entries)}
      selected={undefined}
      label="Changes"
      onOpen={() => {}}
      listRef={() => {}}
      changeView={changeViewFixture({ layout })}
    />,
    { clipboard: { writeText: async () => {} } },
  );
  // The list's status region delays its first write by one timer.
  settle();
  return { container, settle };
}
const letterOf = (row: HTMLElement) => row.querySelector<HTMLElement>("[data-variant=soft]")!;
const press = (element: HTMLElement, key: string) =>
  act(() => {
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });

it.each(["flat", "tree"] as const)("%s: names every status a change can have from its letter after the delay, and goes with the pointer", (layout) => {
  const { container, settle } = render(layout);
  expect(openTooltip()).toBeNull();
  for (const { letter, word, name } of cases) {
    const chip = letterOf(rowOf(container, name));
    expect(chip.textContent).toBe(letter);
    enter(chip);
    expect(openTooltip()).toBeNull();
    settle();
    expect(openTooltip()?.textContent).toBe(word);
    leave(chip);
    expect(openTooltip()).toBeNull();
    // Out of the cooldown that lets the next tooltip open at once.
    settle();
  }
  expect(vi.getTimerCount()).toBe(0);
});

it("keeps the name and the detail in one fading line, and the line counts and the actions button out of it", () => {
  const { container } = render("flat", [change("unstaged", "d/counted.ts", { added: 3, removed: 1 })]);
  const row = rowOf(container, "counted.ts");
  const line = row.querySelector<HTMLElement>(".overflow-hidden")!;
  expect(line.textContent).toBe("counted.tsd");
  expect(line.querySelector("[dir=ltr][aria-hidden=true]")).toBeNull();
  expect(row.querySelector("[dir=ltr][aria-hidden=true]")?.textContent).toBe("+3-1");
  expect(line.querySelector("button")).toBeNull();
  expect(row.querySelector("button")).not.toBeNull();
});

it.each(["flat", "tree"] as const)("%s: opens nothing from the file name or from the detail", (layout) => {
  const { container, settle } = render(layout);
  const row = rowOf(container, "renamed.ts");
  const regions = [...row.querySelectorAll<HTMLElement>("[dir=auto]")];
  // The name, and the rename's origin beside it.
  expect(regions.length).toBeGreaterThanOrEqual(2);
  for (const region of regions) {
    enter(region);
    settle();
    expect(openTooltip()).toBeNull();
    leave(region);
  }
  settle();
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["flat", "tree"] as const)("%s: leaves the row's name as it was, and the chip out of the accessibility tree", (layout) => {
  const { container, settle } = render(layout);
  const row = rowOf(container, "modified.ts");
  const chip = letterOf(row);
  const name = row.getAttribute("aria-label");
  expect(name).toBe(`${isolate("modified.ts")}, modified${layout === "flat" ? `, ${isolate("d")}` : ""}`);
  expect(chip.getAttribute("aria-hidden")).toBe("true");
  expect(chip.hasAttribute("tabindex")).toBe(false);
  expect(chip.hasAttribute("role")).toBe(false);

  enter(chip);
  settle();
  expect(openTooltip()).not.toBeNull();
  // Neither the row nor the chip is linked to the tooltip: it is for the pointer alone.
  expect(row.getAttribute("aria-label")).toBe(name);
  expect(row.hasAttribute("aria-describedby")).toBe(false);
  expect(chip.hasAttribute("aria-describedby")).toBe(false);
  leave(chip);
  settle();
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["flat", "tree"] as const)("%s: adds no stop to a focused row's arrow keys: the actions button, and back", (layout) => {
  const { container, settle } = render(layout);
  const row = rowOf(container, "modified.ts");
  const actions = row.querySelector<HTMLButtonElement>("button[aria-haspopup]")!;
  // The only focusable thing in the row besides the row itself.
  expect(row.querySelectorAll("button, [tabindex], a[href], input")).toHaveLength(1);
  act(() => row.focus());
  press(row, "ArrowRight");
  expect(document.activeElement).toBe(actions);
  press(actions, "ArrowLeft");
  expect(document.activeElement).toBe(row);
  expect(openTooltip()).toBeNull();
  settle();
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["flat", "tree"] as const)("%s: goes when the list is scrolled, and is not opened again until the pointer has left and come back", (layout) => {
  const { container, settle } = render(layout);
  const chip = letterOf(rowOf(container, "modified.ts"));
  enter(chip);
  settle();
  expect(openTooltip()).not.toBeNull();

  act(() => void container.querySelector("[role=grid], [role=treegrid]")!.dispatchEvent(new Event("scroll")));
  expect(openTooltip()).toBeNull();
  settle();
  expect(openTooltip()).toBeNull();

  leave(chip);
  enter(chip);
  settle();
  expect(openTooltip()?.textContent).toBe("Modified");
  leave(chip);
  settle();
  expect(vi.getTimerCount()).toBe(0);
});

const counted = [
  change("unstaged", "d/both.ts", { added: 1234, removed: 2 }),
  change("unstaged", "d/added.ts", { added: 5, removed: 0 }),
  change("unstaged", "d/removed.ts", { added: 0, removed: 7 }),
  change("unstaged", "d/mode.ts", { added: 0, removed: 0 }),
  change("unstaged", "d/binary.ts"),
];
/** The visible counts at the end of a row: its text and the tones of its two halves. */
const countsOf = (row: HTMLElement) => {
  const counts = row.querySelector<HTMLElement>("[dir=ltr][aria-hidden=true]");
  return counts && { text: counts.textContent, tones: [...counts.children].map((part) => part.className) };
};

it.each(["flat", "tree"] as const)("%s: shows a file's added and removed lines at the end of its row, each in its own tone", (layout) => {
  const { container } = render(layout, counted);
  expect(countsOf(rowOf(container, "both.ts"))).toEqual({ text: "+1,234-2", tones: ["text-(--tint-success-fg)", "text-(--tint-danger-fg)"] });
  expect(countsOf(rowOf(container, "added.ts"))).toEqual({ text: "+5", tones: ["text-(--tint-success-fg)"] });
  expect(countsOf(rowOf(container, "removed.ts"))).toEqual({ text: "-7", tones: ["text-(--tint-danger-fg)"] });
});

it.each(["flat", "tree"] as const)("%s: shows nothing for a file the daemon did not count or that changed no line", (layout) => {
  const { container } = render(layout, counted);
  expect(countsOf(rowOf(container, "binary.ts"))).toBeNull();
  expect(countsOf(rowOf(container, "mode.ts"))).toBeNull();
});

it.each(["flat", "tree"] as const)("%s: says the counts in the row's accessible name, and hides the drawn ones from it", (layout) => {
  const { container } = render(layout, counted);
  const folder = layout === "flat" ? `, ${isolate("d")}` : "";
  expect(rowOf(container, "both.ts").getAttribute("aria-label")).toBe(`${isolate("both.ts")}, modified${folder}, lines added: 1,234, removed: 2`);
  expect(rowOf(container, "binary.ts").getAttribute("aria-label")).toBe(`${isolate("binary.ts")}, modified${folder}`);
});

it("gives a directory row no total", () => {
  const { container } = render("tree", counted);
  const directory = container.querySelector<HTMLElement>("[role=row][aria-expanded]")!;
  expect(directory).not.toBeNull();
  expect(countsOf(directory)).toBeNull();
});

it.each(["flat", "tree"] as const)("%s: keeps the room of the actions button while it is hidden, so the counts do not move", (layout) => {
  const { container } = render(layout, counted);
  const row = rowOf(container, "both.ts");
  expect(row.querySelector("button")).not.toBeNull();
  expect(row.querySelector(".max-w-0")).toBeNull();
});
