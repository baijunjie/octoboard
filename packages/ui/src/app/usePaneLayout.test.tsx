// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { resetAppPreferences } from "../appTestSupport";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { visibility } from "../layout/panelVisibility";
import { DEFAULT_SETTINGS } from "../store";
import type { ClientRequestBody, Console, ShellTerminal } from "../protocol";
import { usePaneLayout } from "./usePaneLayout";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const viewport = vi.hoisted(() => ({ floats: false, narrow: false, noHover: false }));
vi.mock("../layout/paneWidth", () => ({ useAsideFloats: () => viewport.floats, usePaneDocking: () => ({ sidebarDockable: !viewport.narrow, asideDockable: !viewport.narrow }) }));
vi.mock("../components/hooks/breakpoint", () => ({
  useIsNarrow: () => viewport.narrow,
  useMediaQuery: () => viewport.noHover,
}));
vi.mock("../process/useProcessPanel", () => ({ useProcessPanel: () => ({ trees: [] }) }));

const shell = (id: string, extra: Partial<ShellTerminal> = {}): ShellTerminal => ({
  id, project_id: "p", worktree: null, cwd: "/p", title: id, renamed: false,
  state: "running", busy: false, created_at: 0, ...extra,
});

function mount(enabled: boolean, { floats = false, narrow = false, pinned = true, noHover = false } = {}) {
  onTestFinished(resetAppPreferences);
  viewport.floats = floats;
  viewport.narrow = narrow;
  viewport.noHover = noHover;
  visibility.aside.set(pinned);
  const request = vi.fn(async (_body: ClientRequestBody) => ({ type: "ack" as const, id: null }));
  const terminals = [shell("idle"), shell("exited", { state: "exited" }), shell("busy", { busy: true }), shell("other", { project_id: "other" })];
  const daemon = testDaemon({ state: {
    settings: { ...DEFAULT_SETTINGS, auto_close_idle_shell_terminals: enabled },
    shellTerminals: new Map(terminals.map((terminal) => [terminal.id, terminal])),
  }, request });
  let layout!: ReturnType<typeof usePaneLayout>;
  const owner = { kind: "project" as const, project: "p" };
  const currentConsole = { id: "c" } as Console;
  function Probe({ project = "p", report = false }: { project?: string; report?: boolean }) {
    layout = usePaneLayout({
      asideOwner: report ? { kind: "report", consoleSession: "s" } : { ...owner, project }, hasAside: true, asideProjectId: report ? undefined : project,
      consoles: new Map([["c", currentConsole]]), currentConsole,
      selectedSession: undefined, selectedSessionId: undefined, focusTerminal: () => {},
    });
    return null;
  }
  const host = mountHost(<Probe />, { daemon });
  act(() => layout.setPanelStack("project", { open: ["terminal"], shares: {} }));
  if (floats) act(() => layout.openFloating("project", "terminal"));
  request.mockClear();
  return {
    layout: () => layout, request, daemon,
    render: (project = "p") => { host.render(<Probe project={project} />); },
    renderReport: () => { host.render(<Probe report />); },
  };
}

it.each([false, true])("only cleans up on explicit closes when enabled: %s", async (enabled) => {
  const test = mount(enabled);
  await act(async () => test.layout().togglePanel("terminal"));
  expect(test.request.mock.calls).toHaveLength(enabled ? 3 : 0);
  if (enabled) expect(test.request.mock.calls.map(([body]) => body)).toEqual([
    { type: "close_shell_terminal", shell_terminal: "idle", only_if_idle: true },
    { type: "close_shell_terminal", shell_terminal: "exited", only_if_idle: true },
    { type: "close_shell_terminal", shell_terminal: "busy", only_if_idle: true },
  ]);
});

it("cleans up on an explicit header close", async () => {
  const test = mount(true);
  await act(async () => test.layout().notePanelClosed("terminal"));
  expect(test.request).toHaveBeenCalledTimes(3);
});

it.each(["toggle", "close"])("cleans up an explicitly closed narrow panel: %s", async (how) => {
  const test = mount(true, { floats: true });
  await act(async () => how === "toggle" ? test.layout().togglePanel("terminal") : test.layout().floating.close());
  expect(test.request).toHaveBeenCalledTimes(3);
});

it("does not clean up when archive navigation hides a narrow panel", () => {
  const test = mount(true, { floats: true });
  act(() => test.layout().revealArchive());
  expect(test.request).not.toHaveBeenCalled();
});

it("does not clean up when hover reveal ends", () => {
  const test = mount(true, { narrow: true });
  act(() => test.layout().stableHoverPanelToggle("terminal", true));
  act(() => test.layout().panes.asidePeek.hide());
  expect(test.request).not.toHaveBeenCalled();
});

it("does not clean up when the project or viewport changes", () => {
  const test = mount(true);
  test.render("other");
  viewport.floats = true;
  test.render("other");
  viewport.floats = false;
  test.render("p");
  expect(test.request).not.toHaveBeenCalled();
});

it.each(["empty", "open", "maximized"] as const)("does not reveal panels on hover while docked with an %s column", (state) => {
  const test = mount(false);
  act(() => test.layout().setPanelStack("project", {
    open: state === "empty" ? [] : ["terminal"], shares: {},
    ...(state === "maximized" ? { maximized: "terminal" } : {}),
  }));
  const before = test.layout().openPanels;
  for (const kind of ["terminal", "files"] as const) {
    act(() => test.layout().stableHoverPanelToggle(kind, true));
    expect(test.layout().panes.asidePeek.active).toBe(false);
    expect(test.layout().openPanels).toEqual(before);
    expect(test.layout().visibleProjectPanels?.kinds).toEqual(before);
  }
  act(() => test.layout().stableHoverAside(true));
  expect(test.layout().panes.asidePeek.active).toBe(false);
});

it.each(["manual", "automatic"] as const)("does not reveal an empty %s floating group from any hover entry", (mode) => {
  const test = mount(false, { pinned: mode !== "manual", narrow: mode === "automatic" });
  act(() => test.layout().setPanelStack("project", { open: [], shares: {} }));
  expect(test.layout().asideOverlay).toBe(true);
  act(() => test.layout().stableHoverAside(true));
  expect(test.layout().panes.asidePeek.active).toBe(false);
  act(() => test.layout().stableHoverPanelToggle("terminal", true));
  expect(test.layout().panes.asidePeek.active).toBe(false);
  expect(test.layout().visibleProjectPanels?.kinds).toEqual([]);
  expect(test.request).not.toHaveBeenCalled();
});

it.each(["manual", "automatic"] as const)("reveals only open cards when hovering a closed toggle in a %s floating group", (mode) => {
  const test = mount(false, { pinned: mode !== "manual", narrow: mode === "automatic" });
  act(() => test.layout().setPanelStack("project", { open: ["files", "processes"], shares: {} }));
  act(() => test.layout().stableHoverPanelToggle("terminal", true));
  expect(test.layout().panes.asidePeek.active).toBe(true);
  expect(test.layout().openPanels).toEqual(["files", "processes"]);
  expect(test.layout().visibleProjectPanels?.kinds).toEqual(["files", "processes"]);
  expect(test.request).not.toHaveBeenCalled();
  act(() => test.layout().panes.asidePeek.hide());
  act(() => test.layout().stableHoverAside(true));
  expect(test.layout().panes.asidePeek.active).toBe(true);
  expect(test.layout().visibleProjectPanels?.kinds).toEqual(["files", "processes"]);
});

it("keeps an empty automatically floating group hidden on repeated group presses", () => {
  const test = mount(false, { narrow: true });
  act(() => test.layout().setPanelStack("project", { open: [], shares: {} }));
  for (let press = 0; press < 2; press += 1) {
    act(() => test.layout().toggleAsideDocked());
    expect(test.layout().panes.asidePeek.active).toBe(false);
    expect(test.layout().visibleProjectPanels?.kinds).toEqual([]);
    expect(test.layout().asidePinned).toBe(true);
  }
});

it("keeps an empty group hidden when switching to manual floating", () => {
  const test = mount(false);
  act(() => test.layout().setPanelStack("project", { open: [], shares: {} }));
  act(() => test.layout().toggleAsideDocked());
  expect(test.layout().asideOverlay).toBe(true);
  expect(test.layout().panes.asidePeek.active).toBe(false);
  act(() => test.layout().stableHoverAside(true));
  expect(test.layout().panes.asidePeek.active).toBe(false);
});

it.each(["manual", "automatic"] as const)("opens on a click and does not reveal a preview after closing the last %s floating card", (mode) => {
  const test = mount(false, { pinned: mode !== "manual", narrow: mode === "automatic" });
  act(() => test.layout().setPanelStack("project", { open: [], shares: {} }));
  act(() => test.layout().togglePanel("files"));
  expect(test.layout().panes.asidePeek.active).toBe(true);
  expect(test.layout().visibleProjectPanels?.kinds).toEqual(["files"]);
  act(() => test.layout().togglePanel("files"));
  expect(test.layout().panes.asidePeek.active).toBe(false);
  expect(test.layout().openPanels).toEqual([]);
  act(() => test.layout().stableHoverPanelToggle("terminal", true));
  act(() => test.layout().stableHoverAside(true));
  expect(test.layout().panes.asidePeek.active).toBe(false);
  expect(test.layout().visibleProjectPanels?.kinds).toEqual([]);
});

it("opens a docked panel by clicking its toggle after hover left it closed", () => {
  const test = mount(false);
  act(() => test.layout().setPanelStack("project", { open: [], shares: {} }));
  act(() => test.layout().stableHoverPanelToggle("files", true));
  expect(test.layout().openPanels).toEqual([]);
  act(() => test.layout().togglePanel("files"));
  expect(test.layout().asideDocked).toBe(true);
  expect(test.layout().visibleProjectPanels?.kinds).toEqual(["files"]);
});

it("does not reveal a floating group on hover without a hover-capable input", () => {
  const test = mount(false, { pinned: false, noHover: true });
  act(() => test.layout().stableHoverAside(true));
  act(() => test.layout().stableHoverPanelToggle("terminal", true));
  expect(test.layout().panes.asidePeek.active).toBe(false);
  expect(test.layout().openPanels).toEqual(["terminal"]);
  act(() => test.layout().togglePanel("terminal"));
  expect(test.layout().panes.asidePeek.active).toBe(true);
  expect(test.layout().visibleProjectPanels?.kinds).toEqual(["terminal"]);
});

it("keeps mobile panels controlled by clicks rather than hover", () => {
  const test = mount(false, { floats: true });
  act(() => test.layout().floating.close());
  act(() => test.layout().stableHoverAside(true));
  act(() => test.layout().stableHoverPanelToggle("files", true));
  expect(test.layout().panes.asidePeek.active).toBe(false);
  expect(test.layout().floating.kind).toBeUndefined();
  act(() => test.layout().togglePanel("files"));
  expect(test.layout().floating.kind).toBe("files");
  expect(test.layout().panelStacks.project.open).toEqual(["terminal"]);
});

it("does not carry a revealed owner's card into another owner's empty stack", () => {
  const test = mount(false, { pinned: false });
  act(() => {
    test.layout().setPanelStack("project", { open: [], shares: {} });
    test.layout().setPanelStack("report", { open: ["processes"], shares: {} });
  });
  test.renderReport();
  act(() => test.layout().stableHoverAside(true));
  expect(test.layout().panes.asidePeek.active).toBe(true);
  expect(test.layout().visibleProjectPanels).toBeUndefined();
  test.render();
  expect(test.layout().peekKind).toBeUndefined();
  expect(test.layout().panes.asidePeek.active).toBe(false);
  expect(test.layout().visibleProjectPanels?.kinds).toEqual([]);
});

it("does not retain a hovered card when widening restores a docked maximized panel", () => {
  const test = mount(false, { narrow: true });
  act(() => test.layout().setPanelStack("project", { open: ["terminal"], shares: {}, maximized: "terminal" }));
  act(() => test.layout().stableHoverPanelToggle("terminal", true));
  expect(test.layout().asideOverlay).toBe(true);
  expect(test.layout().panes.asidePeek.active).toBe(true);
  expect(test.layout().peekKind).toBe("terminal");
  viewport.narrow = false;
  test.render();
  expect(test.layout().asideOverlay).toBe(false);
  expect(test.layout().covered).toBe(true);
  expect(test.layout().columnShown).toBe(false);
  expect(test.layout().peekKind).toBeUndefined();
  expect(test.layout().panes.asidePeek.active).toBe(false);
});
