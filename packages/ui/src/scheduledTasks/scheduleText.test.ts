// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { translatorFor } from "../i18n/catalog";
import { t } from "../i18n/language";
import type { Schedule } from "../protocol";
import { countdownWords, dayOfMonthAt, DAY_MS, formatDayOfMonth, formatTimeOfDay, HOUR_MS, MINUTE_MS, formatInstant, scheduleShort, instantInputValue, parseInstantInput, parseTimeInput, scheduleWords, taskZone, timeInputValue, WINDOW_ZONE } from "./scheduleText";

const MINUTE = MINUTE_MS;
const HOUR = HOUR_MS;
const DAY = DAY_MS;

it.each<[Schedule, string]>([
  [{ kind: "hourly", minute: 15 }, "Every hour at minute 15"],
  [{ kind: "daily", time: { hour: 9, minute: 5 } }, "Every day at 9:05 AM"],
  [{ kind: "weekdays", time: { hour: 18, minute: 0 } }, "Weekdays at 6:00 PM"],
  [{ kind: "weekly", time: { hour: 7, minute: 30 }, days: ["sunday", "monday", "wednesday"] }, "Mon, Wed, and Sun at 7:30 AM"],
  [{ kind: "monthly", day: 15, time: { hour: 9, minute: 0 } }, "On day 15 of every month at 9:00 AM"],
  [{ kind: "monthly", day: 31, time: { hour: 18, minute: 30 } }, "On day 31 of every month at 6:30 PM"],
  [{ kind: "manual" }, "Manual only"],
  [{ kind: "once", run_at: new Date(2026, 11, 24, 20, 0).getTime() }, "Once, Thu, Dec 24, 2026, 8:00 PM"],
])("words %j as %s", (schedule, words) => {
  expect(scheduleWords(t, "en", schedule)).toBe(words);
});

it("reads back the time and the local date and time an input holds, and nothing that is not one", () => {
  expect(parseTimeInput(timeInputValue({ hour: 7, minute: 3 }))).toEqual({ hour: 7, minute: 3 });
  expect(parseTimeInput("24:00")).toBeUndefined();
  expect(parseTimeInput("")).toBeUndefined();
  const at = new Date(2027, 1, 28, 23, 59).getTime();
  expect(parseInstantInput(instantInputValue(at))).toBe(at);
  expect(parseInstantInput("2027-02-30T10:00")).toBeUndefined();
  expect(parseInstantInput("")).toBeUndefined();
});

it("takes the daemon's zone, named where the window's differs, and the window's where it names none or one unknown", () => {
  expect(taskZone("Europe/Berlin", "Asia/Tokyo")).toEqual({ zone: "Europe/Berlin", named: true });
  expect(taskZone("Asia/Tokyo", "Asia/Tokyo")).toEqual({ zone: "Asia/Tokyo", named: false });
  expect(taskZone(null, "Asia/Tokyo")).toBe(WINDOW_ZONE);
  expect(taskZone("Not/AZone", "Asia/Tokyo")).toBe(WINDOW_ZONE);
});

it("writes an instant in the daemon's zone, with the zone's name where it is named", () => {
  const at = Date.UTC(2026, 10, 20, 8, 0);
  expect(formatInstant("en", at, { zone: "Europe/Berlin", named: true })).toBe("Fri, Nov 20, 2026, 9:00 AM GMT+1");
  expect(formatInstant("en", at, { zone: "Asia/Tokyo", named: false })).toBe("Fri, Nov 20, 2026, 5:00 PM");
  expect(scheduleWords(t, "en", { kind: "once", run_at: at }, { zone: "Europe/Berlin", named: false })).toBe("Once, Fri, Nov 20, 2026, 9:00 AM");
});

it("reads a date and time input in the daemon's zone, across daylight saving changes as the daemon does", () => {
  const berlin = Date.UTC(2026, 10, 20, 8, 0);
  expect(instantInputValue(berlin, "Europe/Berlin")).toBe("2026-11-20T09:00");
  expect(parseInstantInput("2026-11-20T09:00", "Europe/Berlin")).toBe(berlin);
  // A time the spring change skips is as far past the change: 02:30 is 03:30 EDT.
  expect(parseInstantInput("2026-03-08T02:30", "America/New_York")).toBe(Date.UTC(2026, 2, 8, 7, 30));
  // A time the autumn change repeats is its first occurrence, still EDT.
  expect(parseInstantInput("2026-11-01T01:30", "America/New_York")).toBe(Date.UTC(2026, 10, 1, 5, 30));
  expect(parseInstantInput("2026-02-30T10:00", "Europe/Berlin")).toBeUndefined();
});

it.each<[Schedule, string]>([
  [{ kind: "hourly", minute: 5 }, "Hourly :05"],
  [{ kind: "daily", time: { hour: 8, minute: 0 } }, "Daily 8:00 AM"],
  [{ kind: "weekdays", time: { hour: 17, minute: 30 } }, "Weekdays 5:30 PM"],
  [{ kind: "weekly", time: { hour: 9, minute: 30 }, days: ["friday", "monday", "wednesday"] }, "Mon, Wed, Fri 9:30 AM"],
  [{ kind: "monthly", day: 15, time: { hour: 9, minute: 0 } }, "Monthly, day 15 9:00 AM"],
  [{ kind: "manual" }, "Manual"],
  [{ kind: "once", run_at: new Date(2026, 10, 21, 4, 7).getTime() }, "Nov 21, 4:07 AM"],
])("writes %j in a few words as %s", (schedule, words) => {
  // The hourly minute is one isolated left-to-right value.
  expect(scheduleShort(t, "en", schedule)?.replace(/[\u2068\u2069]/g, "")).toBe(words);
});

it("isolates an hourly schedule's minute as one value in every language, so that its colon keeps its place in a right-to-left line", () => {
  for (const language of ["en", "ar"] as const) {
    const words = translatorFor(language);
    expect(scheduleShort(words, language, { kind: "hourly", minute: 15 }), language).toContain("\u2068:15\u2069");
  }
});

it("writes a schedule in a few words in the language's own days and times", () => {
  expect(scheduleShort(t, "zh-CN", { kind: "weekly", time: { hour: 9, minute: 30 }, days: ["monday", "wednesday"] })).toMatch(/周一.*周三.*9:30/);
});

it("writes a monthly schedule's day and time through the catalog in the language's own forms", () => {
  const zh = translatorFor("zh-CN");
  const nine = { hour: 9, minute: 0 };
  expect(scheduleShort(zh, "zh-CN", { kind: "monthly", day: 15, time: nine })).toBe(zh("scheduledTasks.scheduleShort.monthly", { day: "15日", time: formatTimeOfDay("zh-CN", nine) }));
  expect(scheduleWords(zh, "zh-CN", { kind: "monthly", day: 15, time: nine })).toBe(zh("scheduledTasks.scheduleWords.monthly", { day: "15日", time: formatTimeOfDay("zh-CN", nine) }));
  expect(scheduleShort(zh, "zh-CN", { kind: "monthly", day: 15, time: nine })).toContain("15日");
});

it("lists a weekly schedule's days in week order as a narrow list in the language's own conjunction", () => {
  const days = ["friday", "monday", "wednesday"] as const;
  expect(scheduleShort(t, "en", { kind: "weekly", time: { hour: 9, minute: 0 }, days: [...days] })).toMatch(/^Mon, Wed, Fri /);
  const zh = translatorFor("zh-CN");
  expect(scheduleShort(zh, "zh-CN", { kind: "weekly", time: { hour: 9, minute: 0 }, days: [...days] })).toMatch(/^周一、周三、周五 /);
});

it("writes a day of the month as the language writes one, and finds the day a moment falls on in a zone", () => {
  expect(formatDayOfMonth("en", 29)).toBe("29");
  expect(formatDayOfMonth("zh-CN", 15)).toBe("15日");
  expect(formatDayOfMonth("ja", 31)).toBe("31日");
  expect(formatDayOfMonth("ko", 1)).toBe("1일");
  expect(dayOfMonthAt(Date.UTC(2026, 10, 19, 23, 0), "Asia/Tokyo")).toBe(20);
  expect(dayOfMonthAt(Date.UTC(2026, 10, 19, 23, 0), "UTC")).toBe(19);
});

describe("countdown", () => {
  const now = Date.UTC(2026, 10, 19, 10, 0);
  const left = (ms: number) => countdownWords(t, "en", now + ms, now);
  it("names only the largest unit where one is asked for", () => {
    const short = (ms: number) => countdownWords(t, "en", now + ms, now, 1);
    expect(short(3 * DAY + 4 * HOUR)).toBe("in 3 days");
    expect(short(2 * HOUR + 15 * MINUTE)).toBe("in 2 hr");
    expect(short(45 * MINUTE)).toBe("in 45 min");
    expect(short(30_000)).toBe("in < 1 min");
  });
  it("names the larger two of days, hours and minutes, rounded down", () => {
    expect(left(3 * DAY + 4 * HOUR + 59 * MINUTE)).toBe("in 3 days 4 hr");
    expect(left(3 * DAY)).toBe("in 3 days");
    expect(left(2 * HOUR + 15 * MINUTE + 59_000)).toBe("in 2 hr 15 min");
    expect(left(HOUR)).toBe("in 1 hr");
    expect(left(45 * MINUTE)).toBe("in 45 min");
    expect(left(MINUTE)).toBe("in 1 min");
  });
  it("says under a minute in the language's own words", () => {
    const zh = translatorFor("zh-CN");
    expect(countdownWords(zh, "zh-CN", now + 30_000, now)).toBe(zh("scheduledTasks.countdown.soon"));
    expect(countdownWords(zh, "zh-CN", now + 30_000, now, 1)).toBe(zh("scheduledTasks.countdown.soon"));
  });
  it("says under a minute for anything less, including a time already past", () => {
    expect(left(MINUTE - 1)).toBe("in < 1 min");
    expect(left(0)).toBe("in < 1 min");
    expect(left(-5 * MINUTE)).toBe("in < 1 min");
  });
});
