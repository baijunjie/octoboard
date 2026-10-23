// @vitest-environment jsdom
import { act, useRef } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { useHeightMotion } from "./useHeightMotion";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Probe() {
  const marker = useRef<HTMLSpanElement>(null);
  useHeightMotion(marker);
  return <div role="dialog"><span ref={marker} /></div>;
}

function mount() {
  const preference = new EventTarget() as MediaQueryList;
  Object.defineProperty(preference, "matches", { value: false, writable: true });
  vi.stubGlobal("matchMedia", () => preference);
  let report: ResizeObserverCallback = () => {};
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: ResizeObserverCallback) { report = callback; }
    observe() {}
    disconnect() {}
  });
  const cancel = vi.fn();
  const animate = vi.fn(() => ({ playState: "running", cancel }));
  const original = HTMLElement.prototype.animate;
  HTMLElement.prototype.animate = animate as unknown as typeof HTMLElement.prototype.animate;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    if (original) HTMLElement.prototype.animate = original;
    else Reflect.deleteProperty(HTMLElement.prototype, "animate");
    vi.unstubAllGlobals();
  });
  act(() => root.render(<Probe />));
  const resize = (height: number) => report([{ borderBoxSize: [{ blockSize: height }] } as unknown as ResizeObserverEntry], undefined as never);
  const request = (reduce: boolean) => act(() => {
    Object.defineProperty(preference, "matches", { value: reduce });
    preference.dispatchEvent(new Event("change"));
  });
  return { animate, cancel, resize, request };
}

it("eases a change of height, and cancels the ease when reduced motion is requested during it", () => {
  const { animate, cancel, resize, request } = mount();
  resize(200);
  resize(300);
  expect(animate).toHaveBeenCalledOnce();
  request(false);
  expect(cancel).not.toHaveBeenCalled();
  request(true);
  expect(cancel).toHaveBeenCalledOnce();
});

it("does not start an ease under reduced motion", () => {
  const { animate, resize, request } = mount();
  request(true);
  resize(200);
  resize(300);
  expect(animate).not.toHaveBeenCalled();
});
