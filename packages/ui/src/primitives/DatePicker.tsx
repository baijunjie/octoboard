import { Calendar as HeroCalendar, DatePicker as HeroDatePicker } from "@heroui/react";
import type { CalendarDate } from "@internationalized/date";
import React, { useState } from "react";
import { Dialog } from "react-aria-components";

import { useDisabled } from "./busy";
import { dateOf, dateText, dateTimeOf, dateTimeText } from "./dateText";
import { FieldLockedContext, type FieldVariant } from "./Field";
import { usePrimitiveLabels } from "./labels";
import { partSelector } from "./parts";
import { definedProps } from "./props";
import { SegmentedInput, useSettledOnBlank } from "./SegmentedInput";
import { Tooltip } from "./Tooltip";

/** HeroUI's date picker: the field around a `Label`, a `DatePickerInput` of date segments (and time segments, at the
 * `minute` granularity) with a calendar button at its end, a `Description` and a `FieldError`, and the popover of a
 * calendar that the button opens (drawn by the picker itself, so it is HeroUI's popover and never the browser's). With
 * the `minute` granularity (the default) `value` is a local date and time as `YYYY-MM-DDTHH:MM`; with `day` it is a date
 * as `YYYY-MM-DD`. It is `null` while the field is empty or incomplete (a segment left blank); `onChange` receives it
 * once every segment is filled, and `null` once every segment is cleared or, as focus leaves the field or Enter is
 * pressed in a segment, once any one is (see `useSettledOnBlank`). Choosing a day in the calendar keeps the time
 * already entered and closes the popover; the calendar is named by the field's label (`label`, or the text of the
 * visible `Label`), and its three buttons have a tooltip. The order, separators and hour cycle follow the language the
 * application hands the layer (`LocaleProvider`), not the browser's. `onBlur` is called as focus leaves the field. Like
 * `TextField`, an invalid field never hands a submit to the browser's constraint validation, and `required` only tells
 * assistive technology that the field needs a value. As on a `TextField`, `readOnly` shows the value and takes no edit
 * (the calendar button then opens nothing), and `locked` draws it as a value to read. `min` and `max` (in the format of
 * `value`) grey out the days of the calendar outside them and stop its month navigation at them, a day being greyed only
 * when the whole of it is out of range; they are the calendar's only, so a value typed outside them is neither marked
 * invalid nor refused here: whoever owns the value decides. */
export function DatePicker({ value, onChange, onBlur, label, describedBy, granularity = "minute", min, max, disabled, invalid, required, readOnly, locked, fullWidth, className, children }: {
  value: string | null;
  onChange: (value: string | null) => void;
  onBlur?: () => void;
  /** The accessible name, for a field with no visible `Label`; it also names the calendar. */
  label?: string;
  describedBy?: string;
  granularity?: "day" | "minute";
  min?: string;
  max?: string;
  disabled?: boolean;
  invalid?: boolean;
  required?: boolean;
  readOnly?: boolean;
  locked?: boolean;
  fullWidth?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const day = granularity === "day";
  const { ref, settle } = useSettledOnBlank(value, onChange, !!(readOnly || locked));
  // The calendar is named as what it is, by the field's label, which a visible `Label` holds as text: read as it opens.
  const [labelText, setLabelText] = useState<string>();
  return (
    <HeroDatePicker
      value={day ? dateOf(value) : dateTimeOf(value)}
      onChange={(next) => onChange(day ? dateText(next) : dateTimeText(next as Parameters<typeof dateTimeText>[0]))}
      ref={ref}
      onBlur={() => {
        settle();
        onBlur?.();
      }}
      onOpenChange={(open) => open && setLabelText(ref.current?.querySelector(partSelector.fieldLabel)?.textContent ?? undefined)}
      granularity={granularity}
      shouldCloseOnSelect
      aria-label={label}
      aria-describedby={describedBy}
      isDisabled={useDisabled(disabled)}
      isInvalid={invalid}
      isRequired={required}
      isReadOnly={readOnly}
      validationBehavior="aria"
      className={fullWidth ? `w-full ${className ?? ""}` : className}
    >
      <FieldLockedContext.Provider value={!!locked}>{children}</FieldLockedContext.Provider>
      <CalendarPopover name={label ?? labelText} min={dateOf(min?.slice(0, 10) ?? null) ?? undefined} max={dateOf(max?.slice(0, 10) ?? null) ?? undefined} />
    </HeroDatePicker>
  );
}

/** The segmented input of a `DatePicker` with the button that opens its calendar at the end, `variant` being the
 * field's (`secondary` on a surface). */
export function DatePickerInput({ variant }: { variant?: FieldVariant }): React.ReactElement {
  const labels = usePrimitiveLabels();
  return (
    <SegmentedInput
      variant={variant}
      suffix={
        <Tooltip content={labels?.calendar.open}>
          {/* The tooltip says what the name does, which the name (with the field's label after it) already holds. */}
          <HeroDatePicker.Trigger aria-describedby="" {...(labels && { "aria-label": labels.calendar.open })}>
            <HeroDatePicker.TriggerIndicator />
          </HeroDatePicker.Trigger>
        </Tooltip>
      }
    />
  );
}

function CalendarPopover({ name, min, max }: { name?: string; min?: CalendarDate; max?: CalendarDate }): React.ReactElement {
  const labels = usePrimitiveLabels();
  return (
    <HeroDatePicker.Popover>
      {/* The picker labels its dialog by the button that opens it and the field ("Open the calendar, Delivery day"), which
          wins over an `aria-label`; the empty `aria-labelledby` drops it so the dialog is named as what it is. */}
      <Dialog aria-label={name} aria-labelledby="">
        <HeroCalendar {...definedProps({ minValue: min, maxValue: max })}>
          <HeroCalendar.Header>
            <HeroCalendar.Heading />
            <Tooltip content={labels?.calendar.previous}>
              <HeroCalendar.NavButton slot="previous" aria-describedby="" {...(labels && { "aria-label": labels.calendar.previous })} />
            </Tooltip>
            <Tooltip content={labels?.calendar.next}>
              <HeroCalendar.NavButton slot="next" aria-describedby="" {...(labels && { "aria-label": labels.calendar.next })} />
            </Tooltip>
          </HeroCalendar.Header>
          <HeroCalendar.Grid>
            <HeroCalendar.GridHeader>{(day) => <HeroCalendar.HeaderCell>{day}</HeroCalendar.HeaderCell>}</HeroCalendar.GridHeader>
            <HeroCalendar.GridBody>{(date) => <HeroCalendar.Cell date={date} />}</HeroCalendar.GridBody>
          </HeroCalendar.Grid>
        </HeroCalendar>
      </Dialog>
    </HeroDatePicker.Popover>
  );
}
