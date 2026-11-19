// @vitest-environment jsdom
import { act, useState } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { setLanguage } from "../../i18n/language";
import { LanguageProvider } from "../../i18n/react";
import { partSelector } from "../../primitives";
import { clearSegment, segmentOf, typeSegment } from "../../test/dom";
import { mountHost } from "../../test/mountHost";
import { TimeOfDayInput } from "./TimeOfDayInput";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => setLanguage("en"));

function Held({ initial, onValue, onBlur, error, description }: { initial: string; onValue?: (value: string) => void; onBlur?: () => void; error?: string; description?: string }) {
  const [value, setValue] = useState(initial);
  return <TimeOfDayInput label="Time" value={value} onChange={(next) => { setValue(next); onValue?.(next); }} onBlur={onBlur} errorMessage={error} description={description} />;
}

const shown = (host: ParentNode) => ["hour", "minute", "dayPeriod"].map((part) => segmentOf(host, part)?.textContent).filter(Boolean).join(" ");

it("shows the time as segments with a day period under a 12-hour language, and without one under a 24-hour language", () => {
  const english = mountHost(<LanguageProvider><Held initial="17:05" /></LanguageProvider>);
  expect(shown(english)).toBe("5 05 PM");
  setLanguage("zh-CN");
  const chinese = mountHost(<LanguageProvider><Held initial="17:05" /></LanguageProvider>);
  expect(shown(chinese)).toBe("17 05");
  expect(segmentOf(chinese, "dayPeriod")).toBeNull();
});

it("names the segments' group by its label and opens no native picker", () => {
  const host = mountHost(<Held initial="09:30" />);
  expect(host.querySelector("input:not([type=hidden])")).toBeNull();
  expect(host.querySelector("[role=group]")!.getAttribute("aria-labelledby")).toContain(host.querySelector("[data-slot=label]")!.id);
  expect(host.querySelector("[data-slot=label]")!.textContent).toBe("Time");
});

it("hands over the time as HH:MM while every segment is filled, and nothing while a blank field is half filled", () => {
  const onValue = vi.fn();
  const host = mountHost(<Held initial="" onValue={onValue} />);
  typeSegment(segmentOf(host, "hour"), "07");
  expect(onValue).not.toHaveBeenCalled();
  typeSegment(segmentOf(host, "minute"), "30");
  typeSegment(segmentOf(host, "dayPeriod"), "P");
  expect(onValue).toHaveBeenLastCalledWith("19:30");
});

it("reports a field left with a segment blank as empty, once, before it reports the field was left", () => {
  const calls: string[] = [];
  const host = mountHost(<Held initial="09:00" onValue={(value) => calls.push(`change:${value}`)} onBlur={() => calls.push("blur")} />);
  clearSegment(segmentOf(host, "minute"));
  expect(calls).toEqual([]);
  act(() => (document.activeElement as HTMLElement).blur());
  expect(calls).toEqual(["change:", "blur"]);
  // The emptied field shows its placeholders again.
  expect(segmentOf(host, "hour").matches(partSelector.blankSegment)).toBe(true);
});

it("shows the error in place of the description and marks the segments invalid", () => {
  const host = mountHost(<Held initial="" description="Times are in UTC." error="Enter a time." />);
  expect(host.textContent).toContain("Enter a time.");
  expect(host.textContent).not.toContain("Times are in UTC.");
  expect(host.querySelector("[role=alert]")!.textContent).toBe("Enter a time.");
  expect(segmentOf(host, "hour").getAttribute("aria-invalid")).toBe("true");
});

it("shows the description while there is no error, and names it on the group", () => {
  const host = mountHost(<Held initial="09:30" description="Times are in UTC." />);
  const description = host.querySelector("[data-slot=description]")!;
  expect(description.textContent).toBe("Times are in UTC.");
  expect(segmentOf(host, "hour").getAttribute("aria-describedby")).toContain(description.id);
});
