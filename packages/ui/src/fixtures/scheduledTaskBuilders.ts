// Scheduled task records for a scenario's `scheduledTasks`, built as the daemon sends them.
import type { ScheduledRunOutcome, ScheduledTask, ScheduledTaskRun } from "../protocol";
import { minutesAgo } from "./builders";
import { nextRun } from "./scheduledTaskNextRun";

/** A console task of `consoleId`, enabled, that runs every day at 9:00 and has never run, with a prompt that names it;
 * its `run_sessions` is empty: the fixture daemon derives it from the scenario's runs and sessions when it sends the task,
 * as the daemon does, so what a scenario gives here is replaced;
 * `extra` sets what a scenario shows otherwise (the project, the schedule, the launch settings, that it is off). Its next
 * run is the one the daemon computes for its schedule, in `zone` (the window's own where `undefined`), unless `extra`
 * gives one. */
export function scheduledTaskOf(id: string, consoleId: string, extra: Partial<ScheduledTask> = {}, zone?: string): ScheduledTask {
  const task = {
    id,
    console_id: consoleId,
    project_id: null,
    launch: { compatibility: {} },
    prompt: "Summarise yesterday's commits",
    schedule: { kind: "daily", time: { hour: 9, minute: 0 } },
    enabled: true,
    created_at: minutesAgo(60 * 24 * 7),
    updated_at: minutesAgo(60 * 24 * 7),
    last_run_at: null,
    run_sessions: [],
    ...extra,
  } satisfies Omit<ScheduledTask, "next_run_at"> & Partial<ScheduledTask>;
  return { ...task, next_run_at: extra.next_run_at !== undefined ? extra.next_run_at : task.enabled ? nextRun(task.schedule, Date.now(), zone) : null };
}

/** A run of the task `taskId` by Run now, `minutes` before the page loaded, with `outcome`; `extra` sets what a scenario
 * shows otherwise (what started it, the occurrence it stands for). */
export function scheduledRunOf(id: string, taskId: string, minutes: number, outcome: ScheduledRunOutcome, extra: Partial<ScheduledTaskRun> = {}): ScheduledTaskRun {
  return { id, task_id: taskId, ran_at: minutesAgo(minutes), due_at: null, trigger: "run_now", outcome, ...extra };
}
