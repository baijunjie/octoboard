import React from "react";

import { FieldErrorMessage, hasErrorMessage } from "./FieldErrorMessage";
import { DatePicker, DatePickerInput, Description, Label } from "../../primitives";

/** A labelled local date and time, in HeroUI's variant for a field on a surface (as `TextInput`), entered as segments
 * (the date, then the time to the minute) with a button at the field's end that opens a calendar for the date. `value`
 * is `YYYY-MM-DDTHH:MM`, or empty while the field is empty or only partly filled; `onChange` is called with
 * `YYYY-MM-DDTHH:MM` once every segment is filled and with an empty string once every segment is cleared or, as focus
 * leaves the field or Enter is pressed in a segment, once any one is. `errorMessage` marks the field invalid and says
 * what is wrong with it right under it, in place of `description`, as `TextInput`'s does. The field is a block, not
 * HeroUI's inline box, which would sit a few pixels low in a row with the fields beside it. */
export function DateTimeInput({ label, value, onChange, onBlur, description, errorMessage, isDisabled }: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  /** Called as focus leaves the field. */
  onBlur?: () => void;
  description?: React.ReactNode;
  errorMessage?: React.ReactNode;
  isDisabled?: boolean;
}): React.ReactElement {
  const invalid = hasErrorMessage(errorMessage);
  return (
    <DatePicker fullWidth className="flex" value={value || null} onChange={(next) => onChange(next ?? "")} onBlur={onBlur} disabled={isDisabled} invalid={invalid}>
      <Label>{label}</Label>
      <DatePickerInput variant="secondary" />
      {description && !invalid && <Description>{description}</Description>}
      <FieldErrorMessage>{invalid ? errorMessage : null}</FieldErrorMessage>
    </DatePicker>
  );
}
