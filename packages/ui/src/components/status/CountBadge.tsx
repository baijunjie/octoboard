import { Badge } from "@heroui/react";
import React from "react";

const NEUTRAL = {
  "--badge-bg": "var(--tint-default-bg)",
  "--badge-fg": "var(--default-foreground)",
  "--badge-border": "transparent",
} as React.CSSProperties;

/** A count in a small badge, set in the flow of a row rather than pinned to a corner. It is decorative
 * (`aria-hidden`): the row it sits in carries the number in its accessible name.
 *
 * `default` is a neutral tint, for a chip that carries its own fill at rest. `accent` is HeroUI's solid accent fill
 * and its foreground, for a row drawn bare until hovered or pressed (a ghost button): its opaque grey hover and
 * pressed fills are about as light as the tint and would swallow it. */
export function CountBadge({ count, color = "default", className }: {
  count: number;
  color?: "default" | "accent";
  className?: string;
}): React.ReactElement {
  const accent = color === "accent";
  // Without `border-0` HeroUI rings the solid fill in the background's colour.
  const classes = ["static transform-none", accent ? "border-0" : undefined, className].filter(Boolean).join(" ");
  return (
    <Badge
      size="sm"
      aria-hidden="true"
      className={classes}
      {...(accent ? { color: "accent", variant: "primary" } as const : { style: NEUTRAL })}
    >
      <Badge.Label>{count}</Badge.Label>
    </Badge>
  );
}
