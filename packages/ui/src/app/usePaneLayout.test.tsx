// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { resetAppPreferences } from "../appTestSupport";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { DEFAULT_SETTINGS } from "../store";
import type { ClientRequestBody, Console, ShellTerminal } from "../protocol";
import { usePaneLayout } from "./usePaneLayout";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const viewport = vi.hoisted(() => ({ floats: false, narrow: false }));
vi.mock("../layout/paneWidth", () => ({ useAsideFloats: () => viewport.floats, usePaneDocking: () => ({ sidebarDockable: !viewport.narrow, asideDockable: !viewport.narrow }) }));
vi.mock("../components/hooks/breakpoint", () => ({
  useIsNarrow: () => viewport.narrow,
  useMediaQuery: () => false,
}));
vi.mock("../process/useProcessPanel", () => ({ useProcessPanel: () => ({ trees: [] }) }));

const shell = (id: string, extra: Partial<ShellTerminal> = {}): ShellTerminal => ({
  id, project_id: "p", worktree: null, cwd: "/p", title: id, renamed: false,
  state: "running", busy: false, created_at: 0, ...extra,
});

function mount(enabled: boolean, { floats = false, narrow = false } = {}) {
  onTestFinished(resetAppPreferences);
  viewport.floats = floats;
  viewport.narrow = narrow;
  const request = vi.fn(async (_body: ClientRequestBody) => ({ type: "ack" as const, id: null }));
  const terminals = [shell("idle"), shell("exited", { state: "exited" }), shell("busy", { busy: true }), shell("other", { project_id: "other" })];
  const daemon = testDaemon({ state: {
    settings: { ...DEFAULT_SETTINGS, auto_close_idle_shell_terminals: enabled },
    shellTerminals: new Map(terminals.map((terminal) => [terminal.id, terminal])),
  }, request });
  let layout!: ReturnType<typeof usePaneLayout>;
  const owner = { kind: "project" as const, project: "p" };
  const currentConsole = { id: "c" } as Console;
  function Probe({ project = "p" }: { project?: string }) {
    layout = usePaneLayout({
      asideOwner: { ...owner, project }, hasAside: true, asideProjectId: project,
      consoles: new Map([["c", currentConsole]]), currentConsole,
      selectedSession: undefined, selectedSessionId: undefined, focusTerminal: () => {},
    });
    return null;
  }
  const host = mountHost(<Probe />, { daemon });
  act(() => layout.setPanelStack("project", { open: ["terminal"], shares: {} }));
  if (floats) act(() => layout.openFloating("project", "terminal"));
  request.mockClear();
  return { layout: () => layout, request, daemon, render: (project = "p") => { host.render(<Probe project={project} />); } };
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
