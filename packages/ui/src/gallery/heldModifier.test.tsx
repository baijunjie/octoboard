// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { useQuickCreateModifier } from "../useQuickCreateModifier";
import { holdPrimaryModifier } from "./heldModifier";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("holds the modifier through plain pointer moves and takes it back after the window lost focus", () => {
  // Before the reader mounts, as the frame does: the stamp has to be the first window listener.
  const { press } = holdPrimaryModifier();
  let held = false;
  function Reader() {
    held = useQuickCreateModifier();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Reader />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const dispatch = (event: Event) => act(() => void window.dispatchEvent(event));
  expect(held).toBe(false);

  // Not held before `press`: a pointer move among a scenario's steps leaves the buttons plain.
  dispatch(new PointerEvent("pointermove"));
  expect(held).toBe(false);

  act(press);
  expect(held).toBe(true);

  dispatch(new PointerEvent("pointermove"));
  expect(held).toBe(true);

  dispatch(new Event("blur"));
  expect(held).toBe(false);

  dispatch(new PointerEvent("pointermove"));
  expect(held).toBe(true);
});
