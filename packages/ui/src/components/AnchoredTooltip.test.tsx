// @vitest-environment jsdom
import React, { act, useRef } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AnchoredTooltip } from "./AnchoredTooltip";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tooltip = () => document.querySelector('[role="tooltip"]');

/** The tooltip's anchor inside a region that scrolls, as a row inside a list is. */
function Anchored({ hovered }: { hovered: boolean }): React.ReactElement {
  const anchor = useRef<HTMLSpanElement>(null);
  return (
    <div data-scroller="">
      <span ref={anchor}>a-long-branch-name</span>
      <AnchoredTooltip tooltip="The whole name" anchor={anchor} hovered={hovered} />
    </div>
  );
}

/** Mounts the anchor and returns the container along with a way to move the pointer on and off it.
 * The app's tooltip delay is counted on faked timers, which `settle` runs out; `finish` takes the
 * pointer off and shows that nothing is left counting. */
function mount(): { container: HTMLElement; hover: (hovered: boolean) => void; settle: () => void; finish: () => void } {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const hover = (hovered: boolean) => act(() => root.render(<Anchored hovered={hovered} />));
  hover(false);
  const settle = () => act(() => void vi.runOnlyPendingTimers());
  const finish = () => {
    hover(false);
    settle();
    expect(vi.getTimerCount()).toBe(0);
  };
  return { container, hover, settle, finish };
}

it("renders nothing until the pointer arrives, waits out the tooltip delay, and adds no element of its own", () => {
  const { container, hover, settle, finish } = mount();
  const anchored = container.innerHTML;
  expect(tooltip()).toBeNull();

  hover(true);
  expect(tooltip()).toBeNull();
  settle();
  expect(tooltip()?.textContent).toBe("The whole name");
  // Marked, for the one thing that has to tell it from a tooltip with a trigger element.
  expect(tooltip()?.hasAttribute("data-anchored-tooltip")).toBe(true);
  // The tooltip is portalled, so the anchor's own markup is untouched: no wrapper, no tab stop.
  expect(container.innerHTML).toBe(anchored);

  hover(false);
  expect(tooltip()).toBeNull();
  finish();
});

it("closes when something under the anchor scrolls, and opens again only once the pointer has left and come back", () => {
  const { container, hover, settle, finish } = mount();
  hover(true);
  settle();
  expect(tooltip()).not.toBeNull();

  act(() => void container.querySelector("[data-scroller]")!.dispatchEvent(new Event("scroll")));
  expect(tooltip()).toBeNull();
  settle();
  expect(tooltip()).toBeNull();

  hover(false);
  hover(true);
  settle();
  expect(tooltip()).not.toBeNull();
  finish();
});

it("leaves Escape to whatever is waiting for it while it is open", () => {
  const { container, hover, settle, finish } = mount();
  hover(true);
  settle();
  expect(tooltip()).not.toBeNull();

  // react-aria takes Escape above the React tree while a tooltip with a trigger element of its own
  // is open; this tooltip has none, so the key reaches the surface the pointer happens to be over.
  let reached = false;
  const listen = () => void (reached = true);
  window.addEventListener("keydown", listen);
  onTestFinished(() => window.removeEventListener("keydown", listen));
  act(() => void container.querySelector("span")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(reached).toBe(true);
  finish();
});
