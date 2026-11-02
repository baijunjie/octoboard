import { Badge } from "@heroui/react";
import React from "react";

/** A count in a small neutral badge, set in the flow of a row rather than pinned to a corner. It is decorative
 * (`aria-hidden`): the row it sits in carries the number in its accessible name. */
export function CountBadge({ count, className }: { count: number; className?: string }): React.ReactElement {
  return (
    <Badge
      size="sm"
      className={className === undefined ? "static transform-none" : `static transform-none ${className}`}
      aria-hidden="true"
      style={{
        "--badge-bg": "var(--tint-default-bg)",
        "--badge-fg": "var(--default-foreground)",
        "--badge-border": "transparent",
      } as React.CSSProperties}
    >
      <Badge.Label>{count}</Badge.Label>
    </Badge>
  );
}
