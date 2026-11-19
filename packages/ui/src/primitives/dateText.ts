import { CalendarDate, CalendarDateTime, parseDate, parseDateTime, parseTime, Time } from "@internationalized/date";

const two = (value: number) => String(value).padStart(2, "0");

// The library's parsers also take a date for a date and time, an hour for a time and seconds; the contract is exactly
// one shape per kind of value.
const TIME = /^\d{2}:\d{2}$/;
const DATE = /^\d{4}-\d{2}-\d{2}$/;
const DATE_TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/;

/** The time of day the library holds for `value` (`HH:MM`), or `null` for none or text that is not a time. */
export function timeOf(value: string | null): Time | null {
  if (value === null || !TIME.test(value)) return null;
  try {
    return parseTime(value);
  } catch {
    return null;
  }
}

/** A time of day as `HH:MM`. */
export function timeText(time: Time | null): string | null {
  return time === null ? null : `${two(time.hour)}:${two(time.minute)}`;
}

/** The local date and time the library holds for `value` (`YYYY-MM-DDTHH:MM`), or `null` for none or text that is not
 * one. */
export function dateTimeOf(value: string | null): CalendarDateTime | null {
  if (value === null || !DATE_TIME.test(value)) return null;
  try {
    return parseDateTime(value);
  } catch {
    return null;
  }
}

/** A local date and time as `YYYY-MM-DDTHH:MM`. */
export function dateTimeText(dateTime: { year: number; month: number; day: number; hour: number; minute: number } | null): string | null {
  return dateTime === null ? null : `${String(dateTime.year).padStart(4, "0")}-${two(dateTime.month)}-${two(dateTime.day)}T${two(dateTime.hour)}:${two(dateTime.minute)}`;
}

/** The calendar date the library holds for `value` (`YYYY-MM-DD`), or `null` for none or text that is not one. */
export function dateOf(value: string | null): CalendarDate | null {
  if (value === null || !DATE.test(value)) return null;
  try {
    return parseDate(value);
  } catch {
    return null;
  }
}

/** A calendar date as `YYYY-MM-DD`. */
export function dateText(date: { year: number; month: number; day: number } | null): string | null {
  return date === null ? null : `${String(date.year).padStart(4, "0")}-${two(date.month)}-${two(date.day)}`;
}
