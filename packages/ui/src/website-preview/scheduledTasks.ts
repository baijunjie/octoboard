import type { ScheduledTask, ScheduledTaskRun } from "../protocol";
import type { ScheduledTasksFixture } from "../fixtures/fixture";
import { scheduledRunOf, scheduledTaskOf } from "../fixtures/scheduledTaskBuilders";
import type { previewCopy as english } from "./locales/en";

/** The latest moment at or before `now` that is `hour:minute` on a day `onDay` accepts (a weekday number, Sunday 0), in the
 * visitor's own time zone, which is the zone the preview's daemon reads schedules in; `earlier` more such moments back. */
function lastOccurrence(now: number, hour: number, minute: number, onDay: (weekday: number) => boolean, earlier = 0): number {
  const at = new Date(now);
  at.setHours(hour, minute, 0, 0);
  let skipped = 0;
  // Stepping back by calendar days, so a change of daylight saving time moves no run to another hour.
  for (let candidate = at.getTime(); ; candidate = new Date(new Date(candidate).setDate(new Date(candidate).getDate() - 1)).getTime()) {
    if (candidate <= now && onDay(new Date(candidate).getDay()) && skipped++ === earlier) return candidate;
  }
}

/** The scheduled tasks the demo's workspace holds, over the sessions and projects it already has: three tasks of three
 * schedule kinds, the nightly check of the order API with a few runs behind it, one of which could not start. Each time is
 * taken from `now`, so the last runs sit at the moments the schedules name, however long after the page was built the visitor
 * comes. */
export function previewScheduledTasks(
  copy: typeof english,
  now: number,
  ids: { console: string; coordinator: string; server: string; merchant: string },
): ScheduledTasksFixture {
  const everyDay = () => true;
  const nightly = (earlier: number) => lastOccurrence(now, 2, 0, everyDay, earlier);
  const friday = (weekday: number) => weekday === 5;
  const tasks: ScheduledTask[] = [
    scheduledTaskOf("preview-task-checks", ids.console, {
      project_id: ids.server,
      launch: { title: copy.scheduledTasks.orderChecksTitle, agent: "claude", compatibility: {}, bound_to: ids.coordinator },
      prompt: copy.scheduledTasks.orderChecksPrompt,
      schedule: { kind: "daily", time: { hour: 2, minute: 0 } },
      last_run_at: nightly(0),
    }),
    scheduledTaskOf("preview-task-progress", ids.console, {
      launch: { title: copy.scheduledTasks.progressTitle, compatibility: {} },
      prompt: copy.scheduledTasks.progressPrompt,
      schedule: { kind: "weekdays", time: { hour: 8, minute: 30 } },
      last_run_at: lastOccurrence(now, 8, 30, (weekday) => weekday >= 1 && weekday <= 5),
    }),
    scheduledTaskOf("preview-task-redemption", ids.console, {
      project_id: ids.merchant,
      launch: { title: copy.scheduledTasks.redemptionTitle, agent: "claude", compatibility: {}, bound_to: ids.coordinator },
      prompt: copy.scheduledTasks.redemptionPrompt,
      schedule: { kind: "weekly", time: { hour: 16, minute: 0 }, days: ["friday"] },
      last_run_at: lastOccurrence(now, 16, 0, friday),
    }),
  ];
  const run = (id: string, outcome: ScheduledTaskRun["outcome"], at: number, trigger: ScheduledTaskRun["trigger"] = "scheduled") =>
    scheduledRunOf(id, "preview-task-checks", 0, outcome, { ran_at: at, due_at: trigger === "scheduled" ? at : null, trigger });
  // Most recent first, as the daemon lists them: two nights that started a session, a night the account was signed out,
  // and a run started by hand whose session no longer exists (the preview's scenario holds no session of that id).
  const runs: ScheduledTaskRun[] = [
    run("preview-run-1", { kind: "started", session: "preview-review", started_independent: false }, nightly(0)),
    run("preview-run-2", { kind: "started", session: "preview-order-checks", started_independent: false }, nightly(1)),
    run(
      "preview-run-3",
      {
        kind: "failed",
        code: "account_not_signed_in",
        params: { account: "Default", agent: "Claude Code" },
        message: "Default (Claude Code) is not logged in, so a session cannot be opened on it.",
      },
      nightly(2),
    ),
    run("preview-run-4", { kind: "started", session: "preview-archived-check", started_independent: false }, nightly(3) + 12 * 3_600_000, "run_now"),
  ];
  return { tasks, runs: { "preview-task-checks": runs } };
}
