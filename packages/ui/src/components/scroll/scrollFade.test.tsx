// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { FadedScroller, useScrollFade } from "./scrollFade";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The width of the content, which each test moves to say the surface started or stopped scrolling
 * sideways, its height, for whether it has anything to scroll at all, and the thickness of the
 * vertical and horizontal scrollbars the box loses room to (0 for overlay ones). jsdom has no layout,
 * so the measurements the marker, the mask's scrollbar strips and the fade at rest follow are faked
 * from here. */
let contentWidth = 100;
let contentHeight = 100;
let scrollbar = 0;
let scrollbarBlock = 0;
const BOX_WIDTH = 100;
const BOX_HEIGHT = 100;

function Hooked({ wrapped }: { wrapped?: boolean }): React.ReactElement {
  const { className, ...props } = useScrollFade<HTMLPreElement>({ className: wrapped ? "wrap" : "nowrap" });
  return <pre className={className} data-testid="scroller" {...props} />;
}

const scrollers = {
  FadedScroller: () => <FadedScroller data-testid="scroller" />,
  useScrollFade: () => <Hooked />,
};

const ORIGINALS = Object.fromEntries((["scrollWidth", "scrollHeight", "clientWidth", "clientHeight", "offsetWidth", "offsetHeight"] as const)
  .map((key) => [key, Object.getOwnPropertyDescriptor(HTMLElement.prototype, key)]));

/** Fakes the measurements for every element, from `contentWidth`, `contentHeight`, the scrollbars and
 * the box. */
function fakeLayout(): () => void {
  const sizes = {
    scrollWidth: () => contentWidth,
    scrollHeight: () => contentHeight,
    clientWidth: () => BOX_WIDTH,
    clientHeight: () => BOX_HEIGHT,
    offsetWidth: () => BOX_WIDTH + scrollbar,
    offsetHeight: () => BOX_HEIGHT + scrollbarBlock,
  };
  const restore = (Object.keys(sizes) as (keyof typeof sizes)[]).map((key) => {
    // The descriptor jsdom had before any test, so an undo restores it whatever order undos run in.
    const original = ORIGINALS[key];
    Object.defineProperty(HTMLElement.prototype, key, { configurable: true, get: sizes[key] });
    return () => (original ? Object.defineProperty(HTMLElement.prototype, key, original) : delete (HTMLElement.prototype as unknown as Record<string, unknown>)[key]);
  });
  return () => restore.forEach((undo) => undo());
}

function mount(element: React.ReactElement): { scroller: () => HTMLElement; rerender: (next: React.ReactElement) => void } {
  const undoLayout = fakeLayout();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    undoLayout();
  });
  act(() => root.render(element));
  return {
    scroller: () => {
      const el = container.querySelector<HTMLElement>("[data-testid=scroller]");
      if (!el) throw new Error("the scroller did not render");
      return el;
    },
    rerender: (next) => act(() => root.render(next)),
  };
}

const marked = (el: HTMLElement) => el.hasAttribute("data-scrolls-sideways");

// The fade's mask dims a horizontal scrollbar inside its band, so only a scroller that has one is marked.
it.each(Object.entries(scrollers))("%s marks itself when its content is wider than its box, and not when it fits", (_name, Scroller) => {
  contentWidth = 500;
  expect(marked(mount(<Scroller />).scroller())).toBe(true);
  contentWidth = BOX_WIDTH;
  expect(marked(mount(<Scroller />).scroller())).toBe(false);
});

// What makes a reading surface start or stop scrolling sideways often leaves nothing an observer
// would see: the word-wrap choice changes only a class, and a new file is one text node replacing
// another. So the measurement is taken again on every commit, not only on the first.
it("follows a change that no observer reports, such as the word-wrap choice", () => {
  contentWidth = 500;
  const { scroller, rerender } = mount(<Hooked />);
  expect(marked(scroller())).toBe(true);

  // wrapping brings the lines inside the box: the class moves, the DOM does not
  contentWidth = BOX_WIDTH;
  rerender(<Hooked wrapped />);
  expect(marked(scroller())).toBe(false);

  contentWidth = 500;
  rerender(<Hooked />);
  expect(marked(scroller())).toBe(true);
});

/** A surface whose scrolling element React replaces in place, as the viewer's document does on a new
 * subject: the component stays mounted and only the keyed element below it is swapped. */
function Keyed(): React.ReactElement {
  const [subject, setSubject] = useState("first");
  const { className, ...props } = useScrollFade();
  return (
    <>
      <button onClick={() => setSubject("second")}>next</button>
      <div key={subject} className={className} data-testid="scroller" {...props} />
    </>
  );
}

it("follows the scroller to a new element when one is put in its place", () => {
  contentWidth = 500;
  const { scroller } = mount(<Keyed />);
  expect(marked(scroller())).toBe(true);

  contentWidth = BOX_WIDTH;
  act(() => document.querySelector("button")!.click());
  expect(marked(scroller())).toBe(false);
});

// A horizontal fade reserves the bottom strip in the library's own styles, so the marker would add
// nothing there.
it("leaves a horizontal scroller unmarked", () => {
  contentWidth = 500;
  expect(marked(mount(<FadedScroller orientation="horizontal" data-testid="scroller" />).scroller())).toBe(false);
});

// HeroUI reserves a fixed strip for the scrollbar that the mask leaves undimmed; an overlay scrollbar
// takes no room, and a strip kept for it would show the content beside the fade at full strength.
const strips = (el: HTMLElement) => [
  el.style.getPropertyValue("--scroll-shadow-scrollbar-size"),
  el.style.getPropertyValue("--scroll-shadow-scrollbar-block-size"),
];

it.each(Object.entries(scrollers))("%s sizes the mask's scrollbar strips to the scrollbars it has", (_name, Scroller) => {
  contentWidth = BOX_WIDTH;
  [scrollbar, scrollbarBlock] = [0, 0];
  expect(strips(mount(<Scroller />).scroller())).toEqual(["0px", "0px"]);
  [scrollbar, scrollbarBlock] = [15, 11];
  expect(strips(mount(<Scroller />).scroller())).toEqual(["15px", "11px"]);
});

it("sizes a horizontal fade's strip to the horizontal scrollbar", () => {
  contentWidth = BOX_WIDTH;
  [scrollbar, scrollbarBlock] = [15, 0];
  const { scroller, rerender } = mount(<FadedScroller orientation="horizontal" data-testid="scroller" />);
  expect(strips(scroller())[0]).toBe("0px");
  scrollbarBlock = 12;
  rerender(<FadedScroller orientation="horizontal" data-testid="scroller" />);
  expect(strips(scroller())[0]).toBe("12px");
});

// The strip is placed from the border box's edge, so it covers the end border and not the start one.
it("counts the end border into the strip, and not the start one", () => {
  contentWidth = BOX_WIDTH;
  [scrollbar, scrollbarBlock] = [2, 2];
  const { scroller } = mount(<FadedScroller data-testid="scroller" style={{ border: "1px solid" }} />);
  expect(strips(scroller())).toEqual(["1px", "1px"]);
});

/** Lets HeroUI's overflow watch see a change: its observer runs, then the frame it reports from. */
async function settle(): Promise<void> {
  await act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
}

// A scroll timeline that goes inactive keeps its animation at the progress it had last, so content
// that shrinks to fit while scrolled would leave the start edge faded with nothing to scroll.
it.each(Object.entries(scrollers))("%s takes the fade's animation off while it has nothing to scroll", async (_name, Scroller) => {
  contentHeight = 500;
  const { scroller } = mount(<Scroller />);
  await settle();
  expect(scroller().style.animation).toBe("");

  contentHeight = BOX_HEIGHT;
  scroller().append(document.createElement("span"));
  await settle();
  expect(scroller().style.animation).toBe("none");

  contentHeight = 500;
  scroller().append(document.createElement("span"));
  await settle();
  expect(scroller().style.animation).toBe("");
});

// HeroUI's overflow watch stays on the element it was attached to, so one put in its place would
// otherwise keep the old element's rest, and with it no fade at all.
it("gives the fade back to a scroller put in the place of one at rest", async () => {
  contentHeight = BOX_HEIGHT;
  const { scroller } = mount(<Keyed />);
  await settle();
  expect(scroller().style.animation).toBe("none");

  contentHeight = 500;
  act(() => document.querySelector("button")!.click());
  // before the watch has reported on the new element at all
  expect(scroller().style.animation).toBe("");
  await settle();
  expect(scroller().style.animation).toBe("");
});

// The watch moves with the element: one that fits when it takes the old one's place and grows later
// is followed through both.
it("follows a scroller put in the place of an overflowing one as it starts to overflow", async () => {
  contentHeight = 500;
  const { scroller } = mount(<Keyed />);
  await settle();
  expect(scroller().style.animation).toBe("");

  contentHeight = BOX_HEIGHT;
  act(() => document.querySelector("button")!.click());
  await settle();
  expect(scroller().style.animation).toBe("none");

  contentHeight = 500;
  scroller().append(document.createElement("span"));
  await settle();
  expect(scroller().style.animation).toBe("");
});
