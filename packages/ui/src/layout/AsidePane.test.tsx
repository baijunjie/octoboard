// @vitest-environment jsdom
import { act, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AsidePane } from "./AsidePane";
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
  const mounted = vi.fn();
  const unmounted = vi.fn();
  function Content() {
    useEffect(() => { mounted(); return unmounted; }, []);
    return <input defaultValue="retained" />;
  }
  const peek = { active: false, reveal: vi.fn(), keep: vi.fn(), leave: vi.fn() } satisfies PanePeek;
  /** `docked` is the pane with a panel open; otherwise it floats while `active`. */
  const render = (docked: boolean, active = false) =>
    act(() => root.render(
      <AsidePane layout={{ width: 300, peek: docked ? undefined : { ...peek, active } }}>
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
    render,
    mounted,
    unmounted,
    peek,
    pane: () => container.querySelector<HTMLElement>("[data-pane=aside]"),
    advance: (ms: number) => act(() => vi.advanceTimersByTime(ms)),
    /** Floats in, and lets the card get its first frame. */
    floatIn: () => {
      act(() => root.render(<AsidePane layout={{ width: 300, peek: { ...peek, active: true } }}><Content /></AsidePane>));
    },
  };
}

it("docks as a row sibling that takes its room from the terminal, at any width", () => {
  const test = mount();
  const pane = test.pane()!;
  expect(pane.className).toContain("w-(--aside-width)");
  expect(pane.className).toContain("min-w-[320px]");
  expect(pane.className).not.toMatch(/fixed|docked:|drawer/);
  expect(pane.style.getPropertyValue("--aside-width")).toBe("300px");
});

it("pads the cards by the panel gap on all four sides and paints nothing in it, in every form", () => {
  const test = mount();
  const painted = /(^|\s)(bg-|shadow|rounded|panel-edge|border|ring)/;
  expect(test.pane()!.className).toContain("ps-(--panel-grip-gap) pe-(--peek-gap) py-(--peek-gap)");
  expect(test.pane()!.className).not.toMatch(painted);
  test.floatIn();
  test.advance(32);
  expect(test.pane()!.className).toContain("ps-(--panel-grip-gap) pe-(--peek-gap) py-(--peek-gap)");
  expect(test.pane()!.className).not.toMatch(painted);
});

it("floats in fading up like the sidebar's card, over the terminal without taking its pointer events", () => {
  const test = mount();
  test.floatIn();
  const pane = test.pane()!;
  expect(pane.className).toContain("fixed");
  expect(pane.className).toContain("pointer-events-none");
  // It starts below its place and clear, without a transition, and is then let to rise.
  expect(pane.className).toContain("translate-y-1.5 opacity-0 invisible");
  expect(pane.classList.contains("transition-none")).toBe(true);
  test.advance(32);
  expect(pane.className).toContain("translate-y-0 opacity-100");
  expect(pane.className).toContain("duration-240");
  expect(pane.classList.contains("transition-none")).toBe(false);
});

it("fades away as it came, and is not rendered once it has gone", () => {
  const test = mount();
  test.floatIn();
  test.advance(32);
  test.render(false, false);
  expect(test.pane()!.className).toContain("translate-y-1.5 opacity-0 invisible");
  test.advance(300);
  expect(test.pane()).toBeNull();
});

it("floats in and goes at once under reduced motion", () => {
  const test = mount(true);
  test.render(false, true);
  expect(test.pane()!.className).toContain("opacity-100");
  expect(test.pane()!.className).toContain("motion-reduce:transition-none");
  test.render(false, false);
  test.advance(0);
  expect(test.pane()).toBeNull();
});

it("docks at once on a press, with no transition", () => {
  const test = mount();
  test.floatIn();
  test.advance(32);
  test.render(true);
  expect(test.pane()!.className).not.toMatch(/transition|duration|translate|opacity|invisible|fixed/);
});

it("keeps a floating pane up while the pointer is on it", () => {
  const test = mount();
  test.floatIn();
  test.advance(32);
  const pane = test.pane()!;
  act(() => void pane.dispatchEvent(new MouseEvent("pointerover", { bubbles: true })));
  expect(test.peek.keep).toHaveBeenCalled();
  act(() => void pane.dispatchEvent(new MouseEvent("pointerout", { bubbles: true })));
  expect(test.peek.leave).toHaveBeenCalled();
});
