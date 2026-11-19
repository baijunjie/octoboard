// @vitest-environment jsdom
// Where the memory dialog puts focus when an entry's row goes, and where it leaves it to React Aria's restore.
import { act } from "react";
import { beforeEach, expect, it } from "vitest";

import { withModality } from "../../components/dialog/dialogTestSupport";
import { fakeTimers, runPendingTimers } from "../../test/clock";
import { buttons, click, type } from "../../test/dom";
import { deletedEvent, entryOf, renderMemory, settled } from "./memoryTestSupport";

// The modality is set before the clock is faked (tests.md), and by key, so a handoff draws its ring.
beforeEach(() => {
  withModality("keyboard");
  fakeTimers();
});

const button = (label: string) => buttons().find((candidate) => candidate.textContent === label);
// The relative times keep a ticker armed, so the clock is advanced one round of pending timers at a time, never run out.
const settle = async () => {
  await settled();
  runPendingTimers();
};
/** The row whose summary is exactly `summary` (`entryOf` describes an entry "About <id>"). */
const row = (summary: string) =>
  Array.from(document.querySelectorAll("li")).find((item) => Array.from(item.querySelectorAll("span")).some((span) => span.textContent === summary))!;
const rowPress = (summary: string) => row(summary).querySelector<HTMLButtonElement>("button")!;
const trigger = (summary: string) => row(summary).querySelector<HTMLButtonElement>('button[aria-label^="Actions for entry"]')!;
const pressEscape = () => act(() => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
/** Opens the actions menu of the row `summary` and chooses `item`. */
async function chooseFromMenu(summary: string, item: string) {
  await act(async () => {
    trigger(summary).dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger(summary).dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger(summary).click();
  });
  runPendingTimers();
  await click(Array.from(document.querySelectorAll('[role="menuitem"]')).find((candidate) => candidate.textContent?.startsWith(item)));
  runPendingTimers();
}
const entries = () => [entryOf("a", { updated_at: 3_000 }), entryOf("b", { updated_at: 2_000 }), entryOf("c", { updated_at: 1_000 })];

it("hands focus to the next row in the commit that removes the focused one, when a session deletes it", async () => {
  const { broadcast } = renderMemory({ entries: entries() });
  await settle();
  act(() => trigger("About a").focus());
  // No frame runs between the removal and the check: focus never sits on <body>.
  broadcast(deletedEvent(["a"]));
  expect(document.activeElement).toBe(trigger("About b"));
  await act(async () => {});
  expect(trigger("About b").hasAttribute("data-focus-visible")).toBe(true);
});

it("hands focus to the row now in a deleted entry's place, else to the one before, else to Add entry", async () => {
  const { broadcast } = renderMemory({ entries: entries() });
  await settle();
  act(() => trigger("About a").focus());
  broadcast(deletedEvent(["a"]));
  await settle();
  await act(async () => {});
  expect(document.activeElement).toBe(trigger("About b"));
  expect(trigger("About b").hasAttribute("data-focus-visible")).toBe(true);
  act(() => trigger("About c").focus());
  broadcast(deletedEvent(["c"]));
  await settle();
  await act(async () => {});
  expect(document.activeElement).toBe(trigger("About b"));
  broadcast(deletedEvent(["b"]));
  await settle();
  await act(async () => {});
  expect(document.activeElement).toBe(button("Add entry"));
  expect(button("Add entry")!.hasAttribute("data-focus-visible")).toBe(true);
});

it("hands focus to the next row once the confirmation of a deletion from the row's menu closes", async () => {
  const { broadcast } = renderMemory({ entries: entries() });
  await settle();
  await chooseFromMenu("About b", "Delete");
  await settle();
  // The daemon broadcasts the deletion before it answers, while the confirmation is still open.
  broadcast(deletedEvent(["b"]));
  await click(buttons().filter((candidate) => candidate.textContent === "Delete").at(-1));
  await settle();
  await settle();
  await act(async () => {});
  expect(document.activeElement).toBe(trigger("About c"));
});

it("hands focus to Add entry once the confirmation of Delete all closes", async () => {
  const { broadcast } = renderMemory({ entries: entries() });
  await settle();
  await click(button("Delete all"));
  await settle();
  type(document.querySelector<HTMLInputElement>('input[type="text"], input:not([type])')!, "delete");
  broadcast(deletedEvent(["a", "b", "c"]));
  await click(buttons().filter((candidate) => candidate.textContent === "Delete all").at(-1));
  await settle();
  await settle();
  await act(async () => {});
  expect(document.activeElement).toBe(button("Add entry"));
});

it("moves focus from Delete all to Add entry as the last entry goes", async () => {
  const { broadcast } = renderMemory({ entries: [entryOf("a")] });
  await settle();
  act(() => button("Delete all")!.focus());
  broadcast(deletedEvent(["a"]));
  await settle();
  await act(async () => {});
  expect(button("Delete all")!.disabled).toBe(true);
  expect(document.activeElement).toBe(button("Add entry"));
  expect(button("Add entry")!.hasAttribute("data-focus-visible")).toBe(true);
});

it("leaves focus to return to the row an entry's view was opened from when the view closes", async () => {
  renderMemory({ entries: entries() });
  await settle();
  act(() => rowPress("About a").focus());
  await click(rowPress("About a"));
  await settle();
  pressEscape();
  await settle();
  await act(async () => {});
  expect(document.activeElement).toBe(rowPress("About a"));
});

it("leaves focus to return to Add entry when the form it opened closes, though a row's menu button held focus before", async () => {
  renderMemory({ entries: entries() });
  await settle();
  act(() => trigger("About a").focus());
  act(() => button("Add entry")!.focus());
  await click(button("Add entry"));
  await settle();
  pressEscape();
  await settle();
  await act(async () => {});
  expect(document.activeElement).toBe(button("Add entry"));
});
