// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { forgetProjectsOtherThan, useProjectBrowserState, type ProjectBrowserState } from "./browserState";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(project: string): { states: ProjectBrowserState[]; unmount: () => void } {
  const states: ProjectBrowserState[] = [];
  function Probe(): null {
    states.push(useProjectBrowserState(project));
    return null;
  }
  const root = createRoot(document.createElement("div"));
  act(() => root.render(<Probe />));
  return { states, unmount: () => act(() => root.unmount()) };
}

it("keeps a project's setters through changes of its state", () => {
  const { states, unmount } = mount("stable");
  act(() => states[0].setSelectedChange("a"));
  act(() => states[0].setByCommit(true));
  act(() => states[0].setCollapsedChangeRows(new Set(["k"])));
  const last = states[states.length - 1];
  expect(last.selectedChange).toBe("a");
  expect(last.byCommit).toBe(true);
  expect([...last.collapsedChangeRows]).toEqual(["k"]);
  for (const state of states) {
    expect(state.setSelectedChange).toBe(states[0].setSelectedChange);
    expect(state.setByCommit).toBe(states[0].setByCommit);
    expect(state.setCollapsedChangeRows).toBe(states[0].setCollapsedChangeRows);
  }
  unmount();
  forgetProjectsOtherThan(new Set());
});

it("renders nothing again for a selection set to what it already is", () => {
  const { states, unmount } = mount("same");
  act(() => states[0].setBranch("topic"));
  const renders = states.length;
  act(() => states[0].setBranch("topic"));
  expect(states.length).toBe(renders);
  unmount();
  forgetProjectsOtherThan(new Set());
});

it("forgets the selections of a project that is gone", () => {
  const { states, unmount } = mount("gone");
  act(() => states[0].setSelected("README.md"));
  act(() => forgetProjectsOtherThan(new Set(["other"])));
  expect(states[states.length - 1].selected).toBeUndefined();
  unmount();
});
