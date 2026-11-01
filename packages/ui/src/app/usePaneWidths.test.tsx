// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { PREFERENCE_KEYS } from "../preferenceKeys";
import { usePaneWidths } from "./usePaneWidths";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("expands three docked or floating panels without replacing the remembered single-column width", () => {
  document.documentElement.style.setProperty("--rail-width", "48px");
  Object.defineProperty(document.documentElement, "clientWidth", { configurable: true, value: 1440 });
  let widths!: ReturnType<typeof usePaneWidths>;
  const peek = { active: false, reveal: () => {}, keep: () => {}, leave: () => {}, hide: () => {} };
  function Probe({ count, overlay }: { count: number; overlay: boolean }) {
    widths = usePaneWidths({
      noConsoles: false,
      isNarrow: overlay,
      sidebarDocked: true,
      terminalPanelStacked: true,
      stackedPanelCount: count,
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
  const render = (count: number, overlay = false) => act(() => root.render(<Probe count={count} overlay={overlay} />));
  render(2);
  act(() => { widths.asideWidth.setWidth(320); widths.sidebarWidth.reset(); });
  expect(widths.asideWidth.width).toBe(320);
  render(3);
  expect(widths.asideWidth.width).toBe(546);
  expect(widths.asideWidth.min).toBe(546);
  expect(widths.sidebarWidth.width).toBe(280);
  expect(localStorage.getItem(PREFERENCE_KEYS.asideWidth)).toBe("320");
  render(4, true);
  expect(widths.asideWidth.width).toBe(546);
  expect(widths.asideLayout.peek).toBe(peek);
  render(2);
  expect(widths.asideWidth.width).toBe(320);
  expect(widths.asideLayout.peek).toBeUndefined();
});
