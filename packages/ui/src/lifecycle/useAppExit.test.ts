// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import type { SessionStatus, ShellTerminal } from "../protocol";
import { quitHolders } from "./useAppExit";

/** Whether anything holds the quit up. */
const holdsQuit = (...args: Parameters<typeof quitHolders>) => {
  const holders = quitHolders(...args);
  return holders.sessions || holders.terminals;
};

describe("quitHolders", () => {
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

  it("asks for a session awaiting instructions only while it has work running in the background", () => {
    const idle = sessionOf("s", "c", "p", "S", "idle");
    expect(holdsQuit([{ ...idle, background_tasks: [{ id: "b1" }] }])).toBe(true);
    expect(holdsQuit([{ ...idle, background_tasks: [] }])).toBe(false);
  });

  it("asks when any one session holds it up", () => {
    expect(holdsQuit([sessionOf("a", "c", "p", "A", "idle"), sessionOf("b", "c", "p", "B", "working")])).toBe(true);
  });

  it("does not ask with no sessions", () => {
    expect(holdsQuit([])).toBe(false);
  });

  it("asks while a shell terminal runs something, never for an idle or exited one", () => {
    const shell = (state: ShellTerminal["state"], busy: boolean) => ({ id: "t", project_id: "p", worktree: null, cwd: "/p", title: "zsh", renamed: false, state, busy, created_at: 0 });
    expect(holdsQuit([], [shell("running", true)])).toBe(true);
    expect(holdsQuit([], [shell("running", false)])).toBe(false);
    expect(holdsQuit([], [shell("exited", false)])).toBe(false);
    expect(quitHolders([sessionOf("a", "c", "p", "A", "working")], [shell("running", true)])).toEqual({ sessions: true, terminals: true });
  });
});
