import { Label, ListBox, Select } from "@heroui/react";
import React from "react";

/** A single-choice dropdown over a fixed list of options. An option's `lang` marks its text as
 * being in a language other than the UI's. With `inline` it is only as wide as its value and
 * `label` is its accessible name alone, for a control whose own row already shows the name (a
 * setting). */
export function OptionSelect<T extends string>({
  label,
  options,
  value,
  onChange,
  inline,
}: {
  label: string;
  options: { value: T; label: string; lang?: string }[];
  value: T;
  onChange: (value: T) => void;
  inline?: boolean;
}): React.ReactElement {
  return (
    <Select
      fullWidth={!inline}
      className={inline ? "w-48" : undefined}
      aria-label={inline ? label : undefined}
      value={value}
      onChange={(key) => key !== null && onChange(key as T)}
    >
      {!inline && <Label>{label}</Label>}
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {options.map((option) => (
            <ListBox.Item key={option.value} id={option.value} textValue={option.label}>
              <span lang={option.lang}>{option.label}</span>
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}
