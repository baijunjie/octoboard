import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { canGo, EMPTY_HISTORY, go, HISTORY_LIMIT, isLive, type Location, type NavigationHistory, push, replaceCurrent } from "./history";

const at = (session: string): Location => ({ console: "c-1", session });
const visit = (...sessions: string[]): NavigationHistory => sessions.reduce((history, s) => push(history, at(s)), EMPTY_HISTORY);
const everything = () => true;
const sessionsOf = (history: NavigationHistory) => history.entries.map((entry) => entry.session);

describe("push", () => {
  it("records locations in order and puts the newest on screen", () => {
    const history = visit("a", "b", "c");
    expect(sessionsOf(history)).toEqual(["a", "b", "c"]);
    expect(history.index).toBe(2);
  });

  it("does not record the location that is already on screen", () => {
    expect(visit("a", "a", "b", "b").entries).toHaveLength(2);
  });

  it("tells locations apart by every part of them", () => {
    const base: Location = { console: "c-1", session: "a" };
    let history = push(EMPTY_HISTORY, base);
    history = push(history, { ...base, focus: "project:p-1" });
    history = push(history, { ...base, focus: "project:p-1", archive: { console: "c-1", project: "p-1" } });
    history = push(history, { ...base, focus: "project:p-1", archive: { console: "c-1", consoleSession: "a" } });
    expect(history.entries).toHaveLength(4);
  });

  it("drops what was ahead when navigating after going back", () => {
    let history = visit("a", "b", "c");
    history = go(history, -1, everything)!.history;
    history = go(history, -1, everything)!.history;
    history = push(history, at("d"));
    expect(sessionsOf(history)).toEqual(["a", "d"]);
    expect(go(history, 1, everything)).toBeUndefined();
  });

  it("keeps only the newest entries once past the limit", () => {
    const history = visit(...Array.from({ length: HISTORY_LIMIT + 5 }, (_, i) => `s${i}`));
    expect(history.entries).toHaveLength(HISTORY_LIMIT);
    expect(history.entries[0].session).toBe("s5");
    expect(history.index).toBe(HISTORY_LIMIT - 1);
  });
});

describe("go", () => {
  it("moves back and forward without recording anything", () => {
    const history = visit("a", "b", "c");
    const back = go(history, -1, everything)!;
    expect(back.location.session).toBe("b");
    expect(back.history.entries).toBe(history.entries);
    expect(go(back.history, 1, everything)!.location.session).toBe("c");
  });

  it("goes nowhere past either end", () => {
    const history = visit("a");
    expect(go(history, -1, everything)).toBeUndefined();
    expect(go(history, 1, everything)).toBeUndefined();
    expect(go(EMPTY_HISTORY, -1, everything)).toBeUndefined();
  });

  it("passes over entries that are no longer live", () => {
    const history = visit("a", "gone", "gone-too", "d");
    const live = (location: Location) => !location.session?.startsWith("gone");
    const back = go(history, -1, live)!;
    expect(back.location.session).toBe("a");
    expect(go(back.history, 1, live)!.location.session).toBe("d");
  });

  it("passes over entries that show what is already on screen, so a move never does nothing", () => {
    // What is on screen has been replaced by what the entry before it shows.
    const history = replaceCurrent(visit("a", "b", "c"), at("b"));
    expect(go(history, -1, everything)!.location.session).toBe("a");
    expect(canGo(replaceCurrent(visit("a", "b"), at("a")), -1, everything)).toBe(false);
    expect(canGo(visit("a", "b"), 1, everything)).toBe(false);
  });

  it("has nowhere to go when every entry that way is gone", () => {
    const history = visit("gone", "b");
    expect(go(history, -1, (location) => location.session !== "gone")).toBeUndefined();
  });
});

describe("replaceCurrent", () => {
  it("swaps the entry on screen and nothing else", () => {
    const history = replaceCurrent(visit("a", "b"), at("x"));
    expect(sessionsOf(history)).toEqual(["a", "x"]);
    expect(replaceCurrent(EMPTY_HISTORY, at("x"))).toBe(EMPTY_HISTORY);
  });
});

describe("isLive", () => {
  const main = consoleOf("c-1", "Main");
  const project = projectOf("p-1", main.id, "Search API");
  const hub = sessionOf("s-hub", main.id, undefined, "Hub", "idle");
  const bound = sessionOf("s-bound", main.id, project.id, "Bound", "idle", { bound_to: hub.id });
  const owner = sessionOf("s-owner", main.id, project.id, "Owner", "idle");
  const started = sessionOf("s-started", main.id, project.id, "Started", "idle", { bound_to: owner.id });
  const byId = <T extends { id: string }>(items: T[]) => new Map(items.map((item) => [item.id, item]));
  const data = { consoles: byId([main]), projects: byId([project]), sessions: byId([hub, bound, owner, started]) };

  it.each<[string, Location, boolean]>([
    ["nothing shown", {}, true],
    ["a console and its session", { console: "c-1", session: "s-hub" }, true],
    ["a deleted console", { console: "c-gone" }, false],
    ["a deleted session", { console: "c-1", session: "s-gone" }, false],
    ["a project's focus mode", { console: "c-1", focus: "project:p-1" }, true],
    ["a deleted project's focus mode", { console: "c-1", focus: "project:p-gone" }, false],
    ["a console session's focus mode", { console: "c-1", focus: "consoleSession:s-hub" }, true],
    ["a focus mode with a session it lists", { console: "c-1", session: "s-bound", focus: "consoleSession:s-hub" }, true],
    ["a focus mode with a session it does not list", { console: "c-1", session: "s-bound", focus: "project:p-1" }, false],
    ["a project's focus mode with a session bound to a project session", { console: "c-1", session: "s-started", focus: "project:p-1" }, true],
    ["a project's archive", { console: "c-1", archive: { console: "c-1", project: "p-1" } }, true],
    ["a deleted project's archive", { console: "c-1", archive: { console: "c-1", project: "p-gone" } }, false],
    ["a deleted console session's archive", { console: "c-1", archive: { console: "c-1", consoleSession: "s-gone" } }, false],
  ])("%s", (_, location, expected) => {
    expect(isLive(location, data)).toBe(expected);
  });
});
