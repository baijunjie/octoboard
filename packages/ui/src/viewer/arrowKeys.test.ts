// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { arrowNavigation } from "./arrowKeys";

afterEach(() => {
  document.body.innerHTML = "";
  document.getSelection()?.removeAllRanges();
});

function viewer(dir = "ltr"): { dialog: HTMLElement; frame: HTMLElement; group: HTMLElement } {
  document.body.innerHTML = `<div role="dialog" dir="${dir}" style="direction:${dir}"><div tabindex="0" id="frame">code</div><div role="radiogroup"><button id="b">Split</button></div></div>`;
  return {
    dialog: document.querySelector<HTMLElement>("[role=dialog]")!,
    frame: document.getElementById("frame")!,
    group: document.getElementById("b")!,
  };
}

function press(target: HTMLElement, key: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  Object.defineProperty(event, "target", { value: target });
  return event;
}

it.each([
  ["Left moves back", "ltr", "ArrowLeft", {}, "previous"],
  ["Right moves on", "ltr", "ArrowRight", {}, "next"],
  ["right-to-left mirrors the keys", "rtl", "ArrowLeft", {}, "next"],
  ["a modifier leaves the key alone", "ltr", "ArrowRight", { shiftKey: true }, undefined],
  ["a composition leaves the key alone", "ltr", "ArrowRight", { isComposing: true }, undefined],
])("%s", (_, dir, key, init, expected) => {
  const { dialog, frame } = viewer(dir);
  expect(arrowNavigation(press(frame, key, init), dialog)).toBe(expected);
});

it("leaves the arrows to a control that uses them, and to a text selection", () => {
  const { dialog, frame, group } = viewer();
  expect(arrowNavigation(press(group, "ArrowRight"), dialog)).toBeUndefined();
  const range = document.createRange();
  range.selectNodeContents(frame);
  document.getSelection()!.addRange(range);
  expect(arrowNavigation(press(frame, "ArrowRight"), dialog)).toBeUndefined();
});

it("leaves the arrows to a selection in the rendered code, which WebKit reports collapsed at its shadow host", () => {
  const { dialog, frame } = viewer();
  // jsdom does not re-scope a shadow-root selection; this is what WebKit's `getSelection()` reports.
  const reported = { type: "Range", isCollapsed: true, anchorNode: frame } as unknown as Selection;
  const spy = vi.spyOn(document, "getSelection").mockReturnValue(reported);
  try {
    expect(arrowNavigation(press(frame, "ArrowRight"), dialog)).toBeUndefined();
  } finally {
    spy.mockRestore();
  }
});
