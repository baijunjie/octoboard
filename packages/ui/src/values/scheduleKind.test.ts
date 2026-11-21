import { expect, it } from "vitest";

import { SCHEDULE_KIND_LABELS } from "../scheduledTasks/scheduleText";
import { isKnownScheduleKind, SCHEDULE_KINDS } from "./scheduleKind";

it("lists the kinds of schedule in the order the form offers them", () => {
  expect([...SCHEDULE_KINDS]).toEqual(["once", "hourly", "daily", "weekdays", "weekly", "monthly", "manual"]);
});

it.each([...SCHEDULE_KINDS])("knows %s, a listed kind", (kind) => {
  expect(isKnownScheduleKind(kind)).toBe(true);
});

it.each(["fortnightly", "", "Daily", "toString", "__proto__"])("does not know %j", (kind) => {
  expect(isKnownScheduleKind(kind)).toBe(false);
});

it("is the list the Scheduled tasks page words its kinds from", () => {
  expect(Object.keys(SCHEDULE_KIND_LABELS).sort()).toEqual([...SCHEDULE_KINDS].sort());
});
