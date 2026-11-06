import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished } from "vitest";

// Self-contained, as the layer imports nothing else of the application: the helpers the wrappers' own tests share.
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Renders `node` into the page, unmounted when the test finishes; `render` replaces what is rendered. */
export function mount(node: React.ReactElement): { container: HTMLDivElement; render: (node: React.ReactElement) => void } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (next: React.ReactElement) => act(() => root.render(next));
  render(node);
  return { container, render };
}

/** A mouse press on `element`, as the pointer events a browser dispatches for one. */
export function pressWithMouse(element: Element): void {
  const init = { bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true, pointerType: "mouse", width: 1, height: 1 };
  act(() => {
    element.dispatchEvent(new PointerEvent("pointerdown", init));
    element.dispatchEvent(new PointerEvent("mousedown", init));
    element.dispatchEvent(new PointerEvent("pointerup", init));
    element.dispatchEvent(new PointerEvent("mouseup", init));
    element.dispatchEvent(new MouseEvent("click", init));
  });
}
