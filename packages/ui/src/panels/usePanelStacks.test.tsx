// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { PREFERENCE_KEYS } from "../preferenceKeys";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  localStorage.clear();
  vi.resetModules();
});

/** Loads the module afresh, as a page load does, since it reads storage as it is evaluated. */
async function load(reportSession?: string) {
  const { usePanelStacks } = await import("./usePanelStacks");
  let latest!: ReturnType<typeof usePanelStacks>;
  function Probe({ session }: { session: string | undefined }) {
    latest = usePanelStacks(session);
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Probe session={reportSession} />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const read = () => latest;
  /** Gives the hook another session's Report panel, as selecting another session does. */
  read.switchTo = (session: string | undefined) => act(() => root.render(<Probe session={session} />));
  return read;
}

it("opens every owner's kinds on a first load", async () => {
  const stacks = (await load())().stacks;
  expect(stacks.project.open).toEqual(["files"]);
  expect(stacks.report.open).toEqual(["report"]);
});

it("carries a pane the old visibility preference hid over as no panel open, but only until a stack is stored", async () => {
  localStorage.setItem(PREFERENCE_KEYS.asideVisible, "false");
  const latest = await load();
  expect(latest().stacks.project.open).toEqual([]);
  expect(latest().stacks.report.open).toEqual([]);
  act(() => latest().setStack("project", { open: ["files"], shares: {} }));
  vi.resetModules();
  expect((await load())().stacks.project.open).toEqual(["files"]);
});

it("keeps the stack of one owner when the other's changes, and writes it for the next load", async () => {
  const latest = await load();
  act(() => latest().setStack("report", { open: [], shares: {} }));
  expect(latest().stacks.project.open).toEqual(["files"]);
  vi.resetModules();
  const next = (await load())().stacks;
  expect(next.report.open).toEqual([]);
  expect(next.project.open).toEqual(["files"]);
});

it("carries a project session's Report panel in the project's stack, open until the user closes it for that session", async () => {
  const latest = await load("session-a");
  expect(latest().stacks.project.open).toEqual(["report", "files"]);
  expect(latest().stacks.report.open).toEqual(["report"]);

  act(() => latest().setStack("project", { open: ["files"], shares: {} }));
  expect(latest().stacks.project.open).toEqual(["files"]);
  // Another session's Report panel is still open, and the project's other panels are as they were.
  latest.switchTo("session-b");
  expect(latest().stacks.project.open).toEqual(["report", "files"]);
  act(() => latest().setStack("project", { open: ["terminal", "report"], shares: {} }));
  latest.switchTo("session-a");
  expect(latest().stacks.project.open).toEqual(["terminal"]);
  // With no session whose Report panel the stack carries, it has none.
  latest.switchTo(undefined);
  expect(latest().stacks.project.open).toEqual(["terminal"]);
  act(() => latest().setStack("project", { open: ["files", "terminal"], shares: {} }));
  latest.switchTo("session-b");
  expect(latest().stacks.project.open).toEqual(["report", "terminal", "files"]);
});

it("never stores the Report panel in the project's stack, and keeps which sessions closed theirs for the next load", async () => {
  const latest = await load("session-a");
  act(() => latest().setStack("project", { open: ["report", "files"], shares: { files: 0.5, report: 0.5 } }));
  act(() => latest().setStack("project", { open: ["files"], shares: {} }));
  expect(JSON.parse(localStorage.getItem(PREFERENCE_KEYS.panelStacks)!).project.open).toEqual(["files"]);
  expect(JSON.parse(localStorage.getItem(PREFERENCE_KEYS.closedReportPanels)!)).toEqual(["session-a"]);

  vi.resetModules();
  const next = await load("session-a");
  expect(next().stacks.project.open).toEqual(["files"]);
  act(() => next().setStack("project", { open: ["report", "files"], shares: {} }));
  expect(localStorage.getItem(PREFERENCE_KEYS.closedReportPanels)).toBeNull();
});

it("remembers only the most recent closed Report panels", async () => {
  const latest = await load("session-0");
  for (let index = 0; index < 205; index++) {
    latest.switchTo(`session-${index}`);
    act(() => latest().setStack("project", { open: ["files"], shares: {} }));
  }
  const closed: string[] = JSON.parse(localStorage.getItem(PREFERENCE_KEYS.closedReportPanels)!);
  expect(closed).toHaveLength(200);
  expect(closed.at(-1)).toBe("session-204");
  expect(closed).not.toContain("session-0");
});
