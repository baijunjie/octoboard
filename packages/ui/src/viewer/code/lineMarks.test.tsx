// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { LINE_MARK_HIGHLIGHT } from "../find/findHighlight";
import { clearLineMarks, codeLineMarks, paintLineMarks, textLineMarks, useLineMarkOwner } from "./lineMarks";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom paints no CSS highlights: the registry is a map of the ranges each highlight holds.
class TestHighlight extends Set<Range> {
  priority = 0;
  constructor(...ranges: Range[]) {
    super(ranges);
  }
}
const highlights = new Map<string, TestHighlight>();
vi.stubGlobal("Highlight", TestHighlight);
vi.stubGlobal("CSS", { highlights });
const marked = () => Array.from(highlights.get(LINE_MARK_HIGHLIGHT) ?? [], (range) => range.toString());

afterEach(() => {
  highlights.clear();
  document.body.replaceChildren();
});

/** A range over `text`'s `[start, end)` of a text node, as a surface paints one. */
function rangeOver(text: string, start: number, end: number): Range {
  const node = document.body.appendChild(document.createElement("p")).appendChild(document.createTextNode(text));
  const range = document.createRange();
  range.setStart(node, start);
  range.setEnd(node, end);
  return range;
}

const pre = (text: string): HTMLPreElement => {
  const element = document.body.appendChild(document.createElement("pre"));
  element.textContent = text;
  return element;
};

it("paints every owner's ranges as one highlight under the find bar's, and a surface taking its own off leaves another's", () => {
  const a = {};
  const b = {};
  paintLineMarks(a, [rangeOver("alpha", 0, 5)]);
  paintLineMarks(b, [rangeOver("beta", 0, 4)]);
  expect(marked().sort()).toEqual(["alpha", "beta"]);
  expect(highlights.get(LINE_MARK_HIGHLIGHT)!.priority).toBeLessThan(0);
  clearLineMarks(a);
  expect(marked()).toEqual(["beta"]);
  // Painting again replaces what the owner painted before.
  paintLineMarks(b, [rangeOver("gamma", 0, 5)]);
  expect(marked()).toEqual(["gamma"]);
  clearLineMarks(b);
  expect(highlights.has(LINE_MARK_HIGHLIGHT)).toBe(false);
});

it("takes an owner's marks off by painting none, and does nothing for an owner that painted nothing", () => {
  const a = {};
  paintLineMarks(a, [rangeOver("alpha", 0, 5)]);
  const painted = highlights.get(LINE_MARK_HIGHLIGHT);
  paintLineMarks({}, []);
  clearLineMarks({});
  expect(highlights.get(LINE_MARK_HIGHLIGHT)).toBe(painted);
  paintLineMarks(a, []);
  expect(highlights.has(LINE_MARK_HIGHLIGHT)).toBe(false);
});

it("paints nothing, and fails nowhere, in a browser without CSS highlights", () => {
  vi.stubGlobal("Highlight", undefined);
  onTestFinished(() => void vi.stubGlobal("Highlight", TestHighlight));
  const owner = {};
  expect(() => paintLineMarks(owner, [rangeOver("alpha", 0, 5)])).not.toThrow();
  expect(() => clearLineMarks(owner)).not.toThrow();
  expect(highlights.size).toBe(0);
});

it("takes a surface's marks off the page as it unmounts, and not another's", () => {
  const other = {};
  paintLineMarks(other, [rangeOver("other", 0, 5)]);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  function Surface() {
    paintLineMarks(useLineMarkOwner(), [rangeOver("mine", 0, 4)]);
    return null;
  }
  act(() => root.render(<Surface />));
  expect(marked().sort()).toEqual(["mine", "other"]);
  act(() => root.unmount());
  expect(marked()).toEqual(["other"]);
});

it("marks the ranges on a line of plain text, holding them to the line and leaving the other lines alone", () => {
  const element = pre("first line\nsecond line\nthird");
  const texts = (line: number, marks: { start: number; end: number }[]) => textLineMarks(element, line, marks).map((range) => range.toString());
  expect(texts(2, [{ start: 0, end: 6 }, { start: 7, end: 11 }])).toEqual(["second", "line"]);
  // Past the end of the line it is held to the line, which does not take the newline after it.
  expect(texts(2, [{ start: 7, end: 99 }])).toEqual(["line"]);
  expect(texts(3, [{ start: 2, end: 99 }])).toEqual(["ird"]);
  expect(texts(1, [{ start: -3, end: 5 }])).toEqual(["first"]);
  // Wholly past the end, or empty: nothing is marked.
  expect(texts(1, [{ start: 10, end: 20 }, { start: 11, end: 15 }, { start: 3, end: 3 }])).toEqual([]);
  // A line the text does not have, and no marks.
  expect(texts(4, [{ start: 0, end: 3 }])).toEqual([]);
  expect(texts(2, [])).toEqual([]);
});

it("counts a range's offsets in UTF-16 units on a line of plain text", () => {
  const element = pre("x\n😀 needle");
  // 😀 is two units, so the needle starts at 3.
  expect(textLineMarks(element, 2, [{ start: 3, end: 9 }]).map((range) => range.toString())).toEqual(["needle"]);
});

it("marks nothing in an element that does not hold its text as one text node", () => {
  const element = document.body.appendChild(document.createElement("pre"));
  expect(textLineMarks(element, 1, [{ start: 0, end: 3 }])).toEqual([]);
  element.innerHTML = "<span>abc</span>";
  expect(textLineMarks(element, 1, [{ start: 0, end: 3 }])).toEqual([]);
});

/** A frame holding a library host whose shadow root has one code column of lines, each of the given tokens. */
function frameOf(lines: string[][]): HTMLElement {
  const frame = document.body.appendChild(document.createElement("div"));
  const shadow = frame.appendChild(document.createElement("div")).attachShadow({ mode: "open" });
  const gutter = shadow.appendChild(document.createElement("div"));
  gutter.setAttribute("data-gutter", "");
  const column = shadow.appendChild(document.createElement("div"));
  column.setAttribute("data-content", "");
  lines.forEach((tokens, index) => {
    const numberCell = gutter.appendChild(document.createElement("div"));
    numberCell.setAttribute("data-line", String(index + 1));
    numberCell.textContent = String(index + 1);
    const line = column.appendChild(document.createElement("div"));
    line.setAttribute("data-line", String(index + 1));
    for (const token of tokens) line.appendChild(document.createElement("span")).textContent = token;
  });
  return frame;
}

it("marks the ranges across the tokens of a drawn line, the code and not its line number, and nothing on a line not drawn", () => {
  const frame = frameOf([["const", " one", " = 1;"], ["const", " two", " = 2;"]]);
  const texts = (line: number, marks: { start: number; end: number }[]) => codeLineMarks(frame, line, marks).map((range) => range.toString());
  expect(texts(2, [{ start: 4, end: 10 }, { start: 12, end: 99 }])).toEqual(["t two ", "2;"]);
  expect(texts(2, [{ start: 40, end: 50 }])).toEqual([]);
  expect(texts(3, [{ start: 0, end: 3 }])).toEqual([]);
  expect(codeLineMarks(document.createElement("div"), 1, [{ start: 0, end: 3 }])).toEqual([]);
});
