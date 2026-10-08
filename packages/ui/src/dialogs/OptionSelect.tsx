import { Label, ListBox, Select } from "@heroui/react";
import React from "react";

/** A single-choice dropdown over a fixed list of options. An option's `lang` marks its text as
 * being in a language other than the UI's; its `icon` is drawn before its text, in the list and
 * in the trigger while it is the chosen one. An option with `isDisabled` is shown, named, but
 * cannot be chosen — the current `value` may still be one, so the reason it cannot be changed
 * away *to* is on screen without taking away what is already picked. With `inline` it is only as
 * wide as its value and `label` is its accessible name alone, for a control whose own row already
 * shows the name (a setting). */
export function OptionSelect<T extends string>({
  label,
  options,
  value,
  onChange,
  inline,
}: {
  label: string;
  options: { value: T; label: string; lang?: string; icon?: React.ReactNode; isDisabled?: boolean }[];
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
        {/* react-aria's keyboard delegate defaults to skipping a disabled item in arrow-key
            traversal (`disabledBehavior="all"`), which would make the reason a user cannot pick
            it invisible to anyone not reading the popover by mouse or in screen-reader browse
            mode. `"selection"` keeps it focusable and announced while still refusing the choice.
            HeroUI's `ListBoxRootProps` omits this prop from its typings in the installed version,
            though the runtime spreads it straight through to react-stately's `useListState`
            (confirmed in `react-aria-components/dist/private/ListBox.mjs`), which does read it —
            hence the cast. */}
        <ListBox {...({ disabledBehavior: "selection" } as React.ComponentProps<typeof ListBox>)}>
          {options.map((option) => (
            <ListBox.Item key={option.value} id={option.value} textValue={option.label} isDisabled={option.isDisabled}>
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
