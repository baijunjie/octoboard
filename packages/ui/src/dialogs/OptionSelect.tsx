import { Label, ListBox, Select } from "@heroui/react";
import React from "react";

/** A single-choice dropdown over a fixed list of options. An option's `lang` marks its text as
 * being in a language other than the UI's; its `icon` is drawn before its text, in the list and
 * in the trigger while it is the chosen one. With `inline` it is only as wide as its value and
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
  options: { value: T; label: string; lang?: string; icon?: React.ReactNode }[];
  value: T;
  onChange: (value: T) => void;
  inline?: boolean;
}): React.ReactElement {
  return (
    // HeroUI's variant for a field on a surface: every one of these sits in a dialog.
    <Select
      variant="secondary"
      fullWidth={!inline}
      className={inline ? "w-48" : undefined}
      aria-label={inline ? label : undefined}
      value={value}
      onChange={(key) => key !== null && onChange(key as T)}
    >
      {!inline && <Label>{label}</Label>}
      <Select.Trigger>
        {/* Renders the chosen item's own content, its icon included, so it lays that out in a row. */}
        <Select.Value className="flex items-center gap-2" />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {options.map((option) => (
            <ListBox.Item key={option.value} id={option.value} textValue={option.label}>
              {option.icon && (
                <span aria-hidden="true" className="flex shrink-0">
                  {option.icon}
                </span>
              )}
              <span lang={option.lang}>{option.label}</span>
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}
