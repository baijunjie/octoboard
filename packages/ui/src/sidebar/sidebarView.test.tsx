// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Project } from "../protocol";
import { belongsToFocus, resolveFocus, shortcutOutcome, type SidebarView, useSidebarView } from "./sidebarView";
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

describe("resolveFocus", () => {
  it.each([
    ["a project", "project:p-1", main.id, projectFocus],
    ["a console session", "consoleSession:s-hub", main.id, hubFocus],
    ["a project that no longer exists", "project:gone", main.id, undefined],
    ["a console session that no longer exists", "consoleSession:gone", main.id, undefined],
    ["a project of another console", "project:p-2", main.id, undefined],
    ["a console session of another console", "consoleSession:s-hub", other.id, undefined],
    ["an archived console session", "consoleSession:s-archived-hub", main.id, undefined],
    ["a project session's id as a console session", "consoleSession:s-bound", main.id, undefined],
    ["nothing stored", undefined, main.id, undefined],
    ["an unreadable value", "p-1", main.id, undefined],
  ])("%s", (_, key, consoleId, expected) => {
    const focus = resolveFocus(key, consoleId, projects, sessions);
    expect(focus && ("project" in focus ? focus.project.id : focus.consoleSession.id)).toBe(
      expected && ("project" in expected ? expected.project.id : expected.consoleSession.id),
    );
  });
});

describe("belongsToFocus", () => {
  const archivedBound = { ...bound, status: "archived" as const };
  it.each([
    ["a project", projectFocus, unbound, true],
    ["a project", projectFocus, bound, false],
    ["a project", projectFocus, archivedBound, true],
    ["a project", projectFocus, hub, false],
    ["a console session", hubFocus, bound, true],
    ["a console session", hubFocus, hub, true],
    ["a console session", hubFocus, unbound, false],
    ["a console session", hubFocus, { ...bound, bound_to: "s-other" }, false],
  ])("in the focus mode of %s, selecting %#", (_, focus, selected, belongs) => {
    expect(belongsToFocus(focus, selected)).toBe(belongs);
  });
});

describe("shortcutOutcome", () => {
  it.each([
    ["enters the project of a selected project session", undefined, unbound, projectFocus],
    ["enters the project of a selected bound session", undefined, bound, projectFocus],
    ["enters a selected console session itself", undefined, hub, hubFocus],
    ["does nothing for an archived console session", undefined, { ...hub, status: "archived" as const }, null],
    ["does nothing with nothing selected", undefined, undefined, null],
    ["does nothing for a session of an unknown project", undefined, { ...unbound, project_id: "gone" }, null],
    ["leaves a project's focus mode", projectFocus, unbound, undefined],
    ["leaves a console session's focus mode", hubFocus, hub, undefined],
    ["leaves focus mode with nothing selected", projectFocus, undefined, undefined],
  ])("%s", (_, focus, selected, expected) => {
    const outcome = shortcutOutcome(focus, selected, projects);
    if (expected === null) expect(outcome).toBeUndefined();
    else expect(outcome?.focus && ("project" in outcome.focus ? outcome.focus.project.id : outcome.focus.consoleSession.id)).toBe(
      expected && ("project" in expected ? expected.project.id : expected.consoleSession.id),
    );
  });
});

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
