// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { readSelection, restoreSelection } from "./textSelection";

afterEach(() => {
  vi.restoreAllMocks();
  document.getSelection()?.removeAllRanges();
  document.body.innerHTML = "";
});

const selectedText = (container: HTMLElement): string => readSelection(container)?.text ?? "";

function content(): HTMLElement {
  document.body.innerHTML = "<header>outside</header><main><div data-find-region><p> beta </p><div></div></div></main>";
  return document.querySelector("main")!;
}

it("keeps spaces in selected document text and ignores collapsed selections", () => {
  const container = content();
  const selection = document.getSelection()!;
  selection.selectAllChildren(container.querySelector("p")!);
  expect(selectedText(container)).toBe(" beta ");
  selection.collapseToStart();
  expect(selectedText(container)).toBe("");
});

it("ignores selections outside the content and ones that cross its boundary", () => {
  const container = content();
  const selection = document.getSelection()!;
  selection.selectAllChildren(document.querySelector("header")!);
  expect(selectedText(container)).toBe("");
  selection.extend(container.querySelector("p")!.firstChild!, 3);
  expect(selectedText(container)).toBe("");
});

it.each([true, false])("reads highlighted code with a shadow-root selection API available: %s", (local) => {
  const container = content();
  const root = container.querySelector("[data-find-region] > div")!.attachShadow({ mode: "open" });
  root.innerHTML = "<pre>selected code</pre>";
  const node = root.querySelector("pre")!.firstChild!;
  // jsdom cannot select inside a shadow root; browsers expose it through either selection API.
  const selection = { anchorNode: node, focusNode: node, isCollapsed: false, toString: () => "selected code" } as unknown as Selection;
  if (local) Object.defineProperty(root, "getSelection", { value: () => selection });
  else vi.spyOn(document, "getSelection").mockReturnValue(selection);
  expect(selectedText(container)).toBe("selected code");
});

it("reads WebKit's composed selection when its document endpoints are collapsed outside the code region", () => {
  const container = content();
  const root = container.querySelector("[data-find-region] > div")!.attachShadow({ mode: "open" });
  root.innerHTML = "<pre>selected code</pre>";
  const node = root.querySelector("pre")!.firstChild!;
  const composed = new StaticRange({ startContainer: node, startOffset: 0, endContainer: node, endOffset: 13 });
  const getComposedRanges = vi.fn().mockReturnValue([composed]);
  const selection = { anchorNode: container, focusNode: container, isCollapsed: true, toString: () => "selected code", getComposedRanges } as unknown as Selection;
  vi.spyOn(document, "getSelection").mockReturnValue(selection);
  expect(selectedText(container)).toBe("selected code");
  expect(getComposedRanges).toHaveBeenCalledWith({ shadowRoots: [root] });
});

it("reads a selection inside a container that is itself the region, and still ignores one outside it", () => {
  document.body.innerHTML = "<header>outside</header><pre data-find-region>some code</pre>";
  const container = document.querySelector("pre")!;
  const selection = document.getSelection()!;
  selection.selectAllChildren(container);
  expect(selectedText(container)).toBe("some code");
  selection.selectAllChildren(document.querySelector("header")!);
  expect(selectedText(container)).toBe("");
});

it("gives the ends of the selection it reads, anchor first, and none for a selection outside a region", () => {
  const container = content();
  const text = container.querySelector("p")!.firstChild!;
  document.getSelection()!.setBaseAndExtent(text, 4, text, 1);
  expect(readSelection(container)).toEqual({ text: "bet", ends: { startContainer: text, startOffset: 4, endContainer: text, endOffset: 1 } });
  document.getSelection()!.selectAllChildren(document.querySelector("header")!);
  expect(readSelection(container)).toBeNull();
  document.getSelection()!.removeAllRanges();
  expect(readSelection(container)).toBeNull();
});

it("gives the composed range's ends where the browser reports one", () => {
  const container = content();
  const root = container.querySelector("[data-find-region] > div")!.attachShadow({ mode: "open" });
  root.innerHTML = "<pre>selected code</pre>";
  const node = root.querySelector("pre")!.firstChild!;
  const composed = new StaticRange({ startContainer: node, startOffset: 2, endContainer: node, endOffset: 9 });
  const selection = { anchorNode: container, focusNode: container, toString: () => "lected ", getComposedRanges: () => [composed] } as unknown as Selection;
  vi.spyOn(document, "getSelection").mockReturnValue(selection);
  expect(readSelection(container)?.ends).toMatchObject({ startContainer: node, startOffset: 2, endContainer: node, endOffset: 9 });
});

it("puts a selection back where the document has none, and in place of another selection", () => {
  document.body.innerHTML = "<p id=\"a\">first</p><p id=\"b\">second</p>";
  const a = document.getElementById("a")!.firstChild!;
  const ends = { startContainer: a, startOffset: 1, endContainer: a, endOffset: 4 };
  restoreSelection(ends, null);
  expect(document.getSelection()!.toString()).toBe("irs");
  // WebKit's word-select under a second right-click, say.
  document.getSelection()!.selectAllChildren(document.getElementById("b")!);
  restoreSelection(ends, null);
  expect(document.getSelection()!.toString()).toBe("irs");
  document.getSelection()!.collapse(document.getElementById("b")!.firstChild, 2);
  restoreSelection(ends, null);
  expect(document.getSelection()!.toString()).toBe("irs");
});

it("puts nothing back over a caret in a text field outside the region, but does over a checkbox", () => {
  document.body.innerHTML = "<p id=\"a\">first</p><input id=\"text\"><input id=\"box\" type=\"checkbox\">";
  const a = document.getElementById("a")!.firstChild!;
  const ends = { startContainer: a, startOffset: 1, endContainer: a, endOffset: 4 };
  document.getElementById("text")!.focus();
  restoreSelection(ends, document.getElementById("a"));
  expect(document.getSelection()!.toString()).toBe("");
  restoreSelection(ends, null);
  expect(document.getSelection()!.toString()).toBe("");
  document.getElementById("box")!.focus();
  restoreSelection(ends, null);
  expect(document.getSelection()!.toString()).toBe("irs");
});

it("puts nothing back, and does not throw, where a text node has shrunk below a kept offset", () => {
  document.body.innerHTML = "<pre id=\"a\">first line</pre><p id=\"b\">other</p>";
  const text = document.getElementById("a")!.firstChild as Text;
  const other = document.getElementById("b")!.firstChild!;
  const ends = { startContainer: text, startOffset: 2, endContainer: text, endOffset: 9 };
  // The file changed under the open menu and the node's text was replaced in place.
  text.data = "abc";
  document.getSelection()!.setBaseAndExtent(other, 1, other, 3);
  expect(() => restoreSelection(ends, null)).not.toThrow();
  expect(document.getSelection()!.toString()).toBe("th");
  document.getSelection()!.removeAllRanges();
  expect(() => restoreSelection(ends, null)).not.toThrow();
  expect(document.getSelection()!.rangeCount).toBe(0);
});

it("puts nothing back where only the start or only the end no longer fits", () => {
  document.body.innerHTML = "<p id=\"a\">first</p><p id=\"b\">second</p>";
  const a = document.getElementById("a")!.firstChild!;
  const b = document.getElementById("b")!.firstChild as Text;
  const stale = { startContainer: a, startOffset: 99, endContainer: b, endOffset: 3 };
  const staleEnd = { startContainer: a, startOffset: 1, endContainer: b, endOffset: 99 };
  expect(() => restoreSelection(stale, null)).not.toThrow();
  expect(document.getSelection()!.rangeCount).toBe(0);
  expect(() => restoreSelection(staleEnd, null)).not.toThrow();
  expect(document.getSelection()!.rangeCount).toBe(0);
});

it("puts nothing back where an element container has fewer children than a kept offset", () => {
  document.body.innerHTML = "<div id=\"list\"><p>one</p><p>two</p><p>three</p></div><p id=\"b\">other</p>";
  const list = document.getElementById("list")!;
  const other = document.getElementById("b")!.firstChild!;
  const ends = { startContainer: list, startOffset: 0, endContainer: list, endOffset: 3 };
  list.lastElementChild!.remove();
  list.lastElementChild!.remove();
  document.getSelection()!.setBaseAndExtent(other, 0, other, 2);
  expect(() => restoreSelection(ends, null)).not.toThrow();
  expect(document.getSelection()!.toString()).toBe("ot");
});

it("puts nothing back, and does not throw, where a kept container has left the document", () => {
  document.body.innerHTML = "<p id=\"a\">first</p><p id=\"b\">second</p>";
  const a = document.getElementById("a")!;
  const text = a.firstChild!;
  const other = document.getElementById("b")!.firstChild!;
  const ends = { startContainer: text, startOffset: 1, endContainer: text, endOffset: 4 };
  a.remove();
  document.getSelection()!.setBaseAndExtent(other, 1, other, 3);
  expect(() => restoreSelection(ends, null)).not.toThrow();
  expect(document.getSelection()!.toString()).toBe("ec");
  document.getSelection()!.removeAllRanges();
  expect(() => restoreSelection(ends, null)).not.toThrow();
  expect(document.getSelection()!.rangeCount).toBe(0);
});

it("still puts back a selection that ends exactly at the end of its text node and child list", () => {
  document.body.innerHTML = "<p id=\"a\">first</p><div id=\"list\"><p>one</p><p>two</p></div>";
  const a = document.getElementById("a")!.firstChild as Text;
  const list = document.getElementById("list")!;
  restoreSelection({ startContainer: a, startOffset: 2, endContainer: a, endOffset: a.length }, null);
  expect(document.getSelection()!.toString()).toBe("rst");
  restoreSelection({ startContainer: list, startOffset: 0, endContainer: list, endOffset: list.childNodes.length }, null);
  expect(document.getSelection()!.toString()).toBe("onetwo");
});
