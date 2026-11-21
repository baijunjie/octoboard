import { expect, it } from "vitest";

import type { Event } from "../protocol";
import { SCHEDULE_KINDS } from "../values/scheduleKind";
import { isScheduledTaskRequest, sessionRequestOf, withRequest } from "./sessionRequests";

const event: Extract<Event, { type: "session_request" }> = {
  type: "session_request", request_id: "r", session: "requester", action: "handoff", kind: "project", console: "c", project: "p", agent: "receiving", target: "original", owner: null, reopened_owners: [], account: null, conversation: null, branches: ["feat/a"], model: "model", effort: "high", compatibility: { first: false, second: true }, requested_at: 2,
};
it("carries the handoff event's compatibility, including false, into the form request", () => {
  expect(sessionRequestOf(event)).toEqual({ requestId: "r", session: "requester", action: "handoff", kind: "project", console: "c", project: "p", agent: "receiving", target: "original", account: null, branches: ["feat/a"], model: "model", effort: "high", compatibility: { first: false, second: true }, requestedAt: 2 });
});
it.each(["handoff", "switch", "resume", "reopen"] as const)("does not offer a %s without its target", (action) => {
  expect(sessionRequestOf({ ...event, action, target: null })).toBeUndefined();
});
it("requires a record's conversation and rejects unknown actions", () => {
  expect(sessionRequestOf({ ...event, action: "resume_record", conversation: null })).toBeUndefined();
  expect(sessionRequestOf({ ...event, action: "future" as never })).toBeUndefined();
});
it("does not offer a request for a session kind this client does not know", () => {
  expect(sessionRequestOf({ ...event, kind: "from_a_newer_daemon" })).toBeUndefined();
  expect(sessionRequestOf({ ...event, kind: "console" })).toMatchObject({ kind: "console" });
});
it.each(["resume", "reopen", "resume_record", "switch", "start"] as const)("retains the fields needed to approve %s", (action) => {
  const request = sessionRequestOf({ ...event, action, conversation: "conversation", owner: "owner", reopened_owners: ["ancestor"] });
  expect(request).toMatchObject({ action, requestId: "r", branches: ["feat/a"] });
  if (action === "resume" || action === "reopen") expect(request).toMatchObject({ target: "original", owner: "owner", reopenedOwners: ["ancestor"] });
  if (action === "resume_record") expect(request).toMatchObject({ conversation: "conversation", account: null });
  if (action === "switch") expect(request).toMatchObject({ target: "original", account: null });
});
it("orders requests by when they were made without modifying the original list", () => {
  const newer = sessionRequestOf(event)!; const older = { ...newer, requestId: "older", requestedAt: 1 };
  const initial = [newer]; expect(withRequest(initial, older)).toEqual([older, newer]); expect(initial).toEqual([newer]);
});

const taskEvent = (extra: Partial<typeof event>): typeof event => ({ ...event, action: "update_scheduled_task", target: null, branches: [], compatibility: {}, ...extra });
it("carries what a scheduled task request asks for, and the text the agent wrote", () => {
  const launch = { title: "Report", compatibility: {} };
  const schedule = { kind: "manual" } as const;
  expect(sessionRequestOf(taskEvent({ action: "create_scheduled_task", prompt: "Do it", scheduled_task: { launch, schedule, enabled: true } })))
    .toMatchObject({ action: "create_scheduled_task", launch, schedule, enabled: true, prompt: "Do it" });
  expect(sessionRequestOf(taskEvent({ scheduled_task: { task: "t", schedule } }))).toMatchObject({ action: "update_scheduled_task", task: "t", schedule });
  expect(sessionRequestOf(taskEvent({ action: "set_scheduled_task_enabled", scheduled_task: { task: "t", enabled: false } }))).toMatchObject({ task: "t", enabled: false });
  expect(sessionRequestOf(taskEvent({ action: "run_scheduled_task", scheduled_task: { task: "t" } }))).toMatchObject({ action: "run_scheduled_task", task: "t" });
  expect(sessionRequestOf({ ...event, action: "start", prompt: "Brief" })).toMatchObject({ prompt: "Brief" });
  expect(sessionRequestOf({ ...event, action: "start" })).not.toHaveProperty("prompt");
});
it.each([
  { name: "a delete without its task", asked: { action: "delete_scheduled_task" as const, scheduled_task: {} } },
  { name: "a create without its prompt", asked: { action: "create_scheduled_task" as const, scheduled_task: { launch: { compatibility: {} }, schedule: { kind: "manual" as const }, enabled: true } } },
  { name: "a create without its schedule", asked: { action: "create_scheduled_task" as const, prompt: "Do it", scheduled_task: { launch: { compatibility: {} }, enabled: true } } },
  { name: "an enable without its value", asked: { action: "set_scheduled_task_enabled" as const, scheduled_task: { task: "t" } } },
  { name: "an update with a schedule of a kind not known", asked: { scheduled_task: { task: "t", schedule: { kind: "fortnightly" } as never } } },
])("does not offer a task request it cannot describe: $name", ({ asked }) => {
  expect(sessionRequestOf(taskEvent(asked))).toBeUndefined();
});

const TASK_ACTIONS = ["create_scheduled_task", "update_scheduled_task", "set_scheduled_task_enabled", "delete_scheduled_task", "run_scheduled_task"] as const;
it.each(TASK_ACTIONS)("does not offer a %s that carries no scheduled task part at all", (action) => {
  expect(sessionRequestOf(taskEvent({ action, prompt: "Do it" }))).toBeUndefined();
  expect(sessionRequestOf(taskEvent({ action, prompt: "Do it", scheduled_task: {} }))).toBeUndefined();
});
it.each(["update_scheduled_task", "set_scheduled_task_enabled", "run_scheduled_task"] as const)("does not offer a %s without the task it acts on", (action) => {
  expect(sessionRequestOf(taskEvent({ action, scheduled_task: { enabled: true, schedule: { kind: "manual" } } }))).toBeUndefined();
});
it("does not offer a create without its launch settings or with a schedule of a kind not known", () => {
  const launch = { compatibility: {} };
  const schedule = { kind: "manual" } as const;
  expect(sessionRequestOf(taskEvent({ action: "create_scheduled_task", prompt: "Do it", scheduled_task: { schedule, enabled: true } }))).toBeUndefined();
  expect(sessionRequestOf(taskEvent({ action: "create_scheduled_task", prompt: "Do it", scheduled_task: { launch, schedule: { kind: "fortnightly" } as never, enabled: true } }))).toBeUndefined();
});
it.each(SCHEDULE_KINDS.filter((kind) => kind !== "manual"))("offers an update whose schedule is of the known kind %s", (kind) => {
  expect(sessionRequestOf(taskEvent({ scheduled_task: { task: "t", schedule: { kind } as never } }))).toMatchObject({ action: "update_scheduled_task", task: "t", schedule: { kind } });
});
it.each([true, false])("offers a request to turn a task on or off: enabled %s", (enabled) => {
  expect(sessionRequestOf(taskEvent({ action: "set_scheduled_task_enabled", scheduled_task: { task: "t", enabled } }))).toMatchObject({ action: "set_scheduled_task_enabled", task: "t", enabled });
});
it("offers a delete of a task, carrying the target the event names", () => {
  expect(sessionRequestOf(taskEvent({ action: "delete_scheduled_task", kind: "console", console: "c-9", project: null, scheduled_task: { task: "t" } })))
    .toMatchObject({ action: "delete_scheduled_task", task: "t", kind: "console", console: "c-9", project: null });
});
it("tells the requests that act on a scheduled task from the others", () => {
  for (const action of TASK_ACTIONS) {
    const asked = sessionRequestOf(taskEvent({ action, prompt: "Do it", scheduled_task: { task: "t", enabled: true, launch: { compatibility: {} }, schedule: { kind: "manual" } } }));
    expect(asked && isScheduledTaskRequest(asked)).toBe(true);
  }
  for (const action of ["start", "handoff", "switch"] as const) expect(isScheduledTaskRequest(sessionRequestOf({ ...event, action })!)).toBe(false);
});
