import React from "react";

import { ToggleButton, Tooltip } from "../../primitives";
import { ROW_ICON_BUTTON_CLASS } from "./RowControls";

/**
 * A two-state view control drawn as one icon button. The icon shows the state the view is in and
 * `label`, which is both the accessible name and the tooltip, says what pressing does: the
 * convention of the pane toggles. Nothing is told by colour, so the caller gives each state its own
 * icon and `.icon-toggle` (`IconToggle.css`) keeps the selected state from taking a fill.
 */
export function IconToggle({
  label,
  isSelected,
  onChange,
  expanded,
  inHeader,
  landing,
  besideField = false,
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
  /** Stands beside a field in one row (`FieldRow`): the filled toggle with `ToggleButton`'s `besideField`, the field's
   * height and corners as every button beside a field has, rather than the bare 24px icon of a row's controls, which
   * beside a field reads as a stray glyph. */
  besideField?: boolean;
  /** The button itself, for a region that has to hand keyboard focus back to it as it goes. */
  ref?: React.Ref<HTMLButtonElement>;
  /** The icon of the current state. */
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Tooltip content={label}>
      <ToggleButton
        ref={ref}
        tabStop={!inHeader}
        data-section-heading-control={inHeader ? "" : undefined}
        iconOnly
        size="sm"
        variant={besideField ? "default" : "ghost"}
        besideField={besideField}
        className={besideField ? "icon-toggle" : `icon-toggle row-icon-control ${ROW_ICON_BUTTON_CLASS}`}
        label={label}
        expanded={expanded}
        data-region-landing={landing || undefined}
        selected={isSelected}
        onSelectedChange={onChange}
        focusOnPress={false}
      >
        {children}
      </ToggleButton>
    </Tooltip>
  );
}
