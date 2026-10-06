import { Tooltip } from "@heroui/react";
import React from "react";

/** Gives a control a HeroUI tooltip, which shows on hover and on keyboard focus. HeroUI's buttons
 * are react-aria-components', which drop a native `title` prop (`filterDOMProps`). The child must
 * be a single react-aria-components control (a HeroUI `Button`, `CloseButton`, `Dropdown.Trigger`,
 * `Modal.CloseTrigger`); any other child silently gets no tooltip. Such a control picks the
 * tooltip's trigger behaviour up from context, so no element is added around it, which keeps a
 * mouse press from taking focus (`preventFocusOnPress` still holds) and leaves layout alone.
 * `Tooltip.Trigger` is deliberately not used: it renders a focusable `role="button"` wrapper. */
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
      <Tooltip.Content>{title}</Tooltip.Content>
    </Tooltip>
  );
}
