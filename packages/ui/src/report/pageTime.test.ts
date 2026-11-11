import { describe, expect, it } from "vitest";

import { formatPageTime } from "./pageTime";

describe("formatPageTime", () => {
  const now = new Date(2026, 10, 11, 22, 30, 0);

  it("shows only the time, to the minute, for a page pushed today", () => {
    const text = formatPageTime(new Date(2026, 10, 11, 9, 5, 42).getTime(), "en-US", now);
    expect(text).toBe("9:05 AM");
  });

  it("shows the date and the time, to the minute, for a page pushed on another day", () => {
    const text = formatPageTime(new Date(2026, 10, 10, 21, 7, 13).getTime(), "en-US", now);
    expect(text).toBe("11/10/2026, 9:07 PM");
  });

  it("tells the same time of day on another date apart from today", () => {
    expect(formatPageTime(new Date(2025, 10, 11, 9, 5).getTime(), "en-US", now)).toContain("2025");
  });
  it("tells today from yesterday at midnight, whatever the time of day either is", () => {
    const justAfterMidnight = new Date(2026, 10, 11, 0, 0, 0);
    expect(formatPageTime(justAfterMidnight.getTime(), "en-US", new Date(2026, 10, 11, 0, 0, 30))).toBe("12:00 AM");
    const justBefore = new Date(2026, 10, 10, 23, 59, 59);
    expect(formatPageTime(justBefore.getTime(), "en-US", new Date(2026, 10, 11, 0, 0, 30))).toBe("11/10/2026, 11:59 PM");
  });

  it("does not show seconds, and reads a page pushed later today as the time alone", () => {
    const text = formatPageTime(new Date(2026, 10, 11, 23, 59, 59).getTime(), "en-US", new Date(2026, 10, 11, 8, 0));
    expect(text).toBe("11:59 PM");
  });

  it("writes the time and date in the language it is given", () => {
    const today = formatPageTime(new Date(2026, 10, 11, 21, 7).getTime(), "de", now);
    expect(today).toBe("21:07");
    const other = formatPageTime(new Date(2026, 10, 10, 21, 7).getTime(), "de", now);
    expect(other).toBe("10.11.2026, 21:07");
  });

  it("reads a page time given as a date string as the same instant as its milliseconds", () => {
    const at = new Date(2026, 10, 10, 21, 7, 13);
    expect(formatPageTime(at.toISOString(), "en-US", now)).toBe(formatPageTime(at.getTime(), "en-US", now));
  });
});
