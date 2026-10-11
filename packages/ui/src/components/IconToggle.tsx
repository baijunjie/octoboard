import { ToggleButton } from "@heroui/react";
import React from "react";

import { TitledControl } from "./TitledControl";

/**
 * A two-state view control drawn as one icon button. The icon shows the state the view is in and
 * `label`, which is both the accessible name and the tooltip, says what pressing does: the
 * convention of the pane toggles. Nothing is told by colour, so the caller gives each state its own
 * icon and `.icon-toggle` (`style.css`) keeps the selected state from taking a fill.
 */
export function IconToggle({
  label,
  className,
  isSelected,
  onChange,
  children,
}: {
  label: string;
  className?: string;
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  /** The icon of the current state. */
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <TitledControl title={label}>
      <ToggleButton
        isIconOnly
        size="sm"
        className={className ? `icon-toggle ${className}` : "icon-toggle"}
        aria-label={label}
        isSelected={isSelected}
        onChange={onChange}
        preventFocusOnPress
      >
        {children}
      </ToggleButton>
    </TitledControl>
  );
}
