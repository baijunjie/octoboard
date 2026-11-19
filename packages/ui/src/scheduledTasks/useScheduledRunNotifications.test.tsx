// @vitest-environment jsdom
// The system notification of a scheduled task's failed run: raised only for a failure, naming the task and the failure,
// and choosing it opens the page on that task.
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { consoleOf } from "../fixtures/builders";
import { setLanguage } from "../i18n/language";
import type { PlatformAdapter } from "../platform";
import type { ScheduledTask, ScheduledTaskRun } from "../protocol";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { withoutIsolates } from "../test/dom";
import { useScheduledRunNotifications } from "./useScheduledRunNotifications";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-main", "Main");
const REPORT: ScheduledTask = {
  id: "t-report",
  console_id: main.id,
  project_id: null,
  launch: { title: "Morning report", compatibility: {} },
  prompt: "Summarise yesterday's commits",
  schedule: { kind: "manual" },
  enabled: true,
  created_at: 1,
  updated_at: 1,
  next_run_at: null,
  last_run_at: null,
  run_sessions: [],
};

function runOf(outcome: ScheduledTaskRun["outcome"]): ScheduledTaskRun {
  return { id: "r-1", task_id: REPORT.id, ran_at: 1, due_at: 1, trigger: "scheduled", outcome };
}

beforeEach(() => setLanguage("en"));

function Probe({ onOpen }: { onOpen: (taskId: string) => void }): null {
  useScheduledRunNotifications(onOpen);
  return null;
}

async function mount({ permitted = true, tasks = [REPORT], notifyFails = false }: { permitted?: boolean; tasks?: ScheduledTask[]; notifyFails?: boolean } = {}) {
  let record!: (run: ScheduledTaskRun) => void;
  let choose!: (target: string) => void;
  const notify = vi.fn((_: { title: string; body: string; target?: string }) => (notifyFails ? Promise.reject(new Error("no notifications")) : Promise.resolve()));
  const platform: PlatformAdapter = {
    kind: "browser",
    host: BROWSER_HOST,
    notifications: {
      ensurePermission: () => Promise.resolve(permitted),
      notify,
      onChosen: (listener) => {
        choose = listener;
        return () => {};
      },
    },
  };
  const daemon = testDaemon({
    state: { consoles: new Map([[main.id, main]]), scheduledTasks: new Map(tasks.map((task) => [task.id, task])) },
    onScheduledTaskRun: (listener) => {
      record = listener;
      return () => {};
    },
  });
  const onOpen = vi.fn<(taskId: string) => void>();
  await act(async () => void mountHost(<Probe onOpen={onOpen} />, { daemon, platform }));
  return { notify, onOpen, record: (run: ScheduledTaskRun) => act(async () => record(run)), choose: (target: string) => act(async () => choose(target)) };
}

it("notifies a failed run, naming the task and the failure, and nothing for a started one", async () => {
  const { notify, record } = await mount();
  await record(runOf({ kind: "started", session: "s-1", started_independent: false }));
  expect(notify).not.toHaveBeenCalled();
  await record(runOf({ kind: "failed", code: "some_future_failure", params: {}, message: "spawn failed" }));
  expect(notify).toHaveBeenCalledTimes(1);
  const [shown] = notify.mock.calls[0]!;
  expect(shown.title).toBe("A scheduled task could not start a session");
  expect(withoutIsolates(shown.body)).toMatch(/^Morning report: /);
  expect(shown.target).toBe("scheduled-task:t-report");
});

it("opens the page on the task when its notification is chosen, and ignores another target", async () => {
  const { onOpen, choose } = await mount();
  await choose("something-else");
  expect(onOpen).not.toHaveBeenCalled();
  await choose("scheduled-task:t-report");
  expect(onOpen).toHaveBeenCalledWith("t-report");
});

it("raises nothing where the permission is not granted, and nothing breaks where the platform's notification fails", async () => {
  const failure = runOf({ kind: "failed", code: "some_future_failure", params: {}, message: "spawn failed" });
  const denied = await mount({ permitted: false });
  await denied.record(failure);
  expect(denied.notify).not.toHaveBeenCalled();

  const failing = await mount({ notifyFails: true });
  await failing.record(failure);
  expect(failing.notify).toHaveBeenCalledTimes(1);
});

it("words the failure alone for a task this window does not hold, still opening the page on it when chosen", async () => {
  const { notify, record } = await mount({ tasks: [] });
  await record(runOf({ kind: "failed", code: "account_disabled", params: { agent: "Codex", account: "Work" }, message: "raw" }));
  const [shown] = notify.mock.calls[0]!;
  expect(withoutIsolates(shown.body)).toContain("is disabled, so a session cannot be opened on it.");
  expect(shown.body).not.toContain("Morning report");
  expect(shown.target).toBe("scheduled-task:t-report");
});

it("raises nothing on a platform that has no notifications", async () => {
  let record!: (run: ScheduledTaskRun) => void;
  const daemon = testDaemon({
    state: { consoles: new Map([[main.id, main]]), scheduledTasks: new Map([[REPORT.id, REPORT]]) },
    onScheduledTaskRun: (listener) => {
      record = listener;
      return () => {};
    },
  });
  await act(async () => void mountHost(<Probe onOpen={vi.fn()} />, { daemon, platform: { kind: "browser", host: BROWSER_HOST } }));
  // No listener was taken, as there is nothing to raise one with.
  expect(record).toBeUndefined();
});
