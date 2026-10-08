// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Project } from "../protocol";
import { type SidebarView, useSidebarView } from "./sidebarView";
import type { FocusTarget } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-1", "Main");
const other = consoleOf("c-2", "Other");
const project = projectOf("p-1", main.id, "Search API");
const hub = sessionOf("s-hub", main.id, undefined, "Hub", "idle", { colour: "teal" });
const bound = sessionOf("s-bound", main.id, project.id, "Bound", "idle", { bound_to: hub.id });
const unbound = sessionOf("s-unbound", main.id, project.id, "Unbound", "idle");
const byId = <T extends { id: string }>(items: T[]) => new Map(items.map((item) => [item.id, item]));
const projects = byId([project, projectOf("p-2", other.id, "Elsewhere")]);
const sessions = byId([hub, bound, unbound, { ...hub, id: "s-archived-hub", status: "archived" as const }]);

const projectFocus: FocusTarget = { project };
const hubFocus: FocusTarget = { consoleSession: hub };

/** Mounts `useSidebarView` over the fixtures and gives the latest view back. */
function mountView(): { view: () => SidebarView; unmount: () => void } {
  let latest!: SidebarView;
  function Probe() {
    latest = useSidebarView([main, other], projects as Map<string, Project>, sessions);
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Probe />));
  return { view: () => latest, unmount: () => (act(() => root.unmount()), container.remove()) };
}

describe("the remembered focus mode", () => {
  it.each([projectFocus, hubFocus])("is left by switching console, and comes back only with its own console", (focus) => {
    const { view, unmount } = mountView();
    act(() => view().selectConsole(main.id));
    act(() => view().setFocus(focus));
    expect(view().focus).toBeDefined();
    act(() => view().selectConsole(other.id));
    expect(view().focus).toBeUndefined();
    act(() => view().selectConsole(main.id));
    expect(view().focus).toBeUndefined();
    unmount();
  });
});

it("forgets a console session that was archived while in focus, so reopening it does not bring focus mode back", () => {
  let all = sessions;
  let latest!: SidebarView;
  function Probe() {
    latest = useSidebarView([main, other], projects as Map<string, Project>, all);
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Probe />));
  act(() => latest.selectConsole(main.id));
  act(() => latest.setFocus(hubFocus));
  all = byId([{ ...hub, status: "archived" as const }, bound, unbound]);
  act(() => root.render(<Probe />));
  expect(latest.focus).toBeUndefined();
  all = sessions;
  act(() => root.render(<Probe />));
  expect(latest.focus).toBeUndefined();
  act(() => root.unmount());
  container.remove();
});
