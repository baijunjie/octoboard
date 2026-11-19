// @vitest-environment jsdom
import { act, useState } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { setLanguage } from "../../i18n/language";
import { LanguageProvider } from "../../i18n/react";
import { partSelector } from "../../primitives";
import { fakeTimers, runOutTimers } from "../../test/clock";
import { clearSegment, segmentOf, typeSegment } from "../../test/dom";
import { mountHost } from "../../test/mountHost";
import { DateTimeInput } from "./DateTimeInput";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => setLanguage("en"));

function Held({ initial, onValue, onBlur, error, description }: { initial: string; onValue?: (value: string) => void; onBlur?: () => void; error?: string; description?: string }) {
  const [value, setValue] = useState(initial);
  return (
    <LanguageProvider>
      <DateTimeInput label="Run at" value={value} onChange={(next) => { setValue(next); onValue?.(next); }} onBlur={onBlur} errorMessage={error} description={description} />
    </LanguageProvider>
  );
}

const shown = (host: ParentNode) => ["month", "day", "year", "hour", "minute", "dayPeriod"].map((part) => segmentOf(host, part).textContent).join(" ");
const calendarButton = () => document.body.querySelector<HTMLButtonElement>("button[aria-haspopup=dialog]")!;
const popover = () => document.body.querySelector<HTMLElement>(partSelector.calendarPopover);
const day = (name: string) => document.body.querySelector<HTMLElement>(`[role=gridcell] [role=button][aria-label="${name}"]`)!;

it("shows the date and the time to the minute as segments, with no native picker", () => {
  const host = mountHost(<Held initial="2026-11-20T17:05" />);
  expect(shown(host)).toBe("11 20 2026 5 05 PM");
  // The form's autofill input is out of sight, out of the tab order and not where the date is entered.
  expect(host.querySelector("input[tabindex='-1']")!.closest("[style*='clip']")).not.toBeNull();
  expect(host.querySelector("[data-slot=label]")!.textContent).toBe("Run at");
});

it("hands over the date and time as YYYY-MM-DDTHH:MM once every segment is filled, and nothing before", () => {
  const onValue = vi.fn();
  const host = mountHost(<Held initial="" onValue={onValue} />);
  typeSegment(segmentOf(host, "month"), "03");
  typeSegment(segmentOf(host, "day"), "09");
  typeSegment(segmentOf(host, "year"), "2027");
  typeSegment(segmentOf(host, "hour"), "08");
  expect(onValue).not.toHaveBeenCalled();
  typeSegment(segmentOf(host, "minute"), "15");
  typeSegment(segmentOf(host, "dayPeriod"), "P");
  expect(onValue).toHaveBeenLastCalledWith("2027-03-09T20:15");
});

it("reports a field left with a segment blank as empty before it reports the field was left", () => {
  const calls: string[] = [];
  const host = mountHost(<Held initial="2026-11-20T09:00" onValue={(value) => calls.push(`change:${value}`)} onBlur={() => calls.push("blur")} />);
  clearSegment(segmentOf(host, "minute"));
  act(() => (document.activeElement as HTMLElement).blur());
  expect(calls).toEqual(["change:", "blur"]);
});

it("opens a calendar popover from its button, and a day picked there keeps the time and closes it", () => {
  fakeTimers();
  const onValue = vi.fn();
  const host = mountHost(<Held initial="2026-11-20T17:05" onValue={onValue} />);
  expect(popover()).toBeNull();
  act(() => calendarButton().click());
  runOutTimers();
  expect(popover()).not.toBeNull();
  expect(popover()!.querySelector("[role=grid]")).not.toBeNull();
  act(() => day("Wednesday, November 25, 2026").click());
  runOutTimers();
  expect(onValue).toHaveBeenLastCalledWith("2026-11-25T17:05");
  expect(shown(host)).toBe("11 25 2026 5 05 PM");
  expect(popover()).toBeNull();
});

it("shows the error in place of the description and marks the field invalid", () => {
  const host = mountHost(<Held initial="" description="Times are in UTC." error="Enter a date and time." />);
  expect(host.textContent).toContain("Enter a date and time.");
  expect(host.textContent).not.toContain("Times are in UTC.");
  expect(segmentOf(host, "month").getAttribute("aria-invalid")).toBe("true");
});
