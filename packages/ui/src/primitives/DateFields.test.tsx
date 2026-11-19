// @vitest-environment jsdom
import { act, type ReactElement, useState } from "react";
import { expect, it } from "vitest";

import { closeOpenTooltip, mountForTooltip, openTooltip } from "../components/hint/tooltipTestSupport";
import { fakeTimers, runOutTimers } from "../test/clock";
import { clearSegment, segmentOf, typeSegment } from "../test/dom";
import { DatePicker, DatePickerInput } from "./DatePicker";
import { Label } from "./Field";
import { BusyContext } from "./busy";
import { PrimitiveLabelsProvider } from "./labels";
import { LocaleProvider } from "./locale";
import { partSelector } from "./parts";
import { mount } from "./testSupport";
import { TimeField, TimeInput } from "./TimeField";

const LABELS = {
  scrollTabs: { up: "", down: "", left: "", right: "" },
  tooltip: "",
  calendar: { open: "Choose a day", previous: "Earlier", next: "Later" },
};

const segments = (container: ParentNode) => [...container.querySelectorAll("[role=spinbutton]")].map((segment) => segment.textContent).join(" ");

it("takes a time of day as HH:MM, and an empty or unreadable one as an empty field", () => {
  const { container, render } = mount(
    <TimeField value="07:45" onChange={() => {}} label="Start"><TimeInput /></TimeField>,
  );
  expect(segments(container)).toContain("7 45");
  render(<TimeField value={null} onChange={() => {}} label="Start"><TimeInput /></TimeField>);
  expect(container.querySelectorAll(partSelector.blankSegment)).toHaveLength(3);
  render(<TimeField value="soon" onChange={() => {}} label="Start"><TimeInput /></TimeField>);
  expect(container.querySelectorAll(partSelector.blankSegment)).toHaveLength(3);
});

it("takes a date and time as YYYY-MM-DDTHH:MM", () => {
  const { container } = mount(
    <DatePicker value="2027-01-09T21:30" onChange={() => {}} label="Start"><DatePickerInput /></DatePicker>,
  );
  expect(segments(container)).toContain("1 9 2027 9 30 PM");
});

it("draws the fields in the language it is given, not the browser's", () => {
  const { container } = mount(
    <LocaleProvider locale="de">
      <DatePicker value="2027-01-09T21:30" onChange={() => {}} label="Start"><DatePickerInput /></DatePicker>
    </LocaleProvider>,
  );
  expect(segments(container)).toBe("9 1 2027 21 30");
});

it("names the calendar button from the application's labels", () => {
  const { container } = mount(
    <PrimitiveLabelsProvider labels={LABELS}>
      <DatePicker value="2027-01-09T21:30" onChange={() => {}} label="Start"><DatePickerInput /></DatePicker>
    </PrimitiveLabelsProvider>,
  );
  expect(container.querySelector("button")!.getAttribute("aria-label")).toBe("Choose a day");
});

it("names the field by its label, and leaves the fields disabled in a busy modal", () => {
  const { container } = mount(
    <BusyContext.Provider value>
      <TimeField value="07:45" onChange={() => {}}><Label>Start</Label><TimeInput /></TimeField>
    </BusyContext.Provider>,
  );
  const segment = container.querySelector("[role=spinbutton]")!;
  expect(segment.getAttribute("aria-labelledby")).toContain(container.querySelector("[data-slot=label]")!.id);
  expect(segment.getAttribute("aria-disabled")).toBe("true");
});

it("takes a date as YYYY-MM-DD at the day granularity, with no time segments, and an unreadable one as an empty field", () => {
  const { container, render } = mount(
    <DatePicker value="2027-01-09" granularity="day" onChange={() => {}} label="Day"><DatePickerInput /></DatePicker>,
  );
  expect(segments(container)).toBe("1 9 2027");
  render(<DatePicker value="2027-01-09T21:30" granularity="day" onChange={() => {}} label="Day"><DatePickerInput /></DatePicker>);
  expect(container.querySelectorAll(partSelector.blankSegment)).toHaveLength(3);
});

it("shows a read-only field's value without taking an edit, locked as a value to read, with the calendar button disabled", () => {
  const changes: (string | null)[] = [];
  const { container } = mount(
    <DatePicker value="2027-01-09" granularity="day" onChange={(next) => changes.push(next)} label="Day" readOnly locked><DatePickerInput /></DatePicker>,
  );
  const group = container.querySelector("[data-slot=date-input-group]")!;
  expect(group.className).toContain("border-separator");
  expect(group.className).toContain("bg-transparent");
  expect(segmentOf(container, "month").getAttribute("aria-readonly")).toBe("true");
  expect(container.querySelector("button")!.disabled).toBe(true);
  typeSegment(segmentOf(container, "month"), "5");
  expect(changes).toEqual([]);
  expect(segments(container)).toBe("1 9 2027");
});

it("leaves a time field that is not locked drawn as a control, and tells assistive technology a required one needs a value", () => {
  const { container, render } = mount(
    <TimeField value="07:45" onChange={() => {}} label="Start" required><TimeInput /></TimeField>,
  );
  expect(container.querySelector("[data-slot=date-input-group]")!.className).not.toContain("bg-transparent");
  expect(segmentOf(container, "hour").getAttribute("aria-required")).toBe("true");
  render(<TimeField value="07:45" onChange={() => {}} label="Start" readOnly locked><TimeInput /></TimeField>);
  expect(container.querySelector("[data-slot=date-input-group]")!.className).toContain("bg-transparent");
  expect(segmentOf(container, "hour").getAttribute("aria-readonly")).toBe("true");
});

const dayCell = (label: string) => [...document.body.querySelectorAll<HTMLElement>("[role=gridcell] [role=button]")].find((candidate) => candidate.textContent === label)!;

it("greys out the calendar's days outside min and max, and still takes a day typed outside them", () => {
  fakeTimers();
  const changes: (string | null)[] = [];
  const { container } = mount(
    <DatePicker value="2026-11-15" granularity="day" min="2026-11-10" max="2026-11-20" onChange={(next) => changes.push(next)} label="Day"><DatePickerInput /></DatePicker>,
  );
  typeSegment(segmentOf(container, "day"), "25");
  expect(changes.at(-1)).toBe("2026-11-25");
  expect(container.querySelector("[aria-invalid=true], [data-invalid=true]")).toBeNull();
  act(() => container.querySelector("button")!.click());
  runOutTimers();
  expect(dayCell("9").getAttribute("aria-disabled")).toBe("true");
  expect(dayCell("10").getAttribute("aria-disabled")).toBeNull();
  expect(dayCell("20").getAttribute("aria-disabled")).toBeNull();
  expect(dayCell("21").getAttribute("aria-disabled")).toBe("true");
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  runOutTimers();
});

const enterIn = (element: Element) => act(() => void element.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })));

it.each([
  ["a time field", "10:30", "hour"],
  ["a date and time field", "2027-01-09T21:30", "month"],
  ["a date field", "2027-01-09", "month"],
] as const)("reports %s with a segment cleared as empty on Enter in a segment, before any handler up the tree reads it, and not on its own", (_, initial, cleared) => {
  const seen: (string | null)[] = [];
  const changes: (string | null)[] = [];
  function Held() {
    const [value, setValue] = useState<string | null>(initial);
    const onChange = (next: string | null) => {
      changes.push(next);
      setValue(next);
    };
    return (
      <div onKeyDownCapture={(event) => event.key === "Enter" && seen.push(value)} onKeyDown={(event) => event.key === "Enter" && seen.push(value)}>
        {initial.length === 5
          ? <TimeField value={value} onChange={onChange} label="Start"><TimeInput /></TimeField>
          : <DatePicker value={value} onChange={onChange} granularity={initial.length === 10 ? "day" : "minute"} label="Start"><DatePickerInput /></DatePicker>}
      </div>
    );
  }
  const { container } = mount(<Held />);
  clearSegment(segmentOf(container, cleared));
  // The field keeps its last complete value while the user is still typing: no empty value is reported yet.
  expect(changes).not.toContain(null);
  enterIn(segmentOf(container, cleared));
  expect(changes.at(-1)).toBeNull();
  expect(changes.filter((change) => change === null)).toHaveLength(1);
  // What an ancestor's handler reads as Enter passes is the owner's value as empty, not the last complete one.
  expect(seen.length).toBeGreaterThan(0);
  expect(seen.every((value) => value === null)).toBe(true);
});

it("reports nothing on Enter in a complete field, in an empty one, or on the calendar button", () => {
  const changes: (string | null)[] = [];
  const { container, render } = mount(
    <DatePicker value="2027-01-09T21:30" onChange={(next) => changes.push(next)} label="Start"><DatePickerInput /></DatePicker>,
  );
  enterIn(segmentOf(container, "month"));
  expect(changes).toEqual([]);
  enterIn(container.querySelector("button")!);
  expect(changes).toEqual([]);
  render(<DatePicker value={null} onChange={(next) => changes.push(next)} label="Start"><DatePickerInput /></DatePicker>);
  enterIn(segmentOf(container, "month"));
  expect(changes).toEqual([]);
  render(<TimeField value="07:45" onChange={(next) => changes.push(next)} label="Start"><TimeInput /></TimeField>);
  enterIn(segmentOf(container, "hour"));
  expect(changes).toEqual([]);
  render(<TimeField value={null} onChange={(next) => changes.push(next)} label="Start"><TimeInput /></TimeField>);
  enterIn(segmentOf(container, "hour"));
  expect(changes).toEqual([]);
});

it("shows a date and time field only a value written as YYYY-MM-DDTHH:MM, and a time field only HH:MM", () => {
  const empty = (container: ParentNode) => container.querySelectorAll(partSelector.blankSegment).length;
  const { container, render } = mount(
    <DatePicker value="2027-01-09" onChange={() => {}} label="Start"><DatePickerInput /></DatePicker>,
  );
  expect(empty(container)).toBe(6);
  render(<DatePicker value="2027-01-09T21" onChange={() => {}} label="Start"><DatePickerInput /></DatePicker>);
  expect(empty(container)).toBe(6);
  render(<DatePicker value="2027-01-09T21:30" onChange={() => {}} label="Start"><DatePickerInput /></DatePicker>);
  expect(empty(container)).toBe(0);
  render(<TimeField value="07:45:30" onChange={() => {}} label="Start"><TimeInput /></TimeField>);
  expect(empty(container)).toBe(3);
  render(<TimeField value="07:45" onChange={() => {}} label="Start"><TimeInput /></TimeField>);
  expect(empty(container)).toBe(0);
});

const openCalendar = (container: ParentNode) => {
  act(() => container.querySelector("button")!.click());
  runOutTimers();
  return document.body.querySelector<HTMLElement>("[role=dialog]")!;
};
const closeCalendar = () => {
  closeOpenTooltip();
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  runOutTimers();
};

it.each([
  ["a label prop", (day: ReactElement) => <DatePicker value="2027-01-09" granularity="day" onChange={() => {}} label="Delivery day">{day}</DatePicker>],
  ["a visible label", (day: ReactElement) => <DatePicker value="2027-01-09" granularity="day" onChange={() => {}}><Label>Delivery day</Label>{day}</DatePicker>],
])("names the calendar's dialog by the field's label, given as %s, and not by the button that opens it", (_, field) => {
  fakeTimers();
  const { container } = mount(<PrimitiveLabelsProvider labels={LABELS}>{field(<DatePickerInput />)}</PrimitiveLabelsProvider>);
  const dialog = openCalendar(container);
  expect(dialog.getAttribute("aria-label")).toBe("Delivery day");
  expect(dialog.getAttribute("aria-labelledby") ?? "").toBe("");
  closeCalendar();
});

it("gives the calendar button and the month buttons a tooltip equal to their name, which does not describe them again", () => {
  fakeTimers();
  const container = mountForTooltip(
    <PrimitiveLabelsProvider labels={LABELS}>
      <DatePicker value="2027-01-09" granularity="day" onChange={() => {}} label="Delivery day"><DatePickerInput /></DatePicker>
    </PrimitiveLabelsProvider>,
  );
  /** Focuses `button` after a key press, the way a keyboard user reaches it, which opens its tooltip at once. */
  const focusedByKeyboard = (button: HTMLElement) => {
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
      button.focus();
    });
    return openTooltip();
  };
  const trigger = container.querySelector("button")!;
  expect(trigger.getAttribute("aria-label")).toBe("Choose a day");
  expect(focusedByKeyboard(trigger)?.textContent).toBe("Choose a day");
  expect(trigger.getAttribute("aria-describedby") ?? "").toBe("");

  openCalendar(container);
  for (const name of ["Earlier", "Later"]) {
    const button = document.body.querySelector<HTMLElement>(`[aria-label="${name}"]`)!;
    expect(button, name).not.toBeNull();
    expect(focusedByKeyboard(button)?.textContent, name).toBe(name);
    expect(button.getAttribute("aria-describedby") ?? "", name).toBe("");
  }
  closeCalendar();
});

it.each([
  ["a time field", (props: { onChange: (next: string | null) => void; readOnly?: boolean; locked?: boolean }) => <TimeField value="10:30" label="Start" {...props}><TimeInput /></TimeField>, "hour"],
  ["a date and time field", (props: { onChange: (next: string | null) => void; readOnly?: boolean; locked?: boolean }) => <DatePicker value="2027-01-09T21:30" label="Start" {...props}><DatePickerInput /></DatePicker>, "month"],
] as const)("reports nothing on %s that has become read-only with a segment blank, as focus leaves it or on Enter", (_, field, part) => {
  for (const state of [{ readOnly: true }, { readOnly: true, locked: true }, { locked: true }]) {
    const changes: (string | null)[] = [];
    const onChange = (next: string | null) => changes.push(next);
    const { container, render } = mount(field({ onChange }));
    clearSegment(segmentOf(container, part));
    // The field is locked while a segment is blank, as a page's form is once it is submitted.
    render(field({ onChange, ...state }));
    enterIn(segmentOf(container, part));
    act(() => (document.activeElement as HTMLElement).blur());
    // Clearing a segment digit by digit reports the shorter numbers on the way; the field's being empty is what is not reported.
    expect(changes.filter((change) => change === null), JSON.stringify(state)).toEqual([]);
  }
});

it.each([
  ["flagged as composing", { isComposing: true }],
  ["reported with key code 229", { keyCode: 229 }],
])("does not report an emptied field on Enter pressed during an IME composition, %s", (_, init) => {
  const changes: (string | null)[] = [];
  function Held() {
    const [value, setValue] = useState<string | null>("10:30");
    return <TimeField value={value} onChange={(next) => { changes.push(next); setValue(next); }} label="Start"><TimeInput /></TimeField>;
  }
  const { container } = mount(<Held />);
  clearSegment(segmentOf(container, "minute"));
  act(() => void segmentOf(container, "minute").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", ...init, bubbles: true, cancelable: true })));
  expect(changes).not.toContain(null);
  enterIn(segmentOf(container, "minute"));
  expect(changes.filter((change) => change === null)).toEqual([null]);
});
