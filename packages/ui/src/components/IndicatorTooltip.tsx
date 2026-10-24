import { Tooltip } from "@heroui/react";
import React from "react";

import { interactionIsTouch } from "../touchInput";

import { TooltipSurface, TouchPopover, useTouchPopover } from "./tooltipCore";

/**
 * A tooltip on a non-control indicator (a badge, a mark): it shows on pointer hover only, and a
 * touch or pen tap on it opens the same content in a popover drawn like the tooltip
 * (`tooltipCore.tsx`), since a finger has no hover. The indicator is no control, so it is no tab
 * stop (`tabIndex={-1}`, which `Tooltip.Trigger` lets a prop override: its own is a focusable
 * `role="button"` `div`) and a press does not focus it, which would take focus off the terminal.
 * That is why it is `Tooltip.Trigger` rather than `TitledControl`, which needs a
 * react-aria-components control as its child. With a `label` the trigger is a named `role="img"`;
 * without one it is `role="presentation"`, for an indicator whose meaning reaches assistive
 * technology some other way (the row's own accessible name); browsers ignore that role on a
 * focusable element, so the trigger is still exposed as a generic one, which is harmless.
 * `className` lays out the trigger around `children`.
 *
 * The trigger takes no `aria-hidden`: `role="presentation"` keeps the trigger itself out of the
 * way, but not what it wraps, so an indicator carrying text rather than a hidden glyph has to hide
 * its own contents, in a wrapper that leaves them the trigger's own layout children (`aria-hidden`
 * on a `display: contents` span). Nothing inside such a wrapper may be focusable.
 *
 * The `tooltip` is any content, for an indicator whose detail has a structure; the `label` stays a
 * plain string, so the accessible name never depends on the tooltip's markup.
 *
 * `touchPopover` is on by default, as a tap on a mark has no action of its own. It has to be turned
 * off for a mark inside something that handles the same tap (a row, a button, an option), or the
 * popover would open over what the press does; the caller says why where it does. A mark that opens
 * the popover is a touch target, so on a device with a coarse pointer its hit area grows to 24 by
 * 24 pixels (`touch-target` in `style.css`) without changing what is drawn.
 *
 * One use is not an indicator: focus mode's include-console-sessions switch, a control whose hover
 * is sent to a hidden input, so it takes no tooltip trigger from context and `TitledControl` cannot
 * serve it. Its trigger is this one, with the popover off because a tap on the switch toggles it.
 * Cancelling the mouse press's focus (`onMouseDown`) does not stop the switch: react-aria's press
 * takes its own focus and toggles on the click.
 */
export function IndicatorTooltip({
  tooltip,
  label,
  className,
  touchPopover = true,
  children,
}: {
  tooltip: React.ReactNode;
  label?: string;
  className?: string;
  /** A touch or pen tap opens the tooltip's content in a popover; see above. */
  touchPopover?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  const popover = useTouchPopover();
  const naming = label ? ({ role: "img", "aria-label": label } as const) : ({ role: "presentation" } as const);
  // The click that ends a press does not carry the pointer; what the interaction came from is
  // recorded at its pointerdown.
  const touch = touchPopover ? { onClick: (event: React.MouseEvent<HTMLElement>) => interactionIsTouch() && popover.open(event.currentTarget) } : undefined;
  return (
    <>
      <Tooltip isDisabled={popover.isOpen}>
        <Tooltip.Trigger
          {...naming}
          {...touch}
          tabIndex={-1}
          className={touchPopover ? `touch-target ${className ?? ""}` : className}
          onMouseDown={(event) => event.preventDefault()}
        >
          {children}
        </Tooltip.Trigger>
        <TooltipSurface>{tooltip}</TooltipSurface>
      </Tooltip>
      {touchPopover && <TouchPopover state={popover} label={label}>{tooltip}</TouchPopover>}
    </>
  );
}
