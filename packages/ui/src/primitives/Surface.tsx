import { Surface as HeroSurface } from "@heroui/react";
import React from "react";

/** HeroUI's surfaces: `default` for content set apart on the page's own background, `secondary` for a block set
 * apart on a surface that is already the default one (a dialog, a popover). */
export type SurfaceVariant = "default" | "secondary";

/** HeroUI's surface: a plain block that takes one of the surface fills, for content to sit on. */
export function Surface({ ref, variant = "default", className, children }: {
  ref?: React.Ref<HTMLDivElement>;
  variant?: SurfaceVariant;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroSurface ref={ref} variant={variant} className={className}>{children}</HeroSurface>;
}
