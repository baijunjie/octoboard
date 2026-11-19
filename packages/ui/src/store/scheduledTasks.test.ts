import { expect, it } from "vitest";

import { consoleOf, projectOf } from "../fixtures/builders";
import type { ScheduledTask, ScheduledTaskRun } from "../protocol";
import { applyDaemonEvent } from "./actions";
import { createStateStore } from "./state";

const main = consoleOf("c-main", "Main");
const other = consoleOf("c-other", "Other");
const site = projectOf("p-site", main.id, "Website");

function taskOf(id: string, consoleId = main.id, projectId: string | null = null): ScheduledTask {
  return {
    id,
    console_id: consoleId,
    project_id: projectId,
    launch: { compatibility: {} },
    prompt: "Do the thing",
    schedule: { kind: "manual" },
    enabled: true,
    created_at: 1,
    updated_at: 1,
    next_run_at: null,
    last_run_at: null,
    run_sessions: [],
  };
}

const storeWith = (tasks?: ScheduledTask[]) =>
  createStateStore({
    consoles: new Map([[main.id, main], [other.id, other]]),
    projects: new Map([[site.id, site]]),
    scheduledTasks: tasks && new Map(tasks.map((task) => [task.id, task])),
  });
const ids = (store: ReturnType<typeof storeWith>) => (store.getState().scheduledTasks ? [...store.getState().scheduledTasks!.keys()] : undefined);

it("holds no tasks until they are listed, and a listing replaces what is held, keeping unchanged records", () => {
  const store = storeWith();
  expect(ids(store)).toBeUndefined();
  const kept = taskOf("t-a");
  applyDaemonEvent(store, { type: "scheduled_tasks_listed", id: null, tasks: [kept, taskOf("t-b")], time_zone: "Europe/Berlin" });
  expect(ids(store)).toEqual(["t-a", "t-b"]);
  applyDaemonEvent(store, { type: "scheduled_tasks_listed", id: null, tasks: [{ ...kept }], time_zone: null });
  expect(ids(store)).toEqual(["t-a"]);
  expect(store.getState().scheduledTasks!.get("t-a")).toBe(kept);
});

it("adds or replaces the task a broadcast upsert carries", () => {
  const store = storeWith([taskOf("t-a")]);
  applyDaemonEvent(store, { type: "scheduled_task_upserted", task: { ...taskOf("t-a"), enabled: false } });
  applyDaemonEvent(store, { type: "scheduled_task_upserted", task: taskOf("t-b") });
  expect(ids(store)).toEqual(["t-a", "t-b"]);
  expect(store.getState().scheduledTasks!.get("t-a")!.enabled).toBe(false);
});

it("seeds a task a save's reply carries only where the window lacks it, so a reply that arrives after a newer upsert does not overwrite it", () => {
  const store = storeWith([taskOf("t-a")]);
  const newer = { ...taskOf("t-a"), enabled: false, updated_at: 3 };
  applyDaemonEvent(store, { type: "scheduled_task_upserted", task: newer });
  applyDaemonEvent(store, { type: "scheduled_task_saved", id: null, task: { ...taskOf("t-a"), prompt: "Older", updated_at: 2 } });
  expect(store.getState().scheduledTasks!.get("t-a")).toBe(newer);
  applyDaemonEvent(store, { type: "scheduled_task_saved", id: null, task: taskOf("t-b") });
  expect(ids(store)).toEqual(["t-a", "t-b"]);
});

it.each(["scheduled_task_saved", "scheduled_task_upserted"] as const)("holds no task a %s carries before the window's first listing, as one task would be taken for the daemon's whole list", (type) => {
  const store = storeWith();
  applyDaemonEvent(store, type === "scheduled_task_saved" ? { type, id: null, task: taskOf("t-a") } : { type, task: taskOf("t-a") });
  expect(store.getState().scheduledTasks).toBeUndefined();
  applyDaemonEvent(store, { type: "scheduled_tasks_listed", id: null, tasks: [taskOf("t-a"), taskOf("t-b")], time_zone: null });
  expect(ids(store)).toEqual(["t-a", "t-b"]);
});

it("leaves the tasks unread, and holds no run, for a run recorded before the window's first listing", () => {
  const store = storeWith();
  applyDaemonEvent(store, { type: "scheduled_task_run_added", run: runOf("r-early") });
  expect(store.getState().scheduledTasks).toBeUndefined();
  expect(runIds(store)).toBeUndefined();
});

it("drops a deleted task, and the tasks of a deleted console or project, which the daemon does not announce", () => {
  const store = storeWith([taskOf("t-a"), taskOf("t-site", main.id, site.id), taskOf("t-other", other.id)]);
  applyDaemonEvent(store, { type: "scheduled_task_deleted", task: "t-a" });
  expect(ids(store)).toEqual(["t-site", "t-other"]);
  applyDaemonEvent(store, { type: "project_deleted", project: site.id });
  expect(ids(store)).toEqual(["t-other"]);
  applyDaemonEvent(store, { type: "console_deleted", console: other.id });
  expect(ids(store)).toEqual([]);
});

it("keeps the zone the last listing names", () => {
  const store = storeWith();
  expect(store.getState().scheduledTasksZone).toBeUndefined();
  applyDaemonEvent(store, { type: "scheduled_tasks_listed", id: null, tasks: [], time_zone: "Asia/Tokyo" });
  expect(store.getState().scheduledTasksZone).toBe("Asia/Tokyo");
  applyDaemonEvent(store, { type: "scheduled_tasks_listed", id: null, tasks: [], time_zone: null });
  expect(store.getState().scheduledTasksZone).toBeNull();
});

function runOf(id: string, taskId = "t-a"): ScheduledTaskRun {
  return { id, task_id: taskId, ran_at: 1, due_at: null, trigger: "run_now", outcome: { kind: "started", session: "s", started_independent: false } };
}
const runIds = (store: ReturnType<typeof storeWith>, taskId = "t-a") => store.getState().scheduledTaskRuns.get(taskId)?.map((run) => run.id);

it("holds a task's runs once listed, adds a recorded run first, once, and only to a listed task's, keeping the last 50", () => {
  const store = storeWith([taskOf("t-a"), taskOf("t-b")]);
  applyDaemonEvent(store, { type: "scheduled_task_run_added", run: runOf("r-early") });
  expect(runIds(store)).toBeUndefined();
  applyDaemonEvent(store, { type: "scheduled_task_runs_listed", id: null, task: "t-a", runs: [runOf("r-1")] });
  applyDaemonEvent(store, { type: "scheduled_task_run_added", run: runOf("r-2") });
  applyDaemonEvent(store, { type: "scheduled_task_run_added", run: runOf("r-2") });
  applyDaemonEvent(store, { type: "scheduled_task_run_added", run: runOf("r-b", "t-b") });
  expect(runIds(store)).toEqual(["r-2", "r-1"]);
  expect(runIds(store, "t-b")).toBeUndefined();
  for (let n = 3; n <= 51; n++) applyDaemonEvent(store, { type: "scheduled_task_run_added", run: runOf(`r-${n}`) });
  expect(runIds(store)).toHaveLength(50);
  expect(runIds(store)![0]).toBe("r-51");
  expect(runIds(store)).not.toContain("r-1");
});

it("drops a task's runs with the task, its console or its project", () => {
  const store = storeWith([taskOf("t-a"), taskOf("t-site", main.id, site.id), taskOf("t-other", other.id)]);
  for (const task of ["t-a", "t-site", "t-other"]) applyDaemonEvent(store, { type: "scheduled_task_runs_listed", id: null, task, runs: [runOf(`r-${task}`, task)] });
  applyDaemonEvent(store, { type: "scheduled_task_deleted", task: "t-a" });
  applyDaemonEvent(store, { type: "project_deleted", project: site.id });
  expect([...store.getState().scheduledTaskRuns.keys()]).toEqual(["t-other"]);
  applyDaemonEvent(store, { type: "console_deleted", console: other.id });
  expect(store.getState().scheduledTaskRuns.size).toBe(0);
  // A listing that answers for a task already gone is not held.
  applyDaemonEvent(store, { type: "scheduled_task_runs_listed", id: null, task: "t-a", runs: [runOf("r-late")] });
  expect(store.getState().scheduledTaskRuns.size).toBe(0);
});
