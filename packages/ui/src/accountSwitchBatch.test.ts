// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { ACCOUNT_SWITCH_CONCURRENCY, runAccountSwitchBatch } from "./accountSwitchBatch";
import { DaemonRequestError } from "./daemon-client";
import { errorAlerts } from "./errorAlerts";
import { consoleOf, projectOf, sessionOf } from "./gallery/fixtures/builders";
import type { Session } from "./protocol";
import { sessionWithDescendants } from "./sidebar/order";
import { applyDaemonEvent, createStateStore, setAccountSwitchesQueued } from "./store";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const session = (id: string, extra: Partial<Session> = {}): Session => ({ ...sessionOf(id, console_.id, project.id, id, "idle"), ...extra });

function storeOf(sessions: Session[]) {
  return createStateStore({ sessions: new Map(sessions.map((s) => [s.id, s])) });
}

function deferred(): { promise: Promise<void>; resolve: () => void; reject: (error: Error) => void } {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

/** Runs a batch to `account` and resolves with its outcome once it is done. */
function batch(store: ReturnType<typeof storeOf>, ids: string[], account: string | null, switchOne: (id: string, account: string | null) => Promise<void>): Promise<{ switched: number; failed: number }> {
  let result = { switched: 0, failed: 0 };
  return runAccountSwitchBatch(store, ids, account, { accountName: "Work", switchOne, onFinished: (outcome) => { result = outcome; } }).then(() => result);
}

const flush = () => new Promise<void>((done) => setTimeout(done, 0));

afterEach(() => {
  while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
});

it("collects a session's descendants through every level of binding, archived ones included, once each", () => {
  const root = session("root");
  const a = session("a", { bound_to: "root" });
  const b = session("b", { bound_to: "a", status: "archived" });
  const c = session("c", { bound_to: "b" });
  const loop = session("loop", { bound_to: "c" });
  const cyclic = { ...root, bound_to: "loop" };
  const other = session("other");
  expect(sessionWithDescendants([root, a, b, c, loop, other], root).map((s) => s.id)).toEqual(["root", "a", "b", "c", "loop"]);
  expect(sessionWithDescendants([cyclic, a, b, c, loop, other], cyclic).map((s) => s.id)).toEqual(["root", "a", "b", "c", "loop"]);
});

it("runs at most four switches at once, starts the next as one finishes, and clears the queued mark as each ends", async () => {
  const ids = Array.from({ length: 7 }, (_, i) => `s${i}`);
  const store = storeOf(ids.map((id) => session(id)));
  const gates = new Map(ids.map((id) => [id, deferred()]));
  let running = 0;
  let peak = 0;
  const started: string[] = [];
  const switchOne = vi.fn(async (id: string) => {
    started.push(id);
    running += 1;
    peak = Math.max(peak, running);
    await gates.get(id)!.promise;
    running -= 1;
  });
  const done = batch(store, ids, "a-1", switchOne);
  await flush();
  expect(ACCOUNT_SWITCH_CONCURRENCY).toBe(4);
  expect(started).toEqual(["s0", "s1", "s2", "s3"]);
  // Queued sessions show as switching, running or not.
  expect(ids.every((id) => store.getState().sessions.get(id)!.switching_account === true)).toBe(true);
  gates.get("s1")!.resolve();
  await flush();
  expect(started).toEqual(["s0", "s1", "s2", "s3", "s4"]);
  expect(store.getState().sessions.get("s1")!.switching_account).toBeUndefined();
  for (const id of ids) gates.get(id)!.resolve();
  await done;
  expect(peak).toBe(4);
  expect(started).toEqual(ids);
  expect(store.getState().queuedAccountSwitches.size).toBe(0);
  expect(ids.some((id) => store.getState().sessions.get(id)!.switching_account)).toBe(false);
});

it("skips a session that was archived, deleted or already moved before its turn, and reports no failure for it", async () => {
  const ids = ["s0", "s1", "s2", "s3", "s4", "s5", "gone", "archived", "moved"];
  const store = storeOf(ids.filter((id) => id !== "gone").map((id) => session(id, id === "archived" ? { status: "archived" } : id === "moved" ? { account_id: "a-1" } : {})));
  const gate = deferred();
  const switched: string[] = [];
  const done = batch(store, ids, "a-1", async (id) => {
    switched.push(id);
    await gate.promise;
  });
  await flush();
  gate.resolve();
  await done;
  expect(switched).toEqual(["s0", "s1", "s2", "s3", "s4", "s5"]);
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
});

it("keeps going after a failure and reports every failure in one error alert once the batch is done", async () => {
  const ids = ["ok-1", "bad-1", "ok-2", "bad-2"];
  const store = storeOf(ids.map((id) => session(id)));
  const switchOne = vi.fn(async (id: string) => {
    if (id.startsWith("bad")) throw new Error(`Copy of ${id} failed`);
  });
  await batch(store, ids, "a-1", switchOne);
  expect(switchOne).toHaveBeenCalledTimes(4);
  const alerts = errorAlerts.getSnapshot();
  expect(alerts).toHaveLength(1);
  expect(alerts[0].message).toContain("Copy of bad-1 failed");
  expect(alerts[0].message).toContain("Copy of bad-2 failed");
  expect(alerts[0].message).not.toContain("ok-");
});

it("holds the alert back until the last switch of the batch has ended", async () => {
  const store = storeOf([session("a"), session("b")]);
  const gate = deferred();
  const done = batch(store, ["a", "b"], "a-1", async (id) => {
    if (id === "a") throw new Error("Boom");
    await gate.promise;
  });
  await flush();
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  gate.resolve();
  await done;
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});

it("does not add a session another batch of the window already holds", async () => {
  const store = storeOf([session("a"), session("b")]);
  const gate = deferred();
  const first = vi.fn(() => gate.promise);
  const firstDone = batch(store, ["a"], "a-1", first);
  const second = vi.fn(async (_id: string) => {});
  await batch(store, ["a", "b"], "a-1", second);
  expect(second.mock.calls.map(([id]) => id)).toEqual(["b"]);
  expect(store.getState().sessions.get("a")!.switching_account).toBe(true);
  gate.resolve();
  await firstDone;
});

it("lets an earlier batch finish first without touching what a later, overlapping batch still holds", async () => {
  const store = storeOf([session("a"), session("b")]);
  const gates = { a: deferred(), b: deferred() };
  const firstDone = batch(store, ["a"], "a-1", () => gates.a.promise);
  const secondDone = batch(store, ["a", "b"], "a-1", () => gates.b.promise);
  gates.a.resolve();
  expect(await firstDone).toEqual({ switched: 1, failed: 0 });
  const shown = (id: string) => store.getState().sessions.get(id)!.switching_account;
  expect(shown("a")).toBeUndefined();
  expect(shown("b")).toBe(true);
  gates.b.resolve();
  expect(await secondDone).toEqual({ switched: 1, failed: 0 });
  expect(shown("b")).toBeUndefined();
  expect(store.getState().queuedAccountSwitches.size).toBe(0);
});

it("leaves the sessions map alone when queueing changes nothing it shows", () => {
  const store = storeOf([session("a")]);
  const before = store.getState().sessions;
  setAccountSwitchesQueued(store, ["gone"], true);
  setAccountSwitchesQueued(store, ["gone"], false);
  setAccountSwitchesQueued(store, ["a"], false);
  expect(store.getState().sessions).toBe(before);
});

it("keeps a queued session shown as switching through the daemon's own updates, and drops the mark when the batch lets it go", async () => {
  const store = storeOf([session("a")]);
  const gate = deferred();
  const done = batch(store, ["a"], "a-1", () => gate.promise);
  applyDaemonEvent(store, { type: "session_upserted", session: session("a", { switching_account: true }) });
  applyDaemonEvent(store, { type: "session_upserted", session: session("a") });
  expect(store.getState().sessions.get("a")!.switching_account).toBe(true);
  gate.resolve();
  await done;
  expect(store.getState().sessions.get("a")!.switching_account).toBeUndefined();
});

it("keeps a session the daemon says is switching shown as such after a batch lets it go", async () => {
  const store = storeOf([session("a")]);
  applyDaemonEvent(store, { type: "session_upserted", session: session("a", { switching_account: true }) });
  await batch(store, ["a"], "a-1", async () => {});
  expect(store.getState().sessions.get("a")!.switching_account).toBe(true);
});

it("reports the failures in one alert with the details of a failure that keeps a diagnostic apart", async () => {
  const store = storeOf([session("ok"), session("bad")]);
  await batch(store, ["ok", "bad"], "a-1", async (id) => {
    if (id === "bad") throw new DaemonRequestError("No space left on device (os error 28)", "relocation_failed", { path: "/home/w/.claude", stage: "copy", detail: "No space left on device (os error 28)" });
  });
  const [alert] = errorAlerts.getSnapshot();
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(alert.details).toBeTruthy();
  expect(alert.classification).toEqual({ kind: "actionable", recovery: "diskSpace" });
});

it("does not merge the alerts of two batches to different accounts", async () => {
  const store = storeOf([session("a")]);
  const fail = async () => { throw new DaemonRequestError("Session is locked", "session_locked", {}); };
  await runAccountSwitchBatch(store, ["a"], "a-1", { accountName: "Work", switchOne: fail });
  await runAccountSwitchBatch(store, ["a"], "a-2", { accountName: "Home", switchOne: fail });
  expect(errorAlerts.getSnapshot()).toHaveLength(2);
});

it("gives a failure with nothing raw to show no details, and an unexpected one its text as details", async () => {
  const store = storeOf([session("a")]);
  await batch(store, ["a"], "a-1", async () => { throw new DaemonRequestError("Session is locked", "session_locked", {}); });
  expect(errorAlerts.getSnapshot()[0].details).toBeFalsy();
  while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  await batch(store, ["a"], "a-1", async () => { throw new Error("Boom"); });
  expect(errorAlerts.getSnapshot()[0].classification.kind).toBe("unexpected");
  expect(errorAlerts.getSnapshot()[0].details).toBeTruthy();
});

it("keeps the raw text of a failure the window has no sentence of its own for in the details", async () => {
  const store = storeOf([session("a")]);
  await batch(store, ["a"], "a-1", async () => { throw new DaemonRequestError("database is locked", "internal_error", { detail: "database is locked" }); });
  expect(errorAlerts.getSnapshot()[0].details).toBeTruthy();
});
