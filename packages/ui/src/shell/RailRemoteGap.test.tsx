// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { mountHost } from "../test/mountHost";
import { RailRemoteGap } from "./RailRemoteGap";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let resize: ((height: number) => void) | undefined;
class RecordingObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe() {
    resize = (height) => this.callback([{ contentRect: { height } } as ResizeObserverEntry], this as unknown as ResizeObserver);
  }
  disconnect() {
    resize = undefined;
  }
  unobserve() {}
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  resize = undefined;
});

/** Mounts the gap with the free space `height` px tall to begin with. */
function mount(height: number) {
  vi.stubGlobal("ResizeObserver", RecordingObserver);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ height } as DOMRect);
  let container!: HTMLElement;
  act(() => void (container = mountHost(<RailRemoteGap />)));
  return container;
}
const groove = (container: ParentNode) => container.querySelector<HTMLElement>("[data-rail-groove]")!;

it("keeps the separator as an item, undrawn, while there is free space between the button and the bottom group", () => {
  const container = mount(40);
  expect(groove(container).className).toContain("invisible");
  expect(groove(container).getAttribute("aria-hidden")).toBe("true");
  expect(groove(container).className).toContain("groove-t");
});

it("draws the separator once the free space is used up, and takes it away when space returns", () => {
  const container = mount(0);
  expect(groove(container).className).not.toContain("invisible");
  act(() => resize!(12));
  expect(groove(container).className).toContain("invisible");
  act(() => resize!(0));
  expect(groove(container).className).not.toContain("invisible");
});

it("keeps the separator an element of its own, after the space it is measured against and not inside it", () => {
  const container = mount(0);
  const space = container.querySelector("[data-rail-gap]")!;
  expect(space.nextElementSibling).toBe(groove(container));
  expect(space.contains(groove(container))).toBe(false);
  // The space takes back the gap after it, so that with none left the neighbours are one gap apart.
  expect(space.className).toContain("-mb-2");
});
