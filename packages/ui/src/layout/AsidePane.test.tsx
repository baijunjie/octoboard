// @vitest-environment jsdom
import { act, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AsidePane } from "./AsidePane";
import type { DockPhase } from "./paneDockMotion";
import type { PanePeek } from "./usePaneToggles";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(reduce = false) {
  vi.useFakeTimers();
  vi.stubGlobal("matchMedia", () => ({ matches: reduce }));
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", clearTimeout);
  document.documentElement.style.setProperty("--peek-gap", "8px");
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let measure: (() => void) | undefined;
  const disconnect = vi.fn();
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) { measure = callback; }
    observe() {}
    disconnect = disconnect;
  });
  const phase = vi.fn<(phase: DockPhase) => void>();
  const mounted = vi.fn();
  const unmounted = vi.fn();
  function Content() {
    useEffect(() => { mounted(); return unmounted; }, []);
    return <input defaultValue="retained" />;
  }
  const peek = { active: false, reveal: vi.fn(), keep: vi.fn(), leave: vi.fn() } satisfies PanePeek;
  const render = (shown: boolean, active = false) => act(() => root.render(
    <AsidePane layout={{ open: false, width: 300, peek: shown ? undefined : { ...peek, active }, onColumnPhase: phase }}>
      <Content />
    </AsidePane>,
  ));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });
  render(true);
  return {
    render, mounted, unmounted, phase, disconnect,
    measure: () => measure?.(),
    pane: () => container.querySelector<HTMLElement>("[data-pane=aside]")!,
    clip: () => container.firstElementChild as HTMLElement,
    advance: (ms: number) => act(() => vi.advanceTimersByTime(ms)),
  };
}

it("closes as a full-height column before preparing the hover card, preserving its contents", () => {
  const test = mount();
  const content = test.pane().firstElementChild;
  expect(test.clip().classList.contains("docked:flex-[0_1_var(--aside-span)]")).toBe(true);
  expect(test.pane().classList.contains("docked:w-full")).toBe(true);
  test.render(false, true);
  expect(test.phase).toHaveBeenLastCalledWith("closing");
  expect(test.clip().style.getPropertyValue("--aside-span")).toBe("0px");
  expect(test.clip().className).toContain("docked:transition-[width]");
  expect(test.clip().classList.contains("docked:shrink-0")).toBe(true);
  expect(test.pane().classList.contains("docked:w-[var(--aside-close-width,var(--aside-width))]")).toBe(true);
  expect(test.pane().className).toContain("docked:h-full");
  expect(test.pane().classList.contains("docked:rounded-xl")).toBe(false);
  test.advance(250);
  expect(test.phase).toHaveBeenLastCalledWith("closed");
  expect(test.pane().className).toContain("docked:rounded-xl");
  expect(test.pane().className).toContain("docked:transition-none");
  test.advance(16);
  test.advance(16);
  expect(test.pane().className).not.toContain("docked:transition-none");
  expect(test.pane().firstElementChild).toBe(content);
  expect(test.mounted).toHaveBeenCalledOnce();
  expect(test.unmounted).not.toHaveBeenCalled();
});

it("eases a docked reopen and leaves the drawer geometry below the breakpoint", () => {
  const test = mount();
  test.render(false);
  test.advance(250);
  test.render(true);
  expect(test.phase).toHaveBeenLastCalledWith("opening");
  expect(test.clip().style.getPropertyValue("--aside-span")).toBe("300px");
  expect(test.clip().className).toContain("contents docked:flex");
  expect(test.pane().className).toContain("max-docked:rounded-xl");
  test.advance(250);
  expect(test.phase).toHaveBeenLastCalledWith("open");
});

it("settles docked toggles and hover reveals immediately under reduced motion", () => {
  const test = mount(true);
  test.render(false, true);
  expect(test.phase).toHaveBeenLastCalledWith("closed");
  expect(test.pane().className).toContain("docked:rounded-xl");
  expect(test.pane().className).not.toContain("docked:transition-none");
  test.render(true);
  expect(test.phase).toHaveBeenLastCalledWith("open");
});


it("preserves the rendered content width when closing during a sibling's ease", () => {
  const test = mount();
  test.clip().getBoundingClientRect = () => ({ width: 330 }) as DOMRect;
  test.measure();
  expect(test.pane().style.getPropertyValue("--aside-close-width")).toBe("330px");
  expect(test.clip().style.getPropertyValue("--aside-rendered-width")).toBe("330px");
  expect(test.clip().classList.contains("docked:w-[var(--aside-rendered-width,var(--aside-span))]")).toBe(true);
  test.render(false);
  expect(test.disconnect).toHaveBeenCalledOnce();
  expect(test.pane().style.getPropertyValue("--aside-close-width")).toBe("330px");
  expect(test.pane().classList.contains("docked:w-[var(--aside-close-width,var(--aside-width))]")).toBe(true);
});
