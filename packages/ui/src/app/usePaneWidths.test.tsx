// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { usePaneWidths } from "./usePaneWidths";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(overlay = true, share = 0.5, initialKinds = ["files", "processes"]) {
  document.documentElement.style.setProperty("--rail-width", "48px");
  const viewport = (width: number) => act(() => {
    Object.defineProperty(document.documentElement, "clientWidth", { configurable: true, value: width });
    window.dispatchEvent(new Event("resize"));
  });
  viewport(1800);
  let widths!: ReturnType<typeof usePaneWidths>;
  const peek = { active: false, reveal: () => {}, keep: () => {}, leave: () => {}, hide: () => {} };
  function Probe({ kinds, maximized, owner }: { kinds: string[]; maximized: boolean; owner: string }) {
    widths = usePaneWidths({
      noConsoles: false,
      isNarrow: overlay,
      sidebarDocked: true,
      stackedPanelCount: kinds.length,
      stackedPanelKinds: kinds,
      columnShare: share,
      panelWidthKey: owner,
      maximized,
      asideOverlay: overlay,
      asideDocked: !overlay,
      paneShown: true,
      columnShown: true,
      asideFloats: false,
      asidePeek: peek,
      floatingKind: undefined,
    });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    localStorage.clear();
  });
  const render = (kinds: string[], maximized = false, owner = "a") => act(() => root.render(<Probe kinds={kinds} maximized={maximized} owner={owner} />));
  render(initialKinds);
  act(() => { widths.asideWidth.reset(); widths.sidebarWidth.reset(); });
  return { render, viewport, widths: () => widths };
}

it.each([
  { overlay: false, share: 0.5, single: 320, expanded: 634 },
  { overlay: true, share: 0.4, single: 400, expanded: 670 },
])("keeps the existing cards' width when adding and removing a column ($overlay, $share)", ({ overlay, share, single, expanded }) => {
  const test = mount(overlay, share);
  act(() => test.widths().asideWidth.setWidth(single));
  for (let cycle = 0; cycle < 2; cycle++) {
    test.render(["terminal", "files", "processes"]);
    expect(test.widths().asideWidth.width).toBeCloseTo(expanded);
    expect(test.widths().asideWidth.min).toBe(546);
    test.render(["files", "processes"]);
    expect(test.widths().asideWidth.width).toBeCloseTo(single);
  }
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe(String(single));
});

it("keeps the total width when a third card needs no extra column", () => {
  const test = mount();
  act(() => test.widths().asideWidth.setWidth(320));
  test.render(["report", "files", "processes"]);
  test.render(["files", "processes"]);
  act(() => test.widths().asideWidth.setWidth(720));
  for (let cycle = 0; cycle < 2; cycle++) {
    test.render(["terminal", "files", "processes"]);
    expect(test.widths().asideWidth.width).toBe(720);
    test.render(["files", "processes"]);
    expect(test.widths().asideWidth.width).toBe(720);
  }
});

it("preserves the surviving column after expansion reaches the docked maximum", () => {
  const test = mount(false);
  test.render(["terminal", "files", "processes"]);
  expect(test.widths().asideWidth.width).toBe(720);
  test.render(["files", "processes"]);
  expect(test.widths().asideWidth.width).toBe(363);
});

it("restores the expanded allocation after a viewport clamp without rewriting the preference", () => {
  const test = mount();
  test.render(["terminal", "files", "processes"]);
  expect(test.widths().asideWidth.width).toBe(834);
  test.viewport(650);
  expect(test.widths().asideWidth.width).toBe(602);
  test.viewport(1800);
  expect(test.widths().asideWidth.width).toBe(834);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBeNull();
});

it("does not remove a column's allocation while maximizing and restoring a card", () => {
  const test = mount();
  test.render(["terminal", "files", "processes"]);
  test.render(["files", "processes"], true);
  expect(test.widths().asideWidth.width).toBe(834);
  test.render(["terminal", "files", "processes"]);
  expect(test.widths().asideWidth.width).toBe(834);
  test.render(["files", "processes"]);
  expect(test.widths().asideWidth.width).toBe(420);
});

it.each([
  { overlay: false, share: 0.5, remaining: ["terminal", "files"] },
  { overlay: true, share: 0.5, remaining: ["terminal", "files"] },
  { overlay: true, share: 0.4, remaining: ["report", "terminal"] },
])("keeps minimum-width cards unchanged when closing a column ($overlay, $share)", ({ overlay, share, remaining }) => {
  const test = mount(overlay, share, ["report", "terminal", "files"]);
  expect(test.widths().asideWidth.width).toBe(546);
  test.render(remaining);
  expect(test.widths().asideWidth.width).toBe(276);
  expect(test.widths().asideWidth.min).toBe(276);
  test.render(["report", "terminal", "files"]);
  expect(test.widths().asideWidth.width).toBe(546);
});

it.each([
  { overlay: false, share: 0.5, width: 720, remaining: "report", collapsed: 363 },
  { overlay: true, share: 0.4, width: 546, remaining: "report", collapsed: 276 },
  { overlay: true, share: 0.4, width: 720, remaining: "files", collapsed: 432.4 },
  { overlay: false, share: 0.4, width: 720, remaining: "report", collapsed: 293.6 },
  { overlay: true, share: 0.5, width: 320, remaining: "report", collapsed: 320 },
])("restores a pair's previous column layout when reopening its closed card ($overlay, $share, $width, $remaining)", ({ overlay, share, width, remaining, collapsed }) => {
  const test = mount(overlay, share, ["report", "files"]);
  act(() => test.widths().asideWidth.setWidth(width));
  for (let cycle = 0; cycle < 2; cycle++) {
    test.render([remaining]);
    expect(test.widths().asideWidth.width).toBeCloseTo(collapsed);
    test.render(["report", "files"]);
    expect(test.widths().asideWidth.width).toBeCloseTo(width);
  }
});

it("shares the aside width across owners without inheriting another owner's column history", () => {
  const test = mount(false, 0.5, ["report", "files"]);
  act(() => test.widths().asideWidth.setWidth(720));
  test.render(["report"]);
  expect(test.widths().asideWidth.width).toBe(363);
  test.render(["report"], false, "b");
  expect(test.widths().asideWidth.width).toBe(363);
  test.render(["report", "processes"], false, "b");
  expect(test.widths().asideWidth.width).toBe(363);
});

it("uses the resized surviving card instead of the previous pair width when reopening", () => {
  const test = mount(true, 0.4, ["report", "files"]);
  act(() => test.widths().asideWidth.setWidth(546));
  test.render(["report"]);
  act(() => test.widths().asideWidth.setWidth(546));
  test.render(["report", "files"]);
  expect(test.widths().asideWidth.width).toBe(1351);
});

it("keeps the surviving card's width when a different pair moves it into the other column", () => {
  const test = mount(true, 0.4, ["report", "files"]);
  act(() => test.widths().asideWidth.setWidth(720));
  test.render(["files"]);
  expect(test.widths().asideWidth.width).toBeCloseTo(432.4);
  test.render(["files", "processes"]);
  expect(test.widths().asideWidth.width).toBeCloseTo(1067);
});

it("forgets the previous pair's exact width when every card closes", () => {
  const test = mount(true, 0.4, ["report", "files"]);
  act(() => test.widths().asideWidth.setWidth(620));
  test.render(["report"]);
  test.render([]);
  test.render(["report"]);
  expect(test.widths().asideWidth.width).toBe(276);
  test.render(["report", "files"]);
  expect(test.widths().asideWidth.width).toBe(546);
});

it("keeps the surviving cards stacked when switching away from a project's report", () => {
  const test = mount(false, 0.5, ["report", "files", "processes"]);
  act(() => test.widths().asideWidth.setWidth(720));
  test.render(["files", "processes"], false, "b");
  expect(test.widths().asideWidth.width).toBe(363);
});

it("keeps the collapsed width after closing a report and changing projects", () => {
  const test = mount(false, 0.5, ["report", "files", "processes"]);
  act(() => test.widths().asideWidth.setWidth(720));
  test.render(["files", "processes"]);
  expect(test.widths().asideWidth.width).toBe(363);
  test.render(["files", "processes"], false, "b");
  expect(test.widths().asideWidth.width).toBe(363);
});
