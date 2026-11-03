import { describe, expect, it } from "vitest";

import { sessionOf } from "../fixtures/builders";
import type { Session, SessionStatus } from "../protocol";
import { delegatingSessionIds, holdsAccountSwitch, sessionShownStatusKind, sessionStatusKind, showsSwitching } from "./sessionStatus";

const NONE: ReadonlySet<string> = new Set();
const kindOf = (status: SessionStatus, extra: Partial<Session> = {}, delegating: ReadonlySet<string> = NONE) =>
  sessionStatusKind(sessionOf("s", "c", "p", "T", status, { has_conversation: false, ...extra }), delegating);

describe("sessionShownStatusKind", () => {
  it("shows a session switching its account as such over its own status, and leaves its status kind alone", () => {
    const switching = sessionOf("s", "c", "p", "T", "interrupted", { switching_account: true });
    expect(sessionShownStatusKind(switching, NONE, NONE)).toBe("switching");
    expect(sessionStatusKind(switching, NONE)).toBe("interrupted");
    expect(sessionShownStatusKind(sessionOf("s", "c", "p", "T", "interrupted"), NONE, NONE)).toBe("interrupted");
  });

  it("shows a session this window has queued for an account switch as switching, though its record does not say so", () => {
    const queued = sessionOf("s", "c", "p", "T", "interrupted");
    expect(sessionShownStatusKind(queued, NONE, new Set(["s"]))).toBe("switching");
    expect(sessionShownStatusKind(queued, NONE, new Set(["other"]))).toBe("interrupted");
  });

  it("shows switching over delegated and done too", () => {
    const switching = sessionOf("s", "c", "p", "T", "idle", { switching_account: true });
    expect(sessionShownStatusKind(switching, new Set(["s"]), NONE)).toBe("switching");
  });
});

describe("sessionStatusKind", () => {
  it("reads idle with work left running as running in the background, and nothing else", () => {
    const tasks = [{ id: "b1" }];
    expect(kindOf("idle", { background_tasks: tasks })).toBe("background");
    expect(kindOf("idle", { background_tasks: [] })).toBe("idle");
    expect(kindOf("working", { background_tasks: tasks })).toBe("working");
  });

  it("reads an idle session as idle until its conversation has begun, then as done", () => {
    expect(kindOf("idle", { has_conversation: false })).toBe("idle");
    expect(kindOf("idle", { has_conversation: true })).toBe("done");
  });

  it("reads an idle session whose sessions are at work as delegated, over done and idle", () => {
    const delegating = new Set(["s"]);
    expect(kindOf("idle", { has_conversation: false }, delegating)).toBe("delegated");
    expect(kindOf("idle", { has_conversation: true }, delegating)).toBe("delegated");
  });

  it("reads a paused idle session as paused over background work, delegated work and done", () => {
    const paused = { paused: true, has_conversation: true, background_tasks: [{ id: "b" }] };
    expect(kindOf("idle", paused, new Set(["s"]))).toBe("paused");
    expect(kindOf("idle", { paused: true })).toBe("paused");
    expect(kindOf("idle", { paused: false, has_conversation: true })).toBe("done");
  });

  it("ranks the session's own background work over delegated work", () => {
    expect(kindOf("idle", { has_conversation: true, background_tasks: [{ id: "b" }] }, new Set(["s"]))).toBe("background");
  });

  it.each(["starting", "working", "waiting_user", "dormant", "interrupted", "archived"] as const)("keeps the wire status %s whatever else holds", (status) => {
    expect(kindOf(status, { has_conversation: true, paused: true, background_tasks: [{ id: "b" }] }, new Set(["s"]))).toBe(status);
  });
});

describe("delegatingSessionIds", () => {
  const at = (id: string, status: SessionStatus, bound_to: string | null = null) => sessionOf(id, "c", "p", id, status, { bound_to });

  it.each(["starting", "working", "waiting_user", "dormant"] as const)("marks the owner of a session that is %s", (status) => {
    expect([...delegatingSessionIds([at("owner", "idle"), at("child", status, "owner")])]).toEqual(["owner"]);
  });

  it("marks the owner of an idle session with work left running in the background, paused or not", () => {
    for (const paused of [false, true]) {
      const child = sessionOf("bg", "c", "p", "bg", "idle", { bound_to: "owner", paused, background_tasks: [{ id: "b" }] });
      expect(delegatingSessionIds([at("owner", "idle"), child])).toEqual(new Set(["owner"]));
    }
  });

  it("marks every owner above a session working in the background, which reads as delegated over done", () => {
    const sessions = [
      at("hub", "idle"),
      sessionOf("coordinator", "c", "p", "coordinator", "idle", { bound_to: "hub", has_conversation: true }),
      sessionOf("worker", "c", "p", "worker", "idle", { bound_to: "coordinator", has_conversation: true, background_tasks: [{ id: "b" }] }),
    ];
    const delegating = delegatingSessionIds(sessions);
    expect(delegating).toEqual(new Set(["hub", "coordinator"]));
    expect(sessionStatusKind(sessions[1], delegating)).toBe("delegated");
    expect(sessionStatusKind(sessions[2], delegating)).toBe("background");
  });

  it("does not count children that are idle, paused, interrupted or archived with nothing left running", () => {
    const children = [
      at("idle", "idle", "owner"),
      sessionOf("paused", "c", "p", "paused", "idle", { bound_to: "owner", paused: true }),
      sessionOf("ended", "c", "p", "ended", "interrupted", { bound_to: "owner", background_tasks: [{ id: "b" }] }),
      at("stopped", "interrupted", "owner"),
      at("gone", "archived", "owner"),
    ];
    expect(delegatingSessionIds([at("owner", "idle"), ...children]).size).toBe(0);
  });

  it("marks every owner above a working session: a console session over an idle coordinator over a worker", () => {
    const sessions = [at("hub", "idle"), at("coordinator", "idle", "hub"), at("worker", "working", "coordinator"), at("other", "idle")];
    expect(delegatingSessionIds(sessions)).toEqual(new Set(["hub", "coordinator"]));
  });

  it("marks an owner of any status, and leaves the status kind of a non-idle one alone", () => {
    const sessions = [at("owner", "working"), at("child", "working", "owner")];
    const delegating = delegatingSessionIds(sessions);
    expect(delegating.has("owner")).toBe(true);
    expect(sessionStatusKind(sessions[0], delegating)).toBe("working");
  });

  it("ends the walk on a binding that leads back on itself", () => {
    const sessions = [at("a", "working", "b"), at("b", "idle", "a")];
    expect(delegatingSessionIds(sessions)).toEqual(new Set(["b"]));
  });

  it("marks an owner the list does not hold, and accepts a one-shot iterator", () => {
    expect(delegatingSessionIds(new Map([["w", at("w", "working", "elsewhere")]]).values())).toEqual(new Set(["elsewhere"]));
  });
});

describe("showsSwitching", () => {
  it("holds for a session an account switch holds, unless it is archived", () => {
    expect(showsSwitching({ id: "s", status: "idle", switching_account: true }, NONE)).toBe(true);
    expect(showsSwitching({ id: "s", status: "archived", switching_account: true }, NONE)).toBe(false);
    expect(showsSwitching({ id: "s", status: "idle", switching_account: false }, NONE)).toBe(false);
    expect(showsSwitching({ id: "s", status: "idle" }, NONE)).toBe(false);
    expect(showsSwitching({ id: "s", status: "idle" }, new Set(["s"]))).toBe(true);
    expect(showsSwitching({ id: "s", status: "archived" }, new Set(["s"]))).toBe(false);
  });
});

describe("holdsAccountSwitch", () => {
  it("holds for the daemon's switch and for one this window has queued", () => {
    expect(holdsAccountSwitch({ id: "s", switching_account: true }, NONE)).toBe(true);
    expect(holdsAccountSwitch({ id: "s" }, new Set(["s"]))).toBe(true);
    expect(holdsAccountSwitch({ id: "s" }, new Set(["other"]))).toBe(false);
  });
});
