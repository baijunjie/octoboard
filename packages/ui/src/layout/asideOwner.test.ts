import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { liveOwner, ownerAfterConsoleSwitch, ownerAfterMove, ownerForSession, type AsideOwner } from "./asideOwner";

const main = consoleOf("c-1", "Main");
const other = consoleOf("c-2", "Other");
const project = projectOf("p-1", main.id, "Search API");
const hub = sessionOf("s-hub", main.id, undefined, "Hub", "idle");
const worker = sessionOf("s-worker", main.id, project.id, "Worker", "idle");
const elsewhere = projectOf("p-2", other.id, "Elsewhere");
const projects = new Map([project, elsewhere].map((p) => [p.id, p]));
const sessions = new Map([hub, worker].map((s) => [s.id, s]));

const browser: AsideOwner = { kind: "project", project: project.id };
const report: AsideOwner = { kind: "report", consoleSession: hub.id };

describe("ownerForSession", () => {
  it.each([
    ["a console session to its report", hub, report],
    ["a project session to its project's browser", worker, browser],
    ["no session to nothing", undefined, undefined],
  ])("gives %s", (_, session, owner) => {
    expect(ownerForSession(session)).toEqual(owner);
  });
});

describe("liveOwner", () => {
  it("drops an owner whose project or console session is gone, with nothing in its place", () => {
    expect(liveOwner(browser, projects, sessions)).toBe(browser);
    expect(liveOwner(browser, new Map(), sessions)).toBeUndefined();
    expect(liveOwner(report, projects, new Map([[worker.id, worker]]))).toBeUndefined();
  });
});

describe("ownerAfterConsoleSwitch", () => {
  it("closes a browser of another console's project and keeps a report", () => {
    expect(ownerAfterConsoleSwitch(browser, main.id, projects)).toBe(browser);
    expect(ownerAfterConsoleSwitch(browser, other.id, projects)).toBeUndefined();
    expect(ownerAfterConsoleSwitch(report, other.id, projects)).toBe(report);
  });
});

describe("ownerAfterMove", () => {
  const otherBrowser: AsideOwner = { kind: "project", project: elsewhere.id };
  it.each([
    ["keeps a browser of the console shown", browser, worker, main.id, browser],
    ["closes a browser of another console, leaving nothing when the session is elsewhere too", browser, worker, other.id, undefined],
    ["gives the selected session's own back on returning to its console", undefined, worker, main.id, browser],
    ["replaces another console's browser by the selected session's own", otherBrowser, worker, main.id, browser],
    ["keeps a report", report, worker, other.id, report],
    ["gives a selected console session's report back in another console", browser, hub, other.id, report],
  ])("%s", (_, owner, selected, consoleId, expected) => {
    expect(ownerAfterMove(owner, selected, consoleId, projects)).toEqual(expected);
  });
});
