import { Switch } from "@heroui/react";
import React from "react";

/** One setting: its name and, when it has one, a line saying what it does at the start, its control
 * at the end, with a thin divider under every row but the last. At phone widths the control moves
 * under the text instead, since beside it a wide control (a select) squeezes the description to a
 * few words per line. `help` is a help tip that ends the name's line (see "Explanations in help
 * tips" in `docs/product/labels-and-tooltips.md`). */
export function SettingRow({
  label,
  help,
  description,
  children,
  motionKey,
}: {
  label: React.ReactNode;
  help?: React.ReactNode;
  description?: React.ReactNode;
  children?: React.ReactNode;
  motionKey?: string;
}): React.ReactElement {
  return (
    <div data-list-motion={motionKey} className="flex flex-col items-start gap-3 groove-b py-4 last:border-b-0 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
      <div className="min-w-0">
        <div className="flex items-center gap-2 text-sm font-medium">
          <div className="min-w-0">{label}</div>
          {help}
        </div>
        {description && <div className="text-sm text-app-muted">{description}</div>}
      </div>
      {children && <div className="shrink-0">{children}</div>}
    </div>
  );
}

/** A `SettingRow` whose control is an on/off switch. The switch is named by the visible label. */
export function SwitchSettingRow({
  label,
  help,
  description,
  isSelected,
  onChange,
}: {
  label: string;
  help?: React.ReactNode;
  description: React.ReactNode;
  isSelected: boolean;
  onChange: (value: boolean) => void;
}): React.ReactElement {
  return (
    <SettingRow label={label} help={help} description={description}>
      <Switch aria-label={label} isSelected={isSelected} onChange={onChange}>
        <Switch.Content>
          <Switch.Control>
            <Switch.Thumb />
          </Switch.Control>
        </Switch.Content>
      </Switch>
    </SettingRow>
  );
}
