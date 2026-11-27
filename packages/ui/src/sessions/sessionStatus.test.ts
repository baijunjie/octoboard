import { describe, expect, it } from "vitest";

import { sessionOf } from "../fixtures/builders";
import type { Session, SessionStatus } from "../protocol";
import type { ShownStatusKind } from "./sessionStatus";
import { delegatingSessionIds, hasNoProcess, holdsAccountSwitch, isLive, isSummaryStatus, sessionIsBusy, sessionIsWorking, sessionStopsAHandoff, sessionShownStatusKind, sessionStatusKind, showsSwitching, SUMMARY_STATUSES } from "./sessionStatus";

const NONE: ReadonlySet<string> = new Set();
const kindOf = (status: SessionStatus, extra: Partial<Session> = {}, delegating: ReadonlySet<string> = NONE) =>
  sessionStatusKind(sessionOf("s", "c", "p", "T", status, { has_conversation: false, ...extra }), delegating);

it.each<[string, SessionStatus, object, boolean]>([
  ["starting", "starting", {}, true],
  ["working", "working", {}, true],
  ["dormant", "dormant", {}, true],
  ["idle with work in the background", "idle", { background_tasks: [{ id: "t" }] }, true],
  ["paused with work in the background", "idle", { paused: true, background_tasks: [{ id: "t" }] }, true],
  ["waiting for the user", "waiting_user", {}, false],
  ["idle with nothing running", "idle", {}, false],
  ["paused", "idle", { paused: true }, false],
  ["interrupted", "interrupted", {}, false],
])("a session %s is at work: %s", (_name, status, extra, working) => {
  expect(sessionIsWorking(sessionOf("s", "c", "p", "Title", status, extra))).toBe(working);
});

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

  const pending = (status: SessionStatus, extra: Partial<Session> = {}) => sessionOf("s", "c", "p", "T", status, { holder_link: "pending", has_conversation: false, ...extra });

  it.each<[string, SessionStatus, Partial<Session>, ShownStatusKind]>([
    ["starting", "starting", {}, "starting"],
    ["working", "working", {}, "working"],
    ["waiting for the user", "waiting_user", {}, "waiting_user"],
    ["dormant", "dormant", {}, "dormant"],
    ["idle", "idle", {}, "idle"],
    ["done", "idle", { has_conversation: true }, "done"],
    ["paused", "idle", { paused: true }, "paused"],
    ["working in the background", "idle", { background_tasks: [{ id: "t" }] }, "background"],
    ["switching its account", "idle", { switching_account: true }, "switching"],
  ])("shows a live session whose process cannot be reached as reconnecting, whether it is %s", (_name, status, extra, restored) => {
    const session = pending(status, extra);
    expect(sessionShownStatusKind(session, NONE, NONE)).toBe("reconnecting");
    expect(sessionShownStatusKind(session, NONE, new Set(["s"]))).toBe("reconnecting");
    const { holder_link: _link, ...cleared } = session;
    expect(sessionShownStatusKind(cleared, NONE, NONE)).toBe(restored);
  });

  it("shows reconnecting over delegated, and leaves the status kind alone", () => {
    const session = pending("idle");
    expect(sessionShownStatusKind(session, new Set(["s"]), NONE)).toBe("reconnecting");
    expect(sessionStatusKind(session, new Set(["s"]))).toBe("delegated");
  });

  it("ignores a stale flag on an archived or interrupted record", () => {
    expect(sessionShownStatusKind(pending("interrupted"), NONE, NONE)).toBe("interrupted");
    expect(sessionShownStatusKind(pending("archived"), NONE, NONE)).toBe("archived");
    expect(sessionShownStatusKind(pending("interrupted", { switching_account: true }), NONE, NONE)).toBe("switching");
  });

  it("keeps reconnecting out of the summary statuses", () => {
    expect(isSummaryStatus("reconnecting")).toBe(false);
    expect(SUMMARY_STATUSES).not.toContain("reconnecting");
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

describe("sessionIsBusy", () => {
  it.each<[string, SessionStatus, Partial<Session>, boolean]>([
    ["working", "working", {}, true],
    ["waiting for the user", "waiting_user", {}, true],
    ["showing its background agents", "dormant", {}, true],
    ["idle with work in the background", "idle", { background_tasks: [{ id: "t" }] }, true],
    ["idle with nothing running", "idle", {}, false],
    ["idle, its last turn cut short", "idle", { paused: true }, false],
    ["interrupted", "interrupted", {}, false],
    ["archived", "archived", {}, false],
    ["waiting only for a usage-limit dialog", "waiting_user", { at_usage_limit: true }, false],
    ["waiting only for a usage-limit dialog, with work in the background", "waiting_user", { at_usage_limit: true, background_tasks: [{ id: "t" }] }, true],
    ["waiting for the user and flagged not at a limit", "waiting_user", { at_usage_limit: false }, true],
    ["working, whatever the limit flag says", "working", { at_usage_limit: true }, true],
  ])("is %s: busy is %s", (_name, status, extra, busy) => {
    expect(sessionIsBusy(sessionOf("s", "c", "p", "T", status, extra))).toBe(busy);
  });

  it("does not count a session that is still starting, which has no process to be busy with", () => {
    expect(sessionIsBusy(sessionOf("s", "c", "p", "T", "starting"))).toBe(false);
  });
});

describe("sessionStopsAHandoff", () => {
  it.each<[string, SessionStatus, Partial<Session>, boolean]>([
    ["working", "working", {}, true],
    ["waiting for the user", "waiting_user", {}, true],
    ["showing its background agents", "dormant", {}, true],
    ["idle with work in the background", "idle", { background_tasks: [{ id: "t" }] }, false],
    ["idle with nothing running", "idle", {}, false],
    ["interrupted", "interrupted", {}, false],
    ["archived", "archived", {}, false],
    ["waiting only for a usage-limit dialog", "waiting_user", { at_usage_limit: true }, false],
    ["waiting only for a usage-limit dialog, with work in the background", "waiting_user", { at_usage_limit: true, background_tasks: [{ id: "t" }] }, false],
    ["waiting for the user and flagged not at a limit", "waiting_user", { at_usage_limit: false }, true],
    ["working, whatever the limit flag says", "working", { at_usage_limit: true }, true],
  ])("is %s: stops a handoff is %s", (_name, status, extra, stops) => {
    expect(sessionStopsAHandoff(sessionOf("s", "c", "p", "T", status, extra))).toBe(stops);
  });

  it("does not count a session that is still starting, which the handoff check refuses on its own", () => {
    expect(sessionStopsAHandoff(sessionOf("s", "c", "p", "T", "starting"))).toBe(false);
  });
});

describe("a status this client does not know (a newer daemon's)", () => {
  const FROM_A_NEWER_DAEMON = "from_a_newer_daemon";
  const unknown = (extra: Partial<Session> = {}) => sessionOf("s", "c", "p", "T", FROM_A_NEWER_DAEMON, { has_conversation: true, ...extra });

  it("reads as the unknown kind whatever else the record holds", () => {
    const busy = { paused: true, background_tasks: [{ id: "b" }] };
    expect(sessionStatusKind(unknown(), NONE)).toBe("unknown");
    expect(sessionStatusKind(unknown(busy), new Set(["s"]))).toBe("unknown");
    expect(sessionShownStatusKind(unknown(), NONE, NONE)).toBe("unknown");
  });

  it("is neither live, nor without a process, nor at work, nor busy, nor a stop to a handoff", () => {
    expect(isLive(FROM_A_NEWER_DAEMON)).toBe(false);
    expect(hasNoProcess(FROM_A_NEWER_DAEMON)).toBe(false);
    const withTasks = unknown({ background_tasks: [{ id: "b" }] });
    expect(sessionIsWorking(withTasks)).toBe(false);
    expect(sessionIsBusy(withTasks)).toBe(false);
    expect(sessionStopsAHandoff(withTasks)).toBe(false);
  });

  it("is not reconnecting even where the daemon cannot reach its process, and is no summary status", () => {
    expect(sessionShownStatusKind(unknown({ holder_link: "pending" }), NONE, NONE)).toBe("unknown");
    expect(isSummaryStatus("unknown")).toBe(false);
    expect(SUMMARY_STATUSES).not.toContain("unknown");
  });

  it("does not mark its owner as delegating", () => {
    const owner = sessionOf("owner", "c", "p", "owner", "idle");
    expect(delegatingSessionIds([owner, unknown({ bound_to: "owner" })])).toEqual(new Set());
  });

  it("leaves the known statuses as they were", () => {
    expect(isLive("working")).toBe(true);
    expect(hasNoProcess("interrupted")).toBe(true);
  });
});
