import { applyCompatibility } from "../agents/launchCompatibility";
import type { LaunchOverrides } from "../agents/agentLaunchSettings";
import type { AccountChoice } from "../accounts/choiceEntries";
import type { LaunchPreset } from "../dialogs/useLaunchChoice";
import type { Schedule, ScheduledTask, ScheduledTaskLaunch, Weekday } from "../protocol";
import { sameValue } from "../values/sameValue";
import { isKnownScheduleKind, type ScheduleKind } from "../values/scheduleKind";
import { dayOfMonthAt, instantInputValue, parseInstantInput, parseTimeInput, timeInputValue, WEEKDAYS } from "./scheduleText";

/** `OptionSelect`'s value for Report to's "None"; a session id is never this. */
export const NO_OWNER = "none";

/** `OptionSelect`'s value for Project's "None", which starts a console session; a project id is never this. */
export const NO_PROJECT = "none";

/** What the task form edits besides the agent and its launch settings, which the session dialog's launch choice holds
 * (`useLaunchChoice`). Each field is held as its control holds it, so a time being typed is kept as typed. */
export interface TaskFields {
  /** The console the task starts a session in. */
  consoleId: string;
  /** The project it starts a session in, or `NO_PROJECT` for a console session. */
  projectId: string;
  name: string;
  /** The console session a project task's session reports to, or `NO_OWNER`. */
  boundTo: string;
  prompt: string;
  /** The schedule's kind; `undefined` for a saved task whose kind this client does not know (a newer daemon's), which
   * the form shows as none picked and saves no schedule for until one is. */
  kind: ScheduleKind | undefined;
  /** A one-off's date and time, as a local date-and-time input holds it. */
  runAt: string;
  /** An hourly schedule's minute past the hour. */
  minute: number;
  /** A monthly schedule's day of the month, 1 to 31. */
  day: number;
  /** The time of day of a daily, weekdays, weekly or monthly schedule, as a time input holds it. */
  time: string;
  /** A weekly schedule's days, in week order. */
  days: Weekday[];
  enabled: boolean;
}

/** The time a new schedule starts at, until one is picked. */
const DEFAULT_TIME = { hour: 9, minute: 0 };

/** The fields as `task` holds them, or as a new task's start: a daily schedule at 9:00, a one-off at the next full hour
 * after `now`, a monthly schedule on the day of the month `now` falls on, and a console session of `consoleId`. A schedule's other kinds start at their defaults, so switching to
 * one shows sensible values. A one-off's date and time and a new monthly schedule's day are read in `zone`, the daemon's (the window's own where
 * `undefined`). A saved task whose schedule is of a kind this client does not know has no kind, and the defaults. */
export function fieldsOf(task: ScheduledTask | undefined, consoleId: string | undefined, now: number, zone?: string): TaskFields {
  const nextHour = Math.ceil((now + 1) / 3_600_000) * 3_600_000;
  const schedule = task && isKnownScheduleKind(task.schedule.kind) ? task.schedule : undefined;
  const time = schedule && "time" in schedule ? schedule.time : DEFAULT_TIME;
  return {
    consoleId: task?.console_id ?? consoleId ?? "",
    projectId: task?.project_id ?? NO_PROJECT,
    name: task?.launch.title ?? "",
    boundTo: task?.project_id ? (task.launch.bound_to ?? NO_OWNER) : NO_OWNER,
    prompt: task?.prompt ?? "",
    kind: task ? schedule?.kind : "daily",
    runAt: instantInputValue(schedule?.kind === "once" ? schedule.run_at : nextHour, zone),
    minute: schedule?.kind === "hourly" ? schedule.minute : 0,
    day: schedule?.kind === "monthly" ? schedule.day : dayOfMonthAt(now, zone),
    time: timeInputValue(time),
    days: schedule?.kind === "weekly" ? WEEKDAYS.filter((day) => schedule.days.includes(day)) : ["monday"],
    enabled: task?.enabled ?? true,
  };
}

/** What is wrong with the schedule the fields describe, by field; none when it can be saved. A one-off's time, read in
 * `zone`, must be in the future only while it is being changed (`checkPast`): a saved one-off whose time has passed
 * stays as it is. */
export function scheduleProblems(fields: TaskFields, now: number, checkPast: boolean, zone?: string): { runAt?: "missing" | "past"; time?: "missing"; days?: "missing" } {
  switch (fields.kind) {
    case "once": {
      const at = parseInstantInput(fields.runAt, zone);
      if (at === undefined) return { runAt: "missing" };
      return checkPast && at <= now ? { runAt: "past" } : {};
    }
    case "daily":
    case "weekdays":
    case "monthly":
      return parseTimeInput(fields.time) ? {} : { time: "missing" };
    case "weekly":
      return { ...(parseTimeInput(fields.time) ? {} : { time: "missing" as const }), ...(fields.days.length === 0 ? { days: "missing" as const } : {}) };
    default:
      return {};
  }
}

/** The schedule the fields describe, a one-off's date and time read in `zone`, or `undefined` while one of its fields
 * holds no value or no kind is picked. */
export function scheduleOf(fields: TaskFields, zone?: string): Schedule | undefined {
  const time = parseTimeInput(fields.time);
  switch (fields.kind) {
    case "once": {
      const at = parseInstantInput(fields.runAt, zone);
      return at === undefined ? undefined : { kind: "once", run_at: at };
    }
    case "hourly":
      return { kind: "hourly", minute: fields.minute };
    case "daily":
    case "weekdays":
      return time && { kind: fields.kind, time };
    case "monthly":
      return time && { kind: "monthly", day: fields.day, time };
    case "weekly":
      return time && fields.days.length > 0 ? { kind: "weekly", time, days: fields.days } : undefined;
    case "manual":
      return { kind: "manual" };
    default:
      return undefined;
  }
}

/** The launch settings the form saves: the name, the agent and account (`choice`: the ones picked, or a saved task's own
 * while the user has picked none, which may leave either to the target at each run), the model, effort and
 * compatibility switches the user set (absent ones follow the target at each run), and for a project task the console
 * session it reports to. */
export function launchOf(fields: Pick<TaskFields, "name" | "boundTo">, choice: AccountChoice | Pick<ScheduledTaskLaunch, "agent" | "account">, overrides: LaunchOverrides | undefined, projectTask: boolean): ScheduledTaskLaunch {
  const title = fields.name.trim();
  return {
    ...(title && { title }),
    ...(choice.agent !== undefined && { agent: choice.agent }),
    ...(choice.account !== undefined && { account: choice.account }),
    ...(overrides?.model !== undefined && { model: overrides.model }),
    ...(overrides?.effort !== undefined && { effort: overrides.effort }),
    compatibility: applyCompatibility({}, overrides?.compatibility),
    ...(projectTask && fields.boundTo !== NO_OWNER && { bound_to: fields.boundTo }),
  };
}

/** `launch` without the parts the form's other fields hold (the name and the console session to report to), which follow
 * the saved task on their own. */
function launchSettings(launch: ScheduledTaskLaunch): ScheduledTaskLaunch {
  const { title: _title, bound_to: _owner, ...rest } = launch;
  return rest;
}

/** Whether two launches save the same: a blank title is none, and a field left out is the same as one given as
 * `undefined`. */
export function sameLaunch(left: ScheduledTaskLaunch, right: ScheduledTaskLaunch): boolean {
  const plain = (launch: ScheduledTaskLaunch) =>
    Object.fromEntries(Object.entries({ ...launch, title: launch.title?.trim() || undefined }).filter(([, value]) => value !== undefined));
  return sameValue(plain(left), plain(right));
}

/** Whether two launches, either of them none, hold the same agent, account and launch settings, whatever their names and
 * the console sessions they report to. */
export function sameLaunchSettings(left: ScheduledTaskLaunch | undefined, right: ScheduledTaskLaunch | undefined): boolean {
  if (left === undefined || right === undefined) return left === right;
  return sameLaunch(launchSettings(left), launchSettings(right));
}

/** Where the launch choice of a saved launch starts: the agent and account it names, and the model, effort and switches
 * it holds. A launch that names no agent (one saved by another client) starts from what its target resolves to. An agent
 * with no account named shows its default account, which the saved launch still leaves to the target until an account
 * is picked (`useTaskLaunch`). */
export function launchPresetOf(launch: ScheduledTaskLaunch | undefined): LaunchPreset | undefined {
  if (!launch?.agent) return undefined;
  return {
    choice: { agent: launch.agent, account: launch.account ?? null },
    ...(launch.model !== undefined && { model: launch.model }),
    ...(launch.effort !== undefined && { effort: launch.effort }),
    compatibility: launch.compatibility,
  };
}
