import { Tooltip } from "@heroui/react";
import React from "react";

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
 * it. */
export function TitledControl({
  title,
  children,
  placement,
  focusOnly,
}: {
  title: string | undefined;
  children: React.ReactElement<{ "aria-label"?: string; "aria-describedby"?: string }>;
  /** Where the tooltip opens relative to the control; HeroUI's default is above it. `end` is the
   * reading direction's, so a control on a vertical strip at the window's start edge opens it
   * toward the content under either direction. */
  placement?: "end";
  /** The tooltip opens on keyboard focus only, not on hover. */
  focusOnly?: boolean;
}): React.ReactElement {
  if (!title) return children;
  const repeatsName = children.props["aria-label"] === title && children.props["aria-describedby"] === undefined;
  return (
    <Tooltip trigger={focusOnly ? "focus" : undefined}>
      {repeatsName ? React.cloneElement(children, { "aria-describedby": "" }) : children}
      <Tooltip.Content placement={placement} className="pointer-events-none">{title}</Tooltip.Content>
    </Tooltip>
  );
}
