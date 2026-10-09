// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { AsidePeekHotZone } from "./paneOverlay";
import type { PanePeek } from "./usePaneToggles";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;
let container: HTMLElement | undefined;

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  act(() => root?.unmount());
  vi.useRealTimers();
  document.body.replaceChildren();
  root = container = undefined;
});

/** An aside that will end up spanning x 1160..1440 and y 40..800, wherever it is on its way. */
function addPane(): void {
  const pane = document.body.appendChild(document.createElement("aside"));
  pane.dataset.pane = "aside";
  Object.defineProperties(pane, { offsetLeft: { value: 1160 }, offsetWidth: { value: 280 } });
  // Where it is on its way in: translated off to the right, so only its layout position tells where
  // it is going to be.
  pane.getBoundingClientRect = () => ({ top: 40, bottom: 800, left: 1440, right: 1720 }) as DOMRect;
}

function mount(active = false) {
  const peek = { active, reveal: vi.fn(), keep: vi.fn(), leave: vi.fn() } satisfies PanePeek;
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  const render = (next: PanePeek) => act(() => root!.render(<AsidePeekHotZone peek={next} />));
  render(peek);
  return { peek, render };
}

const zone = () => container!.firstElementChild!;
const pointer = (type: string, init: MouseEventInit = {}) => {
  const event = new MouseEvent(type, { bubbles: true, ...init });
  Object.defineProperty(event, "pointerType", { value: "mouse" });
  return event;
};
const enter = () => act(() => void zone().dispatchEvent(pointer("pointerover")));
const move = (clientX: number, clientY = 400) => act(() => void window.dispatchEvent(pointer("pointermove", { clientX, clientY })));
const wait = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

it("floats the pane in only after the pointer has stayed for the dwell", () => {
  const { peek } = mount();
  addPane();
  enter();
  wait(199);
  expect(peek.reveal).not.toHaveBeenCalled();
  wait(1);
  expect(peek.reveal).toHaveBeenCalledTimes(1);
});

it("does not float the pane in when the pointer leaves the strip before the dwell is over", () => {
  const { peek } = mount();
  enter();
  wait(100);
  act(() => void zone().dispatchEvent(pointer("pointerout")));
  wait(300);
  expect(peek.reveal).not.toHaveBeenCalled();
});

it("counts a first move outside the pane's final extent as leaving it, once", () => {
  const { peek } = mount();
  addPane();
  enter();
  wait(200);
  move(600);
  move(700);
  expect(peek.leave).toHaveBeenCalledTimes(1);
});

it("leaves a first move within the pane's final extent to the pane, even while it is still sliding in", () => {
  const { peek } = mount();
  addPane();
  enter();
  wait(200);
  move(1200);
  move(700);
  expect(peek.leave).not.toHaveBeenCalled();
});

it("counts a first move above or below the pane as leaving it", () => {
  const { peek } = mount();
  addPane();
  enter();
  wait(200);
  move(1200, 20);
  expect(peek.leave).toHaveBeenCalledTimes(1);
});

it("stops watching the pointer once the pane is no longer floating", () => {
  const { peek, render } = mount();
  addPane();
  enter();
  wait(200);
  render({ ...peek, active: true });
  render({ ...peek, active: false });
  move(600);
  expect(peek.leave).not.toHaveBeenCalled();
});

it("stops watching the pointer on unmount", () => {
  const { peek } = mount();
  addPane();
  enter();
  wait(200);
  act(() => root!.unmount());
  move(600);
  expect(peek.leave).not.toHaveBeenCalled();
});
