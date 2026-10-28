// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { usePressedControl } from "./usePressedControl";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Mounts the hook and returns its reader, and a focusable list holding a button and a plain item. */
function mount() {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  let read!: () => HTMLElement | null;
  function Probe() {
    read = usePressedControl();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Probe />));
  const list = document.body.appendChild(document.createElement("div"));
  list.tabIndex = 0;
  const button = list.appendChild(document.createElement("button"));
  const label = button.appendChild(document.createElement("span"));
  const item = list.appendChild(document.createElement("span"));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    list.remove();
  });
  return { read: () => read(), button, label, item };
}

const key = (target: Element, type: "keydown" | "keyup", name: string) => target.dispatchEvent(new KeyboardEvent(type, { bubbles: true, key: name }));

it("reads the control a press ended on while its events are dispatched, and nothing a turn later", () => {
  const { read, button, label } = mount();
  let during: HTMLElement | null = null;
  button.addEventListener("click", () => { during = read(); });
  label.click();
  expect(during).toBe(button);
  vi.advanceTimersByTime(0);
  expect(read()).toBeNull();
});

it("never takes a focusable container for the control pressed", () => {
  const { read, item } = mount();
  item.click();
  expect(read()).toBeNull();
});

it("drops what an earlier press left when the next event ends on no control, before that press's turn is over", () => {
  const { read, button, item } = mount();
  button.click();
  item.click();
  expect(read()).toBeNull();
});

it("records a key press only for the keys that press a control", () => {
  const { read, button } = mount();
  key(button, "keydown", "a");
  expect(read()).toBeNull();
  key(button, "keydown", "Enter");
  expect(read()).toBe(button);
  key(button, "keyup", "Shift");
  expect(read()).toBeNull();
  key(button, "keyup", " ");
  expect(read()).toBe(button);
});
