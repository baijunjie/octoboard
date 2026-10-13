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
// jsdom has no media queries; the marquee asks one whether motion is wanted, and the answer is
// this, set per test.
let reducedMotion = false;
window.matchMedia ??= ((query: string) => ({ matches: reducedMotion, media: query, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;

const tooltip = () => document.querySelector('[role="tooltip"]');

/**
 * Mounts a label `clipped` or not and returns a way to move the pointer onto it. jsdom lays nothing
 * out, so the overflow the label measures itself by is stubbed; the first measurement is taken
 * before the first paint, so no frame has to be run for it.
 */
function mount(
  props: React.ComponentProps<typeof FadeOverflow>,
  clipped: boolean,
): {
  container: HTMLElement;
  label: Element;
  written: number[];
  overflowBy: (width: number) => void;
  enter: () => void;
  settle: () => void;
  finish: () => void;
} {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  // How much text there is past the box: the box itself is 0 wide, so the whole of it overflows.
  // The geometry is the `Element` interface's, not `HTMLElement`'s, so that is where it is stubbed
  // and where it is put back.
  let overflow = clipped ? 200 : 0;
  for (const [name, value] of [
    ["scrollWidth", () => overflow],
    ["clientWidth", () => 0],
  ] as const) {
    const own = Object.getOwnPropertyDescriptor(Element.prototype, name)!;
    Object.defineProperty(Element.prototype, name, { configurable: true, get: value });
    onTestFinished(() => void Object.defineProperty(Element.prototype, name, own));
  }
  // jsdom keeps `scrollLeft` at 0 whatever is written to it, so the scroll position is backed by a
  // field of its own, per element, and every write to it is kept: what moves the label and what
  // would hold it still are both writes, and only the sequence of them tells the two apart.
  const positions = new WeakMap<HTMLElement, number>();
  const written: number[] = [];
  const ownScrollLeft = Object.getOwnPropertyDescriptor(Element.prototype, "scrollLeft")!;
  Object.defineProperty(Element.prototype, "scrollLeft", {
    configurable: true,
    get(this: HTMLElement) {
      return positions.get(this) ?? 0;
    },
    set(this: HTMLElement, value: number) {
      positions.set(this, value);
      written.push(value);
      this.dispatchEvent(new Event("scroll"));
    },
  });
  onTestFinished(() => void Object.defineProperty(Element.prototype, "scrollLeft", ownScrollLeft));
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
    written,
    // Changes how much text is past the box, as a rename or a change of width would.
    overflowBy: (width: number) => void (overflow = width),
    // React raises `onPointerEnter` from `pointerover`; the marquee listens for the native
    // `pointerenter`, which does not bubble, so both are raised.
    enter: () =>
      act(() => {
        label.dispatchEvent(new MouseEvent("pointerover", { bubbles: true }));
        label.dispatchEvent(new MouseEvent("pointerenter"));
      }),
    settle,
    // Takes the pointer off and shows that nothing is left counting, the tooltip's cooldown and the
    // marquee's return included.
    finish: () => {
      act(() => {
        label.dispatchEvent(new MouseEvent("pointerout", { bubbles: true }));
        label.dispatchEvent(new MouseEvent("pointerleave"));
      });
      // Long enough for the marquee to scroll back to where the label rests, which is what gives
      // up its last frame.
      act(() => void vi.advanceTimersByTime(5000));
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

it("runs a label clipped at its start back toward its beginning, against the pin that holds it at its end", () => {
  const { label, written, enter, settle, finish } = mount({ clip: "start", titleWhenClipped: "a/very/long/path", children: "a/very/long/path" }, true);
  // It rests pinned to its end, where the clipped start is the scroll position.
  expect(label.scrollLeft).toBe(200);

  enter();
  // Past the pause the label rests in, then frame by frame, with the pin measuring all the while.
  act(() => void vi.advanceTimersByTime(1200));
  written.length = 0;
  for (let i = 0; i < 30; i++) act(() => void vi.advanceTimersByTime(16));

  // Every write moves it further back toward its beginning: while the marquee owns the position
  // the pin stands aside, rather than putting the label back at its end between steps.
  expect(written.length).toBeGreaterThan(10);
  expect(written.every((position, i) => i === 0 || position < written[i - 1]!)).toBe(true);

  // Off the label it scrolls back to the end it rests at, to within the sub-pixel the pin itself
  // leaves alone.
  act(() => void label.dispatchEvent(new MouseEvent("pointerleave")));
  act(() => void vi.advanceTimersByTime(3000));
  settle();
  expect(label.scrollLeft).toBeGreaterThan(199.5);
  finish();
});

it("hands a start-clipped label back to the pin when its text comes to fit under the pointer", () => {
  const { label, enter, settle, finish, overflowBy } = mount({ clip: "start", titleWhenClipped: "a/very/long/path", children: "a/very/long/path" }, true);
  enter();
  act(() => void vi.advanceTimersByTime(1200));

  // The text comes to fit while the pointer stays — a shorter name, or room enough for this one —
  // so the marquee has nothing to run and gives the position up.
  overflowBy(0);
  act(() => void vi.advanceTimersByTime(20000));

  // Long again, and it is held at its end once more rather than left showing its beginning.
  overflowBy(200);
  act(() => void label.dispatchEvent(new Event("scroll")));
  settle();
  expect(label.scrollLeft).toBe(200);
  finish();
});

it("leaves a start-clipped label alone where the system asks for no motion", () => {
  reducedMotion = true;
  onTestFinished(() => void (reducedMotion = false));
  const { label, written, enter, settle, finish, overflowBy } = mount({ clip: "start", titleWhenClipped: "a/very/long/path", children: "a/very/long/path" }, true);
  expect(label.scrollLeft).toBe(200);

  // Neither the hover nor the pointer leaving it moves the text: a return the marquee never left
  // would only be motion where none is wanted, and a fight with the pin over where the text that
  // grew under the pointer now rests.
  enter();
  act(() => void vi.advanceTimersByTime(3000));
  overflowBy(400);
  act(() => void label.dispatchEvent(new Event("scroll")));
  settle();
  expect(label.scrollLeft).toBe(400);
  written.length = 0;
  act(() => void label.dispatchEvent(new MouseEvent("pointerleave")));
  act(() => void vi.advanceTimersByTime(3000));
  settle();
  expect(written).toEqual([]);
  expect(label.scrollLeft).toBe(400);
  finish();
});
