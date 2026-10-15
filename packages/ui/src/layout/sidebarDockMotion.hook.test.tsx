// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { useSidebarDockMotion } from "./sidebarDockMotion";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => vi.unstubAllGlobals());

function mount(reduce = false) {
  // Frames advance only on demand, unlike effects, so a test can observe the frame that must
  // paint before the browser receives the transition's destination.
  const frames = new Map<number, FrameRequestCallback>();
  let nextFrame = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.set(++nextFrame, callback);
    return nextFrame;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
  vi.stubGlobal("matchMedia", () => ({ matches: reduce }));
  function Probe({ shown, peek }: { shown: boolean; peek: boolean }) {
    const motion = useSidebarDockMotion(shown, peek, 280);
    return <div data-inset={motion.inset} data-shown={motion.insetShown} data-phase={motion.phase} data-preparing={motion.preparingInset} />;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let unmounted = false;
  const unmount = () => {
    if (unmounted) return;
    act(() => root.unmount());
    container.remove();
    unmounted = true;
  };
  onTestFinished(unmount);
  const render = (shown: boolean, peek: boolean) => act(() => root.render(<Probe shown={shown} peek={peek} />));
  render(false, false);
  return {
    render,
    unmount,
    pane: () => container.firstElementChild as HTMLElement,
    pendingFrames: () => frames.size,
    frame: () => act(() => {
      const pending = [...frames.values()];
      frames.clear();
      for (const callback of pending) callback(0);
    }),
  };
}

it("paints the offscreen hover card before sliding it on screen", () => {
  const test = mount();
  test.render(false, true);
  expect(test.pane().dataset.inset).toBe("true");
  expect(test.pane().dataset.shown).toBe("false");
  expect(test.pane().dataset.preparing).toBe("true");
  test.frame();
  expect(test.pane().dataset.shown).toBe("false");
  expect(test.pane().dataset.preparing).toBe("true");
  test.frame();
  expect(test.pane().dataset.shown).toBe("true");
  expect(test.pane().dataset.preparing).toBe("false");
  test.render(false, false);
  expect(test.pane().dataset.shown).toBe("false");
  expect(test.pane().dataset.preparing).toBe("false");
});

it("paints the zero-width column before opening it from the hover card", () => {
  const test = mount();
  test.render(false, true);
  test.frame();
  test.frame();
  test.render(true, false);
  expect(test.pane().dataset.inset).toBe("false");
  expect(test.pane().dataset.phase).toBe("closed");
  expect(document.documentElement.style.getPropertyValue("--sidebar-span")).toBe("0px");
  test.frame();
  expect(document.documentElement.style.getPropertyValue("--sidebar-span")).toBe("0px");
  test.frame();
  expect(test.pane().dataset.phase).toBe("opening");
  expect(document.documentElement.style.getPropertyValue("--sidebar-span")).toBe("280px");
});

it.each([0, 1])("cancels a hover reveal reversed after %i frames", (elapsed) => {
  const test = mount();
  test.render(false, true);
  if (elapsed) test.frame();
  test.render(false, false);
  expect(test.pendingFrames()).toBe(0);
  test.frame();
  test.frame();
  expect(test.pane().dataset.inset).toBe("false");
  expect(test.pane().dataset.shown).toBe("false");
});

it("cancels a pending column opening when the target reverses", () => {
  const test = mount();
  test.render(false, true);
  test.frame();
  test.frame();
  test.render(true, false);
  test.frame();
  test.render(false, false);
  expect(test.pendingFrames()).toBe(0);
  test.frame();
  expect(test.pane().dataset.phase).toBe("closed");
  expect(document.documentElement.style.getPropertyValue("--sidebar-span")).toBe("0px");
});

it.each([0, 1])("cancels the queued reveal when unmounted after %i frames", (elapsed) => {
  const test = mount();
  test.render(false, true);
  if (elapsed) test.frame();
  test.unmount();
  expect(test.pendingFrames()).toBe(0);
});

it("reveals and docks immediately with reduced motion", () => {
  const test = mount(true);
  test.render(false, true);
  expect(test.pane().dataset.shown).toBe("true");
  expect(test.pendingFrames()).toBe(0);
  test.render(true, false);
  expect(test.pane().dataset.phase).toBe("open");
  expect(test.pendingFrames()).toBe(0);
});
