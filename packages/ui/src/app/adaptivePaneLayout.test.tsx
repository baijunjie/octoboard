// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import type { Console } from "../protocol";
import { usePaneLayout } from "./usePaneLayout";
import { usePaneWidths } from "./usePaneWidths";

vi.mock("../process/useProcessPanel", () => ({ useProcessPanel: () => ({ trees: [] }) }));

it("docks only the open columns, protects 400px and restores preferences after resizing", () => {
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-phone", "594px");
  document.documentElement.style.setProperty("--breakpoint-docked", "1394px");
  const viewport = (width: number) => act(() => {
    Object.defineProperty(document.documentElement, "clientWidth", { configurable: true, value: width });
    window.dispatchEvent(new Event("resize"));
  });
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query.includes("min-width") ? document.documentElement.clientWidth >= Number(/([\d.]+)px/.exec(query)?.[1]) : false,
    addEventListener: () => {}, removeEventListener: () => {},
  }));
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
  viewport(707);
  expect(layout.isNarrow).toBe(true);
  expect(layout.panes.sidebarDocked).toBe(true);
  viewport(708);
  expect(layout.isNarrow).toBe(false);
  expect(layout.panes.sidebarShown).toBe(true);
  viewport(593);
  expect(layout.asideFloats).toBe(true);
  vi.unstubAllGlobals();
});
