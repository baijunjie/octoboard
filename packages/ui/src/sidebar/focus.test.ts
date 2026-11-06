import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { belongsToFocus, cycleConsoleSession, focusAfterSelect, focusFor, focusKey, focusTargetId, resolveFocus, selectedSessionFocusTarget } from "./focus";
import type { FocusTarget } from "./types";

const main = consoleOf("c-1", "Main");
const other = consoleOf("c-2", "Other");
const project = projectOf("p-1", main.id, "Search API");
const hub = sessionOf("s-hub", main.id, undefined, "Hub", "idle", { colour: "teal" });
const bound = sessionOf("s-bound", main.id, project.id, "Bound", "idle", { bound_to: hub.id });
const unbound = sessionOf("s-unbound", main.id, project.id, "Unbound", "idle");
// `bound` is a bound coordinator once it has sessions of its own; `underCoordinator` is one of them.
const underCoordinator = sessionOf("s-under-coordinator", main.id, project.id, "Under the bound coordinator", "idle", { bound_to: bound.id });
const byId = <T extends { id: string }>(items: T[]) => new Map(items.map((item) => [item.id, item]));
const projects = byId([project, projectOf("p-2", other.id, "Elsewhere")]);
// An archived bound coordinator, which the console session's archive reaches, unlike the archived
// sessions of a bound coordinator that is still live.
const archivedCoordinator = sessionOf("s-archived-coordinator", main.id, project.id, "Archived coordinator", "archived", { bound_to: hub.id });
const sessions = byId([hub, bound, unbound, underCoordinator, archivedCoordinator, { ...hub, id: "s-archived-hub", status: "archived" as const }]);

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
  const archivedBoundUnderLive = { ...underCoordinator, status: "archived" as const };
  const archivedUnderArchivedCoordinator = sessionOf("s-archived-under-coordinator", main.id, project.id, "Archived under the coordinator", "archived", { bound_to: archivedCoordinator.id });
  it.each([
    ["a project", projectFocus, unbound, true],
    ["a project", projectFocus, bound, true],
    ["a project", projectFocus, archivedBound, true],
    ["a project", projectFocus, { ...bound, bound_to: unbound.id }, true],
    ["a project", projectFocus, underCoordinator, true],
    ["a project", projectFocus, { ...underCoordinator, status: "archived" as const }, true],
    ["a project", projectFocus, hub, false],
    ["a console session", hubFocus, bound, true],
    ["a console session", hubFocus, underCoordinator, true],
    ["a console session", hubFocus, archivedBoundUnderLive, false],
    ["a console session", hubFocus, archivedCoordinator, true],
    ["a console session", hubFocus, archivedUnderArchivedCoordinator, true],
    ["a console session", hubFocus, hub, true],
    ["a console session", hubFocus, unbound, false],
    ["a console session", hubFocus, { ...bound, bound_to: "s-other" }, false],
  ])("in the focus mode of %s, selecting %#", (_, focus, selected, belongs) => {
    expect(belongsToFocus(focus, selected, sessions)).toBe(belongs);
  });
});

describe("selectedSessionFocusTarget", () => {
  it.each([
    ["an independent project session", unbound, projectFocus],
    ["a directly bound session", bound, hubFocus],
    ["an indirectly bound session", underCoordinator, hubFocus],
    ["a console session", hub, hubFocus],
    ["an archived console session", { ...hub, status: "archived" as const }, undefined],
    ["nothing selected", undefined, undefined],
    ["an unknown project", { ...unbound, project_id: "gone" }, undefined],
    ["an archived independent project session", { ...unbound, status: "archived" as const }, projectFocus],
    ["a missing owner", { ...bound, bound_to: "gone" }, projectFocus],
    ["an archived console owner", { ...bound, bound_to: "s-archived-hub" }, undefined],
    ["an owner chain cycle", { ...bound, bound_to: underCoordinator.id }, projectFocus],
  ])("resolves %s", (_, selected, expected) => {
    const current = selected ? new Map(sessions).set(selected.id, selected) : sessions;
    expect(selectedSessionFocusTarget(selected, projects, current)).toEqual(expected);
  });
});

describe("focusFor", () => {
  it.each([
    ["keeps a project's focus mode for a session it shows", projectFocus, unbound, projectFocus],
    ["keeps a project's focus mode for a session bound to a console session", projectFocus, bound, projectFocus],
    ["keeps a project's focus mode for a session bound to one of its sessions", projectFocus, { ...bound, bound_to: unbound.id }, projectFocus],
    ["keeps a console session's focus mode for a session bound to it", hubFocus, bound, hubFocus],
    ["leaves a console session's focus mode for an unrelated session", hubFocus, unbound, undefined],
    ["keeps the focus mode with nothing selected", projectFocus, undefined, projectFocus],
    ["stays out of focus mode when not in one", undefined, unbound, undefined],
  ])("%s", (_, focus, selected, expected) => {
    expect(focusFor(focus, selected, sessions)).toBe(expected);
  });
});

describe("focusKey", () => {
  it.each([projectFocus, hubFocus])("is read back by resolveFocus", (focus) => {
    const resolved = resolveFocus(focusKey(focus), main.id, projects, sessions);
    expect(resolved && focusTargetId(resolved)).toBe(focusTargetId(focus));
  });
});

describe("cycleConsoleSession", () => {
  const strip = ["a", "b", "c"].map((id) => ({ ...hub, id }));

  it("moves to the next console session, or the previous one, going round at either end", () => {
    expect(cycleConsoleSession(strip, "a", false)?.id).toBe("b");
    expect(cycleConsoleSession(strip, "c", false)?.id).toBe("a");
    expect(cycleConsoleSession(strip, "c", true)?.id).toBe("b");
    expect(cycleConsoleSession(strip, "a", true)?.id).toBe("c");
  });

  it("goes nowhere with a single console session, or from one that is not in the strip", () => {
    expect(cycleConsoleSession(strip.slice(0, 1), "a", false)).toBeUndefined();
    expect(cycleConsoleSession([], "a", true)).toBeUndefined();
    expect(cycleConsoleSession(strip, "gone", false)).toBeUndefined();
  });
});

describe("focusAfterSelect", () => {
  it("enters the selected console session's own focus mode when asked, from anywhere", () => {
    expect(focusAfterSelect(undefined, hub, true, sessions)).toEqual({ consoleSession: hub });
    expect(focusAfterSelect(projectFocus, hub, true, sessions)).toEqual({ consoleSession: hub });
  });

  it("otherwise keeps a focus mode that shows the session and leaves one that does not", () => {
    expect(focusAfterSelect(hubFocus, hub, false, sessions)).toBe(hubFocus);
    expect(focusAfterSelect(projectFocus, hub, false, sessions)).toBeUndefined();
    expect(focusAfterSelect(undefined, hub, false, sessions)).toBeUndefined();
  });
});

it.each([bound, underCoordinator])("project focus keeps its selected console-bound session ($id) through status and owner changes", (session) => {
  expect(belongsToFocus(projectFocus, session, sessions)).toBe(true);
  expect(focusAfterSelect(projectFocus, session, false, sessions)).toBe(projectFocus);
  const updated = { ...session, status: "working" as const };
  const updatedSessions = byId([...sessions.values(), { ...bound, bound_to: "s-archived-hub" }, updated]);
  expect(focusFor(projectFocus, updated, updatedSessions)).toBe(projectFocus);
  expect(belongsToFocus(projectFocus, { ...updated, project_id: "p-2" }, updatedSessions)).toBe(false);
  expect(belongsToFocus(projectFocus, hub, updatedSessions)).toBe(false);
});
