// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { scrollTextToLine, scrollToLine } from "./rendererDom";

/** A frame holding a library host with a shadow root of numbered lines, as the library draws them. */
function frameWithLines(lines: number[]): { frame: HTMLElement; scrolled: number[] } {
  const frame = document.createElement("div");
  const shadow = frame.appendChild(document.createElement("div")).attachShadow({ mode: "open" });
  const scrolled: number[] = [];
  for (const line of lines) {
    const element = shadow.appendChild(document.createElement("div"));
    element.setAttribute("data-line", String(line));
    element.scrollIntoView = vi.fn(() => void scrolled.push(line));
  }
  return { frame, scrolled };
}

it("scrolls the numbered line into view, and says whether the frame had it", () => {
  const { frame, scrolled } = frameWithLines([2, 3]);
  expect(scrollToLine(frame, 3)).toBe(true);
  expect(scrolled).toEqual([3]);
  expect(scrollToLine(frame, 4)).toBe(false);
  expect(scrollToLine(document.createElement("div"), 3)).toBe(false);
});

it("scrolls plain text so the numbered line is in the middle, and leaves a line it lacks alone", () => {
  const pre = document.createElement("pre");
  pre.textContent = "one\ntwo\nthree";
  Object.defineProperty(pre, "clientHeight", { value: 100 });
  pre.getBoundingClientRect = () => ({ top: 10 }) as DOMRect;
  // Each line is 20px tall, below the element's top.
  vi.spyOn(document, "createRange").mockImplementation(() => {
    const range = new Range();
    range.getBoundingClientRect = () => ({ top: 10 + 20 * (range.startOffset === 0 ? 0 : range.startOffset === 4 ? 1 : 2) + 200 }) as DOMRect;
    return range;
  });
  scrollTextToLine(pre, 3);
  expect(pre.scrollTop).toBe(Math.max(0, 240 - 50));
  pre.scrollTop = 5;
  scrollTextToLine(pre, 9);
  expect(pre.scrollTop).toBe(5);
});
