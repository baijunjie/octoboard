import { TimeField as HeroTimeField } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import { SegmentedInput, useSettledOnBlank } from "./SegmentedInput";
import { timeOf, timeText } from "./dateText";
import { FieldLockedContext, type FieldVariant } from "./Field";

/** HeroUI's time field: the field around a `Label`, a `TimeInput` of hour and minute segments, a `Description` and a
 * `FieldError`. `value` is the time of day as `HH:MM`, or `null` while it is empty or incomplete (a segment left
 * blank); `onChange` receives the time once every segment is filled, and `null` once every segment is cleared or, as
 * focus leaves the field or Enter is pressed in a segment, once any one is (see `useSettledOnBlank`). The hour cycle
 * (12 or 24 hours) and the order and separators follow the language the application hands the layer (`LocaleProvider`),
 * not the browser's. The arrow keys step a segment, digits fill it and move on, and there is no popup. `onBlur` is
 * called as focus leaves the whole field. Like `TextField`, an invalid field never hands a submit to the browser's
 * constraint validation, and `required` only tells assistive technology that the field needs a value. As on a
 * `TextField`, `readOnly` shows the value and takes no edit, and `locked` draws it as a value to read. */
export function TimeField({ value, onChange, onBlur, label, describedBy, disabled, invalid, required, readOnly, locked, fullWidth, className, children }: {
  value: string | null;
  onChange: (value: string | null) => void;
  onBlur?: () => void;
  /** The accessible name, for a field with no visible `Label`. */
  label?: string;
  describedBy?: string;
  disabled?: boolean;
  invalid?: boolean;
  required?: boolean;
  readOnly?: boolean;
  locked?: boolean;
  fullWidth?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const { ref, settle } = useSettledOnBlank(value, onChange, !!(readOnly || locked));
  return (
    <HeroTimeField
      value={timeOf(value)}
      onChange={(time) => onChange(timeText(time))}
      ref={ref}
      onBlur={() => {
        settle();
        onBlur?.();
      }}
      granularity="minute"
      aria-label={label}
      aria-describedby={describedBy}
      isDisabled={useDisabled(disabled)}
      isInvalid={invalid}
      isRequired={required}
      isReadOnly={readOnly}
      validationBehavior="aria"
      fullWidth={fullWidth}
      className={className}
    >
      <FieldLockedContext.Provider value={!!locked}>{children}</FieldLockedContext.Provider>
    </HeroTimeField>
  );
}

/** The segmented input of a `TimeField`: its hour and minute, drawn as HeroUI's date input group, whose `variant` is the
 * field's (`secondary` on a surface). */
export function TimeInput({ variant }: { variant?: FieldVariant }): React.ReactElement {
  return <SegmentedInput variant={variant} />;
}
