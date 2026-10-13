// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { enter, leave, openTooltip, withFakeTimers } from "../components/tooltipTestSupport";
import { isolate } from "../i18n/isolate";
import type { ChangeEntry, ChangeSide } from "../protocol";
import type { StatusKey } from "../viewer/statusMarks";
import { change, changeViewFixture, mountRows } from "./browserTestSupport";
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

function render(layout: "flat" | "tree") {
  const settle = withFakeTimers();
  const container = mountRows(
    <ChangeList
      items={changeItems(cases.map((c) => c.entry))}
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
const rowOf = (container: HTMLElement, name: string) =>
  [...container.querySelectorAll<HTMLElement>("[role=row]")].find((row) => row.getAttribute("aria-label")?.startsWith(isolate(name)))!;
const chipOf = (row: HTMLElement) => row.querySelector<HTMLElement>("[data-slot=chip]")!;
const press = (element: HTMLElement, key: string) =>
  act(() => {
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });

it.each(["flat", "tree"] as const)("%s: names every status a change can have from its letter after the delay, and goes with the pointer", (layout) => {
  const { container, settle } = render(layout);
  expect(openTooltip()).toBeNull();
  for (const { letter, word, name } of cases) {
    const chip = chipOf(rowOf(container, name));
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
  const chip = chipOf(row);
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
  const chip = chipOf(rowOf(container, "modified.ts"));
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
