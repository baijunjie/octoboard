// @vitest-environment jsdom

import React from "react";
import { beforeEach, expect, it } from "vitest";

import { LANGUAGES, type Language } from "../i18n/languages";
import { DatePicker, DatePickerInput, Label, LocaleProvider, TimeField, TimeInput } from "../primitives";
import { mount } from "../primitives/testSupport";
import { fakeTimers } from "../test/clock";
import { settleInAct } from "./clockTestSupport";
import { createUi } from "./interact";

beforeEach(() => fakeTimers());

/** What the fields below report, kept by the test. */
type Changes = Record<string, string | null>;

/** A field that holds what it is changed to, as the application's own do, and reports it to the test. */
function Held({ name, record, children }: { name: string; record: (name: string, value: string | null) => void; children: (value: string | null, onChange: (next: string | null) => void) => React.ReactElement }) {
  const [value, setValue] = React.useState<string | null>(null);
  return children(value, (next) => {
    record(name, next);
    setValue(next);
  });
}

/** A date and time field, a date field and two time fields (one named by its `aria-label`), each holding its value. */
function mountFields(language: Language) {
  const changes: Changes = {};
  const record = (name: string, value: string | null) => {
    changes[name] = value;
  };
  mount(
    <LocaleProvider locale={language}>
      <Held name="launch" record={record}>{(value, onChange) => <DatePicker value={value} onChange={onChange} granularity="minute"><Label>Launch</Label><DatePickerInput /></DatePicker>}</Held>
      <Held name="freeze" record={record}>{(value, onChange) => <DatePicker value={value} onChange={onChange} granularity="day"><Label>Code freeze</Label><DatePickerInput /></DatePicker>}</Held>
      <Held name="window" record={record}>{(value, onChange) => <TimeField value={value} onChange={onChange}><Label>Daily window</Label><TimeInput /></TimeField>}</Held>
      <Held name="start" record={record}>{(value, onChange) => <TimeField value={value} onChange={onChange} label="Start"><TimeInput /></TimeField>}</Held>
    </LocaleProvider>,
  );
  return { changes, ui: createUi(document, language) };
}

it.each(LANGUAGES)("fillDateTime fills a date and time, a date and a time by each segment's kind in %s", async (language) => {
  const { changes, ui } = mountFields(language);
  // Afternoon, noon-adjacent and midnight-adjacent hours exercise a 12-hour language's day period either way.
  await settleInAct(ui.fillDateTime("Launch", "2026-11-27T21:05"));
  await settleInAct(ui.fillDateTime("Code freeze", "2026-12-01"));
  await settleInAct(ui.fillDateTime("Daily window", "00:30"));
  expect(changes).toEqual({ launch: "2026-11-27T21:05", freeze: "2026-12-01", window: "00:30" });
});

it.each(LANGUAGES)("fillDateTime tells noon from midnight in %s", async (language) => {
  const { changes, ui } = mountFields(language);
  await settleInAct(ui.fillDateTime("Daily window", "12:00"));
  expect(changes.window).toBe("12:00");
  await settleInAct(ui.fillDateTime("Daily window", "09:30"));
  expect(changes.window).toBe("09:30");
});

it("fillDateTime replaces a value already in the field, and finds a field by its group's aria-label", async () => {
  const { changes, ui } = mountFields("en");
  await settleInAct(ui.fillDateTime("Code freeze", "2026-12-01"));
  await settleInAct(ui.fillDateTime("Code freeze", "2027-01-09"));
  expect(changes.freeze).toBe("2027-01-09");
  await settleInAct(ui.fillDateTime("Start", "13:45"));
  expect(changes.start).toBe("13:45");
});

it("fillDateTime fails on a value that is not a date or time, and on a field that is not there", async () => {
  const { ui } = mountFields("en");
  await expect(settleInAct(ui.fillDateTime("Code freeze", "20 Nov"))).rejects.toThrow("is not YYYY-MM-DD, HH:MM or YYYY-MM-DDTHH:MM");
  await expect(settleInAct(ui.fillDateTime("Nothing", "2026-12-01"))).rejects.toThrow("No date or time field labelled Nothing");
});

it("fillDateTime fails when the day period the field shows is not the one asked for", async () => {
  // A field whose typed first letter of PM lands on AM, as Korean 오후 does where an engine writes it with the same first
  // letter as 오전: here a field that keeps every afternoon hour in the morning, so the segment shows AM after PM was typed.
  mount(
    <LocaleProvider locale="en">
      <Held name="window" record={() => undefined}>
        {(value, onChange) => (
          <TimeField value={value} onChange={(next) => onChange(next && next.replace(/^(1[2-9]|2[0-3])/, (hour) => String(Number(hour) - 12).padStart(2, "0")))}>
            <Label>Daily window</Label>
            <TimeInput />
          </TimeField>
        )}
      </Held>
    </LocaleProvider>,
  );
  const ui = createUi(document, "en");
  await settleInAct(ui.fillDateTime("Daily window", "09:30"));
  await expect(settleInAct(ui.fillDateTime("Daily window", "21:05"))).rejects.toThrow("shows AM as its day period after filling it with 21:05, not PM");
});
