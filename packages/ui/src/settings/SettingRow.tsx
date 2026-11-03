import { Switch } from "@heroui/react";
import React, { useEffect, useRef } from "react";

import { MarkLineBox } from "../components/status/MarkLineBox";

/** One setting: its name and, when it has one, a line saying what it does at the start, its control
 * at the end, with a thin divider under every row but the last (the copy of a row a list motion is
 * still removing does not count, so the divider goes as the row leaves). At phone widths a wide
 * control moves under the text instead, since beside it it squeezes the description to a few words
 * per line, end-aligned so it sits against the same edge the control has beside the text. Pass
 * `compact` when the control is one small control of a bounded width — a switch, one button, one
 * badge, a link of a word or two, with a status mark beside it at most — so it stays beside the
 * text at every width, where under it it would leave a line almost empty. Leave it off for a
 * select, a text field, a group of buttons or a value whose width varies (a URL). On a row without
 * `compact`, pass `fill` when the control is an input (a select, a text field, a path picker with its Browse
 * button): under the text it then spans the row and its one child grows to fill it, whatever width
 * the child has beside the text; a child that holds buttons next to the input is a flex row whose
 * input has `flex-1` and whose buttons keep their size. At `sm` and wider `fill` changes
 * nothing. `help` is a help tip that ends the name's line (see "Explanations in help tips" in
 * `docs/product/labels-and-tooltips.md`). `below` is what the row has to say under itself across
 * its whole width (a progress bar, a full-width field such as a tags input), inside its divider. */
export function SettingRow({
  label,
  help,
  description,
  children,
  below,
  motionKey,
  compact = false,
  fill = false,
}: {
  label: React.ReactNode;
  help?: React.ReactNode;
  description?: React.ReactNode;
  children?: React.ReactNode;
  below?: React.ReactNode;
  motionKey?: string;
  compact?: boolean;
  fill?: boolean;
}): React.ReactElement {
  return (
    <div data-list-motion={motionKey} className="flex flex-col gap-3 groove-b py-4 [&:not(:has(~:not([data-list-motion-exit])))]:border-b-0">
      <div className={compact ? "flex items-center justify-between gap-4 sm:gap-6" : "flex flex-col items-start gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-6"}>
        <div className="min-w-0">
          <div className="flex items-start gap-2 text-sm font-medium text-foreground">
            <div className="min-w-0">{label}</div>
            {help && <MarkLineBox size="text-sm">{help}</MarkLineBox>}
          </div>
          {description && <div className="text-sm text-app-muted">{description}</div>}
        </div>
        {/* A row whose controls all render nothing (`false`) draws no empty box taking the gap. */}
        {React.Children.toArray(children).length > 0 && <div className={compact ? "shrink-0" : fill ? "shrink-0 max-sm:flex max-sm:w-full max-sm:*:min-w-0 max-sm:*:flex-1" : "shrink-0 max-sm:self-end-safe"}>{children}</div>}
      </div>
      {below}
    </div>
  );
}

/** An on/off switch named by `label`, which the setting shows itself. */
export function SettingSwitch({
  label,
  isSelected,
  onChange,
  describedBy,
  isUnavailable,
  opensFocused,
}: {
  label: string;
  isSelected: boolean;
  onChange: (value: boolean) => void;
  /** The ids of the notes that describe the switch. */
  describedBy?: string;
  /** The switch cannot be changed now: for a reason `describedBy` gives, or only for as long as an operation it
   * started is unanswered, which needs no note. It is not natively disabled, which would take
   * it out of the Tab order and drop focus from a user who is on it: it stays focusable, reads as `aria-disabled`,
   * looks faded, and a press or Space leaves it as it is. */
  isUnavailable?: boolean;
  /** The switch is where the dialog around it puts focus on opening (see `openingFocus`). */
  opensFocused?: boolean;
}): React.ReactElement {
  const root = useRef<HTMLDivElement>(null);
  // The library puts `isReadOnly` on the input as `aria-readonly` and leaves out `aria-disabled`, which says it is
  // unavailable rather than merely fixed.
  useEffect(() => {
    const input = root.current?.querySelector("input");
    if (isUnavailable) input?.setAttribute("aria-disabled", "true");
    else input?.removeAttribute("aria-disabled");
  }, [isUnavailable]);
  // The library gives the input no way to take the mark as a prop.
  useEffect(() => {
    const input = root.current?.querySelector("input");
    if (opensFocused) input?.setAttribute("data-opening-focus", "");
    else input?.removeAttribute("data-opening-focus");
  }, [opensFocused]);
  return (
    <Switch
      ref={root}
      aria-label={label}
      aria-describedby={describedBy}
      isReadOnly={isUnavailable}
      isSelected={isSelected}
      onChange={(value) => { if (!isUnavailable) onChange(value); }}
      className={isUnavailable ? "opacity-(--disabled-opacity)" : undefined}
    >
      <Switch.Content>
        <Switch.Control>
          <Switch.Thumb />
        </Switch.Control>
      </Switch.Content>
    </Switch>
  );
}

/** A `SettingRow` whose control is an on/off switch. The switch is named by the visible label. */
export function SwitchSettingRow({
  label,
  help,
  description,
  isSelected,
  onChange,
  motionKey,
}: {
  label: string;
  help?: React.ReactNode;
  description: React.ReactNode;
  isSelected: boolean;
  onChange: (value: boolean) => void;
  motionKey?: string;
}): React.ReactElement {
  return (
    <SettingRow label={label} help={help} description={description} motionKey={motionKey} compact>
      <SettingSwitch label={label} isSelected={isSelected} onChange={onChange} />
    </SettingRow>
  );
}
