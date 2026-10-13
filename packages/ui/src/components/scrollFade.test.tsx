// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { FadedScroller, useScrollFade } from "./scrollFade";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The width of the content, which each test moves to say the surface started or stopped scrolling
 * sideways. jsdom has no layout, so the two measurements the marker follows are faked from here. */
let contentWidth = 100;
const BOX_WIDTH = 100;

function Hooked({ wrapped }: { wrapped?: boolean }): React.ReactElement {
  const { className, ...props } = useScrollFade<HTMLPreElement>({ className: wrapped ? "wrap" : "nowrap" });
  return <pre className={className} data-testid="scroller" {...props} />;
}

const scrollers = {
  FadedScroller: () => <FadedScroller data-testid="scroller" />,
  useScrollFade: () => <Hooked />,
};

/** Fakes the two measurements for every element, from `contentWidth` and `BOX_WIDTH`. */
function fakeLayout(): () => void {
  const sizes = { scrollWidth: () => contentWidth, clientWidth: () => BOX_WIDTH };
  const restore = (["scrollWidth", "clientWidth"] as const).map((key) => {
    const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, key);
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

// A horizontal fade reserves the bottom strip in the library's own styles, so the marker would say
// nothing there, and the work behind it is not worth doing.
it("leaves a horizontal scroller unmarked", () => {
  contentWidth = 500;
  expect(marked(mount(<FadedScroller orientation="horizontal" data-testid="scroller" />).scroller())).toBe(false);
});
