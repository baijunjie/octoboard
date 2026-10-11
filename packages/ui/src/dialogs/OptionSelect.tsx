import { Description, Label, ListBox, Select } from "@heroui/react";
import React from "react";

/** One choice of an `OptionSelect`. */
export type Option<T extends string> = {
  value: T;
  label: string;
  lang?: string;
  icon?: React.ReactNode;
  isDisabled?: boolean;
};

/** A single-choice dropdown over a fixed list of options. An option's `lang` marks its text as
 * being in a language other than the UI's; its `icon` is drawn before its text, in the list and
 * in the trigger while it is the chosen one. An option with `isDisabled` is shown, named, but
 * cannot be chosen — the current `value` may still be one, so the reason it cannot be changed
 * away *to* is on screen without taking away what is already picked. `isDisabled` locks it, and
 * `description` is a note under it. A label too long for the control is cut with an ellipsis, as
 * in `AccountSelect`.
 *
 * With `inline` it is only as wide as its value and `label` is its accessible name alone, for a
 * control whose own row already shows the name (a setting). With `joined` it is one part of an
 * input group rather than a field of its own: it shares the group's label, so `label` is its
 * accessible name alone, it takes an equal share of the group's width, and the corners it meets
 * its neighbour at are squared and the two triggers meet with nothing between them: each half's own
 * chevron and hover fill are what say there are two controls.
 *
 * With `placeholder` the choice is optional: `value` may be `null`, which shows the placeholder
 * instead of an option and stands for "nothing chosen", and once something is chosen a clear
 * button (and Backspace or Delete on the focused control) puts it back to `null`, passed to
 * `onChange`. */
export function OptionSelect<T extends string>({
  label,
  options,
  value,
  onChange,
  placeholder,
  inline,
  joined,
  isDisabled,
  description,
}: {
  label: string;
  options: Option<T>[];
  inline?: boolean;
  /** Which end of an input group this one sits at. */
  joined?: "start" | "end";
  isDisabled?: boolean;
  /** Extra explanation shown under the field. */
  description?: string;
} & (
  | { value: T; onChange: (value: T) => void; placeholder?: undefined }
  | { value: T | null; onChange: (value: T | null) => void; placeholder: string }
)): React.ReactElement {
  return (
    // HeroUI's variant for a field on a surface: every one of these sits in a dialog.
    <Select
      variant="secondary"
      fullWidth={!inline && !joined}
      // A joined half that holds keyboard focus is raised, as its focus ring is drawn outside the
      // trigger and the later half would otherwise paint over it along the shared edge. The raise
      // is on the flex item, the root, which a trigger's own `z-index` would not be.
      className={inline ? "w-48" : joined ? "min-w-0 flex-1 has-[[data-focus-visible=true]]:z-10" : undefined}
      aria-label={inline || joined ? label : undefined}
      isDisabled={isDisabled}
      value={value}
      placeholder={placeholder}
      // Clearing arrives as a `null` key. Without a placeholder the caller has no way to take it,
      // so it is dropped there, as it always was.
      onChange={(key) => {
        if (placeholder !== undefined) (onChange as (value: T | null) => void)(key as T | null);
        else if (key !== null) onChange(key as T);
      }}
      // react-aria's keyboard delegate defaults to skipping a disabled item in arrow-key traversal
      // (`disabledBehavior="all"`), which would make the reason a user cannot pick it invisible to
      // anyone not reading the popover by mouse or in screen-reader browse mode. `"selection"`
      // keeps it focusable and announced while still refusing the choice. It belongs on the
      // `Select`, not the `ListBox`: the list renders the state the `Select` builds
      // (`useSelectState`) and ignores its own. Neither HeroUI's nor react-aria's `SelectProps`
      // declares it, though `useSelectState` passes its props through to react-stately's
      // `useListState`, which reads it — hence the cast.
      {...({ disabledBehavior: "selection" } as React.ComponentProps<typeof Select>)}
    >
      {!inline && !joined && <Label>{label}</Label>}
      {/* Joined: the two parts square the corners they meet at and touch. Nothing is drawn between
          them: a gap shows the dialog's surface through at about 1.3:1 in the light theme, and a
          border would sit close to this variant's fill in the dark one, and HeroUI's own input
          group draws no boundary between its parts either. */}
      <Select.Trigger
        className={
          joined === "start"
            ? "w-full rounded-e-none"
            : joined === "end"
              ? "w-full rounded-s-none"
              : undefined
        }
      >
        {/* Renders the chosen item's own content, its icon included, so it lays that out in a row. */}
        <Select.Value className="flex min-w-0 items-center gap-2" />
        {placeholder !== undefined && <Select.ClearButton />}
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {options.map((option) => (
            <ListBox.Item key={option.value} id={option.value} textValue={option.label} isDisabled={option.isDisabled}>
              {option.icon && (
                <span aria-hidden="true" className="flex shrink-0">
                  {option.icon}
                </span>
              )}
              <span lang={option.lang} dir="auto" className="min-w-0 truncate">
                {option.label}
              </span>
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
      {description && <Description>{description}</Description>}
    </Select>
  );
}
