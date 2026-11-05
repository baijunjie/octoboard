// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import type { Console } from "../protocol";
import { usePaneLayout } from "./usePaneLayout";
import { usePaneWidths } from "./usePaneWidths";
import { matchesWidthQuery } from "../test/viewport";

vi.mock("../process/useProcessPanel", () => ({ useProcessPanel: () => ({ trees: [] }) }));

it("docks only the open columns, protects 400px and restores preferences after resizing", () => {
  const globals = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
  const actEnvironment = globals.IS_REACT_ACT_ENVIRONMENT;
  globals.IS_REACT_ACT_ENVIRONMENT = true;
  const previousWidth = Object.getOwnPropertyDescriptor(document.documentElement, "clientWidth");
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  const listeners = new Set<() => void>();
  const viewport = (width: number) => act(() => {
    Object.defineProperty(document.documentElement, "clientWidth", { configurable: true, value: width });
    window.dispatchEvent(new Event("resize"));
    for (const listener of [...listeners]) listener();
  });
  vi.stubGlobal("matchMedia", (query: string) => ({
    get matches() { return matchesWidthQuery(query, document.documentElement.clientWidth); },
    addEventListener: (_: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
  }));
  onTestFinished(() => {
    vi.unstubAllGlobals();
    if (actEnvironment === undefined) delete globals.IS_REACT_ACT_ENVIRONMENT;
    else globals.IS_REACT_ACT_ENVIRONMENT = actEnvironment;
    if (previousWidth) Object.defineProperty(document.documentElement, "clientWidth", previousWidth);
    else Reflect.deleteProperty(document.documentElement, "clientWidth");
    for (const name of ["--rail-width", "--breakpoint-phone", "--breakpoint-tablet"]) document.documentElement.style.removeProperty(name);
    localStorage.clear();
  });
  viewport(1168);
  const currentConsole = { id: "c" } as Console;
  let layout!: ReturnType<typeof usePaneLayout>;
  let widths!: ReturnType<typeof usePaneWidths>;
  function Probe() {
    layout = usePaneLayout({ asideOwner: { kind: "project", project: "p" }, hasAside: true,
      asideProjectId: "p", consoles: new Map([["c", currentConsole]]), currentConsole,
      selectedSession: undefined, selectedSessionId: undefined, focusTerminal: () => {},
    });
    widths = usePaneWidths({ noConsoles: false, isNarrow: layout.isNarrow,
      sidebarDocked: layout.panes.sidebarDocked, stackedPanelCount: layout.stackedPanels.length,
      stackedPanelKinds: layout.stackedPanels, asideOverlay: layout.asideOverlay,
      asideDocked: layout.asideDocked, paneShown: layout.paneShown, columnShown: layout.columnShown,
      asideFloats: layout.asideFloats, asidePeek: layout.panes.asidePeek, floatingKind: layout.floating.kind,
    });
    return null;
  }
  mountHost(<Probe />, { daemon: testDaemon() });
  act(() => {
    if (!layout.panes.sidebarDocked) layout.panes.toggleSidebar();
    if (!layout.asidePinned) layout.toggleAsideDocked();
    layout.setPanelStack("project", { open: ["files"], shares: {} });
    widths.sidebarWidth.reset(); widths.asideWidth.reset();
  });
  expect(layout.asideDocked).toBe(true);
  expect(layout.panes.sidebarShown).toBe(true);
  expect(widths.asideWidth.width).toBe(420);
  expect(widths.sidebarWidth.width).toBe(280);
  viewport(983);
  expect(layout.asideOverlay).toBe(true);
  viewport(984);
  expect(layout.asideDocked).toBe(true);
  expect(widths.sidebarWidth.width).toBe(260);
  expect(widths.asideWidth.width).toBe(276);
  viewport(1168);
  expect(widths.sidebarWidth.width).toBe(280);
  expect(widths.asideWidth.width).toBe(420);
  act(() => widths.asideWidth.setWidth(720));
  expect(widths.asideWidth.width).toBe(440);
  expect(1168 - 48 - widths.sidebarWidth.width - widths.asideWidth.width).toBe(400);
  act(() => layout.setPanelStack("project", { open: ["terminal", "files", "processes"], shares: {} }));
  expect(layout.asideOverlay).toBe(true);
  expect(layout.asidePinned).toBe(true);
  expect(layout.panes.sidebarShown).toBe(true);
  act(() => layout.panes.toggleSidebar());
  expect(layout.asideDocked).toBe(true);
  expect(widths.asideWidth.width).toBeGreaterThanOrEqual(546);
  viewport(993);
  expect(layout.asideOverlay).toBe(true);
  viewport(994);
  expect(layout.asideDocked).toBe(true);
  act(() => layout.panes.toggleSidebar());
  expect(layout.asideOverlay).toBe(true);
  viewport(1253);
  expect(layout.asideOverlay).toBe(true);
  viewport(1254);
  expect(layout.asideDocked).toBe(true);
  expect(widths.sidebarWidth.width).toBe(260);
  expect(widths.asideWidth.width).toBe(546);
  viewport(1274);
  expect(layout.asideDocked).toBe(true);
  expect(widths.sidebarWidth.width).toBe(280);
  expect(widths.asideWidth.width).toBe(546);
  const storedStack = layout.panelStacks.project;
  for (const width of [769, 768.5]) {
    viewport(width);
    expect(layout.asideFloats).toBe(false);
    expect(layout.isNarrow).toBe(false);
    expect(layout.panes.sidebarShown).toBe(true);
  }
  for (const width of [768, 767, 390, 389]) {
    viewport(width);
    expect(layout.asideFloats).toBe(true);
    expect(layout.isNarrow).toBe(true);
    expect(layout.panes.sidebarShown).toBe(false);
    expect(layout.asideDocked).toBe(false);
    expect(layout.panelStacks.project).toBe(storedStack);
  }
  viewport(1274);
  expect(layout.asideFloats).toBe(false);
  expect(layout.panes.sidebarShown).toBe(true);
  expect(layout.asideDocked).toBe(true);
  expect(layout.panelStacks.project).toBe(storedStack);
});
