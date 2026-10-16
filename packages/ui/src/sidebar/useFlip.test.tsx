// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { useFlip } from "./useFlip";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom lays nothing out: a child stands where `tops` says, and a resize report is made by hand.
let tops: Record<string, number>;
let reportResize: () => void;
let listWidth: number;
let reducedMotion: boolean;
const animate = vi.fn();

class FakeResizeObserver {
  constructor(callback: () => void) {
    reportResize = callback;
  }
  observe() {}
  unobserve() {}
  disconnect() {}
}

beforeEach(() => {
  tops = {};
  listWidth = 200;
  reducedMotion = false;
  animate.mockClear();
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  vi.stubGlobal("matchMedia", () => ({ matches: reducedMotion }));
  Object.defineProperty(HTMLElement.prototype, "offsetTop", { configurable: true, get(this: HTMLElement) { return tops[this.dataset.flip ?? ""] ?? 0; } });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get() { return listWidth; } });
  HTMLElement.prototype.animate = animate;
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function List({ keys }: { keys: string[] }): React.ReactElement {
  const ref = useFlip<HTMLDivElement>();
  return (
    <div ref={ref}>
      {keys.map((key) => (
        <div key={key} data-flip={key} />
      ))}
    </div>
  );
}

function mount(keys: string[]): { rerender: (keys: string[]) => void } {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (next: string[]) => act(() => root.render(<List keys={next} />));
  render(keys);
  return { rerender: render };
}

it("slides a child that a reorder moved, from where it was", () => {
  tops = { a: 0, b: 40 };
  const list = mount(["a", "b"]);
  tops = { a: 40, b: 0 };
  list.rerender(["b", "a"]);
  expect(animate).toHaveBeenCalledTimes(2);
  expect(animate.mock.calls.map((call) => (call[0] as { transform: string }[])[0].transform).sort()).toEqual(["translateY(-40px)", "translateY(40px)"]);
});

it("slides a sibling a resize moved at the list's width, whether or not the owner rendered", () => {
  tops = { a: 0, b: 40, c: 80 };
  const list = mount(["a", "b", "c"]);
  // A row above opened: nothing reorders, so only the resize report finds `b` and `c` moved.
  tops = { a: 0, b: 100, c: 140 };
  list.rerender(["a", "b", "c"]);
  act(() => reportResize());
  expect(animate).toHaveBeenCalledTimes(2);

  // A resize with no render of the owner slides too.
  animate.mockClear();
  tops = { a: 0, b: 60, c: 100 };
  act(() => reportResize());
  expect(animate).toHaveBeenCalledTimes(2);
});

it("moves children without sliding while the list's width changes, and slides from there on", () => {
  tops = { a: 0, b: 40, c: 80 };
  const list = mount(["a", "b", "c"]);
  for (const [width, top] of [[300, 50], [310, 60]]) {
    listWidth = width;
    tops = { a: 0, b: top, c: top + 40 };
    act(() => reportResize());
  }
  expect(animate).not.toHaveBeenCalled();
  // The positions were brought up to date: the next reorder slides from here.
  tops = { a: 0, b: 100, c: 60 };
  list.rerender(["a", "c", "b"]);
  expect(animate.mock.calls.map((call) => (call[0] as { transform: string }[])[0].transform).sort()).toEqual(["translateY(-40px)", "translateY(40px)"]);
});

it("leaves a child that just appeared alone and slides the ones it displaced", () => {
  tops = { a: 0, b: 40 };
  const list = mount(["a", "b"]);
  tops = { a: 0, n: 40, b: 80 };
  list.rerender(["a", "n", "b"]);
  expect(animate).toHaveBeenCalledTimes(1);
  expect((animate.mock.calls[0][0] as { transform: string }[])[0].transform).toBe("translateY(-40px)");
});

it("does not slide where the system asks for reduced motion", () => {
  reducedMotion = true;
  tops = { a: 0, b: 40 };
  const list = mount(["a", "b"]);
  tops = { a: 40, b: 0 };
  list.rerender(["b", "a"]);
  expect(animate).not.toHaveBeenCalled();
});
