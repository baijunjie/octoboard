// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterAll, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { FadeOverflow } from "./FadeOverflow";
import { stubListMotion } from "./listMotionTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Records every observer, including the one the marquee uses to notice a row's hover chrome
// changing a label's width after the pointer has already entered. A native observer would never
// fire here: jsdom lays nothing out.
const observed = new Map<Element, Set<ResizeObserverCallback>>();
class StubResizeObserver {
  private readonly cb: ResizeObserverCallback;
  constructor(cb: ResizeObserverCallback) {
    this.cb = cb;
  }
  observe(target: Element): void {
    let set = observed.get(target);
    if (!set) observed.set(target, (set = new Set()));
    set.add(this.cb);
  }
  unobserve(target: Element): void {
    observed.get(target)?.delete(this.cb);
  }
  disconnect(): void {
    for (const set of observed.values()) set.delete(this.cb);
  }
}
beforeEach(() => {
  vi.stubGlobal("ResizeObserver", StubResizeObserver);
});
afterAll(() => {
  vi.unstubAllGlobals();
});

function fireResize(target: Element): void {
  const entries = [{ target } as ResizeObserverEntry];
  for (const cb of [...(observed.get(target) ?? [])]) cb(entries, null as unknown as ResizeObserver);
}
// jsdom has no media queries; the marquee asks one whether motion is wanted, and the answer is
// this, set per test.
let reducedMotion = false;
window.matchMedia ??= ((query: string) => ({ matches: reducedMotion, media: query, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;

const tooltip = () => document.querySelector('[role="tooltip"]');

/** Stubs the geometry every label measures by. `scrollWidthOf` and `clientWidthOf` are read on each
 * access, so a test can change them after mount. Restored when the test ends. */
function stubGeometry(
  scrollWidthOf: (element: Element) => number,
  clientWidthOf: (element: Element) => number,
  onScrollLeft: (value: number) => void,
): void {
  for (const [name, value] of [
    [
      "scrollWidth",
      function (this: Element) {
        return scrollWidthOf(this);
      },
    ],
    [
      "clientWidth",
      function (this: Element) {
        return clientWidthOf(this);
      },
    ],
  ] as const) {
    const own = Object.getOwnPropertyDescriptor(Element.prototype, name)!;
    Object.defineProperty(Element.prototype, name, { configurable: true, get: value });
    onTestFinished(() => void Object.defineProperty(Element.prototype, name, own));
  }
  // jsdom keeps `scrollLeft` at 0 whatever is written to it, so the scroll position is backed by a
  // field of its own, per element, and every write to it is kept: what moves the label and what
  // would hold it still are both writes, and only the sequence of them tells the two apart.
  const positions = new WeakMap<HTMLElement, number>();
  const ownScrollLeft = Object.getOwnPropertyDescriptor(Element.prototype, "scrollLeft")!;
  Object.defineProperty(Element.prototype, "scrollLeft", {
    configurable: true,
    get(this: HTMLElement) {
      return positions.get(this) ?? 0;
    },
    set(this: HTMLElement, value: number) {
      positions.set(this, value);
      onScrollLeft(value);
      this.dispatchEvent(new Event("scroll"));
    },
  });
  onTestFinished(() => void Object.defineProperty(Element.prototype, "scrollLeft", ownScrollLeft));
}

/** jsdom drops `mask-image` on assignment, so the value is taken from the write React makes. */
function recordMaskWrites(): { masks: string[]; stop: () => void } {
  const masks: string[] = [];
  const proto = HTMLElement.prototype;
  const own = Object.getOwnPropertyDescriptor(proto, "style");
  if (!own?.get) throw new Error("HTMLElement.style has no getter to wrap");
  Object.defineProperty(proto, "style", {
    configurable: true,
    enumerable: own.enumerable,
    get() {
      const style = own.get!.call(this) as CSSStyleDeclaration;
      return new Proxy(style, {
        set(target, prop, value) {
          if (String(prop).toLowerCase().includes("mask")) masks.push(String(value));
          Reflect.set(target, prop, value);
          return true;
        },
      });
    },
  });
  return {
    masks,
    stop: () => Object.defineProperty(proto, "style", own),
  };
}

/**
 * Mounts a label `clipped` or not and returns a way to move the pointer onto it. jsdom lays nothing
 * out, so the overflow the label measures itself by is stubbed; the first measurement is taken
 * before the first paint, so no frame has to be run for it.
 */
function mount(
  props: React.ComponentProps<typeof FadeOverflow>,
  clipped: boolean,
  clientWidth = 0,
): {
  container: HTMLElement;
  unmount: () => void;
  label: Element;
  written: number[];
  overflowBy: (width: number) => void;
  setClientWidth: (width: number) => void;
  enter: () => void;
  settle: () => void;
  finish: () => void;
} {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  // How much text there is past the box. The box width is mutable: a resize that keeps the text
  // clipped is what has to shorten the fade, and the first measurement is taken before the first
  // paint, so no frame has to be run for it.
  let overflow = clipped ? 200 : 0;
  let boxWidth = clientWidth;
  const written: number[] = [];
  stubGeometry(
    () => boxWidth + overflow,
    () => boxWidth,
    (value) => written.push(value),
  );
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
    unmount: () => act(() => root.unmount()),
    label,
    written,
    // Changes how much text is past the box, as a rename or a change of width would.
    overflowBy: (width: number) => void (overflow = width),
    setClientWidth: (width: number) => void (boxWidth = width),
    // React raises `onPointerEnter` from `pointerover`; the marquee listens for the native
    // `pointerenter`, which does not bubble, so both are raised.
    enter: () =>
      act(() => {
        label.dispatchEvent(new MouseEvent("pointerover", { bubbles: true }));
        label.dispatchEvent(new MouseEvent("pointerenter"));
      }),
    settle,
    // Takes the pointer off and shows that nothing is left counting, the tooltip's cooldown and the
    // marquee's frames included.
    finish: () => {
      act(() => {
        label.dispatchEvent(new MouseEvent("pointerout", { bubbles: true }));
        label.dispatchEvent(new MouseEvent("pointerleave"));
      });
      // Let the tooltip's cooldown and any pending measurement finish.
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

it.each([
  ["a left-to-right label lays its plain-text tooltip out left to right too", "ltr" as const, "ltr"],
  ["a label of any other direction leaves its tooltip to the window's", "auto" as const, null],
])("%s", (_, dir, tooltipDir) => {
  // Starts with right-to-left characters, which a tooltip left to the window's direction would
  // lay out in the opposite order from the label.
  const name = "مراجعة/feature";
  const { enter, settle, finish } = mount({ dir, clip: "start", titleWhenClipped: name, children: name }, true);
  enter();
  settle();
  const value = [...tooltip()!.querySelectorAll("span")].find((span) => span.textContent === name);
  expect(value?.getAttribute("dir") ?? null).toBe(tooltipDir);
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

  // Leaving restores the resting end immediately.
  act(() => void label.dispatchEvent(new MouseEvent("pointerleave")));
  expect(label.scrollLeft).toBe(200);
  settle();
  finish();
});

it.each(["end", "start"] as const)("holds the far edge for three seconds and returns rapidly (%s clipping)", (clip) => {
  const { label, written, enter, finish } = mount({ clip, children: "a long label" }, true);
  const rest = clip === "start" ? 200 : 0;
  const far = clip === "start" ? 0 : 200;
  enter();
  act(() => void vi.advanceTimersByTime(592));
  expect(label.scrollLeft).toBe(rest);
  act(() => void vi.advanceTimersByTime(96));
  expect(Math.abs(label.scrollLeft - rest)).toBeGreaterThan(1);
  // Follow individual frames to detect arrival independently of the initial hover pause.
  let elapsed = 0;
  while (Math.abs(label.scrollLeft - far) > 0.5 && elapsed < 5000) {
    act(() => void vi.advanceTimersByTime(16));
    elapsed += 16;
  }
  expect(elapsed).toBeLessThan(5000);
  const arrival = label.scrollLeft;
  written.length = 0;
  act(() => void vi.advanceTimersByTime(2992));
  expect(label.scrollLeft).toBe(arrival);
  expect(written).toEqual([]);
  act(() => void vi.advanceTimersByTime(112));
  expect(Math.abs(label.scrollLeft - rest)).toBeGreaterThan(20);
  act(() => void vi.advanceTimersByTime(160));
  expect(written.some((position) => Math.abs(position - rest) < 0.5)).toBe(true);
  expect(Math.abs(label.scrollLeft - rest)).toBeLessThan(5);
  written.length = 0;
  act(() => void vi.advanceTimersByTime(2800));
  expect(label.scrollLeft).toBe(rest);
  act(() => void vi.advanceTimersByTime(700));
  expect(Math.abs(label.scrollLeft - rest)).toBeGreaterThan(20);
  expect(written.every((position, i) => i === 0 || (clip === "start" ? position <= written[i - 1]! : position >= written[i - 1]!))).toBe(true);
  act(() => void label.dispatchEvent(new MouseEvent("pointerleave")));
  expect(label.scrollLeft).toBe(rest);
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

it("stops a running marquee at the label's rest when the preference turns to reduce, and does not run it again", () => {
  const motion = stubListMotion();
  const { label, written, enter, finish } = mount({ clip: "end", children: "a long label" }, true);
  enter();
  act(() => void vi.advanceTimersByTime(1200));
  expect(label.scrollLeft).toBeGreaterThan(0);

  motion.reduce();
  expect(label.scrollLeft).toBe(0);
  written.length = 0;
  act(() => void vi.advanceTimersByTime(5000));
  expect(written).toEqual([]);
  expect(label.scrollLeft).toBe(0);
  finish();
});

it("shortens the fade when a clipped label gets narrower and keeps the full ramp while the box is wide", () => {
  const { masks, stop } = recordMaskWrites();
  onTestFinished(stop);
  const { label, settle, setClientWidth, finish } = mount({ titleWhenClipped: "a-very-long-name", children: "a-very-long-name" }, true, 200);
  // Wide enough that both edges fading, which is where a marquee spends its run, still uses 24px.
  expect(masks.at(-1)).toContain("24px");
  act(() => {
    label.scrollLeft = 10;
  });
  settle();
  expect(masks.at(-1)).toContain("24px");

  // Narrower, and still clipped at both edges. The ramps have to shorten or they meet and the text paints blank.
  setClientWidth(40);
  act(() => void fireResize(label));
  settle();
  expect(masks.at(-1)).toContain("13px");
  expect(masks.at(-1)).not.toContain("24px");
  finish();
});

it("runs both labels of a hovered row, including one that overflows only after the hover chrome grows", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const overflow = new WeakMap<Element, number>();
  stubGeometry(
    (element) => 100 + (overflow.get(element) ?? 0),
    () => 100,
    () => {},
  );

  const scope = document.body.appendChild(document.createElement("div"));
  scope.setAttribute("data-marquee-scope", "");
  const root = createRoot(scope);
  onTestFinished(() => {
    act(() => root.unmount());
    scope.remove();
  });
  // The name fits until the row's controls grow in; the branch does not.
  const nameText = "a-project-name";
  const branchText = "feature/a-long-branch";
  act(() =>
    root.render(
      <>
        <FadeOverflow as="span" titleWhenClipped={nameText}>
          {nameText}
        </FadeOverflow>
        <FadeOverflow as="span" titleWhenClipped={branchText}>
          {branchText}
        </FadeOverflow>
      </>,
    ),
  );
  const [name, branch] = Array.from(scope.children) as HTMLElement[];
  overflow.set(branch, 200);

  act(() => void scope.dispatchEvent(new MouseEvent("pointerenter")));
  act(() => void vi.advanceTimersByTime(900));
  expect(branch.scrollLeft).toBeGreaterThan(5);
  expect(name.scrollLeft).toBe(0);

  // The hover chrome has taken the name's room. It starts without the pointer having to leave.
  overflow.set(name, 200);
  act(() => void fireResize(name));
  act(() => void vi.advanceTimersByTime(900));
  expect(name.scrollLeft).toBeGreaterThan(5);
  expect(branch.scrollLeft).toBeGreaterThan(5);

  // A frame in which the branch happens to fit must not retire it for the rest of the hover.
  overflow.set(branch, 0);
  act(() => void fireResize(branch));
  act(() => void vi.advanceTimersByTime(500));
  expect(branch.scrollLeft).toBeLessThan(1);
  overflow.set(branch, 200);
  act(() => void fireResize(branch));
  act(() => void vi.advanceTimersByTime(900));
  expect(branch.scrollLeft).toBeGreaterThan(5);

  act(() => void scope.dispatchEvent(new MouseEvent("pointerleave")));
  act(() => void vi.advanceTimersByTime(5000));
  act(() => void vi.runOnlyPendingTimers());
  expect(vi.getTimerCount()).toBe(0);
});


it("forwards its own measured element and clears an object ref on unmount", () => {
  const ref = React.createRef<HTMLElement>();
  const { container, label, unmount, enter, settle, finish } = mount({ ref, children: "long label", titleWhenClipped: "long label" }, true);
  expect(ref.current).toBe(label);
  enter();
  settle();
  expect(tooltip()?.textContent).toBe("long label");
  finish();
  expect(container.firstElementChild).toBe(label);
  unmount();
  expect(ref.current).toBeNull();
});

it("preserves a forwarded callback ref's cleanup", () => {
  const cleanup = vi.fn();
  const ref = vi.fn((element: HTMLElement | null) => element ? cleanup : undefined);
  const { label, unmount } = mount({ ref, children: "label" }, false);
  expect(ref).toHaveBeenCalledWith(label);
  unmount();
  expect(cleanup).toHaveBeenCalledOnce();
});
