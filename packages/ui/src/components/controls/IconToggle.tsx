import React from "react";

import { TitledControl } from "../hint/TitledControl";
import { ToggleButton } from "../../primitives";
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
  inHeader,
  landing,
  ref,
  children,
}: {
  label: string;
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  /** For a toggle that shows or hides a region rather than only restyling one: whether the region
   * is on screen, as `aria-expanded`. */
  expanded?: boolean;
  /** Joins the section heading's arrow-key walk instead of the tab order. */
  inHeader?: boolean;
  /** Makes the toggle where F6 lands while the region it reveals is hidden. */
  landing?: boolean;
  /** The button itself, for a region that has to hand keyboard focus back to it as it goes. */
  ref?: React.Ref<HTMLButtonElement>;
  /** The icon of the current state. */
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <TitledControl title={label}>
      <ToggleButton
        ref={ref}
        tabStop={!inHeader}
        data-section-heading-control={inHeader ? "" : undefined}
        iconOnly
        size="sm"
        variant="ghost"
        className={`icon-toggle row-icon-control ${ROW_ICON_BUTTON_CLASS}`}
        label={label}
        expanded={expanded}
        data-region-landing={landing || undefined}
        selected={isSelected}
        onSelectedChange={onChange}
        focusOnPress={false}
      >
        {children}
      </ToggleButton>
    </TitledControl>
  );
}
