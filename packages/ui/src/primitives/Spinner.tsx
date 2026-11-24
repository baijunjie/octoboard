import { Spinner as HeroSpinner } from "@heroui/react";
import React from "react";

/** The sizes below HeroUI's own: 14px for a compact marker, the device badge and a status strip, 12px for the smallest
 * button. Each is HeroUI's `sm` with its box narrowed. */
const NARROWED = { compact: "size-3.5", xs: "size-3" };

/** HeroUI's spinner, the one glyph for every wait but a refresh. Muted grey wherever it is, except on a surface with a
 * fill of its own (a button, a tinted badge, a status strip), where `color="current"` takes the text colour around it
 * so that it stays visible. Decorative unless it has a `label`, which names it for a spinner that is the only sign of
 * what is loading; a spinner beside words that say it is left unnamed, out of the accessibility tree (HeroUI renders it
 * as a `role="status"` named "Loading", which would otherwise be announced and found among the page's status
 * regions). */
export function Spinner({ label, size = "md", color }: {
  label?: string;
  size?: "xs" | "compact" | "sm" | "md";
  color?: "current";
}): React.ReactElement {
  const heroSize = size === "md" ? "md" : "sm";
  const narrowed = size === "xs" || size === "compact" ? NARROWED[size] : undefined;
  const classes = ["app-spinner", color === "current" ? undefined : "text-app-muted", narrowed].filter(Boolean).join(" ");
  return label === undefined
    ? <HeroSpinner size={heroSize} color="current" role="presentation" aria-label={undefined} aria-hidden="true" className={classes} />
    : <HeroSpinner size={heroSize} color="current" aria-label={label} className={classes} />;
}
