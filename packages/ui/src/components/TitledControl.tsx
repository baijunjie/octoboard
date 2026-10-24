import { Tooltip } from "@heroui/react";
import React from "react";
import type { PressEvent } from "react-aria";

import { CloseTooltipWhile, opensTouchPopover, TooltipSurface, TouchPopover, useTouchPopover } from "./tooltipCore";

/** Gives a control a HeroUI tooltip, which shows on hover and on keyboard focus. HeroUI's buttons
 * are react-aria-components', which drop a native `title` prop (`filterDOMProps`). The child must
 * be a single react-aria-components control (a HeroUI `Button`, `CloseButton`, `Dropdown.Trigger`,
 * `Modal.CloseTrigger`); any other child silently gets no tooltip. Such a control picks the
 * tooltip's trigger behaviour up from context, so no element is added around it, which keeps a
 * mouse press from taking focus (`preventFocusOnPress` still holds) and leaves layout alone.
 * `Tooltip.Trigger` is deliberately not used: it renders a focusable `role="button"` wrapper.
 *
 * A tooltip is a label, not a surface to interact with: it closes the instant the pointer leaves
 * the trigger (`--tooltip-close-delay: 0s` in `style.css`, where HeroUI's default lingers), and it
 * is `pointer-events-none` (which has no token, so every tooltip sets it itself), because
 * react-aria's tooltip otherwise stays open while it is hovered and would swallow clicks aimed at
 * what lies beneath it. Nothing relies on a hoverable tooltip.
 *
 * While a tooltip is open react-aria links it to the control as its description
 * (`aria-describedby`), and VoiceOver speaks a description after the name. A tooltip that only
 * repeats the control's `aria-label` would thus be heard twice, so such a control is given an empty
 * `aria-describedby`, which wins over the tooltip's and refers to nothing. A tooltip that says more
 * than the name stays its description, and a control that sets its own `aria-describedby` keeps
 * it.
 *
 * A touch has no hover, so a tooltip never reaches it. With `touchPopover` a touch or pen press, or
 * an assistive technology's activation, opens the same text in a popover drawn like the tooltip
 * (`tooltipCore.tsx`). A mouse press opens nothing, and neither does Enter or Space: keyboard focus
 * has the tooltip open already, and a press does not close it (as with `keepOpenOnPress`), so the
 * text stays on screen with no popover to close and hold the tooltip back for. It is off by
 * default, because a tap on a control is the control's action (an icon-only button, a menu
 * trigger); it is for a control whose only action is to show its text, such as `HelpTip`'s "?". The
 * control then has to take `onPress`, which it keeps: its own handler runs first. */
export function TitledControl({
  title,
  children,
  placement,
  focusOnly,
  isDisabled,
  keepOpenOnPress,
  touchPopover = false,
  onOpenChange,
}: {
  title: React.ReactNode;
  children: React.ReactElement<{ "aria-label"?: string; "aria-describedby"?: string; onPress?: (event: PressEvent) => void }>;
  /** Where the tooltip opens relative to the control; HeroUI's default is above it. `end` is the
   * reading direction's, so a control on a vertical strip at the window's start edge opens it
   * toward the content under either direction. */
  placement?: "end";
  /** The tooltip opens on keyboard focus only, not on hover. */
  focusOnly?: boolean;
  /** Holds the tooltip back, closing it if open, while the control is showing what it says some other way,
   * such as in a popover the control opened. */
  isDisabled?: boolean;
  /** A press does not close the tooltip, for a control whose tooltip answers the press (a copy
   * button's changes to "Copied"). It still closes as the pointer or focus leaves. */
  keepOpenOnPress?: boolean;
  /** A touch, pen or virtual press opens the text in a popover drawn like the tooltip; see above. */
  touchPopover?: boolean;
  /** Told when the tooltip opens or closes. */
  onOpenChange?: (isOpen: boolean) => void;
}): React.ReactElement {
  const popover = useTouchPopover();
  if (!title) return children;
  const own = children.props;
  const repeatsName = own["aria-label"] === title && own["aria-describedby"] === undefined;
  const extra: Partial<typeof own> = {};
  if (repeatsName) extra["aria-describedby"] = "";
  if (touchPopover) {
    extra.onPress = (event) => {
      own.onPress?.(event);
      if (opensTouchPopover(event.pointerType)) popover.open(event.target);
    };
  }
  return (
    <>
      <Tooltip trigger={focusOnly ? "focus" : undefined} isDisabled={isDisabled || popover.isOpen} shouldCloseOnPress={keepOpenOnPress || touchPopover ? false : undefined} onOpenChange={onOpenChange}>
        {Object.keys(extra).length > 0 ? React.cloneElement(children, extra) : children}
        <TooltipSurface placement={placement}>{title}</TooltipSurface>
        {touchPopover && <CloseTooltipWhile active={popover.isOpen} />}
      </Tooltip>
      {touchPopover && <TouchPopover state={popover} label={own["aria-label"]} placement={placement}>{title}</TouchPopover>}
    </>
  );
}
