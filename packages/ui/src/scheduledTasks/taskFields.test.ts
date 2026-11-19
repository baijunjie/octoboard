import { expect, it } from "vitest";

import type { ScheduledTask } from "../protocol";
import { fieldsOf, launchOf, launchPresetOf, NO_OWNER, NO_PROJECT, sameLaunch, scheduleOf, scheduleProblems, type TaskFields } from "./taskFields";

function taskOf(extra: Partial<ScheduledTask> = {}): ScheduledTask {
  return {
    id: "t-1",
    console_id: "c-1",
    project_id: null,
    launch: { compatibility: {} },
    prompt: "Do the thing",
    schedule: { kind: "daily", time: { hour: 7, minute: 5 } },
    enabled: true,
    created_at: 1,
    updated_at: 1,
    next_run_at: null,
    last_run_at: null,
    run_sessions: [],
    ...extra,
  };
}

// A Thursday, 10:20 UTC.
const NOW = Date.UTC(2026, 10, 19, 10, 20);

it("starts a new task as a daily one at 9:00 in the console it opened on, with a one-off at the next full hour", () => {
  expect(fieldsOf(undefined, "c-1", NOW, "UTC")).toEqual({
    consoleId: "c-1",
    projectId: NO_PROJECT,
    name: "",
    boundTo: NO_OWNER,
    prompt: "",
    kind: "daily",
    runAt: "2026-11-19T11:00",
    minute: 0,
    day: 19,
    time: "09:00",
    days: ["monday"],
    enabled: true,
  });
  expect(fieldsOf(undefined, undefined, NOW, "UTC").consoleId).toBe("");
  // Exactly on the hour, the next full hour is the one after it, not now.
  expect(fieldsOf(undefined, "c-1", Date.UTC(2026, 10, 19, 10, 0), "UTC").runAt).toBe("2026-11-19T11:00");
  // The zone decides what the wall clock reads.
  expect(fieldsOf(undefined, "c-1", NOW, "Asia/Tokyo").runAt).toBe("2026-11-19T20:00");
});

it.each<[string, Partial<ScheduledTask>, Partial<TaskFields>]>([
  ["an hourly schedule", { schedule: { kind: "hourly", minute: 45 } }, { kind: "hourly", minute: 45 }],
  ["a weekdays schedule", { schedule: { kind: "weekdays", time: { hour: 18, minute: 30 } } }, { kind: "weekdays", time: "18:30" }],
  ["a weekly schedule, its days in week order", { schedule: { kind: "weekly", time: { hour: 8, minute: 0 }, days: ["sunday", "wednesday", "monday"] } }, { kind: "weekly", time: "08:00", days: ["monday", "wednesday", "sunday"] }],
  ["a monthly schedule", { schedule: { kind: "monthly", day: 31, time: { hour: 6, minute: 45 } } }, { kind: "monthly", day: 31, time: "06:45" }],
  ["a manual-only schedule", { schedule: { kind: "manual" } }, { kind: "manual" }],
  ["a one-off, read in the zone", { schedule: { kind: "once", run_at: Date.UTC(2026, 11, 24, 12, 0) } }, { kind: "once", runAt: "2026-12-24T21:00" }],
  ["a task that is off", { enabled: false }, { enabled: false }],
  ["a named project task reporting to a console session", { project_id: "p-1", launch: { title: "Nightly", bound_to: "s-hub", compatibility: {} } }, { consoleId: "c-1", projectId: "p-1", name: "Nightly", boundTo: "s-hub" }],
  ["a console task, which has no one to report to", { launch: { bound_to: "s-hub", compatibility: {} } }, { consoleId: "c-1", projectId: NO_PROJECT, boundTo: NO_OWNER }],
])("holds %s as the form edits it", (_, extra, expected) => {
  const fields = fieldsOf(taskOf(extra), "c-other", NOW, "Asia/Tokyo");
  expect(fields).toMatchObject({ prompt: "Do the thing", ...expected });
});

it("keeps the other kinds' fields at their defaults while a saved task has another kind", () => {
  const fields = fieldsOf(taskOf({ schedule: { kind: "hourly", minute: 45 } }), "c-1", NOW, "UTC");
  expect(fields).toMatchObject({ time: "09:00", days: ["monday"], runAt: "2026-11-19T11:00" });
  expect(fieldsOf(taskOf(), "c-1", NOW, "UTC")).toMatchObject({ time: "07:05", minute: 0, day: 19 });
  // A new monthly schedule starts on today's day in the daemon's zone.
  expect(fieldsOf(undefined, "c-1", Date.UTC(2026, 10, 19, 23, 0), "Asia/Tokyo").day).toBe(20);
});

function fieldsWith(patch: Partial<TaskFields>): TaskFields {
  return { ...fieldsOf(undefined, "c-1", NOW, "UTC"), ...patch };
}

it("finds a one-off's time missing when it is not a date and time, and past only while it is being changed", () => {
  const past = fieldsWith({ kind: "once", runAt: "2026-11-19T10:20" });
  const future = fieldsWith({ kind: "once", runAt: "2026-11-19T10:21" });
  expect(scheduleProblems(fieldsWith({ kind: "once", runAt: "" }), NOW, true, "UTC")).toEqual({ runAt: "missing" });
  expect(scheduleProblems(past, NOW, true, "UTC")).toEqual({ runAt: "past" });
  expect(scheduleProblems(future, NOW, true, "UTC")).toEqual({});
  // A saved one-off whose time has passed stays as it is.
  expect(scheduleProblems(past, NOW, false, "UTC")).toEqual({});
  // Read in the daemon's zone: 19:21 in Tokyo is still ahead of 10:20 UTC, 19:19 is behind it.
  expect(scheduleProblems(fieldsWith({ kind: "once", runAt: "2026-11-19T19:21" }), NOW, true, "Asia/Tokyo")).toEqual({});
  expect(scheduleProblems(fieldsWith({ kind: "once", runAt: "2026-11-19T19:19" }), NOW, true, "Asia/Tokyo")).toEqual({ runAt: "past" });
});

it("finds a time of day missing for daily, weekdays, monthly and weekly schedules, and no day for a weekly one", () => {
  for (const kind of ["daily", "weekdays", "monthly"] as const) {
    expect(scheduleProblems(fieldsWith({ kind, time: "" }), NOW, true)).toEqual({ time: "missing" });
    expect(scheduleProblems(fieldsWith({ kind, time: "06:30" }), NOW, true)).toEqual({});
  }
  expect(scheduleProblems(fieldsWith({ kind: "weekly", time: "", days: [] }), NOW, true)).toEqual({ time: "missing", days: "missing" });
  expect(scheduleProblems(fieldsWith({ kind: "weekly", time: "06:30", days: [] }), NOW, true)).toEqual({ days: "missing" });
  expect(scheduleProblems(fieldsWith({ kind: "weekly", time: "06:30", days: ["friday"] }), NOW, true)).toEqual({});
  expect(scheduleProblems(fieldsWith({ kind: "hourly", time: "" }), NOW, true)).toEqual({});
  expect(scheduleProblems(fieldsWith({ kind: "manual", time: "" }), NOW, true)).toEqual({});
});

it("builds the schedule the fields describe, and none while a field it needs holds no value", () => {
  expect(scheduleOf(fieldsWith({ kind: "once", runAt: "2026-11-20T08:00" }), "UTC")).toEqual({ kind: "once", run_at: Date.UTC(2026, 10, 20, 8, 0) });
  expect(scheduleOf(fieldsWith({ kind: "once", runAt: "2026-11-20T08:00" }), "Asia/Tokyo")).toEqual({ kind: "once", run_at: Date.UTC(2026, 10, 19, 23, 0) });
  expect(scheduleOf(fieldsWith({ kind: "once", runAt: "" }))).toBeUndefined();
  expect(scheduleOf(fieldsWith({ kind: "hourly", minute: 15 }))).toEqual({ kind: "hourly", minute: 15 });
  expect(scheduleOf(fieldsWith({ kind: "daily", time: "09:30" }))).toEqual({ kind: "daily", time: { hour: 9, minute: 30 } });
  expect(scheduleOf(fieldsWith({ kind: "weekdays", time: "18:00" }))).toEqual({ kind: "weekdays", time: { hour: 18, minute: 0 } });
  expect(scheduleOf(fieldsWith({ kind: "monthly", day: 31, time: "09:00" }))).toEqual({ kind: "monthly", day: 31, time: { hour: 9, minute: 0 } });
  expect(scheduleOf(fieldsWith({ kind: "monthly", day: 5, time: "" }))).toBeUndefined();
  expect(scheduleOf(fieldsWith({ kind: "daily", time: "" }))).toBeUndefined();
  expect(scheduleOf(fieldsWith({ kind: "weekly", time: "07:00", days: ["monday", "friday"] }))).toEqual({ kind: "weekly", time: { hour: 7, minute: 0 }, days: ["monday", "friday"] });
  expect(scheduleOf(fieldsWith({ kind: "weekly", time: "07:00", days: [] }))).toBeUndefined();
  expect(scheduleOf(fieldsWith({ kind: "weekly", time: "", days: ["monday"] }))).toBeUndefined();
  expect(scheduleOf(fieldsWith({ kind: "manual" }))).toEqual({ kind: "manual" });
});

it("saves a trimmed name and leaves a blank one out, with the agent and account of the choice", () => {
  expect(launchOf(fieldsWith({ name: "  Nightly  " }), { agent: "claude", account: "a-1" }, undefined, false)).toEqual({ title: "Nightly", agent: "claude", account: "a-1", compatibility: {} });
  expect(launchOf(fieldsWith({ name: "   " }), { agent: "claude", account: null }, undefined, false)).toEqual({ agent: "claude", account: null, compatibility: {} });
  // A saved task's own launch may name neither, which stays unsaid.
  expect(launchOf(fieldsWith({}), {}, undefined, false)).toEqual({ compatibility: {} });
});

it("saves the model, effort and switches the user set, and drops a switch set back to the parent's", () => {
  expect(launchOf(fieldsWith({}), { agent: "claude", account: null }, { model: "opus", effort: "high", compatibility: { skip_permissions: true, other: null } }, false)).toEqual({
    agent: "claude",
    account: null,
    model: "opus",
    effort: "high",
    compatibility: { skip_permissions: true },
  });
  // Choosing the agent's own default is a value of its own, not an absent one.
  expect(launchOf(fieldsWith({}), { agent: "claude", account: null }, { model: null, effort: null }, false)).toMatchObject({ model: null, effort: null });
});

it("reports to a console session only for a project task that chose one", () => {
  const reporting = fieldsWith({ boundTo: "s-hub" });
  const choice = { agent: "claude", account: null } as const;
  expect(launchOf(reporting, choice, undefined, true).bound_to).toBe("s-hub");
  expect(launchOf(reporting, choice, undefined, false)).not.toHaveProperty("bound_to");
  expect(launchOf(fieldsWith({ boundTo: NO_OWNER }), choice, undefined, true)).not.toHaveProperty("bound_to");
});

it("compares launches by what they save: a blank title is none and a field left out is one that is undefined", () => {
  expect(sameLaunch({ compatibility: {} }, { title: "  ", compatibility: {} })).toBe(true);
  expect(sameLaunch({ title: "A", compatibility: {} }, { title: " A ", compatibility: {} })).toBe(true);
  expect(sameLaunch({ title: "A", compatibility: {} }, { title: "B", compatibility: {} })).toBe(false);
  expect(sameLaunch({ compatibility: {}, model: undefined }, { compatibility: {} })).toBe(true);
  expect(sameLaunch({ agent: "claude", compatibility: {} }, { compatibility: {} })).toBe(false);
  expect(sameLaunch({ account: null, compatibility: {} }, { compatibility: {} })).toBe(false);
  expect(sameLaunch({ compatibility: { a: true } }, { compatibility: { a: false } })).toBe(false);
  expect(sameLaunch({ bound_to: "s-1", compatibility: {} }, { compatibility: {} })).toBe(false);
});

it("starts the pickers from the agent and account a saved launch names, with what it holds of the rest", () => {
  expect(launchPresetOf(undefined)).toBeUndefined();
  // One that names no agent starts from what its target resolves to.
  expect(launchPresetOf(taskOf().launch)).toBeUndefined();
  expect(launchPresetOf({ agent: "claude", compatibility: {} })).toEqual({ choice: { agent: "claude", account: null }, compatibility: {} });
  expect(launchPresetOf({ agent: "codex", account: "a-2", model: "gpt", effort: null, compatibility: { x: true } })).toEqual({
    choice: { agent: "codex", account: "a-2" },
    model: "gpt",
    effort: null,
    compatibility: { x: true },
  });
});
