// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { findSelection } from "./findSelection";

afterEach(() => {
  vi.restoreAllMocks();
  document.getSelection()?.removeAllRanges();
  document.body.innerHTML = "";
});

function content(): HTMLElement {
  document.body.innerHTML = "<header>outside</header><main><div data-find-region><p> beta </p><div></div></div></main>";
  return document.querySelector("main")!;
}

it("keeps spaces in selected document text and ignores collapsed selections", () => {
  const container = content();
  const selection = document.getSelection()!;
  selection.selectAllChildren(container.querySelector("p")!);
  expect(findSelection(container)).toBe(" beta ");
  selection.collapseToStart();
  expect(findSelection(container)).toBe("");
});

it("ignores selections outside the content and ones that cross its boundary", () => {
  const container = content();
  const selection = document.getSelection()!;
  selection.selectAllChildren(document.querySelector("header")!);
  expect(findSelection(container)).toBe("");
  selection.extend(container.querySelector("p")!.firstChild!, 3);
  expect(findSelection(container)).toBe("");
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
  expect(findSelection(container)).toBe("selected code");
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
  expect(findSelection(container)).toBe("selected code");
  expect(getComposedRanges).toHaveBeenCalledWith({ shadowRoots: [root] });
});
