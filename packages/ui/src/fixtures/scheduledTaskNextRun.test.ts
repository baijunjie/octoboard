// @vitest-environment jsdom
import { expect, it } from "vitest";

import type { Schedule } from "../protocol";
import { nextRun } from "./scheduledTaskNextRun";

const monthly = (day: number): Schedule => ({ kind: "monthly", day, time: { hour: 9, minute: 0 } });

// The fixture daemon reads a monthly schedule as the real one does: a month without the day runs on its last.
it.each<[string, number, string, string]>([
  ["31 after Jan 31's time", 31, "2027-01-31T09:00:00Z", "2027-02-28T09:00:00Z"],
  ["31 before Jan 31's time", 31, "2027-01-31T08:00:00Z", "2027-01-31T09:00:00Z"],
  ["31 after Feb 28", 31, "2027-02-28T09:00:00Z", "2027-03-31T09:00:00Z"],
  ["30 in February", 30, "2027-01-30T09:00:00Z", "2027-02-28T09:00:00Z"],
  ["29 in a leap February", 29, "2028-01-29T09:00:00Z", "2028-02-29T09:00:00Z"],
  ["15 across the new year", 15, "2026-12-20T12:00:00Z", "2027-01-15T09:00:00Z"],
])("runs a monthly schedule with day %s", (_, day, after, expected) => {
  expect(nextRun(monthly(day), Date.parse(after), "UTC")).toBe(Date.parse(expected));
});

it("reads a monthly schedule's day, and the fallback to a short month's last day, in the daemon's zone", () => {
  // 2027-01-31 20:00 UTC is already 1 February 05:00 in Tokyo, so the month the zone reads decides.
  const now = Date.parse("2027-01-31T20:00:00Z");
  expect(nextRun(monthly(31), now, "Asia/Tokyo")).toBe(Date.parse("2027-02-28T00:00:00Z"));
  expect(nextRun(monthly(1), now, "Asia/Tokyo")).toBe(Date.parse("2027-02-01T00:00:00Z"));
  expect(nextRun(monthly(31), now, "UTC")).toBe(Date.parse("2027-02-28T09:00:00Z"));
  expect(nextRun(monthly(1), now, "UTC")).toBe(Date.parse("2027-02-01T09:00:00Z"));
});

// The other kinds are read as the daemon reads them; 18 November 2026 is a Wednesday.
it.each<[string, Schedule, string, string | null]>([
  ["hourly before its minute", { kind: "hourly", minute: 45 }, "2026-11-18T10:20:00Z", "2026-11-18T10:45:00Z"],
  ["hourly after its minute", { kind: "hourly", minute: 5 }, "2026-11-18T10:20:00Z", "2026-11-18T11:05:00Z"],
  ["daily after its time", { kind: "daily", time: { hour: 9, minute: 0 } }, "2026-11-18T10:20:00Z", "2026-11-19T09:00:00Z"],
  ["daily before its time", { kind: "daily", time: { hour: 18, minute: 0 } }, "2026-11-18T10:20:00Z", "2026-11-18T18:00:00Z"],
  ["weekdays on a Friday after its time", { kind: "weekdays", time: { hour: 9, minute: 0 } }, "2026-11-20T10:00:00Z", "2026-11-23T09:00:00Z"],
  ["weekly on its day", { kind: "weekly", time: { hour: 7, minute: 0 }, days: ["sunday"] }, "2026-11-18T10:20:00Z", "2026-11-22T07:00:00Z"],
  ["a one-off ahead", { kind: "once", run_at: Date.parse("2026-11-25T12:00:00Z") }, "2026-11-18T10:20:00Z", "2026-11-25T12:00:00Z"],
  ["a one-off already past", { kind: "once", run_at: Date.parse("2026-11-18T10:00:00Z") }, "2026-11-18T10:20:00Z", null],
  ["manual only", { kind: "manual" }, "2026-11-18T10:20:00Z", null],
])("runs %s", (_, schedule, now, expected) => {
  expect(nextRun(schedule, Date.parse(now), "UTC")).toBe(expected === null ? null : Date.parse(expected));
});
