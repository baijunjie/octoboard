// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import { accountOf, consoleOf, projectOf, sessionOf, snapshotState } from "./builders";
import { createFixtureDaemon } from "./fixtureDaemon";
import type { Fixture } from "./fixture";
import { scheduledRunOf, scheduledTaskOf } from "./scheduledTaskBuilders";
import type { ScheduledTask, ScheduledTaskRun } from "../protocol";
import { fakeTimers } from "../test/clock";

const main = consoleOf("c-main", "Main");
const other = consoleOf("c-other", "Other");
const web = projectOf("p-web", main.id, "Website");
const elsewhere = projectOf("p-elsewhere", other.id, "Elsewhere");
const hub = sessionOf("s-hub", main.id, undefined, "Hub", "idle");

/** A fixed next run, which no schedule would compute, so that a task keeping it is told from one recomputed. */
const KEPT_NEXT_RUN = 1_900_000_000_000;

const task = (extra: Partial<ScheduledTask> = {}): ScheduledTask => scheduledTaskOf("t", main.id, { project_id: web.id, launch: { compatibility: {}, bound_to: hub.id }, next_run_at: KEPT_NEXT_RUN, ...extra });

const make = (tasks: ScheduledTask[] = [task()], extra: Partial<Fixture> = {}) =>
  createFixtureDaemon({ state: snapshotState({ consoles: [main, other], projects: [web, elsewhere], sessions: [hub] }), scheduledTasks: { tasks }, ...extra });

/** The runs the daemon has recorded of task `t` once the first has been, as it lists them. */
async function recordedRuns(daemon: ReturnType<typeof make>) {
  let runs: ScheduledTaskRun[] = [];
  await vi.waitFor(async () => {
    const reply = await daemon.request({ type: "list_scheduled_task_runs", task: "t" });
    runs = reply.runs;
    expect(runs).toHaveLength(1);
  });
  return runs;
}

const saved = (daemon: ReturnType<typeof make>): ScheduledTask | undefined => daemon.store.getState().scheduledTasks?.get("t");

describe("the fixture daemon's creating scheduled tasks", () => {
  it("creates a task off, with no next run, when it is asked to, and on otherwise", async () => {
    const daemon = make([]);
    const base = { type: "create_scheduled_task" as const, console_id: main.id, prompt: "Report", schedule: { kind: "daily" as const, time: { hour: 9, minute: 0 } } };
    const off = await daemon.request({ ...base, enabled: false });
    const on = await daemon.request(base);
    expect(off).toMatchObject({ task: { enabled: false, next_run_at: null } });
    expect(on).toMatchObject({ task: { enabled: true, next_run_at: expect.any(Number) } });
  });
});

describe("the fixture daemon's editing scheduled tasks", () => {
  it("keeps the project when only the console is named, and refuses one the new console does not hold", async () => {
    const daemon = make();
    await expect(daemon.request({ type: "update_scheduled_task", task: "t", console_id: other.id })).rejects.toMatchObject({ code: "unknown_project" });
    await daemon.request({ type: "list_scheduled_tasks" });
    await daemon.request({ type: "update_scheduled_task", task: "t", console_id: main.id });
    expect(saved(daemon)).toMatchObject({ console_id: main.id, project_id: web.id });
  });

  it("changes the project on its own, whatever the console, and a null one makes a console task that reports to none", async () => {
    const daemon = make([task(), task({ id: "u" })]);
    await daemon.request({ type: "list_scheduled_tasks" });
    await expect(daemon.request({ type: "update_scheduled_task", task: "t", project_id: elsewhere.id })).rejects.toMatchObject({ code: "unknown_project" });
    await daemon.request({ type: "update_scheduled_task", task: "t", project_id: null });
    expect(saved(daemon)).toMatchObject({ project_id: null, launch: { compatibility: {} } });
    expect(saved(daemon)?.launch.bound_to).toBeUndefined();
    // The other task is untouched.
    expect(daemon.store.getState().scheduledTasks?.get("u")).toMatchObject({ project_id: web.id, launch: { bound_to: hub.id } });
  });

  it("keeps the next run of a schedule left alone, computes one for a changed schedule, and never turns the task on or off", async () => {
    const daemon = make([task(), task({ id: "off", enabled: false, next_run_at: null })]);
    await daemon.request({ type: "list_scheduled_tasks" });
    await daemon.request({ type: "update_scheduled_task", task: "t", prompt: "Another prompt", schedule: { kind: "daily", time: { minute: 0, hour: 9 } } });
    expect(saved(daemon)).toMatchObject({ prompt: "Another prompt", enabled: true, next_run_at: KEPT_NEXT_RUN });
    await daemon.request({ type: "update_scheduled_task", task: "t", schedule: { kind: "hourly", minute: 5 } });
    expect(saved(daemon)).toMatchObject({ schedule: { kind: "hourly", minute: 5 } });
    expect(saved(daemon)?.next_run_at).not.toBe(KEPT_NEXT_RUN);
    await daemon.request({ type: "update_scheduled_task", task: "off", schedule: { kind: "hourly", minute: 5 } });
    expect(daemon.store.getState().scheduledTasks?.get("off")).toMatchObject({ enabled: false, next_run_at: null });
  });
});

describe("the fixture daemon's turning scheduled tasks on and off", () => {
  it("leaves a task already in the state asked for exactly as it was", async () => {
    const before = task({ updated_at: 1_000 });
    const daemon = make([before]);
    await daemon.request({ type: "list_scheduled_tasks" });
    await daemon.request({ type: "set_scheduled_task_enabled", task: "t", enabled: true });
    expect(saved(daemon)).toEqual(before);
    await daemon.request({ type: "set_scheduled_task_enabled", task: "t", enabled: false });
    expect(saved(daemon)).toMatchObject({ enabled: false, next_run_at: null });
    expect(saved(daemon)?.updated_at).toBeGreaterThan(1_000);
  });
});

describe("the fixture daemon's stamping of a task's changes", () => {
  it("stamps every stored change of the definition, of being on and of a cleared account past the one before, within one millisecond too, and leaves a change of nothing alone", async () => {
    fakeTimers({ now: 5_000, toFake: ["Date"] });
    const account = accountOf("a", "claude", "Work", "/Users/dev/.claude-work");
    const base = snapshotState({ consoles: [main], projects: [web], sessions: [hub] });
    const daemon = createFixtureDaemon({ state: { ...base, settings: { ...base.settings!, accounts: [account] } }, scheduledTasks: { tasks: [task({ updated_at: 5_000, launch: { compatibility: {}, account: account.id } })] } });
    await daemon.request({ type: "list_scheduled_tasks" });
    // The clock does not move: each change is still stamped past the last.
    await daemon.request({ type: "update_scheduled_task", task: "t", prompt: "Another prompt" });
    expect(saved(daemon)?.updated_at).toBe(5_001);
    await daemon.request({ type: "set_scheduled_task_enabled", task: "t", enabled: false });
    expect(saved(daemon)?.updated_at).toBe(5_002);
    await daemon.request({ type: "set_scheduled_task_enabled", task: "t", enabled: false });
    expect(saved(daemon)?.updated_at).toBe(5_002);
    await daemon.request({ type: "delete_account", account: account.id });
    expect(saved(daemon)?.updated_at).toBe(5_003);
    // A clock ahead of the last stamp is taken as it is.
    vi.setSystemTime(9_000);
    await daemon.request({ type: "update_scheduled_task", task: "t", prompt: "A third prompt" });
    expect(saved(daemon)?.updated_at).toBe(9_000);
  });
});

describe("the fixture daemon's removing an account that tasks name", () => {
  it("clears it from those tasks and tells the window of each", async () => {
    const account = accountOf("a", "claude", "Work", "/Users/dev/.claude-work");
    const base = snapshotState({ consoles: [main], projects: [web], sessions: [hub] });
    const named = task({ id: "named", launch: { compatibility: {}, account: account.id } });
    const inheriting = task({ id: "inheriting" });
    const daemon = createFixtureDaemon({ state: { ...base, settings: { ...base.settings!, accounts: [account] } }, scheduledTasks: { tasks: [named, inheriting] } });
    await daemon.request({ type: "list_scheduled_tasks" });
    await daemon.request({ type: "delete_account", account: account.id });
    const tasks = daemon.store.getState().scheduledTasks;
    expect(tasks?.get("named")?.launch).toEqual({ compatibility: {} });
    expect(tasks?.get("inheriting")).toEqual(inheriting);
  });
});

describe("the fixture daemon's running scheduled tasks", () => {
  it("acknowledges at once and records the run afterwards, as started with the session it opened", async () => {
    const daemon = make();
    await daemon.request({ type: "list_scheduled_tasks" });
    await expect(daemon.request({ type: "run_scheduled_task", task: "t" })).resolves.toEqual({ type: "ack", id: null });
    const [run] = await recordedRuns(daemon);
    expect(run).toMatchObject({ trigger: "run_now", outcome: { kind: "started", started_independent: false } });
    expect(saved(daemon)?.last_run_at).toBe(run!.ran_at);
  });

  it.each([
    ["archived", sessionOf("s-hub", main.id, undefined, "Hub", "archived")],
    ["still starting", sessionOf("s-hub", main.id, undefined, "Hub", "starting")],
    ["of another console", sessionOf("s-hub", other.id, undefined, "Hub", "idle")],
  ])("starts independent when the console session to report to is %s", async (_, session) => {
    const daemon = createFixtureDaemon({ state: snapshotState({ consoles: [main, other], projects: [web], sessions: [session] }), scheduledTasks: { tasks: [task()] } });
    await daemon.request({ type: "list_scheduled_tasks" });
    await daemon.request({ type: "run_scheduled_task", task: "t" });
    expect((await recordedRuns(daemon))[0]).toMatchObject({ outcome: { kind: "started", started_independent: true } });
  });

  it("starts independent when that console session is gone", async () => {
    const daemon = make([task({ launch: { compatibility: {}, bound_to: "s-gone" } })]);
    await daemon.request({ type: "list_scheduled_tasks" });
    await daemon.request({ type: "run_scheduled_task", task: "t" });
    expect((await recordedRuns(daemon))[0]).toMatchObject({ outcome: { kind: "started", started_independent: true } });
  });

  it("records a refusal of the session as a failed run with its code, not as an error", async () => {
    const account = accountOf("a", "claude", "Work", "/Users/dev/.claude-work", { disabled: true });
    const base = snapshotState({ consoles: [main], projects: [web], sessions: [hub] });
    const daemon = createFixtureDaemon({ state: { ...base, settings: { ...base.settings!, accounts: [account] } }, scheduledTasks: { tasks: [task({ launch: { compatibility: {}, agent: "claude", account: account.id } })] } });
    await daemon.request({ type: "list_scheduled_tasks" });
    await expect(daemon.request({ type: "run_scheduled_task", task: "t" })).resolves.toEqual({ type: "ack", id: null });
    expect((await recordedRuns(daemon))[0]!.outcome).toMatchObject({ kind: "failed", code: "account_disabled" });
  });
});

describe("a scenario's failing scheduled task listings", () => {
  const error = { code: "internal_error", params: { detail: "database is locked" }, message: "database is locked" };

  it("refuses the listing of the tasks and of the runs with the error it gives, and no other request", async () => {
    const daemon = make([task()], { scheduledTasks: { tasks: [task()], runs: { t: [scheduledRunOf("r", "t", 5, { kind: "started", session: hub.id, started_independent: false })] }, listError: error, runsError: error } });
    await expect(daemon.request({ type: "list_scheduled_tasks" })).rejects.toMatchObject({ code: error.code, params: error.params });
    await expect(daemon.request({ type: "list_scheduled_task_runs", task: "t" })).rejects.toMatchObject({ code: error.code });
    await expect(daemon.request({ type: "set_scheduled_task_enabled", task: "t", enabled: false })).resolves.toMatchObject({ type: "scheduled_task_saved" });
  });
});

describe("the run sessions the fixture daemon sends with a task", () => {
  const working = sessionOf("s-working", main.id, undefined, "Working", "working");
  const archived = sessionOf("s-archived", main.id, undefined, "Archived", "archived");
  const runOf = (id: string, session: string) => scheduledRunOf(id, "t", 1, { kind: "started", session, started_independent: false });
  const failed = scheduledRunOf("r-failed", "t", 2, { kind: "failed", code: "internal_error", params: {}, message: "failed" });

  it("lists the sessions of the kept runs most recent first, each once, leaving out archived and unheld ones and failed runs", async () => {
    const runs = [runOf("r1", "s-working"), failed, runOf("r2", "s-archived"), runOf("r3", "s-gone"), runOf("r4", "s-hub"), runOf("r5", "s-working")];
    const daemon = createFixtureDaemon({ state: snapshotState({ consoles: [main], projects: [web], sessions: [hub, working, archived] }), scheduledTasks: { tasks: [task()], runs: { t: runs } } });
    const reply = await daemon.request({ type: "list_scheduled_tasks" });
    expect(reply.tasks[0]!.run_sessions).toEqual(["s-working", "s-hub"]);
  });

  it("carries the session a run just started in the upserted task, before the run is announced", async () => {
    const daemon = make();
    await daemon.request({ type: "list_scheduled_tasks" });
    expect(saved(daemon)?.run_sessions).toEqual([]);
    // What the window holds of the runs at the moment the task arrives carrying the new session: the run is not announced yet.
    const runsWhenTaskArrived: (number | undefined)[] = [];
    daemon.store.subscribe((state, before) => {
      if (state.scheduledTasks !== before.scheduledTasks && state.scheduledTasks?.get("t")?.run_sessions.length) runsWhenTaskArrived.push(state.scheduledTaskRuns.get("t")?.length);
    });
    await daemon.request({ type: "run_scheduled_task", task: "t" });
    const [run] = await recordedRuns(daemon);
    const session = run!.outcome.kind === "started" ? run!.outcome.session : undefined;
    expect(session).toBeDefined();
    expect(saved(daemon)?.run_sessions).toEqual([session]);
    expect(runsWhenTaskArrived).toEqual([undefined]);
  });
});

describe("the fixture daemon's Report to at save", () => {
  const archived = sessionOf("s-archived", main.id, undefined, "Archived", "archived");
  const starting = sessionOf("s-starting", main.id, undefined, "Starting", "starting");
  const projectSession = sessionOf("s-project", main.id, web.id, "Project work", "idle");
  const otherConsole = sessionOf("s-other", other.id, undefined, "Elsewhere", "idle");
  const daemon = () => createFixtureDaemon({ state: snapshotState({ consoles: [main, other], projects: [web], sessions: [hub, archived, starting, projectSession, otherConsole] }), scheduledTasks: { tasks: [task()] } });
  const saveWith = (bound_to: string) => daemon().request({ type: "update_scheduled_task", task: "t", launch: { compatibility: {}, bound_to } });

  it.each(["s-hub", "s-archived", "s-starting"])("accepts the console session %s of the task's console, whatever its status", async (session) => {
    await expect(saveWith(session)).resolves.toMatchObject({ task: { launch: { bound_to: session } } });
  });

  it.each(["s-gone", "s-project", "s-other"])("refuses %s, which is no console session of the task's console, as an unknown session", async (session) => {
    await expect(saveWith(session)).rejects.toMatchObject({ code: "unknown_session", params: { session } });
  });
});
