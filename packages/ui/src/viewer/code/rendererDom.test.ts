// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { codeLineOf, lineStart, scrollTextToLine, scrollToLine } from "./rendererDom";

/** A scrolling region 100px tall holding a library host with a shadow root of numbered lines, as the library draws them,
 * each 20px tall at 20px times its number. */
function frameWithLines(lines: number[]): { frame: HTMLElement; scrolledIntoView: number[] } {
  const frame = document.createElement("div");
  frame.setAttribute("role", "region");
  Object.defineProperty(frame, "clientHeight", { value: 100 });
  frame.getBoundingClientRect = () => ({ top: 0 }) as DOMRect;
  const shadow = frame.appendChild(document.createElement("div")).attachShadow({ mode: "open" });
  const scrolledIntoView: number[] = [];
  for (const line of lines) {
    const element = shadow.appendChild(document.createElement("div"));
    element.setAttribute("data-line", String(line));
    element.getBoundingClientRect = () => ({ top: 20 * line - frame.scrollTop, height: 20 }) as DOMRect;
    element.scrollIntoView = vi.fn(() => void scrolledIntoView.push(line));
  }
  return { frame, scrolledIntoView };
}

// Up and down only: `scrollIntoView` would also scroll the unwrapped code sideways, under the sticky line numbers.
it("scrolls the numbered line to the middle of its region, up and down only, and says whether the frame had it", () => {
  const { frame, scrolledIntoView } = frameWithLines([2, 3]);
  expect(scrollToLine(frame, 3)).toBe(true);
  expect(frame.scrollTop).toBe(60 - (100 - 20) / 2);
  expect(scrolledIntoView).toEqual([]);
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

it("finds the element holding a line's code and not its line number, and nothing for a line not drawn", () => {
  const frame = document.createElement("div");
  const shadow = frame.appendChild(document.createElement("div")).attachShadow({ mode: "open" });
  const gutter = shadow.appendChild(document.createElement("div"));
  gutter.setAttribute("data-gutter", "");
  const number = gutter.appendChild(document.createElement("div"));
  number.setAttribute("data-line", "2");
  const column = shadow.appendChild(document.createElement("div"));
  column.setAttribute("data-content", "");
  const code = column.appendChild(document.createElement("div"));
  code.setAttribute("data-line", "2");
  expect(codeLineOf(frame, 2)).toBe(code);
  expect(codeLineOf(frame, 3)).toBeUndefined();
  expect(codeLineOf(document.createElement("div"), 2)).toBeUndefined();
});

it.each([
  ["a", 1, 0],
  ["one\ntwo\nthree", 1, 0],
  ["one\ntwo\nthree", 2, 4],
  ["one\ntwo\nthree", 3, 8],
  ["one\ntwo\nthree", 4, undefined],
  ["one\n", 2, 4],
  ["one\n", 3, undefined],
  ["", 2, undefined],
] as const)("starts line %2$s of %1$j at %3$s", (text, line, start) => {
  expect(lineStart(text, line)).toBe(start);
});
