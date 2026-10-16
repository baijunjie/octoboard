import { Description, Label, TextArea, TextField } from "@heroui/react";
import React from "react";

/** A labelled multi-line text field, in HeroUI's variant for a field on a surface, for free text
 * the user writes at length. It does not trim what is typed, so leading and trailing whitespace
 * survive while editing and the form decides what to do with them, and Enter adds a line rather
 * than submitting the dialog. The placeholder names what an empty field means; `description`
 * carries anything else the field has to say. */
export function TextAreaInput({
  label,
  value,
  onChange,
  placeholder,
  description,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  description?: string;
}): React.ReactElement {
  return (
    <TextField fullWidth variant="secondary" value={value} onChange={onChange}>
      <Label>{label}</Label>
      <TextArea rows={6} placeholder={placeholder} />
      {description && <Description>{description}</Description>}
    </TextField>
  );
}
