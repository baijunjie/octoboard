// @vitest-environment jsdom
import { act, useLayoutEffect, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { ThemeProvider } from "../preferences/theme";
import { settleLazyImport } from "../test/lazyImport";
import { RENDER_BUDGETS } from "./code/budgets";
import type { LineRange } from "./code/lineMarks";
import { wordWrap } from "./code/wordWrap";
import type { ViewerSubject } from "./content";
import { LINE_MARK_HIGHLIGHT } from "./find/findHighlight";
import { FileViewer } from "./FileViewer";
import { ViewerHost } from "./viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rendering library does not run under jsdom. The stand-in draws the library's markup that the viewer reads, in a
// shadow root: a `[data-code]` scroller holding the sticky `[data-gutter]` and a `[data-content]` column of one
// `[data-line]` per line, its words in token spans and an empty line drawn as a newline. Every drawing replaces the
// markup, as the library's does.
vi.mock("./code/renderer", () => ({
  HighlightedFile: ({ text, wrap, onDrawn }: { text: string; wrap: boolean; onDrawn?: () => void }) => {
    const host = useRef<HTMLDivElement>(null);
    useLayoutEffect(() => {
      const root = host.current!.shadowRoot ?? host.current!.attachShadow({ mode: "open" });
      const code = document.createElement("code");
      code.setAttribute("data-code", "");
      code.appendChild(document.createElement("div")).setAttribute("data-gutter", "");
      const column = code.appendChild(document.createElement("div"));
      column.setAttribute("data-content", "");
      text.replace(/\n$/, "").split("\n").forEach((words, index) => {
        const line = column.appendChild(document.createElement("div"));
        line.setAttribute("data-line", String(index + 1));
        if (words === "") line.textContent = "\n";
        for (const token of words.split(/(\s+)/).filter(Boolean)) line.appendChild(document.createElement("span")).textContent = token;
      });
      root.replaceChildren(code);
      onDrawn?.();
    }, [text, wrap, onDrawn]);
    return <div ref={host} />;
  },
  RenderedDiff: () => null,
  endRendererPool: () => {},
}));

window.matchMedia ??= ((query: string) =>
  ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList);

// jsdom paints no CSS highlights: the registry is a map of the ranges each highlight holds.
class TestHighlight extends Set<Range> {
  priority = 0;
  constructor(...ranges: Range[]) {
    super(ranges);
  }
}
const highlights = new Map<string, TestHighlight>();
(globalThis as { Highlight?: unknown }).Highlight = TestHighlight;
(globalThis as { CSS?: unknown }).CSS = { ...(globalThis as { CSS?: object }).CSS, highlights };
const marked = () => Array.from(highlights.get(LINE_MARK_HIGHLIGHT) ?? [], (range) => range.toString());

// jsdom lays nothing out and scrolls nothing. The layout here: a scrolling region is 300px wide and 100px tall, at the
// viewport's top left; a line is 20px tall and a character 10px wide; rendered code starts beside a 50px gutter, and its
// `[data-code]` scrolls sideways while the region scrolls up and down; plain text scrolls both ways itself. Every element
// keeps the scroll position it is given.
const WIDTH = 300;
const HEIGHT = 100;
const GUTTER = 50;
const scrolls = new WeakMap<Element, { top: number; left: number }>();
const scrollOf = (element: Element) => scrolls.get(element) ?? { top: 0, left: 0 };
for (const axis of ["top", "left"] as const) {
  Object.defineProperty(HTMLElement.prototype, axis === "top" ? "scrollTop" : "scrollLeft", {
    configurable: true,
    get(this: HTMLElement) {
      return scrollOf(this)[axis];
    },
    set(this: HTMLElement, value: number) {
      scrolls.set(this, { ...scrollOf(this), [axis]: value });
    },
  });
}
const isCodeScroller = (element: Element) => element.matches("[data-code]");
const isRegion = (element: Element) => element.matches("[role=region]");
const longestLine = (element: Element) => Math.max(...(element.textContent ?? "").split("\n").map((line) => line.length));
Object.defineProperties(HTMLElement.prototype, {
  clientWidth: { configurable: true, get(this: HTMLElement) { return isCodeScroller(this) || isRegion(this) ? WIDTH : 0; } },
  clientHeight: { configurable: true, get(this: HTMLElement) { return isRegion(this) ? HEIGHT : 0; } },
  scrollWidth: {
    configurable: true,
    get(this: HTMLElement) {
      if (isCodeScroller(this)) return GUTTER + 10 * Math.max(...Array.from(this.querySelectorAll("[data-line]"), longestLine));
      return this.matches("pre") ? 10 * longestLine(this) : 0;
    },
  },
});
/** The region around an element, across its shadow root. */
const regionOf = (element: Element) => ((element.getRootNode() as ShadowRoot).host ?? element).closest("[role=region]");
const rect = (left: number, top: number, width: number, height: number) =>
  ({ left, top, right: left + width, bottom: top + height, width, height, x: left, y: top }) as DOMRect;
const originalElementRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function (this: Element) {
  if (this.matches("[data-gutter]")) return rect(0, 0, GUTTER, HEIGHT);
  if (this.matches("[data-content] [data-line]")) return rect(GUTTER, (Number(this.getAttribute("data-line")) - 1) * 20 - (regionOf(this)?.scrollTop ?? 0), WIDTH, 20);
  return originalElementRect.call(this);
};
/** Where `offset` in `node` lies in the text of `element`, which holds it. */
function offsetIn(element: Element, node: Node, offset: number): number {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  let before = 0;
  for (let current = walker.nextNode(); current && current !== node; current = walker.nextNode()) before += current.textContent!.length;
  return before + offset;
}
Range.prototype.getBoundingClientRect = function (this: Range) {
  const line = this.startContainer.parentElement?.closest("[data-line]");
  if (line) {
    const code = line.closest("[data-code]")!;
    const box = line.getBoundingClientRect();
    const left = GUTTER + 10 * offsetIn(line, this.startContainer, this.startOffset) - code.scrollLeft;
    return rect(left, box.top, 10 * (offsetIn(line, this.endContainer, this.endOffset) - offsetIn(line, this.startContainer, this.startOffset)), 20);
  }
  const pre = this.startContainer.parentElement!;
  const before = (this.startContainer.textContent ?? "").slice(0, this.startOffset).split("\n");
  return rect(10 * before[before.length - 1].length - pre.scrollLeft, (before.length - 1) * 20 - pre.scrollTop, 10 * (this.endOffset - this.startOffset), 20);
};

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  wordWrap.set(false);
  localStorage.clear();
  highlights.clear();
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const textFile = (path: string, text: string): ViewerSubject => ({
  key: path,
  path,
  content: { state: "file", body: { kind: "text", text, size: text.length } },
});

function show(subject: ViewerSubject | undefined, line?: number, marks?: readonly LineRange[]): void {
  act(() => {
    root.render(
      <ViewerHost>
        <ThemeProvider>{subject && <FileViewer subject={subject} onClose={() => {}} line={line} marks={marks} />}</ThemeProvider>
      </ViewerHost>,
    );
  });
}

const TARGET = "const two = 2;";
// A range within a token, one covering a token and the space after it, and one running past the line's end.
const MARKS: LineRange[] = [
  { start: 0, end: 5 },
  { start: 6, end: 10 },
  { start: 12, end: 99 },
];
const MARKED = ["const", "two ", "2;"];

// Within the highlighting budget, its target line far enough down to be scrolled to.
const SHORT_LINE = 30;
const shortFile = () =>
  textFile("short.ts", ["const one = 1;", ...Array.from({ length: SHORT_LINE - 2 }, (_, index) => `// ${index}`), TARGET, "const three = 3;", ""].join("\n"));
// Past the highlighting budget, so shown as plain text in one preformatted node.
const LONG_LINE = RENDER_BUDGETS.highlightLines - 10;
const longFile = () =>
  textFile(
    "long.ts",
    Array.from({ length: RENDER_BUDGETS.highlightLines + 1 }, (_, index) => (index + 1 === LONG_LINE ? TARGET : `const line${index + 1} = 0;`)).join("\n"),
  );

const plainText = () => document.querySelector<HTMLElement>("[role=dialog] pre[data-find-region]");
const codeRegion = () => document.querySelector<HTMLElement>("[role=dialog] [role=region]")!;
const codeScroller = () =>
  Array.from(codeRegion().querySelectorAll("*")).find((element) => element.shadowRoot)!.shadowRoot!.querySelector<HTMLElement>("[data-code]")!;
/** The rendered line centred in the region; plain text puts the line's top at the middle. */
const centred = (line: number) => (line - 1) * 20 - (HEIGHT - 20) / 2;
const topCentred = (line: number) => (line - 1) * 20 - HEIGHT / 2;

it.each([
  ["a short file, highlighted", shortFile, SHORT_LINE, centred(SHORT_LINE), () => codeRegion(), () => codeScroller()],
  ["a long file, as plain text", longFile, LONG_LINE, topCentred(LONG_LINE), () => plainText()!, () => plainText()!],
] as const)("opens %s at a line, scrolled into view up and down only, with the ranges on it marked", async (_, file, line, top, vertical, sideways) => {
  show(file(), line, MARKS);
  await settleLazyImport();
  expect(vertical().scrollTop).toBe(top);
  // The marks are in view already: the start of the lines stays in view, not under the gutter.
  expect(sideways().scrollLeft).toBe(0);
  expect(marked()).toEqual(MARKED);
  // Under the find bar's highlights.
  expect(highlights.get(LINE_MARK_HIGHLIGHT)!.priority).toBeLessThan(0);
});

// A mark far along a long line is brought into view sideways, beside the gutter and with some text before it.
it.each([
  ["highlighted code", "long-line.ts", () => codeScroller(), GUTTER],
  ["plain text", "long-line.txt", () => plainText()!, 0],
] as const)("scrolls %s sideways to a mark out of view, not under the gutter", async (_, path, scroller, visibleLeft) => {
  const far = `${"x".repeat(100)} needle`;
  const text = path.endsWith(".txt") ? `${far}\n`.repeat(RENDER_BUDGETS.highlightLines + 1) : `${far}\n`;
  show(textFile(path, text), 1, [{ start: 101, end: 107 }]);
  await settleLazyImport();
  expect(marked()).toEqual(["needle"]);
  expect(scroller().scrollLeft).toBeGreaterThan(0);
  const mark = Array.from(highlights.get(LINE_MARK_HIGHLIGHT)!)[0].getBoundingClientRect();
  expect(mark.left).toBeGreaterThan(visibleLeft);
  expect(mark.right).toBeLessThanOrEqual(WIDTH);
});

it("marks nothing past the end of an empty line, not the newline the renderer draws for it", async () => {
  show(textFile("gap.ts", "const one = 1;\n\nconst two = 2;\n"), 2, [{ start: 0, end: 5 }]);
  await settleLazyImport();
  expect(highlights.has(LINE_MARK_HIGHLIGHT)).toBe(false);
});

it.each([
  ["a short file, highlighted", shortFile],
  ["a long file, as plain text", longFile],
] as const)("opens %s without a line as before: not scrolled, nothing marked", async (_, file) => {
  show(file());
  await settleLazyImport();
  expect(codeRegion().scrollTop).toBe(0);
  expect(plainText()?.scrollLeft ?? codeScroller().scrollLeft).toBe(0);
  expect(highlights.has(LINE_MARK_HIGHLIGHT)).toBe(false);
});

it("marks the ranges again when the code is drawn again, and takes them off for another line, another file, or on closing", async () => {
  const subject = shortFile();
  show(subject, SHORT_LINE, MARKS);
  await settleLazyImport();
  expect(marked()).toEqual(MARKED);
  // A redraw replaces the text the ranges were made on.
  act(() => wordWrap.set(true));
  await settleLazyImport();
  expect(marked()).toEqual(MARKED);
  expect(Array.from(highlights.get(LINE_MARK_HIGHLIGHT)!).every((range) => range.startContainer.isConnected)).toBe(true);

  show(subject, SHORT_LINE + 1, [{ start: 6, end: 11 }]);
  await settleLazyImport();
  expect(marked()).toEqual(["three"]);
  show(subject, SHORT_LINE + 1);
  expect(highlights.has(LINE_MARK_HIGHLIGHT)).toBe(false);

  show(subject, 1, [{ start: 6, end: 9 }]);
  await settleLazyImport();
  expect(marked()).toEqual(["one"]);
  show(textFile("other.ts", "const one = 1;\n"));
  await settleLazyImport();
  expect(highlights.has(LINE_MARK_HIGHLIGHT)).toBe(false);

  show(subject, 1, [{ start: 6, end: 9 }]);
  await settleLazyImport();
  expect(marked()).toEqual(["one"]);
  show(undefined);
  expect(highlights.has(LINE_MARK_HIGHLIGHT)).toBe(false);
});

it("marks nothing on a line the file does not have", async () => {
  show(shortFile(), SHORT_LINE + 9, MARKS);
  await settleLazyImport();
  expect(highlights.has(LINE_MARK_HIGHLIGHT)).toBe(false);
});
