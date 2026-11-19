// @vitest-environment jsdom
// The count of running tasks: which sessions of a task's runs make it running, and when the count moves.
import { act } from "react";
import { expect, it } from "vitest";

import { sessionOf } from "../fixtures/builders";
import { scheduledTaskOf } from "../fixtures/scheduledTaskBuilders";
import type { ScheduledTask, Session } from "../protocol";
import type { State } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useRunningTaskCount } from "./useRunningTaskCount";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const task = (id: string, runSessions: string[], extra: Partial<ScheduledTask> = {}) => scheduledTaskOf(id, "c", { run_sessions: runSessions, ...extra });
const session = (id: string, status: Parameters<typeof sessionOf>[4]) => sessionOf(id, "c", undefined, id, status);

function mount(tasks: ScheduledTask[] | undefined, sessions: Session[]) {
  const daemon = testDaemon({ state: stateOf(tasks, sessions) });
  const seen: (number | undefined)[] = [];
  function Probe(): null {
    seen.push(useRunningTaskCount());
    return null;
  }
  mountHost(<Probe />, { daemon });
  return { seen, last: () => seen[seen.length - 1], set: (patch: Partial<State>) => act(() => daemon.store.setState(patch)) };
}

function stateOf(tasks: ScheduledTask[] | undefined, sessions: Session[]): Partial<State> {
  return { scheduledTasks: tasks && new Map(tasks.map((t) => [t.id, t])), sessions: new Map(sessions.map((s) => [s.id, s])) };
}

it("is undefined while the tasks have not been listed", () => {
  expect(mount(undefined, [session("s-1", "working")]).last()).toBeUndefined();
});

it("is zero for tasks with no run session", () => {
  expect(mount([task("t-1", [])], []).last()).toBe(0);
});

it("counts a task with a starting or working session of its runs", () => {
  expect(mount([task("t-1", ["s-1"]), task("t-2", ["s-2"])], [session("s-1", "starting"), session("s-2", "working")]).last()).toBe(2);
});

it("does not count a session in any other status", () => {
  const statuses = ["idle", "waiting_user", "dormant", "interrupted", "archived", "a-newer-status"] as const;
  const sessions = statuses.map((status, i) => session(`s-${i}`, status));
  expect(mount([task("t-1", sessions.map((s) => s.id))], sessions).last()).toBe(0);
});

it("does not count a session the window does not hold", () => {
  expect(mount([task("t-1", ["s-gone"])], []).last()).toBe(0);
});

it("counts a task once however many of its sessions are at work, and whether or not it is enabled", () => {
  const counted = mount([task("t-1", ["s-1", "s-2"]), task("t-2", ["s-3"], { enabled: false })], [session("s-1", "working"), session("s-2", "working"), session("s-3", "working")]);
  expect(counted.last()).toBe(2);
});

it("counts each of two tasks that share a session", () => {
  expect(mount([task("t-1", ["s-1"]), task("t-2", ["s-1"])], [session("s-1", "working")]).last()).toBe(2);
});

it("follows a session's status, a task's run sessions and the task going", async () => {
  const probe = mount([task("t-1", ["s-1"])], [session("s-1", "idle")]);
  expect(probe.last()).toBe(0);
  await probe.set({ sessions: new Map([["s-1", session("s-1", "working")]]) });
  expect(probe.last()).toBe(1);
  await probe.set({ sessions: new Map([["s-1", session("s-1", "idle")]]) });
  expect(probe.last()).toBe(0);
  await probe.set({ scheduledTasks: new Map([["t-1", task("t-1", ["s-1", "s-2"])]]), sessions: new Map([["s-2", session("s-2", "starting")]]) });
  expect(probe.last()).toBe(1);
  await probe.set({ scheduledTasks: new Map() });
  expect(probe.last()).toBe(0);
});

it("does not render again for a status change that leaves the count as it was", async () => {
  const probe = mount([task("t-1", ["s-1"])], [session("s-1", "working"), session("s-other", "idle")]);
  const renders = probe.seen.length;
  await probe.set({ sessions: new Map([["s-1", session("s-1", "starting")], ["s-other", session("s-other", "working")]]) });
  expect(probe.seen.length).toBe(renders);
});
