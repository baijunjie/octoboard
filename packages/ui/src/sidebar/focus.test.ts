import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { belongsToFocus, cycleConsoleSession, focusAfterSelect, focusFor, focusKey, focusTargetId, followsStartedConsoleSession, resolveFocus, shortcutOutcome } from "./focus";
import type { FocusTarget } from "./types";

const main = consoleOf("c-1", "Main");
const other = consoleOf("c-2", "Other");
const project = projectOf("p-1", main.id, "Search API");
const hub = sessionOf("s-hub", main.id, undefined, "Hub", "idle", { colour: "teal" });
const bound = sessionOf("s-bound", main.id, project.id, "Bound", "idle", { bound_to: hub.id });
const unbound = sessionOf("s-unbound", main.id, project.id, "Unbound", "idle");
// `bound` is a lead session once it has sessions of its own; `underLead` is one of them.
const underLead = sessionOf("s-under-lead", main.id, project.id, "Under the lead session", "idle", { bound_to: bound.id });
const byId = <T extends { id: string }>(items: T[]) => new Map(items.map((item) => [item.id, item]));
const projects = byId([project, projectOf("p-2", other.id, "Elsewhere")]);
// An archived lead session, which the console session's archive reaches, unlike the archived
// sessions of a lead session that is still live.
const archivedLead = sessionOf("s-archived-lead", main.id, project.id, "Archived lead", "archived", { bound_to: hub.id });
const sessions = byId([hub, bound, unbound, underLead, archivedLead, { ...hub, id: "s-archived-hub", status: "archived" as const }]);

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
  const archivedBoundUnderLive = { ...underLead, status: "archived" as const };
  const archivedUnderArchivedLead = sessionOf("s-archived-under-lead", main.id, project.id, "Archived under the lead", "archived", { bound_to: archivedLead.id });
  it.each([
    ["a project", projectFocus, unbound, true],
    ["a project", projectFocus, bound, false],
    ["a project", projectFocus, archivedBound, true],
    ["a project", projectFocus, { ...bound, bound_to: unbound.id }, true],
    ["a project", projectFocus, underLead, false],
    ["a project", projectFocus, { ...underLead, status: "archived" as const }, true],
    ["a project", projectFocus, hub, false],
    ["a console session", hubFocus, bound, true],
    ["a console session", hubFocus, underLead, true],
    ["a console session", hubFocus, archivedBoundUnderLive, false],
    ["a console session", hubFocus, archivedLead, true],
    ["a console session", hubFocus, archivedUnderArchivedLead, true],
    ["a console session", hubFocus, hub, true],
    ["a console session", hubFocus, unbound, false],
    ["a console session", hubFocus, { ...bound, bound_to: "s-other" }, false],
  ])("in the focus mode of %s, selecting %#", (_, focus, selected, belongs) => {
    expect(belongsToFocus(focus, selected, sessions)).toBe(belongs);
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

describe("focusFor", () => {
  it.each([
    ["keeps a project's focus mode for a session it shows", projectFocus, unbound, projectFocus],
    ["leaves a project's focus mode for a session bound to a console session", projectFocus, bound, undefined],
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

describe("followsStartedConsoleSession", () => {
  it.each([
    ["the requesting project's focus mode", projectFocus, true],
    ["another project's focus mode", { project: projectOf("p-2", main.id, "Other") }, false],
    ["a console session's focus mode", hubFocus, false],
    ["no focus mode", undefined, false],
  ])("%s", (_, focus, expected) => {
    expect(followsStartedConsoleSession(focus, project.id)).toBe(expected);
  });
});
