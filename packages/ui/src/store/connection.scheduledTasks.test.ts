// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import type { Event, ScheduledTaskRun } from "../protocol";
import { createDaemon, createInertDaemon } from ".";
import type { WindowNotification } from "./notifications";

const client = vi.hoisted(() => ({ onEvent: undefined as ((event: Event) => void) | undefined }));
vi.mock("../daemon-client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../daemon-client")>()),
  DaemonClient: class {
    onEvent(listener: (event: Event) => void) { client.onEvent = listener; }
    onEvents() {}
    onConnectionChange() {}
    onRetryPendingChange() {}
  },
}));

function runOf(id: string): ScheduledTaskRun {
  return { id, task_id: "t-1", ran_at: 1, due_at: null, trigger: "scheduled", outcome: { kind: "failed", code: "internal_error", params: {}, message: "no" } };
}

it("hands each run the daemon records to every listener, never as a notification of the window, and stops at the unsubscribe", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const heard: ScheduledTaskRun[] = [];
  const other = vi.fn();
  const notifications: WindowNotification[] = [];
  daemon.onNotification((notification) => notifications.push(notification));
  const unsubscribe = daemon.onScheduledTaskRun((run) => heard.push(run));
  daemon.onScheduledTaskRun(other);
  client.onEvent!({ type: "scheduled_task_run_added", run: runOf("r-1") });
  client.onEvent!({ type: "scheduled_task_run_added", run: runOf("r-2") });
  expect(heard.map((run) => run.id)).toEqual(["r-1", "r-2"]);
  expect(other).toHaveBeenCalledTimes(2);
  expect(notifications).toEqual([]);
  unsubscribe();
  client.onEvent!({ type: "scheduled_task_run_added", run: runOf("r-3") });
  expect(heard).toHaveLength(2);
  expect(other).toHaveBeenCalledTimes(3);
});

it("drops a recorded run nobody listens for without failing", () => {
  createDaemon({ origin: "http://localhost:1234", key: "k" });
  expect(() => client.onEvent!({ type: "scheduled_task_run_added", run: runOf("r-1") })).not.toThrow();
});

it("has a subscription on the inert daemon that hears nothing and unsubscribes", () => {
  const unsubscribe = createInertDaemon().onScheduledTaskRun(() => {});
  expect(unsubscribe()).toBeUndefined();
});
