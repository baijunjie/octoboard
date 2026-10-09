import { Tooltip } from "@heroui/react";
import React from "react";

/**
 * A tooltip on a non-control indicator (a badge, a mark): it shows on pointer hover only. The
 * indicator is no control, so it is no tab stop (`tabIndex={-1}`, which `Tooltip.Trigger` lets a
 * prop override: its own is a focusable `role="button"` `div`) and a press does not focus it, which
 * would take focus off the terminal. That is why it is `Tooltip.Trigger` rather than
 * `TitledControl`, which needs a react-aria-components control as its child. With a `label` the
 * trigger is a named `role="img"`; without one it is `role="presentation"`, for an indicator whose
 * meaning reaches assistive technology some other way (the row's own accessible name); browsers
 * ignore that role on a focusable element, so the trigger is still exposed as a generic one, which
 * is harmless. `className` lays out the trigger around `children`.
 */
export function IndicatorTooltip({
  tooltip,
  label,
  className,
  children,
}: {
  tooltip: string;
  label?: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const naming = label ? ({ role: "img", "aria-label": label } as const) : ({ role: "presentation" } as const);
  return (
    <Tooltip>
      <Tooltip.Trigger {...naming} tabIndex={-1} className={className} onMouseDown={(event) => event.preventDefault()}>
        {children}
      </Tooltip.Trigger>
      <Tooltip.Content className="pointer-events-none">{tooltip}</Tooltip.Content>
    </Tooltip>
  );
}
