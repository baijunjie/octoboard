import { CalendarDate, CalendarDateTime, Time } from "@internationalized/date";
import { expect, it } from "vitest";

import { dateOf, dateText, dateTimeOf, dateTimeText, timeOf, timeText } from "./dateText";

it("reads a time as HH:MM and writes it back padded, an empty or unreadable one as none", () => {
  expect(timeOf("07:45")).toEqual(new Time(7, 45));
  expect(timeText(new Time(7, 5))).toBe("07:05");
  expect(timeText(timeOf("23:59"))).toBe("23:59");
  expect(timeOf(null)).toBeNull();
  expect(timeOf("")).toBeNull();
  expect(timeOf("soon")).toBeNull();
  expect(timeOf("07:45:30")).toBeNull();
  expect(timeOf("7:45")).toBeNull();
  expect(timeText(null)).toBeNull();
});

it("reads a date and time as YYYY-MM-DDTHH:MM and writes it back padded, an empty or unreadable one as none", () => {
  expect(dateTimeOf("2027-01-09T21:30")).toEqual(new CalendarDateTime(2027, 1, 9, 21, 30));
  expect(dateTimeText({ year: 2027, month: 1, day: 9, hour: 7, minute: 5 })).toBe("2027-01-09T07:05");
  expect(dateTimeText(dateTimeOf("2026-11-20T00:00"))).toBe("2026-11-20T00:00");
  expect(dateTimeOf(null)).toBeNull();
  expect(dateTimeOf("2027-02-30T10:00")).toBeNull();
  expect(dateTimeOf("2027-01-09")).toBeNull();
  expect(dateTimeOf("2027-01-09T21")).toBeNull();
  expect(dateTimeOf("2027-01-09T21:30:00")).toBeNull();
  expect(dateTimeText(null)).toBeNull();
});

it("reads a date as YYYY-MM-DD and writes it back padded, an empty or unreadable one as none", () => {
  expect(dateOf("2027-01-09")).toEqual(new CalendarDate(2027, 1, 9));
  expect(dateText({ year: 2027, month: 1, day: 9 })).toBe("2027-01-09");
  expect(dateText({ year: 987, month: 12, day: 31 })).toBe("0987-12-31");
  expect(dateOf(null)).toBeNull();
  expect(dateOf("2027-01-09T21:30")).toBeNull();
  expect(dateOf("2027-13-01")).toBeNull();
  expect(dateText(null)).toBeNull();
});
