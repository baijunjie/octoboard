// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterAll, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { FadeOverflow, MARQUEE_INITIAL_DELAY, MARQUEE_PAUSE_FAR, MARQUEE_PAUSE_REST, MARQUEE_RETURN_DURATION } from "./FadeOverflow";
import { stubListMotion } from "../motion/listMotionTestSupport";
import { MOVING_AFTER_MS, marqueeRoundMs, marqueeRunMs } from "../motion/marqueeTestSupport";
import { FAKE_FRAME_MS, fakeTimers, runOutTimers, runPendingTimers } from "../../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** How much text a clipped label has past its box in these tests, which is how far the marquee runs. */
const OVERFLOW = 200;

/** How far from rest a label that is moving has come, in px, for a test to take it as moving. */
const MOVED_PX = 20;

/** Frames a test steps through one by one while the marquee runs. */
const FRAMES_OBSERVED = 30;

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
  renders: { count: number };
  overflowBy: (width: number) => void;
  setClientWidth: (width: number) => void;
  enter: () => void;
  settle: () => void;
  finish: () => void;
} {
  fakeTimers({
    beforeDrain: () => {
      root.unmount();
      container.remove();
    },
  });
  // How much text there is past the box. The box width is mutable: a resize that keeps the text
  // clipped is what has to shorten the fade, and the first measurement is taken before the first
  // paint, so no frame has to be run for it.
  let overflow = clipped ? OVERFLOW : 0;
  let boxWidth = clientWidth;
  const written: number[] = [];
  // Counts the commits the label's subtree takes part in, an update that renders the label and bails out included.
  const renders = { count: 0 };
  stubGeometry(
    () => boxWidth + overflow,
    () => boxWidth,
    (value) => written.push(value),
  );
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <React.Profiler id="label" onRender={() => void (renders.count += 1)}>
        <FadeOverflow {...props} />
      </React.Profiler>,
    ),
  );
  const label = container.firstElementChild!;
  const settle = runPendingTimers;
  return {
    container,
    unmount: () => act(() => root.unmount()),
    label,
    written,
    renders,
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
      runOutTimers();
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
  ["a left-to-right label lays its plain-text tooltip out left to right too, as a block of its own", "ltr" as const, "ltr"],
  ["a label of any other direction leaves its tooltip to the window's", "auto" as const, null],
])("%s", (_, dir, tooltipDir) => {
  // Starts with right-to-left characters, which a tooltip left to the window's direction would
  // lay out in the opposite order from the label.
  const name = "مراجعة/feature";
  const { enter, settle, finish } = mount({ dir, clip: "start", titleWhenClipped: name, children: name }, true);
  enter();
  settle();
  const value = tooltip()!.querySelector("[dir]");
  expect(value?.getAttribute("dir") ?? null).toBe(tooltipDir);
  // A block, not a run in the tooltip's own line: under a right-to-left window a run's wrapped
  // lines would align to the right, a left-to-right block's all align to its left.
  if (value) expect(getComputedStyle(value).display).toBe("block");
  finish();
});

it.each([
  ["a monospace left-to-right label's plain-text tooltip is monospace too", "ltr" as const],
  ["a monospace label of any other direction gives its plain-text tooltip its font too", "auto" as const],
])("%s", (_, dir) => {
  // The label's font comes from a class, as `font-mono` gives it, not from anything passed to the
  // label: the tooltip has to find it on the label itself.
  const sheet = document.head.appendChild(document.createElement("style"));
  sheet.textContent = ".mono { font-family: ui-monospace, monospace; }";
  onTestFinished(() => sheet.remove());
  const id = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0";
  const { enter, settle, finish } = mount({ dir, className: "mono", titleWhenClipped: id, children: id }, true);
  enter();
  settle();
  const text = [...tooltip()!.querySelectorAll("*")].find((element) => element.textContent === id && element.children.length === 0);
  expect(text && getComputedStyle(text).fontFamily).toBe("ui-monospace, monospace");
  finish();
});

it("leaves a sentence tooltip, passed as a node, in the tooltip's own font", () => {
  const sheet = document.head.appendChild(document.createElement("style"));
  sheet.textContent = ".mono { font-family: ui-monospace, monospace; }";
  onTestFinished(() => sheet.remove());
  const { enter, settle, finish } = mount({ dir: "ltr", className: "mono", titleWhenClipped: <span>Copy the session ID</span>, children: "0f1e2d3c" }, true);
  enter();
  settle();
  const sentence = [...tooltip()!.querySelectorAll("span")].find((element) => element.textContent === "Copy the session ID");
  expect(sentence && getComputedStyle(sentence).fontFamily).not.toBe("ui-monospace, monospace");
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
  act(() => void vi.advanceTimersByTime(MOVING_AFTER_MS));
  written.length = 0;
  for (let i = 0; i < FRAMES_OBSERVED; i++) act(() => void vi.advanceTimersByTime(FAKE_FRAME_MS));

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
  const rest = clip === "start" ? OVERFLOW : 0;
  const far = clip === "start" ? 0 : OVERFLOW;
  enter();
  act(() => void vi.advanceTimersByTime(MARQUEE_INITIAL_DELAY - FAKE_FRAME_MS / 2));
  expect(label.scrollLeft).toBe(rest);
  act(() => void vi.advanceTimersByTime(MOVING_AFTER_MS - MARQUEE_INITIAL_DELAY + FAKE_FRAME_MS / 2));
  expect(Math.abs(label.scrollLeft - rest)).toBeGreaterThan(1);
  // Follow individual frames to detect arrival independently of the initial hover pause. The run takes
  // `marqueeRunMs`; two frames more allow for the one the run starts a frame late and the one it ends on.
  let elapsed = 0;
  while (Math.abs(label.scrollLeft - far) > 0.5 && elapsed < marqueeRunMs(OVERFLOW) + 2 * FAKE_FRAME_MS) {
    act(() => void vi.advanceTimersByTime(FAKE_FRAME_MS));
    elapsed += FAKE_FRAME_MS;
  }
  expect(Math.abs(label.scrollLeft - far)).toBeLessThanOrEqual(0.5);
  const arrival = label.scrollLeft;
  written.length = 0;
  act(() => void vi.advanceTimersByTime(MARQUEE_PAUSE_FAR - FAKE_FRAME_MS / 2));
  expect(label.scrollLeft).toBe(arrival);
  expect(written).toEqual([]);
  // Half the return has run: the label is still well away from rest.
  act(() => void vi.advanceTimersByTime(MARQUEE_RETURN_DURATION / 2 + FAKE_FRAME_MS / 2));
  expect(Math.abs(label.scrollLeft - rest)).toBeGreaterThan(MOVED_PX);
  act(() => void vi.advanceTimersByTime(MARQUEE_RETURN_DURATION));
  expect(written.some((position) => Math.abs(position - rest) < 0.5)).toBe(true);
  expect(Math.abs(label.scrollLeft - rest)).toBeLessThan(5);
  written.length = 0;
  // The pause at rest began when the return arrived, less than a return ago, so this stays inside it.
  act(() => void vi.advanceTimersByTime(MARQUEE_PAUSE_REST - MARQUEE_RETURN_DURATION));
  expect(label.scrollLeft).toBe(rest);
  // Past its end, and then long enough for the next run to have covered `MOVED_PX` with a margin.
  act(() => void vi.advanceTimersByTime(MARQUEE_RETURN_DURATION + marqueeRunMs(MOVED_PX) * 2));
  expect(Math.abs(label.scrollLeft - rest)).toBeGreaterThan(MOVED_PX);
  expect(written.every((position, i) => i === 0 || (clip === "start" ? position <= written[i - 1]! : position >= written[i - 1]!))).toBe(true);
  act(() => void label.dispatchEvent(new MouseEvent("pointerleave")));
  expect(label.scrollLeft).toBe(rest);
  finish();
});

it("hands a start-clipped label back to the pin when its text comes to fit under the pointer", () => {
  const { label, enter, settle, finish, overflowBy } = mount({ clip: "start", titleWhenClipped: "a/very/long/path", children: "a/very/long/path" }, true);
  enter();
  act(() => void vi.advanceTimersByTime(MOVING_AFTER_MS));

  // The text comes to fit while the pointer stays — a shorter name, or room enough for this one —
  // so the marquee has nothing to run and gives the position up.
  overflowBy(0);
  act(() => void vi.advanceTimersByTime(marqueeRoundMs(OVERFLOW)));

  // Long again, and it is held at its end once more rather than left showing its beginning.
  overflowBy(OVERFLOW);
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
  act(() => void vi.advanceTimersByTime(MOVING_AFTER_MS));
  overflowBy(400);
  act(() => void label.dispatchEvent(new Event("scroll")));
  settle();
  expect(label.scrollLeft).toBe(400);
  written.length = 0;
  act(() => void label.dispatchEvent(new MouseEvent("pointerleave")));
  act(() => void vi.advanceTimersByTime(MARQUEE_RETURN_DURATION * 2));
  settle();
  expect(written).toEqual([]);
  expect(label.scrollLeft).toBe(400);
  finish();
});

it("stops a running marquee at the label's rest when the preference turns to reduce, and does not run it again", () => {
  const motion = stubListMotion();
  const { label, written, enter, finish } = mount({ clip: "end", children: "a long label" }, true);
  enter();
  act(() => void vi.advanceTimersByTime(MOVING_AFTER_MS));
  expect(label.scrollLeft).toBeGreaterThan(0);

  motion.reduce();
  expect(label.scrollLeft).toBe(0);
  written.length = 0;
  act(() => void vi.advanceTimersByTime(marqueeRoundMs(OVERFLOW)));
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

it("does not render the label again when a re-measure finds the same geometry, and still follows the next change", () => {
  const { masks, stop } = recordMaskWrites();
  onTestFinished(stop);
  const { label, renders, settle, setClientWidth, finish } = mount({ titleWhenClipped: "a-very-long-name", children: "a-very-long-name" }, true, 200);
  // A first change of what it measured, so that the label has rendered for a measurement before the unchanged ones:
  // the extra render of an equal set only shows after a processed update, without it React drops the equal set anyway.
  setClientWidth(300);
  act(() => void fireResize(label));
  settle();
  renders.count = 0;

  // A resize report and a scroll that leave the geometry as it was, one after the other and then together.
  act(() => void fireResize(label));
  settle();
  act(() => void label.dispatchEvent(new Event("scroll")));
  settle();
  act(() => {
    fireResize(label);
    label.dispatchEvent(new Event("scroll"));
  });
  settle();
  expect(renders.count).toBe(0);

  // What it skipped must not have left its record of the last measurement behind the label's own: the next real
  // change is still taken up and shown.
  setClientWidth(40);
  act(() => void fireResize(label));
  settle();
  expect(renders.count).toBeGreaterThan(0);
  expect(masks.at(-1)).toContain("20px");
  finish();
});

it("does not render the label again for a width change under half a pixel", () => {
  const { label, renders, settle, setClientWidth, finish } = mount({ titleWhenClipped: "a-very-long-name", children: "a-very-long-name" }, true, 200);
  // The extra render of an equal set only shows after a processed update; without it React drops the equal set anyway.
  setClientWidth(300);
  act(() => void fireResize(label));
  settle();
  renders.count = 0;

  setClientWidth(300.3);
  act(() => void fireResize(label));
  settle();
  expect(renders.count).toBe(0);
  finish();
});

it("updates the fade when a re-measure finds the box narrower, and renders again when the clipping goes", () => {
  const { masks, stop } = recordMaskWrites();
  onTestFinished(stop);
  const { label, renders, settle, setClientWidth, overflowBy, finish } = mount({ titleWhenClipped: "a-very-long-name", children: "a-very-long-name" }, true, 200);
  expect(masks.at(-1)).toContain("24px");
  renders.count = 0;

  // Clipped at its end only, a ramp is at most half the box.
  setClientWidth(40);
  act(() => void fireResize(label));
  settle();
  expect(renders.count).toBeGreaterThan(0);
  expect(masks.at(-1)).toContain("20px");
  expect(masks.at(-1)).not.toContain("24px");

  // The text comes to fit: nothing is clipped any more, which is a change to show too, and the fade is taken off.
  overflowBy(0);
  renders.count = 0;
  act(() => void fireResize(label));
  settle();
  expect(renders.count).toBeGreaterThan(0);
  expect(masks.at(-1)).toBe("");
  finish();
});

it("runs both labels of a hovered row, including one that overflows only after the hover chrome grows", () => {
  fakeTimers();
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
  overflow.set(branch, OVERFLOW);

  act(() => void scope.dispatchEvent(new MouseEvent("pointerenter")));
  act(() => void vi.advanceTimersByTime(MOVING_AFTER_MS));
  expect(branch.scrollLeft).toBeGreaterThan(5);
  expect(name.scrollLeft).toBe(0);

  // The hover chrome has taken the name's room. It starts without the pointer having to leave.
  overflow.set(name, OVERFLOW);
  act(() => void fireResize(name));
  act(() => void vi.advanceTimersByTime(MOVING_AFTER_MS));
  expect(name.scrollLeft).toBeGreaterThan(5);
  expect(branch.scrollLeft).toBeGreaterThan(5);

  // A frame in which the branch happens to fit must not retire it for the rest of the hover.
  overflow.set(branch, 0);
  act(() => void fireResize(branch));
  runPendingTimers();
  expect(branch.scrollLeft).toBeLessThan(1);
  overflow.set(branch, OVERFLOW);
  act(() => void fireResize(branch));
  act(() => void vi.advanceTimersByTime(MOVING_AFTER_MS));
  expect(branch.scrollLeft).toBeGreaterThan(5);

  act(() => void scope.dispatchEvent(new MouseEvent("pointerleave")));
  runOutTimers();
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
