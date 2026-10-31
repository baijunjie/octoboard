// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { PANE_INSET_MS } from "../layout/paneDockMotion";
import { Scrim } from "./Scrim";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(reduce = false) {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", clearTimeout);
  vi.stubGlobal("matchMedia", () => ({ matches: reduce }));
  const onClose = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });
  const render = (open: boolean) => act(() => root.render(<Scrim open={open} label="Close panel" onClose={onClose} />));
  return { render, onClose, scrim: () => container.querySelector("button"), advance: (ms: number) => act(() => vi.advanceTimersByTime(ms)) };
}

it("fades in with its overlay, and fades out out of reach before it goes", () => {
  const test = mount();
  test.render(true);
  expect(test.scrim()!.className).toContain("opacity-0");
  test.advance(40);
  expect(test.scrim()!.className).toContain("opacity-100");
  expect(test.scrim()!.className).toContain("duration-240 ease-[cubic-bezier(0.2,0,0,1)]");
  act(() => test.scrim()!.click());
  expect(test.onClose).toHaveBeenCalledTimes(1);

  test.render(false);
  expect(test.scrim()!.className).toContain("pointer-events-none opacity-0");
  expect(test.scrim()!.hasAttribute("inert")).toBe(true);
  test.advance(PANE_INSET_MS + 60);
  expect(test.scrim()).toBeNull();
});

it("comes and goes at once under reduced motion", () => {
  const test = mount(true);
  test.render(true);
  expect(test.scrim()!.className).toContain("opacity-100");
  test.render(false);
  expect(test.scrim()).toBeNull();
});
