import { knownValues } from "./knownValue";

/** The kinds of schedule a scheduled task has, in the order the form offers them. They stand here, as plain strings, so that the
 * store, which reads a task request's schedule, and the Scheduled tasks page, which words and edits one, share one list;
 * `scheduledTasks/scheduleText.ts` checks it against the protocol's `Schedule`. */
export const SCHEDULE_KINDS = ["once", "hourly", "daily", "weekdays", "weekly", "monthly", "manual"] as const;

export type ScheduleKind = (typeof SCHEDULE_KINDS)[number];

/** Whether `raw`, the `kind` of a schedule, is one this client words: a newer daemon may send another, which is shown
 * as no schedule and never edited as one. */
export const isKnownScheduleKind = knownValues<ScheduleKind>({ once: true, hourly: true, daily: true, weekdays: true, weekly: true, monthly: true, manual: true });
