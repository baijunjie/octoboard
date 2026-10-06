import { Input, Label, TextField } from "@heroui/react";
import React from "react";

/** A labelled single-line text field, in HeroUI's variant for a field on a surface, since it is
 * always in a dialog. `ltr` lays out a value that always reads left to right (a
 * path, a URL) whatever the UI's direction. */
export function TextInput({
  label,
  value,
  onChange,
  placeholder,
  autoFocus,
  dir,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
  dir?: "ltr";
}): React.ReactElement {
  return (
    <TextField fullWidth variant="secondary" value={value} onChange={onChange} autoFocus={autoFocus}>
      <Label>{label}</Label>
      <Input dir={dir} placeholder={placeholder} />
    </TextField>
  );
}
