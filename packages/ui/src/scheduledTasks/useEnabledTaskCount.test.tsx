// @vitest-environment jsdom
// The count of enabled tasks: what counts as enabled, and when the count moves.
import { act } from "react";
import { expect, it } from "vitest";

import { sessionOf } from "../fixtures/builders";
import { scheduledTaskOf } from "../fixtures/scheduledTaskBuilders";
import type { ScheduledTask, Session } from "../protocol";
import type { State } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useEnabledTaskCount } from "./useEnabledTaskCount";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const task = (id: string, extra: Partial<ScheduledTask> = {}) => scheduledTaskOf(id, "c", extra);
const session = (id: string, status: Parameters<typeof sessionOf>[4]) => sessionOf(id, "c", undefined, id, status);
const tasksOf = (...tasks: ScheduledTask[]) => new Map(tasks.map((t) => [t.id, t]));

function mount(tasks: ScheduledTask[] | undefined, sessions: Session[] = []) {
  const daemon = testDaemon({ state: { scheduledTasks: tasks && tasksOf(...tasks), sessions: new Map(sessions.map((s) => [s.id, s])) } });
  const seen: (number | undefined)[] = [];
  function Probe(): null {
    seen.push(useEnabledTaskCount());
    return null;
  }
  mountHost(<Probe />, { daemon });
  return { seen, last: () => seen[seen.length - 1], set: (patch: Partial<State>) => act(() => daemon.store.setState(patch)) };
}

it("is undefined while the tasks have not been listed", () => {
  expect(mount(undefined, [session("s-1", "working")]).last()).toBeUndefined();
});

it("is zero with no task listed", () => {
  expect(mount([]).last()).toBe(0);
});

it("counts the enabled tasks and not the disabled ones", () => {
  expect(mount([task("t-1"), task("t-2", { enabled: false }), task("t-3")]).last()).toBe(2);
});

it("is zero where every listed task is disabled, even one with a session at work", () => {
  const probe = mount([task("t-1", { enabled: false, run_sessions: ["s-1"] }), task("t-2", { enabled: false })], [session("s-1", "working")]);
  expect(probe.last()).toBe(0);
});

it("counts an enabled task whatever its run sessions and next run are", () => {
  const probe = mount([
    task("t-1", { run_sessions: [] }),
    task("t-2", { run_sessions: ["s-1"] }),
    task("t-3", { run_sessions: ["s-gone"], next_run_at: null }),
    task("t-4", { schedule: { kind: "manual" }, next_run_at: null }),
  ], [session("s-1", "idle")]);
  expect(probe.last()).toBe(4);
});

it("follows tasks being added, removed, enabled and disabled", async () => {
  const probe = mount([task("t-1")]);
  expect(probe.last()).toBe(1);
  await probe.set({ scheduledTasks: tasksOf(task("t-1"), task("t-2")) });
  expect(probe.last()).toBe(2);
  await probe.set({ scheduledTasks: tasksOf(task("t-1"), task("t-2", { enabled: false })) });
  expect(probe.last()).toBe(1);
  await probe.set({ scheduledTasks: tasksOf(task("t-1", { enabled: false }), task("t-2", { enabled: false })) });
  expect(probe.last()).toBe(0);
  await probe.set({ scheduledTasks: tasksOf(task("t-2", { enabled: true })) });
  expect(probe.last()).toBe(1);
  await probe.set({ scheduledTasks: tasksOf() });
  expect(probe.last()).toBe(0);
});

it("does not render again for a session status change or an unrelated task change that leaves the count as it was", async () => {
  const probe = mount([task("t-1", { run_sessions: ["s-1"] }), task("t-2", { enabled: false })], [session("s-1", "idle")]);
  const renders = probe.seen.length;
  await probe.set({ sessions: new Map([["s-1", session("s-1", "working")]]) });
  await probe.set({ scheduledTasks: tasksOf(task("t-1", { run_sessions: ["s-1", "s-2"], prompt: "Another prompt", updated_at: 1 }), task("t-2", { enabled: false })) });
  expect(probe.seen.length).toBe(renders);
});
