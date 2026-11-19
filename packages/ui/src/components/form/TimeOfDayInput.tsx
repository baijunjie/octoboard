import React from "react";

import { FieldErrorMessage, hasErrorMessage } from "./FieldErrorMessage";
import { Description, Label, TimeField, TimeInput } from "../../primitives";

/** A labelled time of day, in HeroUI's variant for a field on a surface (as `TextInput`), entered as segments (hour and
 * minute, with a day period under a 12-hour language) with no popup. `value` is `HH:MM`, or empty while the field is
 * empty or only partly filled; `onChange` is called with `HH:MM` once every segment is filled and with an empty string
 * once every segment is cleared or, as focus leaves the field or Enter is pressed in a segment, once any one is.
 * `errorMessage` marks the field invalid and says what is wrong with it right under it, in place of `description`, as
 * `TextInput`'s does. */
export function TimeOfDayInput({ label, value, onChange, onBlur, description, errorMessage, isDisabled }: {
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
    <TimeField fullWidth value={value || null} onChange={(next) => onChange(next ?? "")} onBlur={onBlur} disabled={isDisabled} invalid={invalid}>
      <Label>{label}</Label>
      <TimeInput variant="secondary" />
      {description && !invalid && <Description>{description}</Description>}
      <FieldErrorMessage>{invalid ? errorMessage : null}</FieldErrorMessage>
    </TimeField>
  );
}
