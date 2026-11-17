// Shared by the tests of the zoomable image and of the views that hold one: jsdom lays nothing out, so the scroller's
// size is what a test says it is, and an image has the natural size a test gives it.
import { act } from "react";
import { onTestFinished, vi } from "vitest";

/** The scroller's box. The scroller keeps 8 px free on each side of the image, so the default 1016 x 516 leaves 1000 x 500. */
export const layout = { width: 1016, height: 516 };

const observers: (() => void)[] = [];

/** Gives the image scrollers (and no other element) the size in `layout`, and replaces `ResizeObserver` with one whose
 * callbacks `resizeScrollers` runs. Call it from `beforeEach`; it restores everything when the test ends. */
export function stubScrollerLayout(): void {
  layout.width = 1016;
  layout.height = 516;
  observers.length = 0;
  const isScroller = (element: Element) => element.classList.contains("viewer-image-scroller");
  // A zoomed image is as large as the size it is given; the group reads it to place a scroll relative to the image.
  const drawn = (element: HTMLElement, side: "width" | "height") => (element instanceof HTMLImageElement ? parseFloat(element.style[side]) || 0 : 0);
  // A scroller holds the image and its padding, and is never smaller than its box: the group reads how far it can scroll.
  const content = (element: Element, side: "width" | "height") => {
    const image = element.querySelector("img");
    return Math.max(layout[side], (image ? parseFloat(image.style[side]) || 0 : 0) + 16);
  };
  const stubs = [
    vi.spyOn(Element.prototype, "scrollWidth", "get").mockImplementation(function (this: Element) { return isScroller(this) ? content(this, "width") : 0; }),
    vi.spyOn(Element.prototype, "scrollHeight", "get").mockImplementation(function (this: Element) { return isScroller(this) ? content(this, "height") : 0; }),
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (this: HTMLElement) { return isScroller(this) ? layout.width : drawn(this, "width"); }),
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) { return isScroller(this) ? layout.height : drawn(this, "height"); }),
    vi.spyOn(Element.prototype, "clientWidth", "get").mockImplementation(function (this: Element) { return isScroller(this) ? layout.width : 0; }),
    vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(function (this: Element) { return isScroller(this) ? layout.height : 0; }),
  ];
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) {
      observers.push(callback);
    }
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  });
  onTestFinished(() => {
    stubs.forEach((stub) => stub.mockRestore());
    vi.unstubAllGlobals();
  });
}

/** Tells every scroller its box changed, as the browser does when the view is resized. */
export function resizeScrollers(): void {
  act(() => observers.forEach((callback) => callback()));
}

/** Has the image decode at a natural size. */
export function load(image: HTMLImageElement, width: number, height: number): void {
  Object.defineProperty(image, "naturalWidth", { configurable: true, value: width });
  Object.defineProperty(image, "naturalHeight", { configurable: true, value: height });
  act(() => void image.dispatchEvent(new Event("load")));
}
