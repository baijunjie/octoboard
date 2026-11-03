// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished } from "vitest";

import { MIN_PANEL_COLUMN_WIDTH, MIN_TWO_COLUMN_ASIDE_WIDTH } from "./panelColumnWidth";
import { PaneResizeHandle } from "./PaneResizeHandle";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
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

it("caps a floating pane to the viewport and restores the saved width when widened", () => {
  const test = mount();
  test.render(320, false, { sidebar: false, aside: true });
  act(() => test.aside().setWidth(600));
  test.viewport(600);
  test.render(336, true, { sidebar: false, aside: true });
  expect(test.aside().width).toBe(336);
  test.render(320, true, { sidebar: false, aside: false });
  expect(test.aside().width).toBe(552);
  expect(test.aside().max).toBe(552);
  test.viewport(1440);
  test.render(320, false, { sidebar: false, aside: true });
  expect(test.aside().width).toBe(600);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe("600");
});

it("lets a floating pane reach the docked sidebar and follows changes to that boundary", () => {
  const test = mount();
  test.render(320, false, { sidebar: true, aside: false });
  act(() => test.aside().setWidth(2000));
  expect(test.aside().width).toBe(1112);
  expect(test.aside().max).toBe(1112);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe("1112");

  act(() => test.sidebar().setWidth(480));
  expect(test.aside().width).toBe(912);
  expect(test.aside().max).toBe(912);
  test.render(320, false, { sidebar: false, aside: false });
  expect(test.aside().width).toBe(1112);
  expect(test.aside().max).toBe(1392);

  const handle = test.container.querySelector<HTMLElement>("[role=separator]")!;
  act(() => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })));
  expect(test.aside().width).toBe(1392);
  expect(handle.getAttribute("aria-valuemax")).toBe("1392");
});

it("keeps a floating width above 720 across docking and viewport changes", () => {
  const test = mount();
  test.viewport(1200);
  test.render(320, true, { sidebar: false, aside: false });
  act(() => test.aside().setWidth(1000));
  expect(test.aside().width).toBe(1000);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe("1000");

  test.viewport(600);
  expect(test.aside().width).toBe(552);
  test.viewport(1800);
  test.render();
  expect(test.aside().width).toBe(720);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe("1000");
  test.render(320, false, { sidebar: true, aside: false });
  expect(test.aside().width).toBe(1000);
});

// Below this breakpoint a two-column floating group no longer fits beside the rail.
it("sets the phone breakpoint to the rail and the minimum two-column floating group", () => {
  const style = readFileSync(join(import.meta.dirname, "../style.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const token = (name: string) => Number(new RegExp(`--${name}:\\s*([\\d.]+)px`).exec(style)?.[1]);
  expect(MIN_TWO_COLUMN_ASIDE_WIDTH).toBe(2 * MIN_PANEL_COLUMN_WIDTH + token("panel-stack-gap") + token("panel-grip-gap") + token("peek-gap"));
  expect(token("breakpoint-phone")).toBe(token("rail-width") + MIN_TWO_COLUMN_ASIDE_WIDTH);
});

it("sets the docked breakpoint to the rail and the minimum widths of all three panes", () => {
  const style = readFileSync(join(import.meta.dirname, "../style.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const token = (name: string) => Number(new RegExp(`--${name}:\\s*([\\d.]+)px`).exec(style)?.[1]);
  expect(token("breakpoint-docked")).toBe(token("rail-width") + 280 + 520 + MIN_TWO_COLUMN_ASIDE_WIDTH);
});
