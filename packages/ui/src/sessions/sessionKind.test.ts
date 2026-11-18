import { describe, expect, it } from "vitest";

import { isSessionKind, listedAmongConsoleSessions } from "./sessionKind";

describe("isSessionKind", () => {
  it.each(["console", "project"])("knows %s", (kind) => {
    expect(isSessionKind(kind)).toBe(true);
  });

  it.each(["from_a_newer_daemon", "", "Console", null, undefined])("does not know %j", (kind) => {
    expect(isSessionKind(kind)).toBe(false);
  });
});

describe("listedAmongConsoleSessions", () => {
  it("lists a console session, with or without a project", () => {
    expect(listedAmongConsoleSessions({ kind: "console", project_id: null })).toBe(true);
    expect(listedAmongConsoleSessions({ kind: "console", project_id: "p" })).toBe(true);
  });

  it("does not list a project session", () => {
    expect(listedAmongConsoleSessions({ kind: "project", project_id: "p" })).toBe(false);
    expect(listedAmongConsoleSessions({ kind: "project", project_id: null })).toBe(false);
  });

  it("lists a session of a kind this build does not know that belongs to no project", () => {
    expect(listedAmongConsoleSessions({ kind: "from_a_newer_daemon", project_id: null })).toBe(true);
  });

  it("leaves a session of a kind this build does not know to its project", () => {
    expect(listedAmongConsoleSessions({ kind: "from_a_newer_daemon", project_id: "p" })).toBe(false);
  });
});
