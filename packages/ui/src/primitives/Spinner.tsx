import { Spinner as HeroSpinner } from "@heroui/react";
import React from "react";

/** HeroUI's spinner. Decorative unless it has a `label`, which names it for a spinner that is the only sign of what
 * is loading; a spinner beside words that say it is left unnamed, out of the accessibility tree (HeroUI renders it as a
 * `role="status"` named "Loading", which would otherwise be announced and found among the page's status regions). `color="current"`
 * takes the text colour around it instead of the accent. */
export function Spinner({ label, size = "md", color, className }: {
  label?: string;
  size?: "sm" | "md";
  color?: "current";
  className?: string;
}): React.ReactElement {
  return label === undefined
    ? <HeroSpinner size={size} color={color} role="presentation" aria-label={undefined} aria-hidden="true" className={className} />
    : <HeroSpinner size={size} color={color} aria-label={label} className={className} />;
}
