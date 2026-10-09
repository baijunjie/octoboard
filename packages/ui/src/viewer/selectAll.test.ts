// @vitest-environment jsdom
import { beforeEach, expect, it } from "vitest";

import { IS_MAC } from "../useWindowShortcut";
import { isSelectAll, selectCode } from "./selectAll";

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
