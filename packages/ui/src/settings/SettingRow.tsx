import { Switch } from "@heroui/react";
import React, { useEffect, useRef } from "react";

/** One setting: its name and, when it has one, a line saying what it does at the start, its control
 * at the end, with a thin divider under every row but the last. At phone widths the control moves
 * under the text instead, since beside it a wide control (a select) squeezes the description to a
 * few words per line. `help` is a help tip that ends the name's line (see "Explanations in help
 * tips" in `docs/product/labels-and-tooltips.md`). `below` is what the row has to say under itself across its whole
 * width (a warning, a progress bar), inside its divider. */
export function SettingRow({
  label,
  help,
  description,
  children,
  below,
  motionKey,
}: {
  label: React.ReactNode;
  help?: React.ReactNode;
  description?: React.ReactNode;
  children?: React.ReactNode;
  below?: React.ReactNode;
  motionKey?: string;
}): React.ReactElement {
  return (
    <div data-list-motion={motionKey} className="flex flex-col gap-3 groove-b py-4 last:border-b-0">
      <div className="flex flex-col items-start gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-sm font-medium text-foreground">
            <div className="min-w-0">{label}</div>
            {help}
          </div>
          {description && <div className="text-sm text-app-muted">{description}</div>}
        </div>
        {children && <div className="shrink-0">{children}</div>}
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
}: {
  label: string;
  isSelected: boolean;
  onChange: (value: boolean) => void;
  /** The ids of the notes that describe the switch. */
  describedBy?: string;
  /** The switch cannot be changed now, for a reason `describedBy` gives. It is not natively disabled, which would take
   * it out of the Tab order and drop focus from a user who is on it: it stays focusable, reads as `aria-disabled`,
   * looks faded, and a press or Space leaves it as it is. */
  isUnavailable?: boolean;
}): React.ReactElement {
  const root = useRef<HTMLDivElement>(null);
  // The library puts `isReadOnly` on the input as `aria-readonly` and leaves out `aria-disabled`, which says it is
  // unavailable rather than merely fixed.
  useEffect(() => {
    const input = root.current?.querySelector("input");
    if (isUnavailable) input?.setAttribute("aria-disabled", "true");
    else input?.removeAttribute("aria-disabled");
  }, [isUnavailable]);
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
    <SettingRow label={label} help={help} description={description} motionKey={motionKey}>
      <SettingSwitch label={label} isSelected={isSelected} onChange={onChange} />
    </SettingRow>
  );
}
