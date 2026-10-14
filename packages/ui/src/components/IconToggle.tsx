import { ToggleButton } from "@heroui/react";
import React from "react";

import { TitledControl } from "./TitledControl";
import { ROW_ICON_BUTTON_CLASS } from "./RowControls";

/**
 * A two-state view control drawn as one icon button. The icon shows the state the view is in and
 * `label`, which is both the accessible name and the tooltip, says what pressing does: the
 * convention of the pane toggles. Nothing is told by colour, so the caller gives each state its own
 * icon and `.icon-toggle` (`style.css`) keeps the selected state from taking a fill.
 */
export function IconToggle({
  label,
  isSelected,
  onChange,
  expanded,
  ref,
  children,
}: {
  label: string;
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  /** For a toggle that shows or hides a region rather than only restyling one: whether the region
   * is on screen, as `aria-expanded`. */
  expanded?: boolean;
  /** The button itself, for a region that has to hand keyboard focus back to it as it goes. */
  ref?: React.Ref<HTMLButtonElement>;
  /** The icon of the current state. */
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <TitledControl title={label}>
      <ToggleButton
        ref={ref}
        isIconOnly
        size="sm"
        variant="ghost"
        className={`icon-toggle ${ROW_ICON_BUTTON_CLASS}`}
        aria-label={label}
        aria-expanded={expanded}
        isSelected={isSelected}
        onChange={onChange}
        preventFocusOnPress
      >
        {children}
      </ToggleButton>
    </TitledControl>
  );
}
