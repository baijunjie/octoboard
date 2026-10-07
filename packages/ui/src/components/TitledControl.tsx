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
 * is `pointer-events-none` (which has no token, so it is set here and on `ConnectionIcon`'s
 * tooltip), because react-aria's tooltip otherwise stays open while it is hovered and would
 * swallow clicks aimed at what lies beneath it. Nothing relies on a hoverable tooltip. */
export function TitledControl({
  title,
  children,
}: {
  title: string | undefined;
  children: React.ReactElement;
}): React.ReactElement {
  if (!title) return children;
  return (
    <Tooltip>
      {children}
      <Tooltip.Content className="pointer-events-none">{title}</Tooltip.Content>
    </Tooltip>
  );
}
