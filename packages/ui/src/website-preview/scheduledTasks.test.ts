import { expect, it } from "vitest";

import { previewCopy } from "./locales/en";
import { previewScheduledTasks } from "./scheduledTasks";

const IDS = { console: "c-1", coordinator: "s-coordinator", server: "p-server", merchant: "p-merchant" };

// Local dates: the preview's daemon reads schedules in the visitor's own zone. 19 November 2026 is a Thursday.
const at = (day: number, hour: number, minute = 0) => new Date(2026, 10, day, hour, minute).getTime();
const seed = (now: number) => previewScheduledTasks(previewCopy, now, IDS);

it("seeds a daily 2:00 check, a weekdays 8:30 report and a weekly Friday 16:00 audit, with the copy's names", () => {
  const { tasks } = seed(at(19, 12));
  expect(tasks.map((task) => task.schedule)).toEqual([
    { kind: "daily", time: { hour: 2, minute: 0 } },
    { kind: "weekdays", time: { hour: 8, minute: 30 } },
    { kind: "weekly", time: { hour: 16, minute: 0 }, days: ["friday"] },
  ]);
  expect(tasks.map((task) => task.launch.title)).toEqual([
    previewCopy.scheduledTasks.orderChecksTitle,
    previewCopy.scheduledTasks.progressTitle,
    previewCopy.scheduledTasks.redemptionTitle,
  ]);
  expect(tasks.map((task) => task.prompt)).toEqual([
    previewCopy.scheduledTasks.orderChecksPrompt,
    previewCopy.scheduledTasks.progressPrompt,
    previewCopy.scheduledTasks.redemptionPrompt,
  ]);
  expect(tasks.every((task) => task.enabled && task.console_id === IDS.console)).toBe(true);
});

it("puts the checks and the audit in their projects, reporting to the coordinator, and the report in the console", () => {
  const [checks, report, audit] = seed(at(19, 12)).tasks;
  expect(checks).toMatchObject({ project_id: IDS.server, launch: { bound_to: IDS.coordinator } });
  expect(report!.project_id).toBeNull();
  expect(report!.launch).not.toHaveProperty("bound_to");
  expect(audit).toMatchObject({ project_id: IDS.merchant, launch: { bound_to: IDS.coordinator } });
});

it("sets each task's last run at the latest moment its schedule names, however long after the build the visitor comes", () => {
  const lastRuns = (now: number) => seed(now).tasks.map((task) => task.last_run_at);
  // Thursday noon: today's 2:00 and 8:30, and the Friday before.
  expect(lastRuns(at(19, 12))).toEqual([at(19, 2), at(19, 8, 30), at(13, 16)]);
  // Before today's 2:00 the check last ran the night before; the moment itself counts as past.
  expect(lastRuns(at(19, 1))[0]).toBe(at(18, 2));
  expect(lastRuns(at(19, 2))[0]).toBe(at(19, 2));
  // On a Saturday (21 November) the report last ran on Friday.
  expect(lastRuns(at(21, 12))[1]).toBe(at(20, 8, 30));
});

it("gives the checks four runs, most recent first: two nights that started a session, one that failed, one run by hand", () => {
  const now = at(19, 12);
  const { runs } = seed(now);
  const checks = runs!["preview-task-checks"]!;
  expect(Object.keys(runs!)).toEqual(["preview-task-checks"]);
  expect(checks.map((run) => run.outcome.kind)).toEqual(["started", "started", "failed", "started"]);
  expect(checks.map((run) => run.trigger)).toEqual(["scheduled", "scheduled", "scheduled", "run_now"]);
  expect(checks.map((run) => run.ran_at)).toEqual([at(19, 2), at(18, 2), at(17, 2), at(16, 14)]);
  for (const run of checks) {
    expect(run.task_id).toBe("preview-task-checks");
    expect(run.due_at).toBe(run.trigger === "scheduled" ? run.ran_at : null);
  }
  expect(checks.map((run) => run.ran_at)).toEqual([...checks.map((run) => run.ran_at)].sort((left, right) => right - left));
  expect(checks[0]!.ran_at).toBe(seed(now).tasks[0]!.last_run_at);
  expect(checks[2]!.outcome).toMatchObject({ kind: "failed", code: "account_not_signed_in" });
});
