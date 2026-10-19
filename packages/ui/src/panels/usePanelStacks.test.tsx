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
async function load() {
  const { usePanelStacks } = await import("./usePanelStacks");
  let latest!: ReturnType<typeof usePanelStacks>;
  function Probe() {
    latest = usePanelStacks();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Probe />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return () => latest;
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
