// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished } from "vitest";

import { PaneResizeHandle } from "../components/PaneResizeHandle";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { usePaneWidth, type DockedPanes, type PaneWidth } from "./paneWidth";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => localStorage.clear());

function mount() {
  document.documentElement.style.setProperty("--rail-width", "48px");
  const viewport = (width: number) => act(() => {
    Object.defineProperty(document.documentElement, "clientWidth", { configurable: true, value: width });
    window.dispatchEvent(new Event("resize"));
  });
  viewport(1440);
  let aside!: PaneWidth;
  let sidebar!: PaneWidth;
  function Probe({ min, narrow, docked }: { min: number; narrow: boolean; docked: DockedPanes }) {
    aside = usePaneWidth("aside", docked, narrow, min);
    sidebar = usePaneWidth("sidebar", docked, narrow, min);
    return <PaneResizeHandle side="aside" paneWidth={aside} />;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (min = 320, narrow = false, docked = { sidebar: true, aside: true }) =>
    act(() => root.render(<Probe min={min} narrow={narrow} docked={docked} />));
  render();
  act(() => {
    aside.reset();
    sidebar.reset();
  });
  return { render, viewport, container, aside: () => aside, sidebar: () => sidebar };
}

it("restores the chosen width after a conditional minimum without rewriting the preference", () => {
  const test = mount();
  act(() => test.aside().setWidth(320));
  test.viewport(600);
  test.render(336, true, { sidebar: false, aside: true });
  expect(test.aside().width).toBe(336);
  expect(test.aside().min).toBe(336);
  expect(test.aside().max).toBe(336);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe("320");
  test.render(320, true, { sidebar: false, aside: true });
  expect(test.aside().width).toBe(320);
  test.viewport(1440);
  test.render();
  expect(test.aside().width).toBe(320);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe("320");
});

it("shares the effective floor with the handle and keyboard resize bounds", () => {
  const test = mount();
  test.viewport(600);
  test.render(336, true, { sidebar: false, aside: true });
  const handle = test.container.querySelector<HTMLElement>("[role=separator]")!;
  expect(handle.getAttribute("aria-valuemin")).toBe("336");
  expect(handle.getAttribute("aria-valuemax")).toBe("336");
  act(() => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true })));
  expect(test.aside().width).toBe(336);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe("336");
});

it("reserves the same aside minimum when sharing space with the sidebar", () => {
  const test = mount();
  act(() => test.sidebar().setWidth(480));
  test.viewport(1200);
  test.render(336);
  expect(test.sidebar().width).toBe(296);
  expect(test.aside().width).toBe(336);
  test.render();
  expect(test.sidebar().width).toBe(312);
  expect(test.aside().width).toBe(320);
});

it("restores the saved width after the viewport widens and leaves floating bounds unchanged", () => {
  const test = mount();
  test.render(320, false, { sidebar: false, aside: true });
  act(() => test.aside().setWidth(600));
  test.viewport(600);
  test.render(336, true, { sidebar: false, aside: true });
  expect(test.aside().width).toBe(336);
  test.render(320, true, { sidebar: false, aside: false });
  expect(test.aside().width).toBe(600);
  expect(test.aside().max).toBe(720);
  test.viewport(1440);
  test.render(320, false, { sidebar: false, aside: true });
  expect(test.aside().width).toBe(600);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe("600");
});
