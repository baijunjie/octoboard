// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { FadeOverflow } from "./FadeOverflow";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

const tooltip = () => document.querySelector('[role="tooltip"]');

/**
 * Mounts a label `clipped` or not and returns a way to move the pointer onto it. jsdom lays nothing
 * out, so the overflow the label measures itself by is stubbed; the first measurement is taken
 * before the first paint, so no frame has to be run for it.
 */
function mount(
  props: React.ComponentProps<typeof FadeOverflow>,
  clipped: boolean,
): { container: HTMLElement; label: Element; enter: () => void; settle: () => void; finish: () => void } {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  for (const [name, value] of [
    ["scrollWidth", clipped ? 200 : 0],
    ["clientWidth", 0],
  ] as const) {
    const own = Object.getOwnPropertyDescriptor(HTMLElement.prototype, name);
    Object.defineProperty(HTMLElement.prototype, name, { configurable: true, get: () => value });
    onTestFinished(() => void (own ? Object.defineProperty(HTMLElement.prototype, name, own) : undefined));
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<FadeOverflow {...props} />));
  const label = container.firstElementChild!;
  const settle = () => act(() => void vi.runOnlyPendingTimers());
  return {
    container,
    label,
    // React raises `onPointerEnter` from `pointerover`.
    enter: () => act(() => void label.dispatchEvent(new MouseEvent("pointerover", { bubbles: true }))),
    settle,
    // Takes the pointer off and shows that nothing is left counting, the tooltip's cooldown included.
    finish: () => {
      act(() => void label.dispatchEvent(new MouseEvent("pointerout", { bubbles: true })));
      settle();
      expect(vi.getTimerCount()).toBe(0);
    },
  };
}

it("gives a clipped label the whole of its text as a tooltip while the pointer is on it", () => {
  const { enter, settle, finish } = mount({ titleWhenClipped: "a-very-long-branch-name", children: "a-very-long-branch-name" }, true);
  expect(tooltip()).toBeNull();
  enter();
  settle();
  expect(tooltip()?.textContent).toBe("a-very-long-branch-name");
  finish();
});

it("gives a label that fits no tooltip repeating what it already shows", () => {
  const { enter, settle, finish } = mount({ titleWhenClipped: "short", children: "short" }, false);
  enter();
  settle();
  expect(tooltip()).toBeNull();
  finish();
});

it("gives text that stands for something longer its tooltip even while it fits", () => {
  const { enter, settle, finish } = mount({ title: "/Users/someone/work/site", children: "~/work/site" }, false);
  enter();
  settle();
  expect(tooltip()?.textContent).toBe("/Users/someone/work/site");
  finish();
});

it("keeps its tooltip while its own text scrolls, as the marquee does, and loses it to the box around it scrolling", () => {
  const { container, label, enter, settle, finish } = mount({ titleWhenClipped: "a-very-long-branch-name", children: "a-very-long-branch-name" }, true);
  enter();
  settle();
  expect(tooltip()).not.toBeNull();

  // react-aria closes a tooltip on a scroll of anything containing its anchor, so the anchor must
  // be the box around the text and not the text.
  act(() => void label.dispatchEvent(new Event("scroll")));
  settle();
  expect(tooltip()).not.toBeNull();

  act(() => void container.dispatchEvent(new Event("scroll")));
  expect(tooltip()).toBeNull();
  finish();
});
