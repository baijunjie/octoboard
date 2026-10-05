import { Input, Label, TextField } from "@heroui/react";
import React from "react";

/** A labelled single-line text field. */
export function TextInput({
  label,
  value,
  onChange,
  placeholder,
  autoFocus,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
}): React.ReactElement {
  return (
    <TextField fullWidth value={value} onChange={onChange} autoFocus={autoFocus}>
      <Label>{label}</Label>
      <Input placeholder={placeholder} />
    </TextField>
  );
}
