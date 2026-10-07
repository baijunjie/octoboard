import { Description, Input, Label, TextField } from "@heroui/react";
import React from "react";

/** A labelled single-line text field, in HeroUI's variant for a field on a surface, since it is
 * always in a dialog. `ltr` lays out a value that always reads left to right (a
 * path, a URL) whatever the UI's direction. The placeholder is an example; `description` carries
 * anything else the field has to say. */
export function TextInput({
  label,
  value,
  onChange,
  placeholder,
  description,
  autoFocus,
  dir,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  description?: string;
  autoFocus?: boolean;
  dir?: "ltr";
}): React.ReactElement {
  return (
    <TextField fullWidth variant="secondary" value={value} onChange={onChange} autoFocus={autoFocus}>
      <Label>{label}</Label>
      <Input dir={dir} placeholder={placeholder} />
      {description && <Description>{description}</Description>}
    </TextField>
  );
}
