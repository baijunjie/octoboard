// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import type { SessionStatus } from "../protocol";
import { holdsQuit } from "./useAppExit";

describe("holdsQuit", () => {
  it.each<[SessionStatus, boolean]>([
    ["starting", true],
    ["working", true],
    ["waiting_user", true],
    ["idle", false],
    ["interrupted", false],
    ["archived", false],
  ])("a session %s holds the quit up: %s", (status, expected) => {
    expect(holdsQuit([sessionOf("s", "c", "p", "S", status)])).toBe(expected);
  });

  it("asks when any one session holds it up", () => {
    expect(holdsQuit([sessionOf("a", "c", "p", "A", "idle"), sessionOf("b", "c", "p", "B", "working")])).toBe(true);
  });

  it("does not ask with no sessions", () => {
    expect(holdsQuit([])).toBe(false);
  });
});
