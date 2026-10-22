// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { PANE_INSET_MS, useFadeUp } from "./paneDockMotion";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** `useFadeUp` on a probe that shows its state, with frames that run only when `frame` is called
 * and timers that run only when `advance` is. */
function mount(reduce = false) {
  vi.useFakeTimers();
  const frames = new Map<number, FrameRequestCallback>();
  let nextFrame = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.set(++nextFrame, callback);
    return nextFrame;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
  vi.stubGlobal("matchMedia", () => ({ matches: reduce }));
  function Probe({ open, entry }: { open: boolean; entry: string }) {
    const fade = useFadeUp(open, entry);
    return fade.rendered ? <div data-shown={fade.shown} data-preparing={fade.preparing} onTransitionEnd={fade.onTransitionEnd} /> : null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });
  const render = (open: boolean, entry = "files") => act(() => root.render(<Probe open={open} entry={entry} />));
  const element = () => container.firstElementChild as HTMLElement | null;
  return {
    render,
    element,
    state: () => (element() ? `shown=${element()!.dataset.shown} preparing=${element()!.dataset.preparing}` : "gone"),
    frame: () => act(() => {
      const pending = [...frames.values()];
      frames.clear();
      for (const callback of pending) callback(0);
    }),
    advance: (ms: number) => act(() => vi.advanceTimersByTime(ms)),
    transitionEnd: () => act(() => {
      element()!.dispatchEvent(Object.assign(new Event("transitionend", { bubbles: true }), { propertyName: "opacity" }));
    }),
  };
}

/** Runs the two frames an entrance waits for. */
const enter = (test: ReturnType<typeof mount>) => {
  test.frame();
  test.frame();
};

it("enters hidden and shows once two frames have been painted", () => {
  const test = mount();
  test.render(true);
  expect(test.state()).toBe("shown=false preparing=true");
  test.frame();
  expect(test.state()).toBe("shown=false preparing=true");
  test.frame();
  expect(test.state()).toBe("shown=true preparing=false");
});

it("stays while it fades away, and goes when its fade ends", () => {
  const test = mount();
  test.render(true);
  enter(test);
  test.render(false);
  expect(test.state()).toBe("shown=false preparing=false");
  test.transitionEnd();
  expect(test.state()).toBe("gone");
});

it("goes on the fallback when closed before its entry frames ran", () => {
  const test = mount();
  test.render(true);
  test.render(false);
  expect(test.state()).toBe("shown=false preparing=false");
  test.frame();
  test.frame();
  expect(test.state()).toBe("shown=false preparing=false");
  test.advance(PANE_INSET_MS + 60);
  expect(test.state()).toBe("gone");
});

it("enters again when reopened during its exit, and the exit's fallback no longer removes it", () => {
  const test = mount();
  test.render(true);
  enter(test);
  test.render(false);
  test.render(true);
  expect(test.state()).toBe("shown=false preparing=true");
  enter(test);
  expect(test.state()).toBe("shown=true preparing=false");
  test.advance(PANE_INSET_MS + 60);
  expect(test.state()).toBe("shown=true preparing=false");
});

it("enters with another entry when reopened with it during its exit, and again for another while open", () => {
  const test = mount();
  test.render(true, "files");
  enter(test);
  test.render(false, "files");
  test.render(true, "terminal");
  expect(test.state()).toBe("shown=false preparing=true");
  enter(test);
  expect(test.state()).toBe("shown=true preparing=false");
  test.render(true, "files");
  expect(test.state()).toBe("shown=false preparing=true");
  enter(test);
  expect(test.state()).toBe("shown=true preparing=false");
});

it("ignores the end of the exit's fade arriving after it was reopened", () => {
  const test = mount();
  test.render(true);
  enter(test);
  test.render(false);
  test.render(true);
  test.transitionEnd();
  expect(test.state()).toBe("shown=false preparing=true");
  enter(test);
  expect(test.state()).toBe("shown=true preparing=false");
});

it("comes and goes at once under reduced motion", () => {
  const test = mount(true);
  test.render(true);
  expect(test.state()).toBe("shown=true preparing=false");
  test.render(false);
  expect(test.state()).toBe("gone");
});
