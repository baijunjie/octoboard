import { Badge as HeroBadge } from "@heroui/react";
import React from "react";

/** The corner of its anchor a badge sits on. */
export type BadgePlacement = "top-right" | "top-left" | "bottom-right" | "bottom-left";

/** HeroUI's badge, small and filled, on the corner of the `BadgeAnchor` around it: a dot, or a count or glyph given as
 * `children`. It is decorative, inert and hidden from assistive technology: what it marks is said by its anchor's own
 * name. `dir` is for content that reads the same in every language (a count with an overflow sign). */
export function Badge({ color, placement, dir, children }: {
  color: "accent" | "success" | "warning";
  placement?: BadgePlacement;
  dir?: "ltr";
  children?: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroBadge inert aria-hidden="true" color={color} variant="primary" size="sm" placement={placement} dir={dir}>
      {children}
    </HeroBadge>
  );
}

/** What a `Badge` is placed on: the element it sits on and the badge, together. */
export function BadgeAnchor({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroBadge.Anchor>{children}</HeroBadge.Anchor>;
}

/** `control` with `badge` on its corner, or `control` alone when there is no badge: for a control whose badge comes and
 * goes, so that it takes no anchor of its own meanwhile. */
export function BadgedControl({ badge, children }: { badge?: React.ReactNode; children: React.ReactElement }): React.ReactElement {
  return badge ? <BadgeAnchor>{children}{badge}</BadgeAnchor> : children;
}
