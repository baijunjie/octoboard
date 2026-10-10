// @vitest-environment jsdom
import { beforeEach, expect, it } from "vitest";

import { IS_MAC } from "../useWindowShortcut";
import { copyDocument, isSelectAll, selectCode, selectDocument } from "./selectAll";

const command = IS_MAC ? { metaKey: true, ctrlKey: false } : { metaKey: false, ctrlKey: true };
const other = IS_MAC ? { metaKey: false, ctrlKey: true } : { metaKey: true, ctrlKey: false };

beforeEach(() => {
  document.body.innerHTML = `<h2>name.ts</h2><div id="frame"></div><p>4 kB</p>`;
  document.getSelection()!.removeAllRanges();
});

it.each([
  ["A with the platform's modifier is", { key: "a", ...command, altKey: false, shiftKey: false }, true],
  ["A with the other platform's modifier is not", { key: "a", ...other, altKey: false, shiftKey: false }, false],
  ["A with Shift held is not", { key: "A", ...command, altKey: false, shiftKey: true }, false],
  ["another key with the platform's modifier is not", { key: "b", ...command, altKey: false, shiftKey: false }, false],
])("%s Select All", (_, event, expected) => {
  expect(isSelectAll(event)).toBe(expected);
});

const frame = () => document.getElementById("frame")!;

// How a real browser treats a selection across a shadow root is checked in a page; here the DOM
// jsdom models.
it("selects the code of a plain-text frame and nothing around it", () => {
  frame().textContent = "line 1\nline 2";
  expect(selectCode(frame())).toBe(true);
  expect(document.getSelection()!.toString()).toBe("line 1\nline 2");
});

it("reports a selection the browser ignored across the shadow root", () => {
  const root = frame().appendChild(document.createElement("div")).attachShadow({ mode: "open" });
  root.innerHTML = `<div data-gutter>1\n2</div><div data-content>one\n</div><div data-content>two</div>`;
  // jsdom, like a browser that keeps a document selection out of shadow trees, leaves it empty.
  expect(selectCode(frame())).toBe(false);
  expect(document.getSelection()!.toString()).toBe("");
});

it("takes the key and selects nothing while the renderer has drawn no code yet", () => {
  frame().appendChild(document.createElement("div")).attachShadow({ mode: "open" }).innerHTML = "Loading";
  expect(selectCode(frame())).toBe(true);
  expect(document.getSelection()!.toString()).toBe("");
});

const clipboardEvent = () => {
  const data = new Map<string, string>();
  let prevented = false;
  return {
    data,
    event: {
      clipboardData: { setData: (type: string, value: string) => void data.set(type, value) } as unknown as DataTransfer,
      preventDefault: () => (prevented = true),
    },
    prevented: () => prevented,
  };
};

// Fenced blocks lie in shadow roots a document selection does not hold, so the copy of a whole
// document carries its source; how a real browser behaves is checked in a page.
it("copies the Markdown source when the whole document is selected, and leaves a partial selection alone", () => {
  frame().innerHTML = "<h1>Title</h1><p>text</p>";
  const whole = clipboardEvent();
  expect(selectDocument(frame())).toBe(true);
  expect(copyDocument(whole.event, frame(), "# Title\n\ntext\n")).toBe(true);
  expect(whole.data.get("text/plain")).toBe("# Title\n\ntext\n");
  expect(whole.prevented()).toBe(true);

  const part = clipboardEvent();
  document.getSelection()!.selectAllChildren(frame().querySelector("p")!);
  expect(copyDocument(part.event, frame(), "# Title\n\ntext\n")).toBe(false);
  expect(part.data.size).toBe(0);
  expect(part.prevented()).toBe(false);
});
