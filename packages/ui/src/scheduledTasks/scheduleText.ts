import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Language } from "../i18n/languages";
import type { Schedule, TimeOfDay, Weekday } from "../protocol";
import { SCHEDULE_KINDS, type ScheduleKind } from "../values/scheduleKind";

// The shared list of kinds and the protocol's `Schedule` name the same ones: a kind in either and not the other fails
// type-checking, the first line for the list holding a kind the protocol lacks, the second for the reverse.
SCHEDULE_KINDS satisfies readonly Schedule["kind"][];
true satisfies Schedule["kind"] extends ScheduleKind ? true : false;

/** The name of each kind of schedule, as the form's Schedule picker lists it. */
export const SCHEDULE_KIND_LABELS: Record<ScheduleKind, PlainMessageKey> = {
  once: "scheduledTasks.schedule.once",
  hourly: "scheduledTasks.schedule.hourly",
  daily: "scheduledTasks.schedule.daily",
  weekdays: "scheduledTasks.schedule.weekdays",
  weekly: "scheduledTasks.schedule.weekly",
  monthly: "scheduledTasks.schedule.monthly",
  manual: "scheduledTasks.schedule.manual",
};

/** The days of the week, Monday first, as the daemon names them. */
export const WEEKDAYS = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"] as const satisfies readonly Weekday[];

/** A Monday in January 1990 at noon UTC, from which each day of the week is reached by adding days to name it. */
const A_MONDAY = Date.UTC(1990, 0, 1, 12);

const formats = new Map<string, Intl.DateTimeFormat>();

function dateFormat(language: Language, options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const key = `${language} ${JSON.stringify(options)}`;
  let format = formats.get(key);
  if (!format) {
    format = new Intl.DateTimeFormat(language, options);
    formats.set(key, format);
  }
  return format;
}

/** The name of `day` in `language`, short ("Mon") or long ("Monday"). */
export function weekdayName(language: Language, day: Weekday, width: "short" | "long" = "short"): string {
  const offset = WEEKDAYS.indexOf(day);
  return dateFormat(language, { weekday: width, timeZone: "UTC" }).format(new Date(A_MONDAY + offset * 86_400_000));
}

/** A day of the month as `language` writes one ("15", "15日"), formatted on a month of 31 days so that every day has a
 * name. */
export function formatDayOfMonth(language: Language, day: number): string {
  return dateFormat(language, { day: "numeric", timeZone: "UTC" }).format(new Date(Date.UTC(1990, 0, day, 12)));
}

/** A time of day as `language` writes one ("9:30 AM", "09:30"). */
export function formatTimeOfDay(language: Language, time: TimeOfDay): string {
  return dateFormat(language, { hour: "numeric", minute: "2-digit", timeZone: "UTC" }).format(new Date(Date.UTC(1990, 0, 1, time.hour, time.minute)));
}

/** The time zone the page shows and enters times in: the daemon's (`zone`, an IANA name), or the window's own where it
 * is `undefined`; `named` writes the zone after an instant, where it differs from the window's. */
export interface TaskZone {
  zone: string | undefined;
  named: boolean;
}

/** The window's own zone, which times are shown in where the daemon names none. */
export const WINDOW_ZONE: TaskZone = { zone: undefined, named: false };

/** The page's zone for the daemon's `zone` (`State.scheduledTasksZone`): the window's own where the daemon names none
 * or one this client cannot use, named where it differs from the window's (`windowZone`). */
export function taskZone(zone: string | null | undefined, windowZone: string | undefined = Intl.DateTimeFormat().resolvedOptions().timeZone): TaskZone {
  if (!zone || !validZone(zone)) return WINDOW_ZONE;
  return { zone, named: zone !== windowZone };
}

function validZone(zone: string): boolean {
  try {
    new Intl.DateTimeFormat("en", { timeZone: zone });
    return true;
  } catch {
    return false;
  }
}

/** An instant in milliseconds as a date and time in `zone` ("Mon, Nov 24, 9:30 AM", with the zone's short name after it
 * where it is named: "Mon, Nov 24, 9:30 AM GMT+1"). */
export function formatInstant(language: Language, at: number, zone: TaskZone = WINDOW_ZONE): string {
  return dateFormat(language, {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
    ...(zone.zone && { timeZone: zone.zone }),
    ...(zone.named && { timeZoneName: "short" }),
  }).format(new Date(at));
}

/** The day of the month `at` falls on in `zone` (the window's own where `undefined`). */
export function dayOfMonthAt(at: number, zone: string | undefined): number {
  return wallClock(at, zone).day;
}

/** The wall-clock reading of `at` in `zone` (the window's own where `undefined`), to the minute. */
function wallClock(at: number, zone: string | undefined): { year: number; month: number; day: number; hour: number; minute: number } {
  if (!zone) {
    const date = new Date(at);
    return { year: date.getFullYear(), month: date.getMonth() + 1, day: date.getDate(), hour: date.getHours(), minute: date.getMinutes() };
  }
  const parts = Object.fromEntries(
    dateFormat("en", { timeZone: zone, hourCycle: "h23", year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "numeric" })
      .formatToParts(new Date(at))
      .map((part) => [part.type, Number(part.value)]),
  );
  return { year: parts.year!, month: parts.month!, day: parts.day!, hour: parts.hour! % 24, minute: parts.minute! };
}

/** How far `zone`'s clock is ahead of UTC at `at`, in milliseconds, to the minute. */
function zoneOffset(at: number, zone: string | undefined): number {
  const wall = wallClock(at, zone);
  return Date.UTC(wall.year, wall.month - 1, wall.day, wall.hour, wall.minute) - Math.floor(at / 60_000) * 60_000;
}

/** The instant a wall-clock time reads in `zone`, as the daemon reads a schedule's: one a daylight saving change
 * repeats is its first occurrence, and one it skips is as far past the change as it is past the hour before it (02:30
 * on a night that jumps from 02:00 to 03:00 is 03:30). */
function instantOfWallClock(year: number, month: number, day: number, hour: number, minute: number, zone: string | undefined): number {
  const asUtc = Date.UTC(year, month - 1, day, hour, minute);
  // The offsets around it, before and after any change that day.
  const before = zoneOffset(asUtc - 86_400_000, zone);
  const after = zoneOffset(asUtc + 86_400_000, zone);
  const reads = (at: number) => {
    const wall = wallClock(at, zone);
    return wall.year === year && wall.month === month && wall.day === day && wall.hour === hour && wall.minute === minute;
  };
  const candidates = [asUtc - before, asUtc - after].filter(reads).sort((left, right) => left - right);
  return candidates[0] ?? asUtc - before;
}

/** The days of a weekly schedule, in week order, as a list ("Mon, Wed, and Fri"). Days the daemon names that this
 * client does not know are left out. */
function daysList(language: Language, days: readonly string[]): string {
  const known = WEEKDAYS.filter((day) => days.includes(day));
  return new Intl.ListFormat(language, { style: "long", type: "conjunction" }).format(known.map((day) => weekdayName(language, day)));
}

/** A schedule in words, for the task list ("Every day at 9:00 AM"). An instant is written in `zone`, as a time of day,
 * which is a wall-clock time there, is read. `undefined` for a kind a newer daemon sends, which the caller leaves out
 * with whatever it would be joined to. */
export function scheduleWords(t: Translate, language: Language, schedule: Schedule, zone: TaskZone = WINDOW_ZONE): string | undefined {
  switch (schedule.kind) {
    case "once":
      return t("scheduledTasks.scheduleWords.once", { time: formatInstant(language, schedule.run_at, zone) });
    case "hourly":
      return t("scheduledTasks.scheduleWords.hourly", { minute: schedule.minute });
    case "daily":
      return t("scheduledTasks.scheduleWords.daily", { time: formatTimeOfDay(language, schedule.time) });
    case "weekdays":
      return t("scheduledTasks.scheduleWords.weekdays", { time: formatTimeOfDay(language, schedule.time) });
    case "weekly":
      return t("scheduledTasks.scheduleWords.weekly", { days: daysList(language, schedule.days), time: formatTimeOfDay(language, schedule.time) });
    case "monthly":
      return t("scheduledTasks.scheduleWords.monthly", { day: formatDayOfMonth(language, schedule.day), time: formatTimeOfDay(language, schedule.time) });
    case "manual":
      return t("scheduledTasks.scheduleWords.manual");
    default:
      return undefined;
  }
}

/** The days of a weekly schedule, short and in week order, as a run of names ("Mon, Wed, Fri"). */
function daysRun(language: Language, days: readonly string[]): string {
  const known = WEEKDAYS.filter((day) => days.includes(day));
  return new Intl.ListFormat(language, { style: "narrow", type: "conjunction" }).format(known.map((day) => weekdayName(language, day)));
}

/** An instant as a short date and time in `zone`, for a one-off schedule in a narrow row ("Nov 21, 4:07 AM"). */
function shortInstant(language: Language, at: number, zone: TaskZone): string {
  return dateFormat(language, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit", ...(zone.zone && { timeZone: zone.zone }) }).format(new Date(at));
}

/** A schedule in a few words for a narrow row ("Mon, Wed, Fri 9:30 AM", "Daily 8:00 AM", "Monthly, day 15 9:00 AM", "Hourly :15"). A one-off's time is
 * written in `zone` without its name; `scheduleWords` has the full sentence. `undefined` for a kind a newer daemon sends. */
export function scheduleShort(t: Translate, language: Language, schedule: Schedule, zone: TaskZone = WINDOW_ZONE): string | undefined {
  switch (schedule.kind) {
    case "once":
      return shortInstant(language, schedule.run_at, zone);
    case "hourly":
      // One left-to-right value, so that under a right-to-left language it does not read "15:".
      return t("scheduledTasks.scheduleShort.hourly", { minute: isolate(`:${String(schedule.minute).padStart(2, "0")}`) });
    case "daily":
      return t("scheduledTasks.scheduleShort.daily", { time: formatTimeOfDay(language, schedule.time) });
    case "weekdays":
      return t("scheduledTasks.scheduleShort.weekdays", { time: formatTimeOfDay(language, schedule.time) });
    case "weekly":
      return t("scheduledTasks.scheduleShort.weekly", { days: daysRun(language, schedule.days), time: formatTimeOfDay(language, schedule.time) });
    case "monthly":
      return t("scheduledTasks.scheduleShort.monthly", { day: formatDayOfMonth(language, schedule.day), time: formatTimeOfDay(language, schedule.time) });
    case "manual":
      return t("scheduledTasks.scheduleShort.manual");
    default:
      return undefined;
  }
}

export const MINUTE_MS = 60_000;
export const HOUR_MS = 60 * MINUTE_MS;
export const DAY_MS = 24 * HOUR_MS;

/** How long until `at` as `now` stands, in words that name no more than `units` units, the largest of days, hours and
 * minutes, rounded down ("in 3 days", "in 2 hr 15 min", "in 45 min"; with one unit, "in 2 hr"); under a minute, or past,
 * it is "in < 1 min". The granularity is the minute, so the words only change as one passes. */
export function countdownWords(t: Translate, language: Language, at: number, now: number, units: 1 | 2 = 2): string {
  const left = at - now;
  if (left < MINUTE_MS) return t("scheduledTasks.countdown.soon");
  const counts: [Intl.NumberFormatOptions["unit"], number][] =
    left >= DAY_MS ? [["day", Math.floor(left / DAY_MS)], ["hour", Math.floor((left % DAY_MS) / HOUR_MS)]]
      : left >= HOUR_MS ? [["hour", Math.floor(left / HOUR_MS)], ["minute", Math.floor((left % HOUR_MS) / MINUTE_MS)]]
        : [["minute", Math.floor(left / MINUTE_MS)]];
  const parts = counts.filter(([, count]) => count > 0).slice(0, units).map(([unit, count]) => new Intl.NumberFormat(language, { style: "unit", unit, unitDisplay: "short" }).format(count));
  return t("scheduledTasks.countdown.in", { duration: new Intl.ListFormat(language, { style: "narrow", type: "unit" }).format(parts) });
}

/** A time of day as a time input holds it (`HH:MM`). */
export function timeInputValue(time: TimeOfDay): string {
  return `${String(time.hour).padStart(2, "0")}:${String(time.minute).padStart(2, "0")}`;
}

/** The time of day a time input holds, or `undefined` for one empty or not a time. */
export function parseTimeInput(value: string): TimeOfDay | undefined {
  const match = /^(\d{2}):(\d{2})(?::\d{2}(?:\.\d+)?)?$/.exec(value);
  if (!match) return undefined;
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  return hour <= 23 && minute <= 59 ? { hour, minute } : undefined;
}

/** An instant as a local date-and-time input holds it (`YYYY-MM-DDTHH:MM`), read in `zone` (the window's own where
 * `undefined`). */
export function instantInputValue(at: number, zone?: string): string {
  const wall = wallClock(at, zone);
  const two = (value: number) => String(value).padStart(2, "0");
  return `${wall.year}-${two(wall.month)}-${two(wall.day)}T${two(wall.hour)}:${two(wall.minute)}`;
}

/** The instant a local date-and-time input holds, read in `zone` (the window's own where `undefined`), or `undefined`
 * for one empty or not a date and time. */
export function parseInstantInput(value: string, zone?: string): number | undefined {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::\d{2}(?:\.\d+)?)?$/.exec(value);
  if (!match) return undefined;
  const [year, month, day, hour, minute] = match.slice(1).map(Number) as [number, number, number, number, number];
  // A day past the month's end (Feb 30) rolls over; that is not a date the input holds.
  const calendar = new Date(Date.UTC(year, month - 1, day));
  if (calendar.getUTCMonth() !== month - 1 || calendar.getUTCDate() !== day || hour > 23 || minute > 59) return undefined;
  return instantOfWallClock(year, month, day, hour, minute, zone);
}
