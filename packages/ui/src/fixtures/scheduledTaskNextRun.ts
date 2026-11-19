// When the daemon next runs a scheduled task, as the fixtures compute it. It imports nothing of the app's store, so the
// gallery's scenario files can use it (through `scheduledTaskBuilders`) without loading, ahead of the page's own
// preparation, the module that reads the stored language.
import type { Schedule, Weekday } from "../protocol";

const DAY_MS = 86_400_000;

/** The days of the week as `Date.getUTCDay` numbers them (Sunday is 0). */
const WEEKDAY_NUMBERS: Record<Weekday, number> = { sunday: 0, monday: 1, tuesday: 2, wednesday: 3, thursday: 4, friday: 5, saturday: 6 };

/** The days of the week in the order the daemon stores a weekly schedule's days in. */
export const WEEK_ORDER: Weekday[] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

/** How far `zone`'s clock is ahead of UTC at `at`, in milliseconds, to the minute (the machine's own zone where `zone`
 * is `undefined`). */
function zoneOffset(at: number, zone: string | undefined): number {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat("en", { ...(zone && { timeZone: zone }), hourCycle: "h23", year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "numeric" })
      .formatToParts(new Date(at))
      .map((part) => [part.type, Number(part.value)]),
  );
  const wall = Date.UTC(parts.year!, parts.month! - 1, parts.day!, parts.hour! % 24, parts.minute!);
  return wall - Math.floor(at / 60_000) * 60_000;
}

/** The instant the wall clock of `zone` reads the given date and time, taken at the zone's offset on that day. The
 * fixture does not model the hours a daylight saving change skips or repeats. */
function instantOf(date: number, hour: number, minute: number, zone: string | undefined): number {
  const asUtc = date + (hour * 60 + minute) * 60_000;
  return asUtc - zoneOffset(asUtc - zoneOffset(asUtc, zone), zone);
}

/** When a task on `schedule` next runs after `now`, in `zone` (`next_run` of `coordinator/scheduled_tasks/next_run.rs`,
 * without the daylight saving rules): a one-off's own time while it is ahead, the next occurrence of a recurring
 * schedule, and none for a manual-only task. */
export function nextRun(schedule: Schedule, now: number, zone: string | undefined): number | null {
  switch (schedule.kind) {
    case "manual":
      return null;
    case "once":
      return schedule.run_at > now ? schedule.run_at : null;
    case "hourly": {
      const offset = zoneOffset(now, zone);
      const hour = Math.floor((now + offset) / 3_600_000) * 3_600_000 - offset + schedule.minute * 60_000;
      return hour > now ? hour : hour + 3_600_000;
    }
    case "monthly": {
      const { day, time } = schedule;
      // The calendar day `now` falls on in `zone`, as a UTC midnight; a month without `day` runs on its last day.
      const today = new Date(Math.floor((now + zoneOffset(now, zone)) / DAY_MS) * DAY_MS);
      for (let ahead = 0; ahead <= 2; ahead++) {
        const lastDay = new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth() + ahead + 1, 0)).getUTCDate();
        const at = instantOf(Date.UTC(today.getUTCFullYear(), today.getUTCMonth() + ahead, Math.min(day, lastDay)), time.hour, time.minute, zone);
        if (at > now) return at;
      }
      return null;
    }
    default: {
      const { time } = schedule;
      // The calendar day `now` falls on in `zone`, as a UTC midnight.
      const today = Math.floor((now + zoneOffset(now, zone)) / DAY_MS) * DAY_MS;
      for (let ahead = 0; ahead <= 8; ahead++) {
        const date = today + ahead * DAY_MS;
        const day = new Date(date).getUTCDay();
        const runs = schedule.kind === "daily" || (schedule.kind === "weekdays" ? day >= 1 && day <= 5 : schedule.kind === "weekly" && schedule.days.some((one) => WEEKDAY_NUMBERS[one as Weekday] === day));
        const at = instantOf(date, time.hour, time.minute, zone);
        if (runs && at > now) return at;
      }
      return null;
    }
  }
}
