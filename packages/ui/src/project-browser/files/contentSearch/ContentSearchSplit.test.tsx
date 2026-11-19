// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { ContentSearchSplit, contentSearchSplit, MIN_SPLIT_PART } from "./ContentSearchSplit";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(preview: boolean) {
  // The split is measured from its box, which jsdom does not lay out.
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<ContentSearchSplit list={<div data-list="" />} preview={preview ? <div data-preview="" /> : null} />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    contentSearchSplit.set(0.5);
    delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
    vi.unstubAllGlobals();
  });
  return container;
}

const separator = (container: HTMLElement) => container.querySelector<HTMLElement>('[role="separator"]');
const key = (element: HTMLElement, k: string) => act(() => {
  element.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
});

it("puts a divider between the list and the preview that resizes them from the keyboard and remembers the split", () => {
  const container = mount(true);
  const divider = separator(container)!;
  expect(divider.getAttribute("aria-label")).toBe("Resize the result list and the preview");
  expect(divider.getAttribute("aria-valuenow")).toBe("200");
  expect(divider.getAttribute("aria-valuemin")).toBe(String(MIN_SPLIT_PART));
  expect(divider.getAttribute("aria-valuemax")).toBe(String(400 - MIN_SPLIT_PART));
  key(divider, "ArrowDown");
  expect(contentSearchSplit.get()).toBeCloseTo(216 / 400);
  expect(divider.getAttribute("aria-valuenow")).toBe("216");
  // The list's box follows the share.
  expect(container.querySelector<HTMLElement>("[data-content-search-list]")!.style.height).toContain("54%");
  key(divider, "Home");
  expect(divider.getAttribute("aria-valuenow")).toBe(String(MIN_SPLIT_PART));
  act(() => divider.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })));
  expect(contentSearchSplit.get()).toBe(0.5);
});

it("gives the list the whole height, with no divider, when there is no preview", () => {
  const container = mount(false);
  expect(container.querySelector("[data-list]")).not.toBeNull();
  expect(separator(container)).toBeNull();
});
